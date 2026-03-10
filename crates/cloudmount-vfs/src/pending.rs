use std::path::{Path, PathBuf};
use std::time::Duration;

use cloudmount_cache::CacheManager;
use cloudmount_graph::GraphClient;

pub(crate) const UNMOUNT_FLUSH_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn flush_pending(cache: &CacheManager, graph: &GraphClient, drive_id: &str) {
    let pending = match cache.writeback.list_pending().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("failed to list pending writes on unmount: {e}");
            return;
        }
    };

    let drive_pending: Vec<_> = pending.into_iter().filter(|(d, _)| d == drive_id).collect();

    if drive_pending.is_empty() {
        return;
    }

    tracing::info!(
        "flushing {} pending writes for drive {}",
        drive_pending.len(),
        drive_id
    );

    let flush_result = tokio::time::timeout(UNMOUNT_FLUSH_TIMEOUT, async {
        for (drive_id, item_id) in &drive_pending {
            recover_single(cache, graph, drive_id, item_id, None, "unmount flush").await;
        }
    })
    .await;

    if flush_result.is_err() {
        tracing::warn!(
            "unmount flush timed out after {}s, {} writes may be pending",
            UNMOUNT_FLUSH_TIMEOUT.as_secs(),
            drive_pending.len()
        );
    }
}

/// Resolve the parent_id and name for uploading a pending item.
///
/// Looks up the item in SQLite to get its `parent_reference.id` and `name`.
/// Falls back to empty parent_id and item_id as name if not found.
fn resolve_upload_params(cache: &CacheManager, item_id: &str) -> (String, String) {
    match cache.sqlite.get_item_by_id(item_id) {
        Ok(Some((_inode, item))) => {
            let parent_id = item
                .parent_reference
                .as_ref()
                .and_then(|p| p.id.as_deref())
                .unwrap_or("")
                .to_string();
            (parent_id, item.name)
        }
        _ => (String::new(), item_id.to_string()),
    }
}

/// Save a `local:*` file's content to a recovery folder instead of discarding it.
async fn save_to_recovery(
    cache: &CacheManager,
    drive_id: &str,
    item_id: &str,
    recovery_dir: &Path,
    label: &str,
) -> bool {
    let content = match cache.writeback.read(drive_id, item_id).await {
        Some(c) => c,
        None => {
            tracing::warn!("{label}: no content for {drive_id}/{item_id}, removing entry");
            let _ = cache.writeback.remove(drive_id, item_id).await;
            return false;
        }
    };

    let sanitized = item_id.replace(':', "_");
    let file_path = recovery_dir.join(format!("{sanitized}.bin"));

    if let Err(e) = tokio::fs::create_dir_all(recovery_dir).await {
        tracing::error!("{label}: failed to create recovery dir: {e}");
        return false;
    }

    if let Err(e) = tokio::fs::write(&file_path, &content).await {
        tracing::error!("{label}: failed to save {drive_id}/{item_id} to recovery: {e}");
        return false;
    }

    // Append to manifest
    let manifest = recovery_dir.join("manifest.txt");
    let entry = format!("{drive_id}\t{item_id}\t{}\n", content.len());
    match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest)
        .await
    {
        Ok(mut f) => {
            if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut f, entry.as_bytes()).await {
                tracing::warn!("{label}: failed to write manifest entry: {e}");
            }
        }
        Err(e) => tracing::warn!("{label}: failed to open manifest: {e}"),
    }

    let _ = cache.writeback.remove(drive_id, item_id).await;
    tracing::error!(
        "{label}: recovered local file {drive_id}/{item_id} to {}",
        file_path.display()
    );
    true
}

/// Build the recovery directory path with a timestamp suffix.
fn recovery_dir() -> Option<PathBuf> {
    let config = cloudmount_core::config::config_dir().ok()?;
    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    Some(config.join("recovered").join(timestamp.to_string()))
}

/// Recover a single pending write: save `local:*` files to recovery folder,
/// or resolve parent_id and upload regular files.
async fn recover_single(
    cache: &CacheManager,
    graph: &GraphClient,
    drive_id: &str,
    item_id: &str,
    recovery_base: Option<&Path>,
    label: &str,
) -> bool {
    if item_id.starts_with("local:") {
        let dir = match recovery_base {
            Some(d) => d.to_path_buf(),
            None => match recovery_dir() {
                Some(d) => d,
                None => {
                    tracing::error!(
                        "{label}: cannot determine recovery dir, discarding {drive_id}/{item_id}"
                    );
                    let _ = cache.writeback.remove(drive_id, item_id).await;
                    return false;
                }
            },
        };
        return save_to_recovery(cache, drive_id, item_id, &dir, label).await;
    }

    let content = match cache.writeback.read(drive_id, item_id).await {
        Some(c) => c,
        None => return false,
    };

    let (parent_id, name) = resolve_upload_params(cache, item_id);

    match graph
        .upload(
            drive_id,
            &parent_id,
            Some(item_id),
            &name,
            bytes::Bytes::from(content),
            None,
        )
        .await
    {
        Ok(_) => {
            let _ = cache.writeback.remove(drive_id, item_id).await;
            tracing::info!("{label}: uploaded {drive_id}/{item_id}");
            true
        }
        Err(e) => {
            tracing::warn!("{label}: upload failed for {drive_id}/{item_id}: {e}");
            false
        }
    }
}

/// Shared recovery loop used by crash recovery and re-auth flush.
///
/// Returns the number of `local:*` files recovered to disk.
pub async fn recover_pending_writes(
    cache: &CacheManager,
    graph: &GraphClient,
    label: &str,
) -> usize {
    let pending = match cache.writeback.list_pending().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("{label}: failed to list pending writes: {e}");
            return 0;
        }
    };

    if pending.is_empty() {
        return 0;
    }

    tracing::info!("{label}: {} pending writes found", pending.len());

    let recovery_base = recovery_dir();
    let mut recovered = 0;

    for (drive_id, item_id) in &pending {
        if item_id.starts_with("local:")
            && recover_single(
                cache,
                graph,
                drive_id,
                item_id,
                recovery_base.as_deref(),
                label,
            )
            .await
        {
            recovered += 1;
        } else if !item_id.starts_with("local:") {
            recover_single(cache, graph, drive_id, item_id, None, label).await;
        }
    }

    recovered
}
