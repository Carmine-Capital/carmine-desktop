use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Observer for delta sync content change notifications.
///
/// Implemented by the VFS layer to react when delta sync detects that a file's
/// content has changed on the server (eTag mismatch). This enables the VFS to
/// mark open file handles as stale and optionally invalidate the kernel page cache.
///
/// The trait lives in `carminedesktop-core` (shared dependency) to avoid a circular
/// dependency between `carminedesktop-cache` (where delta sync runs) and `carminedesktop-vfs`
/// (where the open file table lives).
pub trait DeltaSyncObserver: Send + Sync {
    /// Called when delta sync detects that the content of the given inode has changed.
    fn on_inode_content_changed(&self, ino: u64);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveItem {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(rename = "lastModifiedDateTime")]
    pub last_modified: Option<DateTime<Utc>>,
    #[serde(rename = "createdDateTime")]
    pub created: Option<DateTime<Utc>>,
    #[serde(rename = "eTag")]
    pub etag: Option<String>,
    #[serde(rename = "parentReference")]
    pub parent_reference: Option<ParentReference>,
    pub folder: Option<FolderFacet>,
    pub file: Option<FileFacet>,
    pub publication: Option<PublicationFacet>,
    #[serde(rename = "@microsoft.graph.downloadUrl")]
    pub download_url: Option<String>,
    #[serde(rename = "webUrl")]
    pub web_url: Option<String>,
}

impl DriveItem {
    pub fn is_folder(&self) -> bool {
        self.folder.is_some()
    }

    /// Returns `true` if the file is locked (checked out or co-authoring lock).
    pub fn is_locked(&self) -> bool {
        self.publication
            .as_ref()
            .is_some_and(|p| p.level.as_deref() == Some("checkout"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParentReference {
    #[serde(rename = "driveId")]
    pub drive_id: Option<String>,
    pub id: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderFacet {
    #[serde(rename = "childCount", default)]
    pub child_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFacet {
    #[serde(rename = "mimeType")]
    pub mime_type: Option<String>,
    pub hashes: Option<FileHashes>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHashes {
    #[serde(rename = "sha256Hash")]
    pub sha256: Option<String>,
    #[serde(rename = "quickXorHash")]
    pub quick_xor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicationFacet {
    pub level: Option<String>,
    #[serde(rename = "versionId")]
    pub version_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    pub id: String,
    pub name: String,
    #[serde(rename = "driveType")]
    pub drive_type: Option<String>,
    pub owner: Option<serde_json::Value>,
    pub quota: Option<DriveQuota>,
    #[serde(rename = "webUrl")]
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveQuota {
    pub total: Option<i64>,
    pub used: Option<i64>,
    pub remaining: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    #[serde(rename = "displayName")]
    pub display_name: Option<String>,
    #[serde(rename = "webUrl")]
    pub web_url: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaResponse {
    pub value: Vec<DriveItem>,
    #[serde(rename = "@odata.deltaLink")]
    pub delta_link: Option<String>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadSession {
    #[serde(rename = "uploadUrl")]
    pub upload_url: String,
    #[serde(rename = "expirationDateTime")]
    pub expiration: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphErrorResponse {
    pub error: GraphErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphCollection<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyMonitorResponse {
    pub status: String,
    #[serde(rename = "percentageComplete")]
    pub percentage_complete: Option<f64>,
    #[serde(rename = "resourceId")]
    pub resource_id: Option<String>,
    pub error: Option<GraphErrorBody>,
}

// ---------------------------------------------------------------------------
// Observability types — ObsEvent enum and Tauri command response structs
// ---------------------------------------------------------------------------

/// Real-time observability event, carried on a `tokio::sync::broadcast` and
/// fanned out by the event bridge (app/src/observability.rs) to typed,
/// per-topic Tauri emits such as `error:append`, `activity:append`,
/// `drive:status`, `drive:online` and `auth:state`.
///
/// The `#[serde(tag = "type")]` attribute produces a JSON discriminator field
/// named `"type"` with camelCase variant names (e.g. `"syncStateChanged"`).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ObsEvent {
    /// A persistent error that should appear in the dashboard error log.
    Error {
        #[serde(rename = "driveId")]
        drive_id: Option<String>,
        #[serde(rename = "fileName")]
        file_name: Option<String>,
        #[serde(rename = "remotePath")]
        remote_path: Option<String>,
        #[serde(rename = "errorType")]
        error_type: String,
        message: String,
        #[serde(rename = "actionHint")]
        action_hint: Option<String>,
        timestamp: String,
    },
    /// A file-level activity entry for the activity feed.
    ///
    /// Carries the full `ActivityEntry` struct. With `#[serde(tag = "type")]`
    /// internal tagging, the struct fields are flattened alongside the
    /// discriminator — so the emitted JSON is
    /// `{"type":"activity","id":"...","driveId":"...","kind":{"op":"...","...":"..."},...}`.
    Activity(ActivityEntry),
    /// Sync state transition for a drive.
    SyncStateChanged {
        #[serde(rename = "driveId")]
        drive_id: String,
        /// One of: "syncing", "up_to_date", "error".
        state: String,
    },
    /// Online/offline state change for a drive.
    OnlineStateChanged {
        #[serde(rename = "driveId")]
        drive_id: String,
        online: bool,
    },
    /// Auth degradation state change (global, not per-drive).
    AuthStateChanged { degraded: bool },
}

/// Response for `get_dashboard_status` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStatus {
    pub drives: Vec<DriveStatus>,
    pub authenticated: bool,
    pub auth_degraded: bool,
}

/// Per-drive status within a `DashboardStatus` response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveStatus {
    pub drive_id: String,
    pub name: String,
    pub mount_point: String,
    pub online: bool,
    pub last_synced: Option<String>,
    /// One of: "up_to_date", "syncing", "error".
    pub sync_state: String,
    pub upload_queue: UploadQueueInfo,
}

/// Upload queue snapshot within a `DriveStatus`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadQueueInfo {
    pub queue_depth: usize,
    pub in_flight: usize,
    pub failed_count: usize,
    pub total_uploaded: u64,
    pub total_failed: u64,
}

/// A single error entry for `get_recent_errors` response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardError {
    pub drive_id: Option<String>,
    pub file_name: Option<String>,
    pub remote_path: Option<String>,
    pub error_type: String,
    pub message: String,
    pub action_hint: Option<String>,
    pub timestamp: String,
}

/// Source of an activity event.
///
/// - `Local` — user action on the mounted VFS (WinFSP callback reached
///   `core_ops` or `sync_processor` completed a writeback).
/// - `Remote` — a change pulled from OneDrive via delta sync.
/// - `System` — produced by the application itself (pin/unpin, conflict
///   resolution, cache cleanup).
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivitySource {
    Local,
    Remote,
    System,
}

/// Operation carried by an activity event.
///
/// Serialized as an internally-tagged object with discriminator field `op`:
/// `{"op":"created"}`, `{"op":"renamed","from":"old.xlsx"}`,
/// `{"op":"conflict","conflictName":"doc.conflict.xlsx"}`.
///
/// Unit variants serialize as a single-field object (not a bare string) so the
/// discriminator is always present and the payload is uniform on the TS side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ActivityKind {
    Created,
    Modified,
    Deleted,
    Renamed {
        from: String,
    },
    Moved {
        from: String,
    },
    Conflict {
        #[serde(rename = "conflictName")]
        conflict_name: String,
    },
    Pinned,
    Unpinned,
}

/// A single activity entry for `get_activity_feed` response and the
/// `activity:append` Tauri event.
///
/// The `id` is a ULID-like string generated by `ActivityCollector` — it is
/// opaque to consumers and serves as a stable React key plus pagination
/// cursor if we later persist the feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: String,
    pub drive_id: String,
    pub timestamp: String,
    pub file_path: String,
    pub file_name: String,
    pub is_folder: bool,
    pub source: ActivitySource,
    pub kind: ActivityKind,
    pub size_bytes: Option<u64>,
    pub group_id: Option<String>,
}

/// Response for `get_cache_stats` Tauri command.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStatsResponse {
    pub disk_used_bytes: u64,
    pub disk_max_bytes: u64,
    pub memory_entry_count: usize,
    pub pinned_items: Vec<PinHealthInfo>,
}

/// Health information for a single pinned folder.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinHealthInfo {
    pub drive_id: String,
    pub item_id: String,
    pub folder_name: String,
    /// One of: "downloaded", "partial", "stale".
    pub status: String,
    pub total_files: usize,
    pub cached_files: usize,
    pub pinned_at: String,
    pub expires_at: String,
}

/// Internal stats returned by `CacheManager::stats()`.
///
/// Not serialized directly to JSON — the Tauri command maps this into
/// `CacheStatsResponse` with additional pin health and writeback data.
#[derive(Debug, Clone)]
pub struct CacheManagerStats {
    pub memory_entry_count: usize,
    pub disk_used_bytes: u64,
    pub disk_max_bytes: u64,
    pub dirty_inode_count: usize,
}

/// Single-pin push payload emitted on the `pin:health` Tauri event.
///
/// Same shape as `PinHealthInfo` plus `mount_name` so the frontend can render a
/// full PinCard without a second invoke.  Emitted by the debounced pin
/// aggregator whenever the (totalFiles, cachedFiles, status) tuple changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinHealthEvent {
    pub drive_id: String,
    pub item_id: String,
    pub folder_name: String,
    pub mount_name: String,
    pub status: String,
    pub total_files: usize,
    pub cached_files: usize,
    pub pinned_at: String,
    pub expires_at: String,
}

/// Emitted on the `pin:removed` Tauri event when a pin disappears from the
/// `pinned_folders` table (explicit unpin or TTL expiry).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinRemovedEvent {
    pub drive_id: String,
    pub item_id: String,
}

/// Payload for the `drive:status` Tauri event (sync state transition).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveStatusEvent {
    pub drive_id: String,
    /// One of: "syncing", "up_to_date", "error".
    pub state: String,
}

/// Payload for the `drive:online` Tauri event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveOnlineEvent {
    pub drive_id: String,
    pub online: bool,
}

/// Payload for the `auth:state` Tauri event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStateEvent {
    pub degraded: bool,
}

/// Payload for the `drive:upload-progress` Tauri event: a live snapshot of
/// the sync processor's upload queue metrics for a single drive, emitted by
/// a debounced watcher on top of the existing `watch::Receiver<SyncMetrics>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveUploadProgressEvent {
    pub drive_id: String,
    pub queue_depth: usize,
    pub in_flight: usize,
    pub failed_count: usize,
    pub total_uploaded: u64,
    pub total_failed: u64,
    pub total_deduplicated: u64,
}
