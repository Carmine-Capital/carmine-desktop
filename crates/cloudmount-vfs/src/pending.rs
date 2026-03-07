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
        for (_, item_id) in &drive_pending {
            if let Some(content) = cache.writeback.read(drive_id, item_id).await {
                match graph
                    .upload(
                        drive_id,
                        "",
                        Some(item_id),
                        item_id,
                        bytes::Bytes::from(content),
                    )
                    .await
                {
                    Ok(_) => {
                        let _ = cache.writeback.remove(drive_id, item_id).await;
                    }
                    Err(e) => {
                        tracing::error!("flush upload failed for {item_id}: {e}");
                    }
                }
            }
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
