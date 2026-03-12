use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Semaphore, mpsc, watch};
use tokio::task::JoinHandle;

use crate::core_ops::VfsEvent;
use crate::inode::InodeTable;
use cloudmount_cache::CacheManager;
use cloudmount_graph::GraphClient;

/// Request sent to the sync processor.
#[derive(Debug)]
pub enum SyncRequest {
    /// Schedule an upload for the given inode.
    Flush { ino: u64 },
    /// Drain pending/in-flight uploads and exit.
    Shutdown,
}

/// Configuration for the sync processor.
pub struct SyncProcessorConfig {
    pub max_concurrent_uploads: usize,
    pub debounce_ms: u64,
    pub tick_interval_ms: u64,
    pub shutdown_timeout_secs: u64,
}

impl Default for SyncProcessorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_uploads: 4,
            debounce_ms: 500,
            tick_interval_ms: 1000,
            shutdown_timeout_secs: 30,
        }
    }
}

/// Snapshot of sync processor metrics, updated each tick.
#[derive(Debug, Clone, Default)]
pub struct SyncMetrics {
    pub queue_depth: usize,
    pub in_flight: usize,
    pub failed_count: usize,
    pub total_uploaded: u64,
    pub total_failed: u64,
    pub total_deduplicated: u64,
}

/// Dependencies needed by the sync processor to perform uploads.
pub struct SyncProcessorDeps {
    pub graph: Arc<GraphClient>,
    pub cache: Arc<CacheManager>,
    pub inodes: Arc<InodeTable>,
    pub drive_id: String,
    pub event_tx: Option<mpsc::UnboundedSender<VfsEvent>>,
}

/// Handle for sending requests to a running sync processor.
#[derive(Clone)]
pub struct SyncHandle {
    tx: mpsc::UnboundedSender<SyncRequest>,
    metrics_rx: watch::Receiver<SyncMetrics>,
}

impl SyncHandle {
    /// Send a request to the sync processor.
    ///
    /// If the processor has exited (channel closed), logs a warning and returns.
    pub fn send(&self, req: SyncRequest) {
        if let Err(e) = self.tx.send(req) {
            tracing::warn!("sync processor channel closed, dropping request: {e}");
        }
    }

    /// Read the latest metrics snapshot without blocking the processor.
    pub fn metrics(&self) -> SyncMetrics {
        self.metrics_rx.borrow().clone()
    }
}

/// Result of a single upload task.
struct UploadResult {
    ino: u64,
    success: bool,
}

/// State for a failed upload awaiting retry.
struct FailedEntry {
    retry_count: u32,
    next_retry: Instant,
}

const MAX_RETRIES: u32 = 10;

fn backoff_duration(retry_count: u32) -> Duration {
    let secs = 2u64.pow(retry_count).min(30);
    Duration::from_secs(secs)
}

/// Spawn the sync processor task.
///
/// Returns a `SyncHandle` for sending requests and a `JoinHandle` for awaiting termination.
pub fn spawn_sync_processor(
    deps: SyncProcessorDeps,
    config: SyncProcessorConfig,
) -> (SyncHandle, JoinHandle<()>) {
    let (req_tx, req_rx) = mpsc::unbounded_channel::<SyncRequest>();
    let (result_tx, result_rx) =
        mpsc::channel::<UploadResult>(config.max_concurrent_uploads);
    let (metrics_tx, metrics_rx) = watch::channel(SyncMetrics::default());

    let handle = SyncHandle {
        tx: req_tx,
        metrics_rx,
    };

    let join = tokio::spawn(processor_loop(deps, config, req_rx, result_tx, result_rx, metrics_tx));

    (handle, join)
}

/// The main processor event loop.
async fn processor_loop(
    deps: SyncProcessorDeps,
    config: SyncProcessorConfig,
    mut req_rx: mpsc::UnboundedReceiver<SyncRequest>,
    result_tx: mpsc::Sender<UploadResult>,
    mut result_rx: mpsc::Receiver<UploadResult>,
    metrics_tx: watch::Sender<SyncMetrics>,
) {
    let semaphore = Arc::new(Semaphore::new(config.max_concurrent_uploads));
    let debounce = Duration::from_millis(config.debounce_ms);
    let tick_interval = Duration::from_millis(config.tick_interval_ms);
    let shutdown_timeout = Duration::from_secs(config.shutdown_timeout_secs);

    let mut pending: HashMap<u64, Instant> = HashMap::new();
    let mut in_flight: HashSet<u64> = HashSet::new();
    let mut failed: HashMap<u64, FailedEntry> = HashMap::new();

    let mut total_uploaded: u64 = 0;
    let mut total_failed: u64 = 0;
    let mut total_deduplicated: u64 = 0;

    let deps = Arc::new(deps);

    // Crash recovery: enqueue flushes for all writeback cache entries.
    recover_pending(&deps, &mut pending, &mut total_deduplicated).await;

    let mut tick = tokio::time::interval(tick_interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // Normal operation loop
    loop {
        tokio::select! {
            biased;

            // Priority 1: drain upload results to free concurrency slots
            Some(result) = result_rx.recv() => {
                handle_result(
                    result, &deps, &mut in_flight, &mut failed,
                    &mut total_uploaded, &mut total_failed,
                );
            }

            // Priority 2: receive external requests
            req = req_rx.recv() => {
                match req {
                    Some(SyncRequest::Flush { ino }) => {
                        if in_flight.contains(&ino) {
                            total_deduplicated += 1;
                        } else {
                            use std::collections::hash_map::Entry;
                            match pending.entry(ino) {
                                Entry::Occupied(mut e) => {
                                    e.insert(Instant::now());
                                    total_deduplicated += 1;
                                }
                                Entry::Vacant(e) => {
                                    e.insert(Instant::now());
                                }
                            }
                        }
                    }
                    Some(SyncRequest::Shutdown) => {
                        // Flush all pending immediately (no debounce)
                        let ready: Vec<u64> = pending.keys().copied().collect();
                        for ino in ready {
                            pending.remove(&ino);
                            if !in_flight.contains(&ino) {
                                spawn_upload(ino, &deps, &semaphore, &result_tx, &mut in_flight);
                            }
                        }
                        // Enter shutdown drain phase
                        break;
                    }
                    None => {
                        // Channel closed — exit
                        return;
                    }
                }
            }

            // Priority 3: periodic tick
            _ = tick.tick() => {
                let now = Instant::now();

                // Flush debounced entries whose window has expired
                let ready: Vec<u64> = pending
                    .iter()
                    .filter(|(_, ts)| now.duration_since(**ts) >= debounce)
                    .map(|(&ino, _)| ino)
                    .collect();

                for ino in ready {
                    pending.remove(&ino);
                    if !in_flight.contains(&ino) {
                        spawn_upload(ino, &deps, &semaphore, &result_tx, &mut in_flight);
                    }
                }

                // Retry failed uploads past their backoff
                let retryable: Vec<u64> = failed
                    .iter()
                    .filter(|(_, entry)| now >= entry.next_retry)
                    .map(|(&ino, _)| ino)
                    .collect();

                for ino in retryable {
                    if !in_flight.contains(&ino) && !pending.contains_key(&ino) {
                        pending.insert(ino, Instant::now() - debounce); // ready immediately
                    }
                }

                // Update metrics
                let _ = metrics_tx.send(SyncMetrics {
                    queue_depth: pending.len(),
                    in_flight: in_flight.len(),
                    failed_count: failed.len(),
                    total_uploaded,
                    total_failed,
                    total_deduplicated,
                });
            }
        }
    }

    // Shutdown drain phase: wait for in-flight uploads with timeout
    if !in_flight.is_empty() {
        tracing::info!(
            "sync processor shutting down, waiting for {} in-flight uploads",
            in_flight.len()
        );

        let deadline = tokio::time::Instant::now() + shutdown_timeout;

        while !in_flight.is_empty() {
            tokio::select! {
                biased;

                Some(result) = result_rx.recv() => {
                    handle_result(
                        result, &deps, &mut in_flight, &mut failed,
                        &mut total_uploaded, &mut total_failed,
                    );
                }

                _ = tokio::time::sleep_until(deadline) => {
                    tracing::warn!(
                        "{} uploads still in-flight at shutdown, content preserved in writeback cache",
                        in_flight.len()
                    );
                    break;
                }
            }
        }
    }

    // Final metrics update
    let _ = metrics_tx.send(SyncMetrics {
        queue_depth: pending.len(),
        in_flight: in_flight.len(),
        failed_count: failed.len(),
        total_uploaded,
        total_failed,
        total_deduplicated,
    });
}

/// Handle an upload result: update in-flight set, failed map, and counters.
fn handle_result(
    result: UploadResult,
    deps: &Arc<SyncProcessorDeps>,
    in_flight: &mut HashSet<u64>,
    failed: &mut HashMap<u64, FailedEntry>,
    total_uploaded: &mut u64,
    total_failed: &mut u64,
) {
    in_flight.remove(&result.ino);
    if result.success {
        *total_uploaded += 1;
        failed.remove(&result.ino);
    } else {
        *total_failed += 1;
        let entry = failed.entry(result.ino).or_insert(FailedEntry {
            retry_count: 0,
            next_retry: Instant::now(),
        });
        entry.retry_count += 1;
        if entry.retry_count >= MAX_RETRIES {
            let item_id = deps
                .inodes
                .get_item_id(result.ino)
                .unwrap_or_else(|| format!("ino:{}", result.ino));
            tracing::error!(
                ino = result.ino,
                item_id = %item_id,
                "upload failed {} consecutive times, giving up (content preserved in writeback cache)",
                entry.retry_count
            );
            failed.remove(&result.ino);
        } else {
            entry.next_retry = Instant::now() + backoff_duration(entry.retry_count);
        }
    }
}

/// Spawn an upload task for the given inode.
fn spawn_upload(
    ino: u64,
    deps: &Arc<SyncProcessorDeps>,
    semaphore: &Arc<Semaphore>,
    result_tx: &mpsc::Sender<UploadResult>,
    in_flight: &mut HashSet<u64>,
) {
    in_flight.insert(ino);
    let deps = deps.clone();
    let sem = semaphore.clone();
    let tx = result_tx.clone();

    tokio::spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");
        let success = flush_inode_async(
            ino,
            &deps.graph,
            &deps.cache,
            &deps.inodes,
            &deps.drive_id,
            deps.event_tx.as_ref(),
        )
        .await;

        let _ = tx.send(UploadResult { ino, success }).await;
    });
}

/// Crash recovery: scan writeback cache and enqueue flushes for pending entries.
async fn recover_pending(
    deps: &SyncProcessorDeps,
    pending: &mut HashMap<u64, Instant>,
    total_deduplicated: &mut u64,
) {
    let entries = match deps.cache.writeback.list_pending().await {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!("sync processor: failed to list pending writes for recovery: {e}");
            return;
        }
    };

    let drive_entries: Vec<_> = entries
        .into_iter()
        .filter(|(d, _)| d == &deps.drive_id)
        .collect();

    if drive_entries.is_empty() {
        return;
    }

    tracing::info!(
        "sync processor: recovering {} pending writes from writeback cache",
        drive_entries.len()
    );

    let now = Instant::now();
    for (_, item_id) in drive_entries {
        if item_id.starts_with("local:") {
            tracing::warn!(
                "sync processor: orphaned local file in writeback cache: {}/{}",
                deps.drive_id,
                item_id
            );
            continue;
        }

        match deps.inodes.get_inode(&item_id) {
            Some(ino) => {
                use std::collections::hash_map::Entry;
                match pending.entry(ino) {
                    Entry::Occupied(_) => {
                        *total_deduplicated += 1;
                    }
                    Entry::Vacant(e) => {
                        e.insert(now);
                    }
                }
            }
            None => {
                tracing::warn!(
                    "sync processor: writeback entry {}/{} has no inode mapping, skipping",
                    deps.drive_id,
                    item_id
                );
            }
        }
    }
}

/// Async version of flush_inode that the processor calls for uploads.
///
/// This is the extracted free function that both CoreOps (fallback) and the
/// sync processor use.
pub(crate) async fn flush_inode_async(
    ino: u64,
    graph: &GraphClient,
    cache: &CacheManager,
    inodes: &InodeTable,
    drive_id: &str,
    event_tx: Option<&mpsc::UnboundedSender<VfsEvent>>,
) -> bool {
    let item_id = match inodes.get_item_id(ino) {
        Some(id) => id,
        None => return true, // nothing to flush
    };

    let content = match cache.writeback.read(drive_id, &item_id).await {
        Some(data) => data,
        None => return true, // nothing to flush
    };

    // Look up item metadata from memory cache or SQLite
    let item = cache
        .memory
        .get(ino)
        .or_else(|| {
            cache
                .sqlite
                .get_item_by_id(&item_id)
                .ok()
                .flatten()
                .map(|(_, item)| item)
        });

    let item = match item {
        Some(item) => item,
        None => {
            tracing::error!(ino, item_id = %item_id, "flush_inode_async: item metadata not found");
            return false;
        }
    };

    // Skip transient files
    if crate::core_ops::is_transient_file(&item.name) {
        tracing::debug!(ino, name = %item.name, "skipping upload for transient file");
        let _ = cache.writeback.remove(drive_id, &item_id).await;
        return true;
    }

    let parent_id = item
        .parent_reference
        .as_ref()
        .and_then(|p| p.id.as_deref())
        .unwrap_or("")
        .to_string();

    let is_new_file = item_id.starts_with("local:");
    let content_bytes = bytes::Bytes::from(content);

    // Conflict check for existing files
    let mut server_etag: Option<String> = None;
    if let Some(cached_etag) = item.etag.as_ref()
        && !is_new_file
    {
        match graph.get_item(drive_id, &item_id).await {
            Ok(server_item) => {
                if server_item.etag.as_deref() != Some(cached_etag) {
                    tracing::warn!(
                        "conflict detected for {}: cached={:?}, server={:?}",
                        item.name,
                        item.etag,
                        server_item.etag
                    );
                    let timestamp = chrono::Utc::now().timestamp();
                    let cname = crate::core_ops::conflict_name(&item.name, timestamp);
                    if !parent_id.is_empty()
                        && let Err(e) = graph
                            .upload_small(drive_id, &parent_id, &cname, content_bytes.clone(), None)
                            .await
                    {
                        tracing::error!(
                            "conflict copy upload failed for {}, aborting flush: {e}",
                            item.name
                        );
                        if let Some(tx) = event_tx {
                            let _ = tx.send(VfsEvent::UploadFailed {
                                file_name: item.name.clone(),
                                reason: format!("conflict copy upload failed: {e}"),
                            });
                        }
                        return false;
                    }
                    if let Some(tx) = event_tx {
                        let _ = tx.send(VfsEvent::ConflictDetected {
                            file_name: item.name.clone(),
                            conflict_name: cname,
                        });
                    }
                } else {
                    server_etag = server_item.etag;
                }
            }
            Err(e) => {
                tracing::warn!("conflict check failed for {item_id}: {e}");
            }
        }
    }

    // Persist to disk for crash safety
    let _ = cache.writeback.persist(drive_id, &item_id).await;

    let if_match = server_etag.as_deref();

    let upload_result = if is_new_file {
        if parent_id.is_empty() {
            tracing::error!(ino, "no parent for new file, cannot upload");
            return false;
        }
        graph
            .upload_small(drive_id, &parent_id, &item.name, content_bytes, None)
            .await
    } else {
        graph
            .upload(
                drive_id,
                &parent_id,
                Some(&item_id),
                &item.name,
                content_bytes,
                if_match,
            )
            .await
    };

    match upload_result {
        Ok(updated_item) => {
            if is_new_file {
                inodes.reassign(ino, &updated_item.id);
            }
            cache.memory.insert(ino, updated_item);
            let _ = cache.writeback.remove(drive_id, &item_id).await;
            true
        }
        Err(cloudmount_core::Error::PreconditionFailed) => {
            tracing::warn!(
                "upload precondition failed for {} (412), treating as conflict",
                item.name
            );
            if let Some(tx) = event_tx {
                let _ = tx.send(VfsEvent::ConflictDetected {
                    file_name: item.name.clone(),
                    conflict_name: format!("{} (server version changed)", item.name),
                });
            }
            false
        }
        Err(cloudmount_core::Error::Locked) => {
            tracing::warn!(
                "upload locked for {} (423), saving as conflict copy",
                item.name
            );
            let timestamp = chrono::Utc::now().timestamp();
            let cname = crate::core_ops::conflict_name(&item.name, timestamp);
            if !parent_id.is_empty() {
                // Re-read content for the conflict copy (content_bytes was moved)
                if let Some(content) = cache.writeback.read(drive_id, &item_id).await {
                    match graph
                        .upload_small(
                            drive_id,
                            &parent_id,
                            &cname,
                            bytes::Bytes::from(content),
                            None,
                        )
                        .await
                    {
                        Ok(_) => {
                            let _ = cache.writeback.remove(drive_id, &item_id).await;
                            if let Some(tx) = event_tx {
                                let _ = tx.send(VfsEvent::FileLocked {
                                    file_name: item.name.clone(),
                                });
                            }
                            return true;
                        }
                        Err(e) => {
                            tracing::error!(
                                "conflict copy upload failed for {} (locked): {e}",
                                item.name
                            );
                        }
                    }
                }
            }
            if let Some(tx) = event_tx {
                let _ = tx.send(VfsEvent::UploadFailed {
                    file_name: item.name,
                    reason: "file is locked on OneDrive".to_string(),
                });
            }
            false
        }
        Err(e) => {
            tracing::error!("flush upload failed for {item_id}: {e}");
            if let Some(tx) = event_tx {
                let _ = tx.send(VfsEvent::UploadFailed {
                    file_name: item.name,
                    reason: format!("upload failed: {e}"),
                });
            }
            false
        }
    }
}
