use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::CacheManager;
use carminedesktop_core::DeltaSyncObserver;
use carminedesktop_core::types::DriveItem;
use carminedesktop_graph::GraphClient;

/// Strip the `/drive/root:` or `/drives/{id}/root:` prefix from a Graph API parent path,
/// returning the path relative to the drive root. Returns an empty string for root-level items.
fn strip_drive_root_prefix(path: &str) -> &str {
    // Format: `/drive/root:` or `/drives/{drive-id}/root:`
    if let Some(rest) = path.strip_prefix("/drive/root:") {
        rest.strip_prefix('/').unwrap_or(rest)
    } else if let Some(idx) = path.find("/root:") {
        let rest = &path[idx + "/root:".len()..];
        rest.strip_prefix('/').unwrap_or(rest)
    } else {
        ""
    }
}

/// Resolve the mount-relative filesystem path for a `DriveItem` from its `parentReference.path`
/// and `name`. Returns `None` if the path cannot be resolved (e.g., missing `parentReference`).
pub fn resolve_relative_path(item: &DriveItem) -> Option<PathBuf> {
    let parent_path = item
        .parent_reference
        .as_ref()
        .and_then(|pr| pr.path.as_deref())?;

    let relative_parent = strip_drive_root_prefix(parent_path);
    let path = if relative_parent.is_empty() {
        PathBuf::from(&item.name)
    } else {
        PathBuf::from(relative_parent).join(&item.name)
    };
    Some(path)
}

/// Resolve the mount-relative filesystem path for a deleted item from its captured
/// parent path and name. Returns `None` if the name is empty or parent path is missing.
pub fn resolve_deleted_path(info: &DeletedItemInfo) -> Option<PathBuf> {
    if info.name.is_empty() {
        return None;
    }

    let parent_path = info.parent_path.as_deref()?;
    let relative_parent = strip_drive_root_prefix(parent_path);
    let path = if relative_parent.is_empty() {
        PathBuf::from(&info.name)
    } else {
        PathBuf::from(relative_parent).join(&info.name)
    };
    Some(path)
}

/// Result of a delta sync operation, containing items that changed or were deleted.
/// Callers can use this to propagate updates to platform-specific layers
/// (e.g., WinFsp placeholder updates on Windows).
#[derive(Debug, Clone, Default)]
pub struct DeltaSyncResult {
    /// Items whose eTag changed (content was modified on the server).
    pub changed_items: Vec<DriveItem>,
    /// Items that were deleted on the server, with path info captured before cache removal.
    pub deleted_items: Vec<DeletedItemInfo>,
}

/// Information about a deleted item, captured before it is removed from caches.
#[derive(Debug, Clone)]
pub struct DeletedItemInfo {
    pub id: String,
    pub name: String,
    pub parent_path: Option<String>,
}

pub struct DeltaSyncTimer {
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
    interval_secs: Arc<AtomicU64>,
}

impl DeltaSyncTimer {
    pub fn start(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        drive_ids: Arc<RwLock<Vec<String>>>,
        inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync>,
        interval_secs: u64,
        observer: Option<Arc<dyn DeltaSyncObserver>>,
    ) -> Self {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let interval = Arc::new(AtomicU64::new(interval_secs));
        let interval_clone = interval.clone();

        let handle = tokio::spawn(async move {
            loop {
                let wait = Duration::from_secs(interval_clone.load(Ordering::Relaxed));
                tokio::select! {
                    _ = cancel_clone.cancelled() => break,
                    _ = tokio::time::sleep(wait) => {}
                }

                let drives = drive_ids.read().unwrap().clone();
                for drive_id in &drives {
                    match run_delta_sync(
                        &graph,
                        &cache,
                        drive_id,
                        &inode_allocator,
                        observer.as_deref(),
                    )
                    .await
                    {
                        Ok(_result) => {}
                        Err(e) => {
                            tracing::error!("delta sync failed for drive {drive_id}: {e}");
                        }
                    }
                }
            }
        });

        Self {
            cancel,
            handle: Some(handle),
            interval_secs: interval,
        }
    }

    pub fn set_interval(&self, secs: u64) {
        self.interval_secs.store(secs, Ordering::Relaxed);
    }

    pub fn stop(&mut self) {
        self.cancel.cancel();
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

impl Drop for DeltaSyncTimer {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn run_delta_sync(
    graph: &GraphClient,
    cache: &CacheManager,
    drive_id: &str,
    inode_allocator: &Arc<dyn Fn(&str) -> u64 + Send + Sync>,
    observer: Option<&dyn DeltaSyncObserver>,
) -> carminedesktop_core::Result<DeltaSyncResult> {
    let delta_token = cache.sqlite.get_delta_token(drive_id)?;
    let response = graph.delta_query(drive_id, delta_token.as_deref()).await?;

    let mut upserts = Vec::new();
    let mut deletes = Vec::new();
    let mut result = DeltaSyncResult::default();

    for item in &response.value {
        if item.name.is_empty() {
            // Capture deleted item info and parent inode BEFORE removing from caches
            let (deleted_info, parent_inode) = match cache.sqlite.get_item_by_id(&item.id) {
                Ok(Some((_, old_item))) => {
                    let parent_ino = old_item
                        .parent_reference
                        .as_ref()
                        .and_then(|pr| pr.id.as_deref())
                        .map(|pid| inode_allocator(pid));
                    (
                        DeletedItemInfo {
                            id: item.id.clone(),
                            name: old_item.name.clone(),
                            parent_path: old_item
                                .parent_reference
                                .as_ref()
                                .and_then(|pr| pr.path.clone()),
                        },
                        parent_ino,
                    )
                }
                _ => (
                    DeletedItemInfo {
                        id: item.id.clone(),
                        name: String::new(),
                        parent_path: None,
                    },
                    None,
                ),
            };
            result.deleted_items.push(deleted_info);

            deletes.push(item.id.clone());
            cache.memory.invalidate(inode_allocator(&item.id));
            if let Some(parent_ino) = parent_inode {
                cache.memory.invalidate(parent_ino);
            }
            let _ = cache.disk.remove(drive_id, &item.id).await;
            continue;
        }

        let inode = inode_allocator(&item.id);
        let parent_inode = item
            .parent_reference
            .as_ref()
            .and_then(|p| p.id.as_deref())
            .map(|pid| inode_allocator(pid));

        // For file items, check if content changed (eTag mismatch) and invalidate disk cache
        if item.file.is_some() {
            let old_etag = cache
                .sqlite
                .get_item_by_id(&item.id)
                .ok()
                .flatten()
                .and_then(|(_, old_item)| old_item.etag);

            let new_etag = &item.etag;
            let etag_changed = match (&old_etag, new_etag) {
                (Some(old), Some(new)) => old != new,
                (None, _) => false, // new item or no prior eTag — no invalidation needed
                (Some(_), None) => false, // server didn't provide eTag — skip
            };

            if etag_changed {
                let _ = cache.disk.remove(drive_id, &item.id).await;
                cache.dirty_inodes.insert(inode);
                result.changed_items.push(item.clone());
                if let Some(obs) = observer {
                    obs.on_inode_content_changed(inode);
                }
                tracing::debug!(
                    "delta sync: eTag changed for {}, invalidated disk cache and marked inode {inode} dirty",
                    item.id
                );
            }
        }

        cache.memory.insert(inode, item.clone());
        if let Some(parent_ino) = parent_inode {
            cache.memory.invalidate(parent_ino);
        }
        upserts.push((inode, item.clone(), parent_inode));
    }

    let new_token = response.delta_link.as_deref().unwrap_or_default();

    if !new_token.is_empty() {
        cache
            .sqlite
            .apply_delta(drive_id, &upserts, &deletes, new_token)?;
    }

    tracing::debug!(
        "delta sync for {drive_id}: {} upserts, {} deletes, {} changed, {} deleted_with_info",
        upserts.len(),
        deletes.len(),
        result.changed_items.len(),
        result.deleted_items.len()
    );

    Ok(result)
}
