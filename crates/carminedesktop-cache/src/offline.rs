use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use carminedesktop_core::types::DriveItem;
use carminedesktop_graph::GraphClient;

use crate::manager::CacheManager;
use crate::pin_store::PinStore;

/// Callback invoked when a background download fails.
/// Arguments: (folder_name, error_message).
type DownloadErrorCallback = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Result of a pin attempt.
pub enum PinResult {
    /// Pin succeeded, background download spawned.
    Ok,
    /// Pin rejected (e.g. folder too large).
    Rejected { reason: String },
}

pub struct OfflineManager {
    pin_store: Arc<PinStore>,
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,
    drive_id: String,
    ttl_secs: AtomicU64,
    max_folder_bytes: AtomicU64,
    on_download_error: std::sync::RwLock<Option<DownloadErrorCallback>>,
}

impl OfflineManager {
    pub fn new(
        pin_store: Arc<PinStore>,
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        drive_id: String,
        ttl_secs: u64,
        max_folder_bytes: u64,
    ) -> Self {
        Self {
            pin_store,
            graph,
            cache,
            drive_id,
            ttl_secs: AtomicU64::new(ttl_secs),
            max_folder_bytes: AtomicU64::new(max_folder_bytes),
            on_download_error: std::sync::RwLock::new(None),
        }
    }

    pub async fn pin_folder(
        &self,
        item_id: &str,
        folder_name: &str,
    ) -> carminedesktop_core::Result<PinResult> {
        // Fetch item metadata
        let item = self.graph.get_item(&self.drive_id, item_id).await?;

        // Validate it's a folder
        if !item.is_folder() {
            return Ok(PinResult::Rejected {
                reason: "only folders can be pinned for offline use".to_string(),
            });
        }

        // Size validation: if size <= 0, re-fetch to get accurate size
        let actual_size = if item.size <= 0 {
            let refreshed_item = self.graph.get_item(&self.drive_id, item_id).await?;
            refreshed_item.size.max(0) as u64
        } else {
            item.size.max(0) as u64
        };

        // Compare size vs max_folder_bytes
        let max_bytes = self.max_folder_bytes.load(Ordering::Relaxed);
        if max_bytes > 0 && actual_size > max_bytes {
            return Ok(PinResult::Rejected {
                reason: format!(
                    "folder '{}' ({}) exceeds maximum size limit ({})",
                    folder_name,
                    format_bytes(actual_size),
                    format_bytes(max_bytes)
                ),
            });
        }

        // Pin the folder
        let ttl = self.ttl_secs.load(Ordering::Relaxed);
        self.pin_store.pin(&self.drive_id, item_id, ttl)?;

        // Spawn background download task
        let graph = self.graph.clone();
        let cache = self.cache.clone();
        let drive_id = self.drive_id.clone();
        let item_id = item_id.to_string();
        let folder_name = folder_name.to_string();
        let error_handler = self.on_download_error.read().unwrap().clone();

        tokio::spawn(async move {
            if let Err(e) = recursive_download(&graph, &cache, &drive_id, &item_id).await {
                tracing::error!(
                    "offline: recursive download failed for {}/{}: {}",
                    drive_id,
                    item_id,
                    e
                );
                if let Some(handler) = &error_handler {
                    handler(&folder_name, &e.to_string());
                }
            }
        });

        Ok(PinResult::Ok)
    }

    pub fn unpin_folder(&self, item_id: &str) -> carminedesktop_core::Result<()> {
        self.pin_store.unpin(&self.drive_id, item_id)
    }

    pub fn process_expired(&self) -> carminedesktop_core::Result<Vec<String>> {
        let expired = self.pin_store.list_expired()?;
        let mut expired_ids = Vec::new();

        for record in expired {
            self.pin_store.unpin(&record.drive_id, &record.item_id)?;
            tracing::info!(
                "offline: expired pin for {}/{}",
                record.drive_id,
                record.item_id
            );
            expired_ids.push(record.item_id);
        }

        Ok(expired_ids)
    }

    pub async fn redownload_changed_items(
        &self,
        changed_items: &[DriveItem],
    ) -> carminedesktop_core::Result<()> {
        if changed_items.is_empty() {
            return Ok(());
        }

        let pinned_folders = self.pin_store.list_all()?;
        if pinned_folders.is_empty() {
            return Ok(());
        }

        // Build HashSet of pinned item_ids
        let pinned_set: HashSet<String> = pinned_folders.into_iter().map(|pf| pf.item_id).collect();

        for item in changed_items {
            // Check if this item's parent is in the pinned set
            if let Some(parent_ref) = &item.parent_reference
                && let Some(parent_id) = &parent_ref.id
                && pinned_set.contains(parent_id)
            {
                // Re-download the item if it's a file
                if !item.is_folder() {
                    let content = self
                        .graph
                        .download_content(&self.drive_id, &item.id)
                        .await?;
                    self.cache
                        .disk
                        .put(&self.drive_id, &item.id, &content, item.etag.as_deref())
                        .await?;
                    tracing::debug!("offline: re-downloaded {}/{}", self.drive_id, item.id);
                }
            }
        }

        Ok(())
    }

    pub fn set_ttl_secs(&self, ttl: u64) {
        self.ttl_secs.store(ttl, Ordering::Relaxed);
    }

    pub fn set_max_folder_bytes(&self, max: u64) {
        self.max_folder_bytes.store(max, Ordering::Relaxed);
    }

    /// Set a callback invoked when a background download fails.
    pub fn set_download_error_handler(&self, handler: DownloadErrorCallback) {
        *self.on_download_error.write().unwrap() = Some(handler);
    }
}

async fn recursive_download(
    graph: &GraphClient,
    cache: &CacheManager,
    drive_id: &str,
    folder_id: &str,
) -> carminedesktop_core::Result<()> {
    let children = graph.list_children(drive_id, folder_id).await?;

    for child in &children {
        if child.is_folder() {
            Box::pin(recursive_download(graph, cache, drive_id, &child.id)).await?;
        } else {
            // Download file content if not already cached
            if cache.disk.get(drive_id, &child.id).await.is_none() {
                let content = graph.download_content(drive_id, &child.id).await?;
                cache
                    .disk
                    .put(drive_id, &child.id, &content, child.etag.as_deref())
                    .await?;
                tracing::debug!("offline: downloaded {}/{}", drive_id, child.id);
            }
        }
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1_073_741_824;
    const MB: u64 = 1_048_576;
    const KB: u64 = 1_024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} bytes")
    }
}
