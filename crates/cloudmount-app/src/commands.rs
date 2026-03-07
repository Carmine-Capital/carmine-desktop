#![cfg(feature = "desktop")]

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use cloudmount_cache::sync::run_delta_sync;
use cloudmount_core::config::{
    AccountMetadata, EffectiveConfig, config_file_path, derive_mount_point, expand_mount_point,
};

use crate::AppState;

#[derive(Serialize)]
pub struct MountInfo {
    pub id: String,
    pub name: String,
    pub mount_type: String,
    pub mount_point: String,
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct SettingsInfo {
    pub app_name: String,
    pub auto_start: bool,
    pub cache_max_size: String,
    pub sync_interval_secs: u64,
    pub metadata_ttl_secs: u64,
    pub cache_dir: Option<String>,
    pub log_level: String,
    pub notifications: bool,
    pub root_dir: String,
    pub account_display: Option<String>,
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
    let (url_tx, url_rx) = tokio::sync::oneshot::channel::<String>();

    let auth = state.auth.clone();
    let app_handle = app.clone();
    tokio::spawn(async move {
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

    url_rx
        .await
        .map_err(|_| "auth URL channel closed unexpectedly".to_string())
}

async fn complete_sign_in(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    let drive = state
        .graph
        .get_my_drive()
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!("discovered OneDrive: {} ({})", drive.name, drive.id);

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

        let has_onedrive = user_config.mounts.iter().any(|m| m.mount_type == "drive");
        if !has_onedrive {
            let root_dir = {
                let config = state.effective_config.lock().map_err(|e| e.to_string())?;
                config.root_dir.clone()
            };
            let mount_point = derive_mount_point(&root_dir, "drive", None, None);
            user_config
                .add_onedrive_mount(&drive.id, &mount_point)
                .map_err(|e| e.to_string())?;
        }

        user_config
            .save_to_file(&config_file_path())
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(app)?;
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

    crate::stop_all_mounts(&app);

    if let Some(cancel) = state.sync_cancel.lock().unwrap().take() {
        cancel.cancel();
    }

    state.auth.sign_out().await.map_err(|e| e.to_string())?;

    {
        let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;
        user_config.accounts.clear();
        user_config
            .save_to_file(&config_file_path())
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(&app)?;

    state.authenticated.store(false, Ordering::Relaxed);
    state.auth_degraded.store(false, Ordering::Relaxed);
    crate::tray::update_tray_menu(&app);

    app.get_webview_window("settings").map(|w| w.hide());

    if let Some(win) = app.get_webview_window("wizard") {
        let _ = win.reload();
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        crate::tray::open_or_focus_window(&app, "wizard", "Setup", "wizard.html");
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
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mount_id;

    {
        let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;

        match mount_type.as_str() {
            "sharepoint" => {
                let sid = site_id.ok_or("site_id required for SharePoint mount")?;
                let did = drive_id.ok_or("drive_id required for SharePoint mount")?;
                let sn = site_name.unwrap_or_default();
                let ln = library_name.unwrap_or_default();
                user_config
                    .add_sharepoint_mount(&sid, &did, &sn, &ln, &mount_point)
                    .map_err(|e| e.to_string())?;
            }
            _ => {
                let did = drive_id.ok_or("drive_id required for OneDrive mount")?;
                user_config
                    .add_onedrive_mount(&did, &mount_point)
                    .map_err(|e| e.to_string())?;
            }
        }

        mount_id = user_config.mounts.last().map(|m| m.id.clone());

        user_config
            .save_to_file(&config_file_path())
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(&app)?;

    if state.authenticated.load(Ordering::Relaxed)
        && let Some(id) = &mount_id
    {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        if let Some(mount_config) = config.mounts.iter().find(|m| &m.id == id)
            && let Err(e) = crate::start_mount(&app, mount_config)
        {
            tracing::error!("failed to start new mount: {e}");
        }
    }

    crate::tray::update_tray_menu(&app);
    Ok(())
}

#[tauri::command]
pub fn remove_mount(app: AppHandle, id: String) -> Result<bool, String> {
    let state = app.state::<AppState>();

    let _ = crate::stop_mount(&app, &id);

    let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;
    let removed = user_config.remove_mount(&id);
    user_config
        .save_to_file(&config_file_path())
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
    user_config
        .save_to_file(&config_file_path())
        .map_err(|e| e.to_string())?;
    drop(user_config);

    rebuild_effective_config(&app)?;

    if state.authenticated.load(Ordering::Relaxed)
        && let Some(enabled) = result
    {
        if enabled {
            let config = state.effective_config.lock().map_err(|e| e.to_string())?;
            if let Some(mount_config) = config.mounts.iter().find(|m| m.id == id) {
                let _ = crate::start_mount(&app, mount_config);
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
        app_name: config.app_name.clone(),
        auto_start: config.auto_start,
        cache_max_size: config.cache_max_size.clone(),
        sync_interval_secs: config.sync_interval_secs,
        metadata_ttl_secs: config.metadata_ttl_secs,
        cache_dir: config.cache_dir.clone(),
        log_level: config.log_level.clone(),
        notifications: config.notifications,
        root_dir: config.root_dir.clone(),
        account_display,
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
) -> Result<(), String> {
    let state = app.state::<AppState>();

    {
        let mut user_config = state.user_config.lock().map_err(|e| e.to_string())?;

        let general = user_config.general.get_or_insert_with(Default::default);
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

        user_config
            .save_to_file(&config_file_path())
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(&app)?;
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

    let inodes = state.inodes.clone();
    let inode_allocator: std::sync::Arc<dyn Fn(&str) -> u64 + Send + Sync> =
        std::sync::Arc::new(move |item_id: &str| inodes.allocate(item_id));

    run_delta_sync(&state.graph, &state.cache, &drive_id, &inode_allocator)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn clear_cache(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    crate::stop_all_mounts(&app);

    state.cache.clear().await.map_err(|e| e.to_string())?;
    tracing::info!("cache cleared");

    if state.authenticated.load(Ordering::Relaxed) {
        crate::start_all_mounts(&app);
    }

    crate::tray::update_tray_menu(&app);
    Ok(())
}

fn rebuild_effective_config(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let user_config = state.user_config.lock().map_err(|e| e.to_string())?;
    let new_effective = EffectiveConfig::build(&state.packaged, &user_config);
    let mut effective = state.effective_config.lock().map_err(|e| e.to_string())?;
    *effective = new_effective;
    Ok(())
}
