//! Centralized activity feed collector.
//!
//! Every site that wants to publish an entry to the activity feed routes
//! through `ActivityCollector::record()`. The collector:
//!
//! 1. Skips transient filenames (Office lock, `~$*`, `*.tmp`, `.DS_Store`,
//!    `Thumbs.db`, `desktop.ini`) via `is_transient_file` from the VFS crate.
//! 2. Deduplicates `Created`/`Modified`/`Deleted` for the same
//!    `(drive_id, item_id, kind)` within a short TTL, so a local upload that
//!    succeeds is not doubled when the next delta sync re-sees the same item.
//! 3. Groups events sharing `(drive_id, parent_path, kind, source)` inside a
//!    2 s sliding window under a shared `group_id` — the UI collapses those
//!    into a single expandable row ("Reports/ — 50 fichiers créés").
//! 4. Stamps an opaque `id` (UUIDv4) and the RFC3339 timestamp, derives
//!    `file_name` from `file_path`, then emits `ObsEvent::Activity(entry)`
//!    on the broadcast channel. The event bridge in `observability.rs` does
//!    the rest (ring buffer + Tauri emit).
//!
//! The collector must never be called from a context holding an `AppState`
//! lock — `record()` itself takes internal mutexes and it emits on the
//! broadcast sender. Snapshot fields, drop locks, then call.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;
use uuid::Uuid;

use carminedesktop_core::types::{ActivityEntry, ActivityKind, ActivitySource, ObsEvent};
use carminedesktop_vfs::core_ops::is_transient_file;

/// TTL for the dedup LRU — a local emission blocks a remote emission for
/// the same `(drive_id, item_id, kind)` during this window. 5 s comfortably
/// covers the delay between CommitUpload success and the next delta tick.
const DEDUP_TTL: Duration = Duration::from_secs(5);

/// Upper bound on concurrent dedup entries. Oldest entries are evicted when
/// the capacity is reached, regardless of TTL.
const DEDUP_CAPACITY: usize = 512;

/// Sliding window during which consecutive events sharing
/// `(drive_id, parent_path, kind, source)` are assigned the same
/// `group_id`. 2 s accommodates a large Explorer copy without merging two
/// unrelated bursts.
const GROUP_WINDOW: Duration = Duration::from_secs(2);

/// Input for `ActivityCollector::record()`. Callers describe the observed
/// event; the collector derives `id`, `file_name`, `timestamp`, `group_id`.
#[derive(Debug, Clone)]
pub struct ActivityInput {
    pub drive_id: String,
    pub source: ActivitySource,
    pub kind: ActivityKind,
    pub file_path: String,
    /// Canonical Graph/SQLite item id. Required for dedup on
    /// `Created`/`Modified`/`Deleted`; `None` skips dedup (still emits).
    pub item_id: Option<String>,
    pub is_folder: bool,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct DedupKey {
    drive_id: String,
    item_id: String,
    kind_tag: &'static str,
}

#[derive(Default)]
struct DedupState {
    queue: VecDeque<(DedupKey, Instant)>,
    by_key: HashMap<DedupKey, Instant>,
}

impl DedupState {
    fn contains(&self, key: &DedupKey) -> bool {
        self.by_key.contains_key(key)
    }

    fn insert(&mut self, key: DedupKey, now: Instant) {
        if self.queue.len() >= DEDUP_CAPACITY
            && let Some((old, _)) = self.queue.pop_front()
        {
            self.by_key.remove(&old);
        }
        self.queue.push_back((key.clone(), now));
        self.by_key.insert(key, now);
    }

    fn purge_expired(&mut self, now: Instant) {
        while let Some((_, ts)) = self.queue.front() {
            if now.duration_since(*ts) <= DEDUP_TTL {
                break;
            }
            if let Some((k, _)) = self.queue.pop_front() {
                self.by_key.remove(&k);
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct GroupKey {
    drive_id: String,
    parent_path: String,
    source: ActivitySource,
    kind_tag: &'static str,
}

#[derive(Default)]
struct GroupState {
    by_key: HashMap<GroupKey, (String, Instant)>,
}

impl GroupState {
    fn assign(&mut self, key: GroupKey, now: Instant) -> String {
        self.by_key
            .retain(|_, (_, last_seen)| now.duration_since(*last_seen) <= GROUP_WINDOW);
        match self.by_key.get_mut(&key) {
            Some((gid, last_seen)) => {
                *last_seen = now;
                gid.clone()
            }
            None => {
                let gid = new_id("grp-");
                self.by_key.insert(key, (gid.clone(), now));
                gid
            }
        }
    }
}

pub struct ActivityCollector {
    obs_tx: broadcast::Sender<ObsEvent>,
    dedup: Mutex<DedupState>,
    groups: Mutex<GroupState>,
}

impl ActivityCollector {
    pub fn new(obs_tx: broadcast::Sender<ObsEvent>) -> Self {
        Self {
            obs_tx,
            dedup: Mutex::new(DedupState::default()),
            groups: Mutex::new(GroupState::default()),
        }
    }

    /// Record an activity event. Emits exactly zero or one `ObsEvent::Activity`
    /// on the broadcast channel.
    pub fn record(&self, input: ActivityInput) {
        let file_name = file_name_of(&input.file_path);
        if is_transient_file(&file_name) {
            return;
        }

        let kind_tag = kind_discriminant(&input.kind);
        let now = Instant::now();

        if let Some(item_id) = &input.item_id
            && is_dedupable(&input.kind)
        {
            let key = DedupKey {
                drive_id: input.drive_id.clone(),
                item_id: item_id.clone(),
                kind_tag,
            };
            let mut dedup = self.dedup.lock().expect("dedup poisoned");
            dedup.purge_expired(now);
            if dedup.contains(&key) {
                return;
            }
            dedup.insert(key, now);
        }

        let group_key = GroupKey {
            drive_id: input.drive_id.clone(),
            parent_path: parent_path_of(&input.file_path),
            source: input.source,
            kind_tag,
        };
        let group_id = {
            let mut groups = self.groups.lock().expect("groups poisoned");
            groups.assign(group_key, now)
        };

        let entry = ActivityEntry {
            id: new_id("act-"),
            drive_id: input.drive_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            file_path: input.file_path,
            file_name,
            is_folder: input.is_folder,
            source: input.source,
            kind: input.kind,
            size_bytes: input.size_bytes,
            group_id: Some(group_id),
        };

        let _ = self.obs_tx.send(ObsEvent::Activity(entry));
    }
}

fn kind_discriminant(kind: &ActivityKind) -> &'static str {
    match kind {
        ActivityKind::Created => "created",
        ActivityKind::Modified => "modified",
        ActivityKind::Deleted => "deleted",
        ActivityKind::Renamed { .. } => "renamed",
        ActivityKind::Moved { .. } => "moved",
        ActivityKind::Conflict { .. } => "conflict",
        ActivityKind::Pinned => "pinned",
        ActivityKind::Unpinned => "unpinned",
    }
}

fn is_dedupable(kind: &ActivityKind) -> bool {
    matches!(
        kind,
        ActivityKind::Created | ActivityKind::Modified | ActivityKind::Deleted
    )
}

fn file_name_of(file_path: &str) -> String {
    file_path
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(file_path)
        .to_string()
}

fn parent_path_of(file_path: &str) -> String {
    match file_path.rsplit_once('/') {
        Some(("", _)) => "/".to_string(),
        Some((parent, _)) => parent.to_string(),
        None => "/".to_string(),
    }
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_collector() -> (ActivityCollector, broadcast::Receiver<ObsEvent>) {
        let (tx, rx) = broadcast::channel(32);
        (ActivityCollector::new(tx), rx)
    }

    fn drain(rx: &mut broadcast::Receiver<ObsEvent>) -> Vec<ActivityEntry> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let ObsEvent::Activity(e) = ev {
                out.push(e);
            }
        }
        out
    }

    #[test]
    fn transient_files_are_skipped() {
        let (col, mut rx) = new_collector();
        col.record(ActivityInput {
            drive_id: "d".into(),
            source: ActivitySource::Local,
            kind: ActivityKind::Created,
            file_path: "/Reports/~$Book1.xlsx".into(),
            item_id: Some("x".into()),
            is_folder: false,
            size_bytes: None,
        });
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn dedup_blocks_same_item_created_across_sources() {
        let (col, mut rx) = new_collector();
        col.record(ActivityInput {
            drive_id: "d".into(),
            source: ActivitySource::Local,
            kind: ActivityKind::Created,
            file_path: "/Reports/a.xlsx".into(),
            item_id: Some("ABC".into()),
            is_folder: false,
            size_bytes: Some(100),
        });
        col.record(ActivityInput {
            drive_id: "d".into(),
            source: ActivitySource::Remote,
            kind: ActivityKind::Created,
            file_path: "/Reports/a.xlsx".into(),
            item_id: Some("ABC".into()),
            is_folder: false,
            size_bytes: Some(100),
        });
        let entries = drain(&mut rx);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source, ActivitySource::Local);
    }

    #[test]
    fn dedup_does_not_block_different_kind() {
        let (col, mut rx) = new_collector();
        col.record(ActivityInput {
            drive_id: "d".into(),
            source: ActivitySource::Local,
            kind: ActivityKind::Created,
            file_path: "/a.xlsx".into(),
            item_id: Some("ABC".into()),
            is_folder: false,
            size_bytes: None,
        });
        col.record(ActivityInput {
            drive_id: "d".into(),
            source: ActivitySource::Remote,
            kind: ActivityKind::Modified,
            file_path: "/a.xlsx".into(),
            item_id: Some("ABC".into()),
            is_folder: false,
            size_bytes: None,
        });
        assert_eq!(drain(&mut rx).len(), 2);
    }

    #[test]
    fn dedup_is_skipped_for_conflict_rename() {
        let (col, mut rx) = new_collector();
        let payload = ActivityInput {
            drive_id: "d".into(),
            source: ActivitySource::System,
            kind: ActivityKind::Conflict {
                conflict_name: "a.conflict.xlsx".into(),
            },
            file_path: "/a.xlsx".into(),
            item_id: Some("ABC".into()),
            is_folder: false,
            size_bytes: None,
        };
        col.record(payload.clone());
        col.record(payload);
        assert_eq!(drain(&mut rx).len(), 2);
    }

    #[test]
    fn burst_in_same_parent_shares_group_id() {
        let (col, mut rx) = new_collector();
        for i in 0..5 {
            col.record(ActivityInput {
                drive_id: "d".into(),
                source: ActivitySource::Local,
                kind: ActivityKind::Created,
                file_path: format!("/Reports/f{i}.xlsx"),
                item_id: Some(format!("ITEM-{i}")),
                is_folder: false,
                size_bytes: None,
            });
        }
        let entries = drain(&mut rx);
        assert_eq!(entries.len(), 5);
        let gid = entries[0].group_id.clone().expect("group_id set");
        for e in &entries {
            assert_eq!(e.group_id.as_deref(), Some(gid.as_str()));
        }
    }

    #[test]
    fn different_parents_do_not_share_group_id() {
        let (col, mut rx) = new_collector();
        col.record(ActivityInput {
            drive_id: "d".into(),
            source: ActivitySource::Local,
            kind: ActivityKind::Created,
            file_path: "/A/x.xlsx".into(),
            item_id: Some("1".into()),
            is_folder: false,
            size_bytes: None,
        });
        col.record(ActivityInput {
            drive_id: "d".into(),
            source: ActivitySource::Local,
            kind: ActivityKind::Created,
            file_path: "/B/y.xlsx".into(),
            item_id: Some("2".into()),
            is_folder: false,
            size_bytes: None,
        });
        let entries = drain(&mut rx);
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].group_id, entries[1].group_id);
    }

    #[test]
    fn file_name_and_parent_derivation() {
        assert_eq!(file_name_of("/a/b/c.txt"), "c.txt");
        assert_eq!(file_name_of("/c.txt"), "c.txt");
        assert_eq!(file_name_of("c.txt"), "c.txt");
        assert_eq!(parent_path_of("/a/b/c.txt"), "/a/b");
        assert_eq!(parent_path_of("/c.txt"), "/");
        assert_eq!(parent_path_of("c.txt"), "/");
    }
}
