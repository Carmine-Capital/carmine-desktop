// Settings store — shell-level account/version plus the fields bound to the
// General panel.  `info` holds the raw snapshot for panels that need extra
// keys (root_dir, log_level, …) without re-fetching.

import { createStore, produce } from 'solid-js/store';

import { api, type SaveSettingsArgs } from '../ipc';
import type { SettingsInfo } from '../bindings';

export interface SettingsState {
  loaded: boolean;
  accountDisplay: string | null;
  appVersion: string;
  autoStart: boolean;
  notifications: boolean;
  syncIntervalSecs: number;
  cacheDir: string;
  cacheMaxSize: string;
  offlineTtlSecs: number;
  offlineMaxFolderSize: string;
  logLevel: string;
  explorerNavPane: boolean;
  info: SettingsInfo | null;
}

const [state, setState] = createStore<SettingsState>({
  loaded: false,
  accountDisplay: null,
  appVersion: '',
  autoStart: false,
  notifications: true,
  syncIntervalSecs: 300,
  cacheDir: '',
  cacheMaxSize: '5Go',
  offlineTtlSecs: 7 * 24 * 3600,
  offlineMaxFolderSize: '10Go',
  logLevel: 'info',
  explorerNavPane: true,
  info: null,
});

export const settings = state;

function ingest(info: SettingsInfo): void {
  setState(
    produce((s) => {
      s.loaded = true;
      s.info = info;
      s.accountDisplay = info.account_display ?? null;
      s.appVersion = info.app_version;
      s.autoStart = info.auto_start;
      s.notifications = info.notifications;
      s.syncIntervalSecs = info.sync_interval_secs;
      s.cacheDir = info.cache_dir ?? '';
      s.cacheMaxSize = info.cache_max_size;
      s.offlineTtlSecs = info.offline_ttl_secs;
      s.offlineMaxFolderSize = info.offline_max_folder_size;
      s.logLevel = info.log_level;
      s.explorerNavPane = info.explorer_nav_pane;
    }),
  );
}

export async function bootstrapSettings(): Promise<void> {
  ingest(await api.getSettings());
}

// Local mutators used by the General panel — they update the signal
// synchronously so the control feels responsive, then persist via save_settings.
// Callers await saveSettings() when they want to surface failures.
export function setAutoStart(value: boolean): void {
  if (state.autoStart === value) return;
  setState('autoStart', value);
}

export function setNotifications(value: boolean): void {
  if (state.notifications === value) return;
  setState('notifications', value);
}

export function setSyncIntervalSecs(value: number): void {
  if (state.syncIntervalSecs === value) return;
  setState('syncIntervalSecs', value);
}

export function setCacheDir(value: string): void {
  if (state.cacheDir === value) return;
  setState('cacheDir', value);
}

export function setCacheMaxSize(value: string): void {
  if (state.cacheMaxSize === value) return;
  setState('cacheMaxSize', value);
}

export function setOfflineTtlSecs(value: number): void {
  if (state.offlineTtlSecs === value) return;
  setState('offlineTtlSecs', value);
}

export function setOfflineMaxFolderSize(value: string): void {
  if (state.offlineMaxFolderSize === value) return;
  setState('offlineMaxFolderSize', value);
}

export function setLogLevel(value: string): void {
  if (state.logLevel === value) return;
  setState('logLevel', value);
}

export function setExplorerNavPane(value: boolean): void {
  if (state.explorerNavPane === value) return;
  setState('explorerNavPane', value);
}

// Full-snapshot save — mirrors the vanilla behaviour.  `save_settings` merges
// partial updates but we pass the whole General slice so the user config file
// stays coherent even after several concurrent edits.
export async function saveSettings(): Promise<void> {
  const args: SaveSettingsArgs = {
    autoStart: state.autoStart,
    notifications: state.notifications,
    syncIntervalSecs: state.syncIntervalSecs,
    cacheDir: state.cacheDir ? state.cacheDir : null,
    cacheMaxSize: state.cacheMaxSize,
    offlineTtlSecs: state.offlineTtlSecs,
    offlineMaxFolderSize: state.offlineMaxFolderSize,
    logLevel: state.logLevel,
    explorerNavPane: state.explorerNavPane,
  };
  await api.saveSettings(args);
}
