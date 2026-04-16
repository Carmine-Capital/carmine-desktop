// Typed wrappers around Tauri's invoke() so the rest of the frontend never
// touches window.__TAURI__ directly. One entry per #[tauri::command] we call.

import type {
  ActivityEntry,
  CacheStatsResponse,
  DashboardError,
  DashboardStatus,
  DriveInfo,
  MountInfo,
  OfflinePinInfo,
  PrimarySiteInfo,
  SettingsInfo,
} from './bindings';

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

function tauri(): { invoke: InvokeFn } {
  const w = window as unknown as { __TAURI__?: { core?: { invoke?: InvokeFn } } };
  const fn = w.__TAURI__?.core?.invoke;
  if (!fn) throw new Error('Tauri bridge non disponible (window.__TAURI__.core.invoke manquant)');
  return { invoke: fn };
}

export function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  return tauri().invoke<T>(cmd, args);
}

export interface AddMountArgs {
  mountType: 'drive' | 'sharepoint';
  mountPoint: string;
  driveId: string;
  siteId?: string | null;
  siteName?: string | null;
  libraryName?: string | null;
}

// Mirrors save_settings's camelCased `Option<T>` arguments — every field is
// optional so the command merges partial updates without nuking unrelated keys.
export interface SaveSettingsArgs {
  autoStart?: boolean;
  cacheMaxSize?: string;
  syncIntervalSecs?: number;
  metadataTtlSecs?: number;
  cacheDir?: string | null;
  logLevel?: string;
  notifications?: boolean;
  rootDir?: string;
  explorerNavPane?: boolean;
  offlineTtlSecs?: number;
  offlineMaxFolderSize?: string;
}

export const api = {
  getSettings: () => invoke<SettingsInfo>('get_settings'),
  saveSettings: (args: SaveSettingsArgs) =>
    invoke<void>('save_settings', args as Record<string, unknown>),
  getDashboardStatus: () => invoke<DashboardStatus>('get_dashboard_status'),
  getCacheStats: () => invoke<CacheStatsResponse>('get_cache_stats'),
  getRecentErrors: () => invoke<DashboardError[]>('get_recent_errors'),
  getActivityFeed: () => invoke<ActivityEntry[]>('get_activity_feed'),
  listOfflinePins: () => invoke<OfflinePinInfo[]>('list_offline_pins'),
  removeOfflinePin: (driveId: string, itemId: string) =>
    invoke<void>('remove_offline_pin', { driveId, itemId }),
  extendOfflinePin: (driveId: string, itemId: string, ttlSecs: number) =>
    invoke<void>('extend_offline_pin', { driveId, itemId, ttlSecs }),
  signOut: () => invoke<void>('sign_out'),
  clearCache: () => invoke<void>('clear_cache'),
  listMounts: () => invoke<MountInfo[]>('list_mounts'),
  addMount: (args: AddMountArgs) =>
    invoke<string>('add_mount', {
      mountType: args.mountType,
      mountPoint: args.mountPoint,
      driveId: args.driveId,
      siteId: args.siteId ?? null,
      siteName: args.siteName ?? null,
      libraryName: args.libraryName ?? null,
    }),
  removeMount: (id: string) => invoke<boolean>('remove_mount', { id }),
  getDefaultMountRoot: () => invoke<string>('get_default_mount_root'),
  getDriveInfo: () => invoke<DriveInfo>('get_drive_info'),
  getPrimarySiteInfo: () => invoke<PrimarySiteInfo>('get_primary_site_info'),
  listPrimarySiteLibraries: () => invoke<DriveInfo[]>('list_primary_site_libraries'),
  startSignIn: () => invoke<string>('start_sign_in'),
  cancelSignIn: () => invoke<void>('cancel_sign_in'),
  completeWizard: () => invoke<void>('complete_wizard'),
};
