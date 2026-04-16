//! Debounced fan-out of `pin:health` / `pin:removed` events to the frontend.
//!
//! The Solid frontend renders each pin card from a per-pin signal; to keep it
//! zero-flicker we push a granular event only when the snapshot for that pin
//! actually changed (total/cached/status tuple).  A single aggregator task
//! owns the last-emitted snapshot and compares it against a freshly computed
//! one whenever the cache signals a write or an explicit pin/unpin lands.
//!
//! Triggers are coalesced inside a 250 ms debounce window so a burst of
//! recursive-download `disk.put` calls collapses into a single re-emit per
//! pin.  A single in-flight flush is allowed at a time — any event that
//! arrives during a flush simply re-arms the debounce.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio::time::{Instant as TokioInstant, sleep_until};

use carminedesktop_cache::CacheManager;
use carminedesktop_core::types::{PinHealthEvent, PinRemovedEvent};

use crate::AppState;

const DEBOUNCE: Duration = Duration::from_millis(250);

/// Input to the aggregator.  `Cache` signals a disk put/remove/eviction;
/// `DriveRefresh` forces a re-scan for a drive even if no cache change was
/// seen (e.g. an explicit pin/unpin command that affects the pin list itself,
/// not individual file counts).
#[derive(Debug, Clone)]
pub enum PinDirty {
    Cache {
        drive_id: String,
        #[allow(dead_code)] // item_id is informational; aggregator rescans the whole drive anyway
        item_id: String,
    },
    DriveRefresh {
        drive_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Snapshot {
    folder_name: String,
    mount_name: String,
    status: String,
    total_files: usize,
    cached_files: usize,
    pinned_at: String,
    expires_at: String,
}

impl Snapshot {
    fn differs_from_event(&self, ev: &PinHealthEvent) -> bool {
        self.folder_name != ev.folder_name
            || self.mount_name != ev.mount_name
            || self.status != ev.status
            || self.total_files != ev.total_files
            || self.cached_files != ev.cached_files
            || self.pinned_at != ev.pinned_at
            || self.expires_at != ev.expires_at
    }
}

pub fn spawn_aggregator(app: AppHandle, mut rx: mpsc::UnboundedReceiver<PinDirty>) {
    tauri::async_runtime::spawn(async move {
        // Last value we emitted, keyed by (drive_id, item_id).  A pin missing
        // from a newly computed snapshot for its drive means it was removed.
        let mut last: HashMap<(String, String), Snapshot> = HashMap::new();
        let mut drives_dirty: HashSet<String> = HashSet::new();
        let mut deadline: Option<TokioInstant> = None;

        loop {
            let sleep_fut = async {
                match deadline {
                    Some(t) => sleep_until(t).await,
                    None => std::future::pending::<()>().await,
                }
            };

            tokio::select! {
                biased;
                maybe_ev = rx.recv() => {
                    match maybe_ev {
                        None => break,
                        Some(ev) => {
                            let drive_id = match ev {
                                PinDirty::Cache { drive_id, .. } => drive_id,
                                PinDirty::DriveRefresh { drive_id } => drive_id,
                            };
                            drives_dirty.insert(drive_id);
                            deadline = Some(TokioInstant::now() + DEBOUNCE);
                        }
                    }
                }
                _ = sleep_fut => {
                    deadline = None;
                    if drives_dirty.is_empty() { continue; }
                    let drives: Vec<String> = drives_dirty.drain().collect();
                    flush(&app, &drives, &mut last).await;
                }
            }
        }
    });
}

async fn flush(app: &AppHandle, drives: &[String], last: &mut HashMap<(String, String), Snapshot>) {
    // Snapshot inputs we need from AppState under the lock, then drop it.
    let snapshot: Vec<(String, String, Arc<CacheManager>)> = {
        let state = app.state::<AppState>();
        let mount_names: HashMap<String, String> = {
            let config = state.effective_config.lock().unwrap();
            config
                .mounts
                .iter()
                .filter_map(|m| m.drive_id.as_ref().map(|d| (d.clone(), m.name.clone())))
                .collect()
        };
        let caches = state.mount_caches.lock().unwrap();
        drives
            .iter()
            .filter_map(|drive_id| {
                let entry = caches.get(drive_id)?;
                let mount_name = mount_names
                    .get(drive_id)
                    .cloned()
                    .unwrap_or_else(|| drive_id.clone());
                Some((drive_id.clone(), mount_name, entry.0.clone()))
            })
            .collect()
    };
    let stale_pins = {
        let state = app.state::<AppState>();
        state.stale_pins.lock().unwrap().clone()
    };

    // Track which pin keys we saw for each drive so we can emit `pin:removed`
    // for anything that dropped out of the pin list.
    let mut seen: HashMap<String, HashSet<(String, String)>> = HashMap::new();

    for (drive_id, mount_name, cache) in &snapshot {
        let health = match cache.pin_store.health(&stale_pins) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("pin aggregator: health() failed for {drive_id}: {e}");
                continue;
            }
        };
        let drive_seen = seen.entry(drive_id.clone()).or_default();

        for (pin, total_files, cached_files) in health {
            let folder_name = cache
                .sqlite
                .get_item_by_id(&pin.item_id)
                .ok()
                .flatten()
                .map(|(_, item)| item.name)
                .unwrap_or_else(|| pin.item_id.clone());

            let status = if stale_pins.contains(&(pin.drive_id.clone(), pin.item_id.clone())) {
                "stale".to_string()
            } else if cached_files >= total_files {
                "downloaded".to_string()
            } else {
                "partial".to_string()
            };

            let key = (pin.drive_id.clone(), pin.item_id.clone());
            drive_seen.insert(key.clone());

            let ev = PinHealthEvent {
                drive_id: pin.drive_id,
                item_id: pin.item_id,
                folder_name,
                mount_name: mount_name.clone(),
                status,
                total_files,
                cached_files,
                pinned_at: pin.pinned_at,
                expires_at: pin.expires_at,
            };

            let changed = match last.get(&key) {
                Some(prev) => prev.differs_from_event(&ev),
                None => true,
            };

            if changed {
                last.insert(
                    key,
                    Snapshot {
                        folder_name: ev.folder_name.clone(),
                        mount_name: ev.mount_name.clone(),
                        status: ev.status.clone(),
                        total_files: ev.total_files,
                        cached_files: ev.cached_files,
                        pinned_at: ev.pinned_at.clone(),
                        expires_at: ev.expires_at.clone(),
                    },
                );
                if let Err(e) = app.emit("pin:health", &ev) {
                    tracing::warn!("pin aggregator: emit pin:health failed: {e}");
                }
            }
        }
    }

    // Detect removed pins: any key we used to know about for one of the
    // refreshed drives that did not appear in this flush has been unpinned
    // or expired.  Emit one `pin:removed` for each.
    let removed: Vec<(String, String)> = last
        .keys()
        .filter(|(drive_id, _)| drives.iter().any(|d| d == drive_id))
        .filter(|key| seen.get(&key.0).map(|s| !s.contains(key)).unwrap_or(true))
        .cloned()
        .collect();

    for key in removed {
        last.remove(&key);
        let ev = PinRemovedEvent {
            drive_id: key.0,
            item_id: key.1,
        };
        if let Err(e) = app.emit("pin:removed", &ev) {
            tracing::warn!("pin aggregator: emit pin:removed failed: {e}");
        }
    }
}
