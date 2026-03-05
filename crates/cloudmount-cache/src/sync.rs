use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::CacheManager;
use cloudmount_graph::GraphClient;

pub struct DeltaSyncTimer {
    cancel: CancellationToken,
    handle: Option<JoinHandle<()>>,
    interval_secs: AtomicU64,
}

impl DeltaSyncTimer {
    pub fn start(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        drive_ids: Arc<RwLock<Vec<String>>>,
        inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync>,
        interval_secs: u64,
    ) -> Self {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let interval = AtomicU64::new(interval_secs);

        let handle = tokio::spawn(async move {
            loop {
                let wait = Duration::from_secs(interval_secs);
                tokio::select! {
                    _ = cancel_clone.cancelled() => break,
                    _ = tokio::time::sleep(wait) => {}
                }

                let drives = drive_ids.read().unwrap().clone();
                for drive_id in &drives {
                    if let Err(e) = run_delta_sync(&graph, &cache, drive_id, &inode_allocator).await
                    {
                        tracing::error!("delta sync failed for drive {drive_id}: {e}");
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
) -> cloudmount_core::Result<()> {
    let delta_token = cache.sqlite.get_delta_token(drive_id)?;
    let response = graph.delta_query(drive_id, delta_token.as_deref()).await?;

    let mut upserts = Vec::new();
    let mut deletes = Vec::new();

    for item in &response.value {
        if item.name.is_empty() && item.file.is_none() && item.folder.is_none() {
            deletes.push(item.id.clone());
            cache.memory.invalidate(inode_allocator(&item.id));
            let _ = cache.disk.remove(drive_id, &item.id).await;
            continue;
        }

        let inode = inode_allocator(&item.id);
        let parent_inode = item
            .parent_reference
            .as_ref()
            .and_then(|p| p.id.as_deref())
            .map(|pid| inode_allocator(pid));

        cache.memory.insert(inode, item.clone());
        upserts.push((inode, item.clone(), parent_inode));
    }

    let new_token = response.delta_link.as_deref().unwrap_or_default();

    if !new_token.is_empty() {
        cache
            .sqlite
            .apply_delta(drive_id, &upserts, &deletes, new_token)?;
    }

    tracing::debug!(
        "delta sync for {drive_id}: {} upserts, {} deletes",
        upserts.len(),
        deletes.len()
    );

    Ok(())
}
