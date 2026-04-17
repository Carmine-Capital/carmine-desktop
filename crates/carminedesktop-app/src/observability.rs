//! Observability infrastructure: ring buffers for errors and activity,
//! and an event bridge that routes ObsEvent from the broadcast channel
//! to Tauri emit, error accumulator, and activity buffer.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;

use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

use carminedesktop_core::types::{
    ActivityEntry, AuthStateEvent, DashboardError, DriveOnlineEvent, DriveStatusEvent, ObsEvent,
};

/// Fixed-capacity ring buffer for dashboard errors.
/// Oldest entries dropped when buffer is full.
pub struct ErrorAccumulator {
    entries: VecDeque<DashboardError>,
    capacity: usize,
}

impl ErrorAccumulator {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, entry: DashboardError) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Return all entries in insertion order (oldest first).
    pub fn drain(&self) -> Vec<DashboardError> {
        self.entries.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Fixed-capacity ring buffer for activity feed entries.
/// Oldest entries dropped when buffer is full.
pub struct ActivityBuffer {
    entries: VecDeque<ActivityEntry>,
    capacity: usize,
}

impl ActivityBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, entry: ActivityEntry) {
        if self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Append multiple entries at once (for batch delta sync results).
    pub fn push_batch(&mut self, entries: impl IntoIterator<Item = ActivityEntry>) {
        for entry in entries {
            self.push(entry);
        }
    }

    /// Return all entries in insertion order (oldest first).
    pub fn drain(&self) -> Vec<ActivityEntry> {
        self.entries.iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Spawn the event bridge task that subscribes to the ObsEvent broadcast channel
/// and routes events to:
/// 1. Tauri emit() for real-time frontend delivery
/// 2. ErrorAccumulator for error ring buffer
/// 3. ActivityBuffer for activity ring buffer
pub fn spawn_event_bridge(
    app: AppHandle,
    mut obs_rx: broadcast::Receiver<ObsEvent>,
    errors: Arc<Mutex<ErrorAccumulator>>,
    activity: Arc<Mutex<ActivityBuffer>>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            match obs_rx.recv().await {
                Ok(event) => {
                    // Fan out to a granular, typed topic per variant and route to
                    // the appropriate ring buffer when the event has one.
                    match &event {
                        ObsEvent::Error {
                            drive_id,
                            file_name,
                            remote_path,
                            error_type,
                            message,
                            action_hint,
                            timestamp,
                        } => {
                            let entry = DashboardError {
                                drive_id: drive_id.clone(),
                                file_name: file_name.clone(),
                                remote_path: remote_path.clone(),
                                error_type: error_type.clone(),
                                message: message.clone(),
                                action_hint: action_hint.clone(),
                                timestamp: timestamp.clone(),
                            };
                            if let Ok(mut buf) = errors.lock() {
                                buf.push(entry.clone());
                            }
                            let _ = app.emit("error:append", &entry);
                        }
                        ObsEvent::Activity(entry) => {
                            if let Ok(mut buf) = activity.lock() {
                                buf.push(entry.clone());
                            }
                            let _ = app.emit("activity:append", entry);
                        }
                        ObsEvent::SyncStateChanged { drive_id, state } => {
                            let _ = app.emit(
                                "drive:status",
                                &DriveStatusEvent {
                                    drive_id: drive_id.clone(),
                                    state: state.clone(),
                                },
                            );
                        }
                        ObsEvent::OnlineStateChanged { drive_id, online } => {
                            let _ = app.emit(
                                "drive:online",
                                &DriveOnlineEvent {
                                    drive_id: drive_id.clone(),
                                    online: *online,
                                },
                            );
                        }
                        ObsEvent::AuthStateChanged { degraded } => {
                            let _ = app.emit(
                                "auth:state",
                                &AuthStateEvent {
                                    degraded: *degraded,
                                },
                            );
                        }
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("event bridge lagged, missed {n} events");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("event bridge: broadcast channel closed, exiting");
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_error(msg: &str) -> DashboardError {
        DashboardError {
            drive_id: None,
            file_name: None,
            remote_path: None,
            error_type: "test".to_string(),
            message: msg.to_string(),
            action_hint: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_activity(path: &str) -> ActivityEntry {
        use carminedesktop_core::types::{ActivityKind, ActivitySource};
        ActivityEntry {
            id: "act-test".to_string(),
            drive_id: "drive-1".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            file_path: path.to_string(),
            file_name: path.rsplit('/').next().unwrap_or(path).to_string(),
            is_folder: false,
            source: ActivitySource::Remote,
            kind: ActivityKind::Modified,
            size_bytes: None,
            group_id: None,
        }
    }

    #[test]
    fn test_error_accumulator_push_and_drain() {
        let mut acc = ErrorAccumulator::new(100);
        assert!(acc.is_empty());

        acc.push(make_error("err1"));
        let drained = acc.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].message, "err1");
    }

    #[test]
    fn test_error_accumulator_drops_oldest_when_full() {
        let mut acc = ErrorAccumulator::new(3);
        for i in 0..5 {
            acc.push(make_error(&format!("err{i}")));
        }
        let drained = acc.drain();
        assert_eq!(drained.len(), 3);
        // Oldest (err0, err1) should have been dropped
        assert_eq!(drained[0].message, "err2");
        assert_eq!(drained[1].message, "err3");
        assert_eq!(drained[2].message, "err4");
    }

    #[test]
    fn test_error_accumulator_drain_returns_insertion_order() {
        let mut acc = ErrorAccumulator::new(100);
        acc.push(make_error("first"));
        acc.push(make_error("second"));
        acc.push(make_error("third"));
        let drained = acc.drain();
        assert_eq!(drained[0].message, "first");
        assert_eq!(drained[1].message, "second");
        assert_eq!(drained[2].message, "third");
    }

    #[test]
    fn test_activity_buffer_push_and_drain() {
        let mut buf = ActivityBuffer::new(500);
        assert!(buf.is_empty());

        buf.push(make_activity("/docs/file.txt"));
        let drained = buf.drain();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].file_path, "/docs/file.txt");
    }

    #[test]
    fn test_activity_buffer_drops_oldest_when_full() {
        let mut buf = ActivityBuffer::new(3);
        for i in 0..5 {
            buf.push(make_activity(&format!("/file{i}.txt")));
        }
        let drained = buf.drain();
        assert_eq!(drained.len(), 3);
        assert_eq!(drained[0].file_path, "/file2.txt");
        assert_eq!(drained[1].file_path, "/file3.txt");
        assert_eq!(drained[2].file_path, "/file4.txt");
    }

    #[test]
    fn test_activity_buffer_drain_returns_insertion_order() {
        let mut buf = ActivityBuffer::new(500);
        buf.push(make_activity("/first.txt"));
        buf.push(make_activity("/second.txt"));
        buf.push(make_activity("/third.txt"));
        let drained = buf.drain();
        assert_eq!(drained[0].file_path, "/first.txt");
        assert_eq!(drained[1].file_path, "/second.txt");
        assert_eq!(drained[2].file_path, "/third.txt");
    }

    #[test]
    fn test_error_accumulator_len() {
        let mut acc = ErrorAccumulator::new(100);
        assert_eq!(acc.len(), 0);
        acc.push(make_error("e1"));
        assert_eq!(acc.len(), 1);
        acc.push(make_error("e2"));
        assert_eq!(acc.len(), 2);
    }
}
