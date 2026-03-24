use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use carminedesktop_cache::sync::run_delta_sync;
use carminedesktop_core::config::{
    AccountMetadata, EffectiveConfig, autostart, config_file_path, expand_mount_point,
};
use carminedesktop_core::types::{
    ActivityEntry, CacheStatsResponse, DashboardError, DashboardStatus, DriveItem, DriveStatus,
    PinHealthInfo, UploadQueueInfo, WritebackEntry,
};

use std::collections::HashMap;

use crate::AppState;

#[derive(Serialize)]
pub struct MountInfo {
    pub id: String,
    pub name: String,
    pub mount_type: String,
    pub mount_point: String,
    pub enabled: bool,
    pub drive_id: Option<String>,
}

#[derive(Serialize)]
pub struct SettingsInfo {
    pub app_name: String,
    pub app_version: String,
    pub auto_start: bool,
    pub cache_max_size: String,
    pub sync_interval_secs: u64,
    pub metadata_ttl_secs: u64,
    pub cache_dir: Option<String>,
    pub log_level: String,
    pub notifications: bool,
    pub root_dir: String,
    pub account_display: Option<String>,
    pub explorer_nav_pane: bool,
    pub offline_ttl_secs: u64,
    pub offline_max_folder_size: String,
    pub platform: String,
}

#[derive(Serialize)]
pub struct SiteInfo {
    pub id: String,
    pub display_name: String,
    pub web_url: String,
}

#[derive(Serialize)]
pub struct DriveInfo {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct OfflinePinInfo {
    pub drive_id: String,
    pub item_id: String,
    pub folder_name: String,
    pub mount_name: String,
    pub pinned_at: String,
    pub expires_at: String,
}

#[tauri::command]
pub fn is_authenticated(app: AppHandle) -> bool {
    app.state::<AppState>()
        .authenticated
        .load(Ordering::Relaxed)
}

#[tauri::command]
pub async fn sign_in(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.auth.sign_in(None).await.map_err(|e| e.to_string())?;
    tracing::info!("sign-in successful");
    complete_sign_in(&app).await
}

/// Called by the wizard: returns the auth URL immediately, then emits `auth-complete`
/// or `auth-error` when the PKCE flow finishes in the background.
#[tauri::command]
pub async fn start_sign_in(app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();

    {
        let mut guard = state.active_sign_in.lock().unwrap();
        if let Some(handle) = guard.take() {
            handle.abort();
        }
    }

    let (url_tx, url_rx) = tokio::sync::oneshot::channel::<String>();

    let auth = state.auth.clone();
    let app_handle = app.clone();
    let handle = tokio::spawn(async move {
        match auth.sign_in(Some(url_tx)).await {
            Ok(()) => {
                tracing::info!("sign-in successful");
                match complete_sign_in(&app_handle).await {
                    Ok(()) => {
                        let _ = app_handle.emit("auth-complete", ());
                    }
                    Err(e) => {
                        let _ = app_handle.emit("auth-error", e);
                    }
                }
            }
            Err(e) => {
                let _ = app_handle.emit("auth-error", e.to_string());
            }
        }
    });

    *state.active_sign_in.lock().unwrap() = Some(handle);

    url_rx
        .await
        .map_err(|_| "auth URL channel closed unexpectedly".to_string())
}

#[tauri::command]
pub async fn cancel_sign_in(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.auth.cancel();
    let mut guard = state.active_sign_in.lock().unwrap();
    if let Some(handle) = guard.take() {
        handle.abort();
    }
    Ok(())
}

async fn complete_sign_in(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    let drive = state
        .graph
        .get_my_drive()
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!("discovered OneDrive: {} ({})", drive.name, drive.id);

    // Finalize sign-in: sets account_id and migrates tokens from client_id key to account_id key
    state
        .auth
        .finalize_sign_in(&drive.id)
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;

        if user_config.accounts.is_empty() {
            user_config.accounts.push(AccountMetadata {
                id: drive.id.clone(),
                email: None,
                display_name: Some(drive.name.clone()),
                tenant_id: None,
            });
        }

        let cfg_path = config_file_path().map_err(|e| e.to_string())?;
        user_config
            .save_to_file(&cfg_path)
            .map_err(|e| e.to_string())?;
    }

    *state.account_id.lock().map_err(|e| e.to_string())? = Some(drive.id.clone());
    rebuild_effective_config(app)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    if !crate::fuse_available() {
        crate::notify::fuse_unavailable(app);
    }
    crate::start_all_mounts(app);
    crate::run_crash_recovery(app);
    crate::start_delta_sync(app);

    state.authenticated.store(true, Ordering::Relaxed);
    state.auth_degraded.store(false, Ordering::Relaxed);
    crate::tray::update_tray_menu(app);

    Ok(())
}

#[tauri::command]
pub async fn sign_out(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut errors: Vec<String> = Vec::new();

    // Phase 1: best-effort cleanup
    crate::stop_all_mounts(&app);

    if let Some(cancel) = state.sync_cancel.lock().unwrap().take() {
        cancel.cancel();
    }

    if let Err(e) = crate::shell_integration::unregister_context_menu() {
        tracing::warn!("offline context menu unregistration failed: {e}");
        errors.push(format!("context menu cleanup: {e}"));
    }

    if let Err(e) = state.auth.sign_out().await {
        tracing::error!("auth sign_out failed: {e}");
        errors.push(e.to_string());
    }

    *state.account_id.lock().unwrap() = None;

    match state.user_config.lock() {
        Ok(mut user_config) => {
            user_config.accounts.clear();
            match config_file_path() {
                Ok(cfg_path) => {
                    if let Err(e) = user_config.save_to_file(&cfg_path) {
                        tracing::error!("failed to save config after sign-out: {e}");
                        errors.push(e.to_string());
                    }
                }
                Err(e) => {
                    tracing::error!("config path unavailable: {e}");
                    errors.push(e.to_string());
                }
            }
        }
        Err(e) => {
            tracing::error!("user_config lock poisoned during sign-out: {e}");
            errors.push(e.to_string());
        }
    }

    if let Err(e) = rebuild_effective_config(&app) {
        tracing::error!("rebuild_effective_config failed during sign-out: {e}");
        errors.push(e);
    }

    // Phase 2: guaranteed UI reset
    state.authenticated.store(false, Ordering::Relaxed);
    state.auth_degraded.store(false, Ordering::Relaxed);
    crate::tray::update_tray_menu(&app);

    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.reload();
    }

    if let Some(win) = app.get_webview_window("wizard") {
        let _ = win.reload();
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        crate::tray::open_or_focus_window(&app, "wizard", "Setup", "wizard.html");
    }

    if !errors.is_empty() {
        let msg = errors.join("; ");
        crate::notify::sign_out_failed(&app, &msg);
        return Err(msg);
    }

    Ok(())
}

#[tauri::command]
pub fn list_mounts(app: AppHandle) -> Result<Vec<MountInfo>, String> {
    let state = app.state::<AppState>();
    let config = state.effective_config.lock().map_err(|e| e.to_string())?;
    Ok(config
        .mounts
        .iter()
        .map(|m| MountInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            mount_type: m.mount_type.clone(),
            mount_point: expand_mount_point(&m.mount_point),
            enabled: m.enabled,
            drive_id: m.drive_id.clone(),
        })
        .collect())
}

#[tauri::command]
pub fn add_mount(
    app: AppHandle,
    mount_type: String,
    mount_point: String,
    drive_id: Option<String>,
    site_id: Option<String>,
    site_name: Option<String>,
    library_name: Option<String>,
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let mount_id;

    {
        let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;
        let account_id = user_config.accounts.first().map(|a| a.id.clone());

        match mount_type.as_str() {
            "sharepoint" => {
                let sid = site_id.ok_or("site_id required for SharePoint mount")?;
                let did = drive_id.ok_or("drive_id required for SharePoint mount")?;
                let sn = site_name.unwrap_or_default();
                let ln = library_name.unwrap_or_default();
                user_config
                    .add_sharepoint_mount(&sid, &did, &sn, &ln, &mount_point, account_id)
                    .map_err(|e| e.to_string())?;
            }
            _ => {
                let did = drive_id.ok_or("drive_id required for OneDrive mount")?;
                user_config
                    .add_onedrive_mount(&did, &mount_point, account_id)
                    .map_err(|e| e.to_string())?;
            }
        }

        mount_id = user_config
            .mounts
            .last()
            .map(|m| m.id.clone())
            .ok_or_else(|| "mount was not saved".to_string())?;

        let cfg_path = config_file_path().map_err(|e| e.to_string())?;
        user_config
            .save_to_file(&cfg_path)
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(&app)?;

    if state.authenticated.load(Ordering::Relaxed) {
        let mount_config_opt = {
            let config = state.effective_config.lock().map_err(|e| e.to_string())?;
            config.mounts.iter().find(|m| m.id == mount_id).cloned()
        };
        if let Some(mount_config) = mount_config_opt {
            let mountpoint = expand_mount_point(&mount_config.mount_point);
            match crate::start_mount(&app, &mount_config) {
                Ok(()) => {
                    crate::notify::mount_success(&app, &mount_config.name, &mountpoint);
                }
                Err(e) => {
                    tracing::error!("failed to start new mount: {e}");
                    crate::notify::mount_failed(&app, &mount_config.name, &e);
                }
            }
        }
    }

    crate::tray::update_tray_menu(&app);
    Ok(mount_id)
}

#[tauri::command]
pub fn remove_mount(app: AppHandle, id: String) -> Result<bool, String> {
    let state = app.state::<AppState>();

    let _ = crate::stop_mount(&app, &id);

    let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;
    let removed = user_config.remove_mount(&id);
    let cfg_path = config_file_path().map_err(|e| e.to_string())?;
    user_config
        .save_to_file(&cfg_path)
        .map_err(|e| e.to_string())?;
    drop(user_config);

    rebuild_effective_config(&app)?;
    crate::tray::update_tray_menu(&app);
    Ok(removed)
}

#[tauri::command]
pub fn toggle_mount(app: AppHandle, id: String) -> Result<Option<bool>, String> {
    let state = app.state::<AppState>();
    let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;
    let result = user_config.toggle_mount(&id);
    let cfg_path = config_file_path().map_err(|e| e.to_string())?;
    user_config
        .save_to_file(&cfg_path)
        .map_err(|e| e.to_string())?;
    drop(user_config);

    rebuild_effective_config(&app)?;

    if state.authenticated.load(Ordering::Relaxed)
        && let Some(enabled) = result
    {
        if enabled {
            let mount_config_opt = {
                let config = state.effective_config.lock().map_err(|e| e.to_string())?;
                config.mounts.iter().find(|m| m.id == id).cloned()
            };
            if let Some(mount_config) = mount_config_opt {
                let mountpoint = expand_mount_point(&mount_config.mount_point);
                match crate::start_mount(&app, &mount_config) {
                    Ok(()) => {
                        crate::notify::mount_success(&app, &mount_config.name, &mountpoint);
                    }
                    Err(e) => {
                        tracing::error!("failed to start mount '{}': {e}", mount_config.name);
                        crate::notify::mount_failed(&app, &mount_config.name, &e);
                    }
                }
            }
        } else {
            let _ = crate::stop_mount(&app, &id);
        }
    }

    crate::tray::update_tray_menu(&app);
    Ok(result)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<SettingsInfo, String> {
    let state = app.state::<AppState>();
    let config = state.effective_config.lock().map_err(|e| e.to_string())?;
    let account_display = config
        .accounts
        .first()
        .and_then(|a| a.email.clone().or_else(|| a.display_name.clone()));
    Ok(SettingsInfo {
        app_name: "Carmine Desktop".to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        auto_start: config.auto_start,
        cache_max_size: config.cache_max_size.clone(),
        sync_interval_secs: config.sync_interval_secs,
        metadata_ttl_secs: config.metadata_ttl_secs,
        cache_dir: config.cache_dir.clone(),
        log_level: config.log_level.clone(),
        notifications: config.notifications,
        root_dir: config.root_dir.clone(),
        account_display,
        explorer_nav_pane: config.explorer_nav_pane,
        offline_ttl_secs: config.offline_ttl_secs,
        offline_max_folder_size: config.offline_max_folder_size.clone(),
        platform: std::env::consts::OS.to_string(),
    })
}

#[tauri::command]
pub fn list_offline_pins(app: AppHandle) -> Result<Vec<OfflinePinInfo>, String> {
    let state = app.state::<AppState>();

    // Extract mount name mapping from config (separate lock scope).
    let mount_names: std::collections::HashMap<String, String> = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        config
            .mounts
            .iter()
            .filter_map(|m| m.drive_id.as_ref().map(|d| (d.clone(), m.name.clone())))
            .collect()
    };

    // Collect Arc refs under the lock, then drop it.
    let entries: Vec<(
        String,
        String,
        std::sync::Arc<carminedesktop_cache::CacheManager>,
    )> = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        caches
            .iter()
            .map(|(drive_id, (cache, _, _, _, _, _))| {
                let mount_name = mount_names
                    .get(drive_id)
                    .cloned()
                    .unwrap_or_else(|| drive_id.clone());
                (drive_id.clone(), mount_name, cache.clone())
            })
            .collect()
    };

    let mut pins = Vec::new();
    for (drive_id, mount_name, cache) in &entries {
        let all_pins = cache.pin_store.list_all().map_err(|e| e.to_string())?;

        for pin in all_pins {
            let folder_name = cache
                .sqlite
                .get_item_by_id(&pin.item_id)
                .ok()
                .flatten()
                .map(|(_, item)| item.name)
                .unwrap_or_else(|| pin.item_id.clone());

            pins.push(OfflinePinInfo {
                drive_id: drive_id.clone(),
                item_id: pin.item_id,
                folder_name,
                mount_name: mount_name.clone(),
                pinned_at: pin.pinned_at,
                expires_at: pin.expires_at,
            });
        }
    }

    Ok(pins)
}

#[tauri::command]
pub fn remove_offline_pin(app: AppHandle, drive_id: String, item_id: String) -> Result<(), String> {
    let state = app.state::<AppState>();

    // Clone Arc out of the lock, then drop it.
    let offline_mgr = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        let (_, _, _, mgr, _, _) = caches
            .get(&drive_id)
            .ok_or_else(|| format!("no mount found for drive {drive_id}"))?;
        mgr.clone()
    };

    offline_mgr
        .unpin_folder(&item_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_dashboard_status(app: AppHandle) -> Result<DashboardStatus, String> {
    let state = app.state::<AppState>();
    let authenticated = state.authenticated.load(Ordering::Relaxed);
    let auth_degraded = state.auth_degraded.load(Ordering::Relaxed);

    // Snapshot mount data -- lock, clone Arcs, release (snapshot-then-release to avoid contention)
    let mount_snapshot: Vec<(
        String,
        Arc<std::sync::atomic::AtomicBool>,
        Option<carminedesktop_vfs::SyncHandle>,
    )> = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        caches
            .iter()
            .map(|(drive_id, (_, _, _, _, offline_flag, sync_handle))| {
                (drive_id.clone(), offline_flag.clone(), sync_handle.clone())
            })
            .collect()
    };

    // Get mount configs for name/mount_point mapping
    let mount_configs: Vec<(String, String, Option<String>)> = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        config
            .mounts
            .iter()
            .map(|m| {
                (
                    expand_mount_point(&m.mount_point),
                    m.name.clone(),
                    m.drive_id.clone(),
                )
            })
            .collect()
    };

    // Get last_synced timestamps
    let last_synced_map = state.last_synced.lock().map_err(|e| e.to_string())?.clone();

    let mut drives = Vec::new();
    for (drive_id, offline_flag, sync_handle) in &mount_snapshot {
        let online = !offline_flag.load(Ordering::Relaxed);

        // Find mount config for this drive
        let (mount_name, mount_point) = mount_configs
            .iter()
            .find(|(_, _, did)| did.as_deref() == Some(drive_id.as_str()))
            .map(|(mp, name, _)| (name.clone(), mp.clone()))
            .unwrap_or_else(|| (drive_id.clone(), String::new()));

        // Get upload queue metrics from SyncHandle if available
        let upload_queue = if let Some(sh) = sync_handle {
            let m = sh.metrics();
            UploadQueueInfo {
                queue_depth: m.queue_depth,
                in_flight: m.in_flight,
                failed_count: m.failed_count,
                total_uploaded: m.total_uploaded,
                total_failed: m.total_failed,
            }
        } else {
            UploadQueueInfo {
                queue_depth: 0,
                in_flight: 0,
                failed_count: 0,
                total_uploaded: 0,
                total_failed: 0,
            }
        };

        // Determine sync state from metrics and online status
        let sync_state = if !online {
            "offline".to_string()
        } else if upload_queue.queue_depth > 0 || upload_queue.in_flight > 0 {
            "syncing".to_string()
        } else {
            "up_to_date".to_string()
        };

        let last_synced = last_synced_map.get(drive_id).cloned();

        drives.push(DriveStatus {
            drive_id: drive_id.clone(),
            name: mount_name,
            mount_point,
            online,
            last_synced,
            sync_state,
            upload_queue,
        });
    }

    Ok(DashboardStatus {
        drives,
        authenticated,
        auth_degraded,
    })
}

#[tauri::command]
pub async fn get_recent_errors(app: AppHandle) -> Result<Vec<DashboardError>, String> {
    let state = app.state::<AppState>();
    let errors = state.error_ring.lock().map_err(|e| e.to_string())?.drain();
    Ok(errors)
}

#[tauri::command]
pub async fn get_activity_feed(app: AppHandle) -> Result<Vec<ActivityEntry>, String> {
    let state = app.state::<AppState>();
    let entries = state
        .activity_ring
        .lock()
        .map_err(|e| e.to_string())?
        .drain();
    Ok(entries)
}

#[tauri::command]
pub async fn get_cache_stats(app: AppHandle) -> Result<CacheStatsResponse, String> {
    let state = app.state::<AppState>();

    // Snapshot caches -- lock, clone Arcs, release
    let cache_snapshot: Vec<(String, Arc<carminedesktop_cache::CacheManager>)> = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        caches
            .iter()
            .map(|(did, (c, _, _, _, _, _))| (did.clone(), c.clone()))
            .collect()
    };

    // Get stale pins set
    let stale_pins = state.stale_pins.lock().map_err(|e| e.to_string())?.clone();

    // Aggregate stats across all mounted drives
    let mut total_disk_used: u64 = 0;
    let mut total_disk_max: u64 = 0;
    let mut total_memory_entries: usize = 0;
    let mut all_pinned_items: Vec<PinHealthInfo> = Vec::new();
    let mut all_writeback: Vec<WritebackEntry> = Vec::new();

    for (_drive_id, cache) in &cache_snapshot {
        let stats = cache.stats();
        total_disk_used += stats.disk_used_bytes;
        total_disk_max += stats.disk_max_bytes;
        total_memory_entries += stats.memory_entry_count;

        // Pin health -- computed on-demand from SQLite
        if let Ok(health) = cache.pin_store.health(&stale_pins) {
            for (pin, total_files, cached_files) in health {
                let status = if stale_pins.contains(&(pin.drive_id.clone(), pin.item_id.clone())) {
                    "stale".to_string()
                } else if cached_files >= total_files {
                    "downloaded".to_string()
                } else {
                    "partial".to_string()
                };

                // Resolve folder name from SQLite items table
                let folder_name = cache
                    .sqlite
                    .get_item_by_id(&pin.item_id)
                    .ok()
                    .flatten()
                    .map(|(_, item)| item.name)
                    .unwrap_or_else(|| pin.item_id.clone());

                all_pinned_items.push(PinHealthInfo {
                    drive_id: pin.drive_id,
                    item_id: pin.item_id,
                    folder_name,
                    status,
                    total_files,
                    cached_files,
                    pinned_at: pin.pinned_at,
                    expires_at: pin.expires_at,
                });
            }
        }

        // Writeback queue -- list_pending() is async, called OUTSIDE lock scope
        if let Ok(pending) = cache.writeback.list_pending().await {
            for (pending_drive_id, item_id) in pending {
                let file_name = cache
                    .sqlite
                    .get_item_by_id(&item_id)
                    .ok()
                    .flatten()
                    .map(|(_, item)| item.name);

                all_writeback.push(WritebackEntry {
                    drive_id: pending_drive_id,
                    item_id,
                    file_name,
                });
            }
        }
    }

    Ok(CacheStatsResponse {
        disk_used_bytes: total_disk_used,
        disk_max_bytes: total_disk_max,
        memory_entry_count: total_memory_entries,
        pinned_items: all_pinned_items,
        writeback_queue: all_writeback,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_settings(
    app: AppHandle,
    auto_start: Option<bool>,
    cache_max_size: Option<String>,
    sync_interval_secs: Option<u64>,
    metadata_ttl_secs: Option<u64>,
    cache_dir: Option<String>,
    log_level: Option<String>,
    notifications: Option<bool>,
    root_dir: Option<String>,
    explorer_nav_pane: Option<bool>,
    offline_ttl_secs: Option<u64>,
    offline_max_folder_size: Option<String>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    #[cfg(target_os = "windows")]
    let root_dir_changed = root_dir.is_some();

    let old_auto_start;
    #[cfg(target_os = "windows")]
    let old_explorer_nav_pane;

    {
        let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;

        let general = user_config.general.get_or_insert_with(Default::default);
        old_auto_start = general.auto_start;
        #[cfg(target_os = "windows")]
        {
            old_explorer_nav_pane = general.explorer_nav_pane;
        }
        if let Some(v) = auto_start {
            general.auto_start = Some(v);
        }
        if let Some(v) = cache_max_size {
            general.cache_max_size = Some(v);
        }
        if let Some(v) = sync_interval_secs {
            general.sync_interval_secs = Some(v);
        }
        if let Some(v) = metadata_ttl_secs {
            general.metadata_ttl_secs = Some(v);
        }
        if cache_dir.is_some() {
            general.cache_dir = cache_dir;
        }
        if let Some(v) = log_level {
            general.log_level = Some(v);
        }
        if let Some(v) = notifications {
            general.notifications = Some(v);
        }
        if let Some(v) = root_dir {
            general.root_dir = Some(v);
        }
        if let Some(v) = explorer_nav_pane {
            general.explorer_nav_pane = Some(v);
        }
        if let Some(v) = offline_ttl_secs {
            general.offline_ttl_secs = Some(v);
        }
        if let Some(v) = offline_max_folder_size {
            general.offline_max_folder_size = Some(v);
        }

        let cfg_path = config_file_path().map_err(|e| e.to_string())?;
        user_config
            .save_to_file(&cfg_path)
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(&app)?;

    if let Some(v) = auto_start
        && old_auto_start != Some(v)
    {
        match std::env::current_exe() {
            Ok(exe) => {
                let exe_path = exe.to_string_lossy();
                match autostart::set_enabled(v, &exe_path) {
                    Ok(()) => {
                        tracing::info!("auto-start {}", if v { "enabled" } else { "disabled" });
                    }
                    Err(e) => {
                        tracing::warn!("auto-start registration failed: {e}");
                        crate::notify::auto_start_failed(&app, &e.to_string());
                    }
                }
            }
            Err(e) => {
                tracing::warn!("failed to resolve exe path for auto-start: {e}");
                crate::notify::auto_start_failed(&app, &e.to_string());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(true) = explorer_nav_pane
            && old_explorer_nav_pane != Some(true)
        {
            let config = state.effective_config.lock().map_err(|e| e.to_string())?;
            let cloud_root = expand_mount_point(&format!("~/{}", config.root_dir));
            if let Err(e) =
                crate::shell_integration::register_nav_pane(std::path::Path::new(&cloud_root))
            {
                tracing::warn!("Explorer navigation pane registration failed: {e}");
            }
        } else if let Some(false) = explorer_nav_pane
            && old_explorer_nav_pane != Some(false)
            && let Err(e) = crate::shell_integration::unregister_nav_pane()
        {
            tracing::warn!("Explorer navigation pane unregistration failed: {e}");
        }
        if root_dir_changed && crate::shell_integration::is_nav_pane_registered() {
            let config = state.effective_config.lock().map_err(|e| e.to_string())?;
            let cloud_root = expand_mount_point(&format!("~/{}", config.root_dir));
            if let Err(e) =
                crate::shell_integration::update_nav_pane_target(std::path::Path::new(&cloud_root))
            {
                tracing::warn!("Explorer navigation pane target update failed: {e}");
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn search_sites(app: AppHandle, query: String) -> Result<Vec<SiteInfo>, String> {
    let state = app.state::<AppState>();
    let sites = state
        .graph
        .search_sites(&query)
        .await
        .map_err(|e| e.to_string())?;
    Ok(sites
        .into_iter()
        .map(|s| SiteInfo {
            id: s.id,
            display_name: s.display_name.unwrap_or_default(),
            web_url: s.web_url.unwrap_or_default(),
        })
        .collect())
}

#[tauri::command]
pub async fn list_drives(app: AppHandle, site_id: String) -> Result<Vec<DriveInfo>, String> {
    let state = app.state::<AppState>();
    let drives = state
        .graph
        .list_site_drives(&site_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(drives
        .into_iter()
        .map(|d| DriveInfo {
            id: d.id,
            name: d.name,
        })
        .collect())
}

#[tauri::command]
pub async fn refresh_mount(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();

    let drive_id = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        config
            .mounts
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.drive_id.clone())
            .ok_or_else(|| format!("mount '{id}' not found or has no drive_id"))?
    };

    let (cache, inodes, observer) = {
        let mount_caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        mount_caches
            .get(&drive_id)
            .map(|(c, i, obs, _, _, _)| (c.clone(), i.clone(), obs.clone()))
            .ok_or_else(|| format!("no active cache for drive '{drive_id}'"))?
    };

    let inode_allocator: std::sync::Arc<dyn Fn(&str) -> u64 + Send + Sync> =
        std::sync::Arc::new(move |item_id: &str| inodes.allocate(item_id));

    run_delta_sync(
        &state.graph,
        &cache,
        &drive_id,
        &inode_allocator,
        observer.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn clear_cache(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    // Collect cache references before stopping — stop_mount removes entries from mount_caches.
    let caches: Vec<std::sync::Arc<carminedesktop_cache::CacheManager>> = state
        .mount_caches
        .lock()
        .map_err(|e| e.to_string())?
        .values()
        .map(|(c, _, _, _, _, _)| c.clone())
        .collect();

    crate::stop_all_mounts(&app);

    for cache in &caches {
        cache.clear().await.map_err(|e| e.to_string())?;
    }
    tracing::info!("cache cleared");

    if state.authenticated.load(Ordering::Relaxed) {
        crate::start_all_mounts(&app);
    }

    crate::tray::update_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub async fn open_wizard(app: AppHandle) -> Result<(), String> {
    crate::tray::open_or_focus_wizard(&app, true);
    Ok(())
}

#[tauri::command]
pub async fn get_drive_info(app: AppHandle) -> Result<DriveInfo, String> {
    let state = app.state::<AppState>();
    let drive = state
        .graph
        .get_my_drive()
        .await
        .map_err(|e| e.to_string())?;
    Ok(DriveInfo {
        id: drive.id,
        name: drive.name,
    })
}

#[tauri::command]
pub async fn get_followed_sites(app: AppHandle) -> Result<Vec<SiteInfo>, String> {
    let state = app.state::<AppState>();
    // MSA accounts don't support followed sites — propagate error so frontend hides SP section
    let sites = state.graph.get_followed_sites().await.map_err(|e| {
        tracing::info!("get_followed_sites unavailable (MSA account?): {e}");
        e.to_string()
    })?;
    Ok(sites
        .into_iter()
        .map(|s| SiteInfo {
            id: s.id,
            display_name: s.display_name.unwrap_or_default(),
            web_url: s.web_url.unwrap_or_default(),
        })
        .collect())
}

#[tauri::command]
pub async fn complete_wizard(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn check_fuse_available() -> bool {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        crate::fuse_available()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        true // Windows uses WinFsp, always available after preflight
    }
}

#[tauri::command]
pub fn get_default_mount_root(app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let config = state.effective_config.lock().map_err(|e| e.to_string())?;
    let expanded = expand_mount_point(&format!("~/{}", config.root_dir));
    Ok(std::path::PathBuf::from(&expanded)
        .to_string_lossy()
        .into_owned())
}

// ---------------------------------------------------------------------------
// File handler overrides
// ---------------------------------------------------------------------------

use crate::shell_integration::OFFICE_EXTENSIONS;

#[derive(Serialize)]
pub struct FileHandlerInfo {
    pub extension: String,
    pub handler_name: String,
    pub handler_id: String,
    /// One of: "override", "saved", "discovered", "none"
    pub source: String,
}

/// Return handler info for all managed extensions.
///
/// Resolution order per extension:
/// 1. Config override (`file_handler_overrides`)
/// 2. Saved previous handler (`get_previous_handler`)
/// 3. Runtime discovery (`discover_office_handler`)
/// 4. None
#[tauri::command]
pub fn get_file_handlers(app: AppHandle) -> Result<Vec<FileHandlerInfo>, String> {
    let state = app.state::<AppState>();
    let overrides: HashMap<String, String> = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        config.file_handler_overrides.clone()
    };

    let mut results = Vec::with_capacity(OFFICE_EXTENSIONS.len());

    for &ext in OFFICE_EXTENSIONS {
        // 1. Config override
        if let Some(handler_id) = overrides.get(ext) {
            results.push(FileHandlerInfo {
                extension: ext.to_string(),
                handler_name: handler_id.clone(),
                handler_id: handler_id.clone(),
                source: "override".to_string(),
            });
            continue;
        }

        // 2. Saved previous handler
        if let Some(handler_id) = crate::shell_integration::get_previous_handler(ext) {
            results.push(FileHandlerInfo {
                extension: ext.to_string(),
                handler_name: handler_id.clone(),
                handler_id,
                source: "saved".to_string(),
            });
            continue;
        }

        // 3. Runtime discovery
        if let Some(handler_id) = crate::shell_integration::discover_office_handler(ext) {
            results.push(FileHandlerInfo {
                extension: ext.to_string(),
                handler_name: handler_id.clone(),
                handler_id,
                source: "discovered".to_string(),
            });
            continue;
        }

        // 4. None
        results.push(FileHandlerInfo {
            extension: ext.to_string(),
            handler_name: String::new(),
            handler_id: String::new(),
            source: "none".to_string(),
        });
    }

    Ok(results)
}

/// Re-run handler discovery for all managed extensions.
///
/// Returns fresh results ignoring saved handlers — only discovers what is
/// currently installed on the system.
#[tauri::command]
pub fn redetect_file_handlers(_app: AppHandle) -> Result<Vec<FileHandlerInfo>, String> {
    let mut results = Vec::with_capacity(OFFICE_EXTENSIONS.len());

    for &ext in OFFICE_EXTENSIONS {
        if let Some(handler_id) = crate::shell_integration::discover_office_handler(ext) {
            results.push(FileHandlerInfo {
                extension: ext.to_string(),
                handler_name: handler_id.clone(),
                handler_id,
                source: "discovered".to_string(),
            });
        } else {
            results.push(FileHandlerInfo {
                extension: ext.to_string(),
                handler_name: String::new(),
                handler_id: String::new(),
                source: "none".to_string(),
            });
        }
    }

    Ok(results)
}

/// Save a per-extension handler override to config.
#[tauri::command]
pub fn save_file_handler_override(
    app: AppHandle,
    extension: String,
    handler_id: String,
) -> Result<(), String> {
    let state = app.state::<AppState>();

    {
        let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;
        let general = user_config.general.get_or_insert_with(Default::default);
        let overrides = general
            .file_handler_overrides
            .get_or_insert_with(HashMap::new);
        overrides.insert(extension.clone(), handler_id.clone());

        let cfg_path = config_file_path().map_err(|e| e.to_string())?;
        user_config
            .save_to_file(&cfg_path)
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(&app)?;
    tracing::info!("saved handler override for {extension}: {handler_id}");
    Ok(())
}

/// Remove a per-extension handler override from config.
#[tauri::command]
pub fn clear_file_handler_override(app: AppHandle, extension: String) -> Result<(), String> {
    let state = app.state::<AppState>();

    {
        let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;
        if let Some(ref mut general) = user_config.general
            && let Some(ref mut overrides) = general.file_handler_overrides
        {
            overrides.remove(&extension);
            // Clean up empty map
            if overrides.is_empty() {
                general.file_handler_overrides = None;
            }
        }

        let cfg_path = config_file_path().map_err(|e| e.to_string())?;
        user_config
            .save_to_file(&cfg_path)
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(&app)?;
    tracing::info!("cleared handler override for {extension}");
    Ok(())
}

fn rebuild_effective_config(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let user_config = state.user_config.lock().map_err(|e| e.to_string())?;
    let account_id = state.account_id.lock().map_err(|e| e.to_string())?.clone();
    let mut new_effective = EffectiveConfig::build(&user_config);
    new_effective.mounts = match &account_id {
        Some(aid) => new_effective
            .mounts
            .into_iter()
            .filter(|m| match m.account_id.as_deref() {
                Some(mid) if mid == aid.as_str() => true,
                None => true,
                Some(_) => {
                    tracing::warn!(
                        mount_id = %m.id,
                        mount_name = %m.name,
                        "skipping mount: account_id does not match signed-in account"
                    );
                    false
                }
            })
            .collect(),
        None => Vec::new(),
    };
    let mut effective = state.effective_config.lock().map_err(|e| e.to_string())?;
    *effective = new_effective;
    Ok(())
}

// ---------------------------------------------------------------------------
// Open in SharePoint
// ---------------------------------------------------------------------------

/// Open the Windows "Default Apps" settings panel for Carmine Desktop.
///
/// Uses the `IApplicationAssociationRegistrationUI` COM interface to show a
/// system dialog where the user can set Carmine Desktop as the default handler
/// for Office file types.  Falls back to opening `ms-settings:defaultapps` if
/// the COM call fails.
#[tauri::command]
pub fn prompt_set_default_handler() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        launch_default_apps_ui().map_err(|e| format!("{e}"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("not supported on this platform".to_string())
    }
}

#[cfg(target_os = "windows")]
fn launch_default_apps_ui() -> carminedesktop_core::Result<()> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::UI::Shell::{
        ApplicationAssociationRegistrationUI, IApplicationAssociationRegistrationUI,
    };
    use windows::core::HSTRING;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let ui: IApplicationAssociationRegistrationUI = CoCreateInstance(
            &ApplicationAssociationRegistrationUI,
            None,
            CLSCTX_INPROC_SERVER,
        )
        .map_err(|e| {
            carminedesktop_core::Error::Config(format!("failed to create association UI: {e}"))
        })?;
        ui.LaunchAdvancedAssociationUI(&HSTRING::from("CarmineDesktop"))
            .map_err(|e| {
                carminedesktop_core::Error::Config(format!(
                    "failed to open Default Apps panel: {e}"
                ))
            })?;
    }
    Ok(())
}

/// Open a mounted file in SharePoint / Office Online.
///
/// Resolves the local path to its SharePoint `webUrl`, applies Office URI scheme
/// mapping on Windows/macOS for desktop co-authoring, and opens the result.
/// Falls back to the plain browser URL if the Office URI fails.
#[tauri::command]
pub async fn open_online(app: AppHandle, path: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (drive_id, item) = resolve_item_for_path(&state, &path).await?;

    let extension = dotted_extension(&path);

    // Try the Office URI scheme with a direct document URL (requires the
    // drive's webUrl to construct).  Falls back to the browser if anything
    // in the chain fails.  Only on Windows — Office URI schemes are not
    // supported on Linux/macOS.
    if cfg!(target_os = "windows")
        && let Some(direct_url) = build_direct_url(&state.graph, &drive_id, &item).await
        && let Some(uri) = carminedesktop_core::open_online::office_uri(&extension, &direct_url)
    {
        tracing::info!("open_online: launching Office URI {uri}");
        if crate::open_with_clean_env(&uri).is_ok() {
            return Ok(());
        }
        tracing::warn!("Office URI scheme failed, falling back to browser");
    }

    // Fallback: open the _layouts web view URL in the browser.
    let web_url = item
        .web_url
        .or_else(|| {
            // If cached item has no web_url, we can't make another async call here,
            // so log a warning.  The caller should have it from the Graph response.
            tracing::warn!("item has no web_url, cannot open in browser");
            None
        })
        .ok_or_else(|| "item has no SharePoint URL".to_string())?;
    tracing::info!("open_online: opening in browser {web_url}");
    crate::open_with_clean_env(&web_url).map_err(|e| format!("failed to open URL: {e}"))?;

    Ok(())
}

/// Build a direct SharePoint document URL for use with Office URI schemes.
///
/// Fetches the drive's `webUrl` (the document library root) and combines it
/// with the item's parent path and name.
async fn build_direct_url(
    graph: &carminedesktop_graph::GraphClient,
    drive_id: &str,
    item: &DriveItem,
) -> Option<String> {
    let drive = graph.get_drive(drive_id).await.ok()?;
    let drive_web_url = drive.web_url.as_deref()?;
    let parent_path = item.parent_reference.as_ref()?.path.as_deref()?;
    Some(carminedesktop_core::open_online::direct_document_url(
        drive_web_url,
        parent_path,
        &item.name,
    ))
}

/// Resolve a local mount path to its `(drive_id, DriveItem)`.
pub(crate) async fn resolve_item_for_path(
    state: &AppState,
    local_path: &str,
) -> Result<(String, DriveItem), String> {
    let path = std::path::Path::new(local_path);

    let (drive_id, mount_point) = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        config
            .mounts
            .iter()
            .filter_map(|m| {
                let expanded = expand_mount_point(&m.mount_point);
                let drive_id = m.drive_id.as_ref()?;
                if path.starts_with(&expanded) {
                    Some((drive_id.clone(), expanded))
                } else {
                    None
                }
            })
            .next()
            .ok_or_else(|| format!("path is not inside any Carmine Desktop mount: {local_path}"))?
    };

    let (cache, inodes) = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        let (c, i, _, _, _, _) = caches
            .get(&drive_id)
            .ok_or_else(|| format!("no active cache for drive '{drive_id}'"))?;
        (c.clone(), i.clone())
    };

    let relative = path
        .strip_prefix(&mount_point)
        .map_err(|_| format!("failed to strip mount prefix from {local_path}"))?;
    let components: Vec<&str> = relative
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => s.to_str(),
            _ => None,
        })
        .collect();

    let item = resolve_path_to_item(&cache, &inodes, &state.graph, &drive_id, &components).await?;
    Ok((drive_id, item))
}

/// Walk path components through cache tiers (memory → SQLite → Graph API).
async fn resolve_path_to_item(
    cache: &carminedesktop_cache::CacheManager,
    inodes: &carminedesktop_vfs::inode::InodeTable,
    graph: &carminedesktop_graph::GraphClient,
    drive_id: &str,
    components: &[&str],
) -> Result<DriveItem, String> {
    use carminedesktop_vfs::inode::ROOT_INODE;

    if components.is_empty() {
        return lookup_cached_item(cache, inodes, ROOT_INODE)
            .ok_or_else(|| "root item not found in cache".to_string());
    }

    let mut current_ino = ROOT_INODE;
    for &name in components {
        current_ino = find_child_by_name(cache, inodes, graph, drive_id, current_ino, name).await?;
    }

    lookup_cached_item(cache, inodes, current_ino)
        .ok_or_else(|| "resolved inode has no cached item".to_string())
}

/// Look up a [`DriveItem`] by inode from memory cache, then SQLite.
fn lookup_cached_item(
    cache: &carminedesktop_cache::CacheManager,
    inodes: &carminedesktop_vfs::inode::InodeTable,
    inode: u64,
) -> Option<DriveItem> {
    if let Some(item) = cache.memory.get(inode) {
        return Some(item);
    }
    let item_id = inodes.get_item_id(inode)?;
    if let Ok(Some((_, item))) = cache.sqlite.get_item_by_id(&item_id) {
        cache.memory.insert(inode, item.clone());
        return Some(item);
    }
    None
}

/// Extract the dotted file extension (e.g. ".docx") from a path.
fn dotted_extension(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default()
}

/// Open a path with the OS default handler, returning a user-facing error on failure.
fn open_with_os_default(path: &str) -> Result<(), String> {
    crate::open_with_clean_env(path).map_err(|e| format!("failed to open with OS handler: {e}"))
}

/// Show a "no handler found" notification and return an error string.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn no_handler_error(app: &tauri::AppHandle, ext: &str) -> String {
    tracing::warn!(
        "open_file: no previous or discovered handler for {ext}, cannot fall through (we are the handler)"
    );
    let msg = format!(
        "Carmine Desktop could not find the original application to open {ext} files. \
         Please right-click the file and choose 'Open with' to select your \
         preferred application, then try again."
    );
    crate::notify::send(app, "Cannot open file", &msg);
    msg
}

/// Parse a Windows shell command template and spawn the process.
///
/// Templates are typically `"C:\path\to\app.exe" "%1"` or `app.exe %1`.
#[cfg(target_os = "windows")]
fn spawn_from_cmd_template(template: &str, path: &str) -> std::io::Result<std::process::Child> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let cmd = template.replace("%1", path);
    if let Some(stripped) = cmd.strip_prefix('"') {
        if let Some(end_quote) = stripped.find('"') {
            let exe = &stripped[..end_quote];
            let args = stripped[end_quote + 1..].trim();
            std::process::Command::new(exe)
                .raw_arg(args)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "malformed command template",
            ))
        }
    } else {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let exe = parts[0];
        let args = parts.get(1).unwrap_or(&"");
        std::process::Command::new(exe)
            .raw_arg(args)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    }
}

/// Try opening a file via a macOS bundle ID.
///
/// Resolves the bundle ID to an app path and tries `open -a`; falls back to `open -b`.
#[cfg(target_os = "macos")]
fn try_open_with_bundle(bundle_id: &str, path: &str) -> std::io::Result<()> {
    if let Some(app_path) = crate::shell_integration::resolve_app_path(bundle_id) {
        std::process::Command::new("open")
            .args(["-a", &app_path, path])
            .spawn()
            .map(|_| ())
    } else {
        std::process::Command::new("open")
            .args(["-b", bundle_id, path])
            .spawn()
            .map(|_| ())
    }
}

/// Try to open a file with the locally installed Office handler (Windows).
///
/// Checks config override → previous handler → runtime-discovered handler.
/// Returns `Ok(())` if the file was opened, `Err` if no handler was found.
/// Used for offline-cached files on Carmine Desktop mounts and for non-mount
/// paths where Carmine Desktop is the registered default handler.
#[cfg(target_os = "windows")]
fn try_local_handler(state: &AppState, path: &str) -> Result<(), String> {
    let ext = dotted_extension(path);

    let override_handler = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        config.file_handler_overrides.get(&ext).cloned()
    };

    if let Some(ref progid) = override_handler
        && let Some(cmd_template) = crate::shell_integration::get_progid_command(progid)
    {
        tracing::info!("try_local_handler: using config override handler {progid}");
        match spawn_from_cmd_template(&cmd_template, path) {
            Ok(_) => return Ok(()),
            Err(e) => {
                tracing::warn!(
                    "failed to invoke config override handler: {e}, trying other fallbacks"
                );
            }
        }
    }

    if let Some(progid) = crate::shell_integration::get_previous_handler(&ext)
        && let Some(cmd_template) = crate::shell_integration::get_progid_command(&progid)
    {
        tracing::info!("try_local_handler: invoking previous handler for {ext}");
        match spawn_from_cmd_template(&cmd_template, path) {
            Ok(_) => return Ok(()),
            Err(e) => {
                tracing::warn!("failed to invoke previous handler: {e}, trying other fallbacks");
            }
        }
    }

    if crate::shell_integration::is_handled_extension(&ext)
        && let Some(discovered) = crate::shell_integration::discover_office_handler(&ext)
        && let Some(cmd_template) = crate::shell_integration::get_progid_command(&discovered)
    {
        tracing::info!("try_local_handler: using discovered handler {discovered}");
        match spawn_from_cmd_template(&cmd_template, path) {
            Ok(_) => return Ok(()),
            Err(e) => {
                tracing::warn!("failed to invoke discovered handler: {e}");
            }
        }
    }

    Err(format!("no local handler found for {ext}"))
}

/// Try to open a file with the locally installed Office handler (macOS).
///
/// Checks config override → previous handler → runtime-discovered handler.
/// Returns `Ok(())` if the file was opened, `Err` if no handler was found.
#[cfg(target_os = "macos")]
fn try_local_handler(state: &AppState, path: &str) -> Result<(), String> {
    let ext = dotted_extension(path);

    let override_handler = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        config.file_handler_overrides.get(&ext).cloned()
    };

    if let Some(ref bundle_id) = override_handler {
        tracing::info!("try_local_handler: using config override handler {bundle_id}");
        match try_open_with_bundle(bundle_id, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!(
                    "failed to invoke config override handler: {e}, trying other fallbacks"
                );
            }
        }
    }

    if let Some(bundle_id) = crate::shell_integration::get_previous_handler(&ext) {
        tracing::info!("try_local_handler: invoking previous handler {bundle_id}");
        match try_open_with_bundle(&bundle_id, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!("failed to invoke previous handler: {e}, trying other fallbacks");
            }
        }
    }

    if crate::shell_integration::is_handled_extension(&ext)
        && let Some(discovered) = crate::shell_integration::discover_office_handler(&ext)
    {
        tracing::info!("try_local_handler: using discovered handler {discovered}");
        match try_open_with_bundle(&discovered, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!("failed to invoke discovered handler: {e}");
            }
        }
    }

    Err(format!("no local handler found for {ext}"))
}

/// Try to open a file with the OS default handler (Linux).
///
/// Linux has no file associations for Carmine Desktop, so `xdg-open` is safe.
#[cfg(target_os = "linux")]
fn try_local_handler(_state: &AppState, path: &str) -> Result<(), String> {
    open_with_os_default(path)
}

/// Open a file: if on a Carmine Desktop drive, open online; otherwise fall through to OS handler.
///
/// This is the entry point for file associations. When the user double-clicks
/// an Office file, the OS invokes `carminedesktop --open <path>`. If the path is
/// inside a Carmine Desktop mount, we resolve it to SharePoint and open online. If not,
/// we pass through to the OS default handler (e.g. local Office installation).
#[tauri::command]
pub async fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    tracing::debug!("open_file: invoked with path={path}");

    let state = app.state::<AppState>();

    // Check if the path is inside any Carmine Desktop mount
    let is_carminedesktop_path = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        let path_obj = std::path::Path::new(&path);
        config.mounts.iter().any(|m| {
            let expanded = expand_mount_point(&m.mount_point);
            path_obj.starts_with(&expanded)
        })
    };

    if is_carminedesktop_path {
        // Check if the drive is offline and the file is cached locally.
        // When offline, open with the local Office handler directly.
        // When online, always delegate to open_online for co-authoring.
        let (is_offline, has_local) = if let Ok((drive_id, item)) =
            resolve_item_for_path(&state, &path).await
            && item.file.is_some()
        {
            let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
            caches
                .get(&drive_id)
                .map(|(cache, _, _, _, offline_flag, _)| {
                    let offline = offline_flag.load(Ordering::Relaxed);
                    let cached = cache.disk.has(&drive_id, &item.id);
                    (offline, cached)
                })
                .unwrap_or((false, false))
        } else {
            (false, false)
        };

        if is_offline && has_local {
            // Drive is offline but file is cached — open with local Office handler
            // directly (not open_with_os_default, which would loop back to us).
            tracing::info!("open_file: drive offline, file cached, opening with local handler");
            return try_local_handler(&state, &path);
        } else if is_offline {
            // Offline and file not cached — nothing we can do
            let msg = "This file is not available offline. Pin the parent folder for offline use.";
            crate::notify::send(&app, "Cannot open file", msg);
            return Err(msg.to_string());
        }

        // Online — delegate to open_online (Office URI scheme / browser)
        tracing::info!("open_file: delegating to open_online");
        open_online(app, path).await
    } else {
        // Path is NOT on a Carmine Desktop drive — use the previous handler to avoid
        // infinite loop when Carmine Desktop is registered as the default handler.
        tracing::info!("open_file: path is not on Carmine Desktop, falling through to OS handler");

        match try_local_handler(&state, &path) {
            Ok(()) => Ok(()),
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            Err(_) if crate::shell_integration::is_handled_extension(&dotted_extension(&path)) => {
                Err(no_handler_error(&app, &dotted_extension(&path)))
            }
            Err(_) => open_with_os_default(&path),
        }
    }
}

/// Find a child item by name under a parent inode.
///
/// Searches memory cache → SQLite → Graph API (async). Mirrors the logic in
/// `CoreOps::find_child` but uses `.await` instead of `rt.block_on()`.
async fn find_child_by_name(
    cache: &carminedesktop_cache::CacheManager,
    inodes: &carminedesktop_vfs::inode::InodeTable,
    graph: &carminedesktop_graph::GraphClient,
    drive_id: &str,
    parent_ino: u64,
    name: &str,
) -> Result<u64, String> {
    // 1. Memory cache
    if let Some(children_map) = cache.memory.get_children(parent_ino) {
        #[cfg(not(target_os = "windows"))]
        let child_ino = children_map.get(name).copied();
        #[cfg(target_os = "windows")]
        let child_ino = children_map
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, &v)| v);

        if let Some(ino) = child_ino {
            return Ok(ino);
        }
    }

    // 2. SQLite
    if let Ok(children) = cache.sqlite.get_children(parent_ino) {
        for (_, item) in children {
            #[cfg(not(target_os = "windows"))]
            let matches = item.name == name;
            #[cfg(target_os = "windows")]
            let matches = item.name.eq_ignore_ascii_case(name);

            if matches {
                let ino = inodes.allocate(&item.id);
                cache.memory.insert(ino, item);
                return Ok(ino);
            }
        }
    }

    // 3. Graph API fallback
    let parent_item_id = inodes
        .get_item_id(parent_ino)
        .ok_or_else(|| format!("parent inode {parent_ino} not found"))?;
    let children = graph
        .list_children(drive_id, &parent_item_id)
        .await
        .map_err(|e| format!("failed to list children: {e}"))?;

    let mut children_map = std::collections::HashMap::new();
    let mut found_ino = None;

    for item in &children {
        let child_ino = inodes.allocate(&item.id);
        children_map.insert(item.name.clone(), child_ino);
        cache.memory.insert(child_ino, item.clone());

        #[cfg(not(target_os = "windows"))]
        let matches = item.name == name;
        #[cfg(target_os = "windows")]
        let matches = item.name.eq_ignore_ascii_case(name);

        if matches && found_ino.is_none() {
            found_ino = Some(child_ino);
        }
    }

    // Populate parent's children in memory cache
    if let Some(parent_item) = lookup_cached_item(cache, inodes, parent_ino) {
        cache
            .memory
            .insert_with_children(parent_ino, parent_item, children_map);
    }

    found_ino.ok_or_else(|| format!("'{name}' not found"))
}
