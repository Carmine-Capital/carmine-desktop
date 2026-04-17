// Types mirroring Rust structs in carminedesktop-core::types and
// carminedesktop-app::commands. All fields camelCase per serde rename_all.
// Kept hand-written for phase 1; tauri-specta will generate these later.

export interface DashboardStatus {
  drives: DriveStatus[];
  authenticated: boolean;
  authDegraded: boolean;
}

export interface DriveStatus {
  driveId: string;
  name: string;
  mountPoint: string;
  online: boolean;
  lastSynced: string | null;
  syncState: 'up_to_date' | 'syncing' | 'error';
  uploadQueue: UploadQueueInfo;
}

export interface UploadQueueInfo {
  queueDepth: number;
  inFlight: number;
  failedCount: number;
  totalUploaded: number;
  totalFailed: number;
}

export interface DashboardError {
  driveId: string | null;
  fileName: string | null;
  remotePath: string | null;
  errorType: string;
  message: string;
  actionHint: string | null;
  timestamp: string;
}

export type ActivitySource = 'local' | 'remote' | 'system';

export type ActivityKind =
  | { op: 'created' }
  | { op: 'modified' }
  | { op: 'deleted' }
  | { op: 'renamed'; from: string }
  | { op: 'moved'; from: string }
  | { op: 'conflict'; conflictName: string }
  | { op: 'pinned' }
  | { op: 'unpinned' };

export interface ActivityEntry {
  id: string;
  driveId: string;
  timestamp: string;
  filePath: string;
  fileName: string;
  isFolder: boolean;
  source: ActivitySource;
  kind: ActivityKind;
  sizeBytes: number | null;
  groupId: string | null;
}

export interface CacheStatsResponse {
  diskUsedBytes: number;
  diskMaxBytes: number;
  memoryEntryCount: number;
  pinnedItems: PinHealthInfo[];
}

export type PinStatus = 'downloaded' | 'partial' | 'stale' | 'analyzing' | 'expired' | 'unknown';

export interface PinHealthInfo {
  driveId: string;
  itemId: string;
  folderName: string;
  status: 'downloaded' | 'partial' | 'stale';
  totalFiles: number;
  cachedFiles: number;
  pinnedAt: string;
  expiresAt: string;
}

export interface OfflinePinInfo {
  drive_id: string;
  item_id: string;
  folder_name: string;
  mount_name: string;
  pinned_at: string;
  expires_at: string;
}

export interface MountInfo {
  id: string;
  name: string;
  mount_type: string;
  mount_point: string;
  enabled: boolean;
  drive_id: string | null;
}

export interface DriveInfo {
  id: string;
  name: string;
}

export interface PrimarySiteInfo {
  site_id: string;
  site_name: string;
}

export interface SettingsInfo {
  app_name: string;
  app_version: string;
  auto_start: boolean;
  cache_max_size: string;
  sync_interval_secs: number;
  metadata_ttl_secs: number;
  cache_dir: string | null;
  log_level: string;
  notifications: boolean;
  root_dir: string;
  account_display: string | null;
  explorer_nav_pane: boolean;
  offline_ttl_secs: number;
  offline_max_folder_size: string;
  platform: string;
}

// Granular realtime events emitted by Rust: pin:*, drive:*, activity:append,
// error:append and auth:state.  The legacy multiplexed `obs-event` topic was
// dropped in phase 7 and no longer has a TypeScript counterpart.
export interface PinHealthEvent {
  driveId: string;
  itemId: string;
  folderName: string;
  mountName: string;
  status: 'downloaded' | 'partial' | 'stale';
  totalFiles: number;
  cachedFiles: number;
  pinnedAt: string;
  expiresAt: string;
}

export interface PinRemovedEvent {
  driveId: string;
  itemId: string;
}

export type DriveSyncState = 'syncing' | 'up_to_date' | 'error';

export interface DriveStatusEvent {
  driveId: string;
  state: DriveSyncState;
}

export interface DriveOnlineEvent {
  driveId: string;
  online: boolean;
}

export interface AuthStateEvent {
  degraded: boolean;
}

export interface DriveUploadProgressEvent {
  driveId: string;
  queueDepth: number;
  inFlight: number;
  failedCount: number;
  totalUploaded: number;
  totalFailed: number;
  totalDeduplicated: number;
}

export type PanelId = 'dashboard' | 'general' | 'mounts' | 'offline' | 'about';
