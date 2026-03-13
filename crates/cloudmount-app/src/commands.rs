use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use cloudmount_cache::sync::run_delta_sync;
use cloudmount_core::config::{
    AccountMetadata, EffectiveConfig, autostart, config_file_path, expand_mount_point,
};
use cloudmount_core::types::DriveItem;

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
        app_name: "CloudMount".to_string(),
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

        let cfg_path = config_file_path().map_err(|e| e.to_string())?;
        user_config
            .save_to_file(&cfg_path)
            .map_err(|e| e.to_string())?;
    }

    rebuild_effective_config(&app)?;

    if let Some(v) = auto_start {
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
            .map(|(c, i, obs)| (c.clone(), i.clone(), obs.clone()))
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
    let caches: Vec<std::sync::Arc<cloudmount_cache::CacheManager>> = state
        .mount_caches
        .lock()
        .map_err(|e| e.to_string())?
        .values()
        .map(|(c, _, _)| c.clone())
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

/// Open a mounted file in SharePoint / Office Online.
///
/// Resolves the local path to its SharePoint `webUrl`, applies Office URI scheme
/// mapping on Windows/macOS for desktop co-authoring, and opens the result.
/// Falls back to the plain browser URL if the Office URI fails.
#[tauri::command]
pub async fn open_online(app: AppHandle, path: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (drive_id, item) = resolve_item_for_path(&state, &path).await?;

    let extension = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();

    // Try the Office URI scheme with a direct document URL (requires the
    // drive's webUrl to construct).  Falls back to the browser if anything
    // in the chain fails.
    if let Some(direct_url) = build_direct_url(&state.graph, &drive_id, &item).await
        && let Some(uri) = cloudmount_core::open_online::office_uri(&extension, &direct_url)
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
    graph: &cloudmount_graph::GraphClient,
    drive_id: &str,
    item: &DriveItem,
) -> Option<String> {
    let drive = graph.get_drive(drive_id).await.ok()?;
    let drive_web_url = drive.web_url.as_deref()?;
    let parent_path = item.parent_reference.as_ref()?.path.as_deref()?;
    Some(cloudmount_core::open_online::direct_document_url(
        drive_web_url,
        parent_path,
        &item.name,
    ))
}

/// Resolve a local mount path to its `(drive_id, DriveItem)`.
async fn resolve_item_for_path(
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
            .ok_or_else(|| format!("path is not inside any CloudMount mount: {local_path}"))?
    };

    let (cache, inodes) = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        let (c, i, _) = caches
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
    cache: &cloudmount_cache::CacheManager,
    inodes: &cloudmount_vfs::inode::InodeTable,
    graph: &cloudmount_graph::GraphClient,
    drive_id: &str,
    components: &[&str],
) -> Result<DriveItem, String> {
    use cloudmount_vfs::inode::ROOT_INODE;

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
    cache: &cloudmount_cache::CacheManager,
    inodes: &cloudmount_vfs::inode::InodeTable,
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

/// Environment variable used as a recursion guard to prevent infinite loops
/// when CloudMount is invoked as a file handler but cannot find the original handler.
#[cfg(target_os = "windows")]
const OPEN_GUARD_ENV: &str = "CLOUDMOUNT_OPEN_GUARD";

/// Open a file: if on a CloudMount drive, open online; otherwise fall through to OS handler.
///
/// This is the entry point for Windows file associations. When the user double-clicks
/// an Office file, Windows invokes `CloudMount.exe --open <path>`. If the path is
/// inside a CloudMount mount, we resolve it to SharePoint and open online. If not,
/// we pass through to the OS default handler (e.g. local Office installation).
#[tauri::command]
pub async fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    // P1 Fix 3: Recursion guard — if we've been re-invoked in a fallback chain, bail immediately
    #[cfg(target_os = "windows")]
    if std::env::var(OPEN_GUARD_ENV).is_ok() {
        tracing::error!(
            "open_file: recursion guard triggered — CloudMount was re-invoked in fallback chain"
        );
        crate::notify::send(
            &app,
            "Cannot open file",
            "CloudMount detected an infinite loop while trying to open the file. \
             The original application handler could not be found.",
        );
        return Err(
            "recursion guard: CloudMount was re-invoked while trying to open the file".to_string(),
        );
    }

    let state = app.state::<AppState>();

    // Check if the path is inside any CloudMount mount
    let is_cloudmount_path = {
        let config = state.effective_config.lock().map_err(|e| e.to_string())?;
        let path_obj = std::path::Path::new(&path);
        config.mounts.iter().any(|m| {
            let expanded = expand_mount_point(&m.mount_point);
            path_obj.starts_with(&expanded)
        })
    };

    if is_cloudmount_path {
        // Path is on a CloudMount drive — delegate to open_online
        tracing::info!("open_file: path is on CloudMount, delegating to open_online");
        open_online(app, path).await
    } else {
        // Path is NOT on a CloudMount drive — use the previous handler to avoid infinite loop
        tracing::info!("open_file: path is not on CloudMount, falling through to OS handler");

        // On Windows: try the previous handler first to avoid infinite loop when
        // CloudMount is registered as the default handler for Office files
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let ext = std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{e}"))
                .unwrap_or_default();

            if let Some(progid) = crate::shell_integration::get_previous_handler(&ext)
                && let Some(cmd_template) = crate::shell_integration::get_progid_command(&progid)
            {
                // Parse the command template: typically "C:\...\app.exe" "%1" or similar
                // Replace %1 with the actual path
                let cmd = cmd_template.replace("%1", &path);

                tracing::info!("open_file: invoking previous handler: {cmd}");

                // Set recursion guard before spawning, in case the handler somehow re-invokes us
                std::env::set_var(OPEN_GUARD_ENV, "1");

                // Parse the command line to extract executable and arguments
                // The command is typically: "C:\path\to\app.exe" args...
                let result = if cmd.starts_with('"') {
                    // Quoted executable path
                    if let Some(end_quote) = cmd[1..].find('"') {
                        let exe = &cmd[1..=end_quote];
                        let args = cmd[end_quote + 2..].trim();
                        std::process::Command::new(exe).raw_arg(args).spawn()
                    } else {
                        // Malformed command, fall through
                        Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "malformed command template",
                        ))
                    }
                } else {
                    // Unquoted — split on first space
                    let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
                    let exe = parts[0];
                    let args = parts.get(1).unwrap_or(&"");
                    std::process::Command::new(exe).raw_arg(args).spawn()
                };

                std::env::remove_var(OPEN_GUARD_ENV);

                match result {
                    Ok(_) => return Ok(()),
                    Err(e) => {
                        tracing::warn!(
                            "failed to invoke previous handler: {e}, trying other fallbacks"
                        );
                    }
                }
            }
        }

        // Fall through to default OS handler for non-Windows or if previous handler failed
        #[cfg(target_os = "windows")]
        {
            let ext = std::path::Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| format!(".{e}"))
                .unwrap_or_default();

            // P0 Fix 2: Do NOT call open_with_clean_env for extensions we handle — that would
            // cause an infinite loop since ShellExecute would find CloudMount as the handler.
            if crate::shell_integration::is_handled_extension(&ext) {
                tracing::error!(
                    "open_file: no previous handler found for {ext}, refusing to use OS fallback to avoid infinite loop"
                );
                crate::notify::send(
                    &app,
                    "Cannot open file",
                    &format!(
                        "CloudMount could not find the original application to open {ext} files. \
                         Please ensure Microsoft Office or another compatible application is installed."
                    ),
                );
                return Err(format!(
                    "no previous handler found for {ext} — cannot use OS fallback for handled extension"
                ));
            }

            // For extensions we don't handle, set the recursion guard and use OS fallback
            std::env::set_var(OPEN_GUARD_ENV, "1");
            let result = crate::open_with_clean_env(&path)
                .map_err(|e| format!("failed to open with OS handler: {e}"));
            std::env::remove_var(OPEN_GUARD_ENV);
            result
        }

        #[cfg(not(target_os = "windows"))]
        {
            crate::open_with_clean_env(&path)
                .map_err(|e| format!("failed to open with OS handler: {e}"))
        }
    }
}

/// Find a child item by name under a parent inode.
///
/// Searches memory cache → SQLite → Graph API (async). Mirrors the logic in
/// `CoreOps::find_child` but uses `.await` instead of `rt.block_on()`.
async fn find_child_by_name(
    cache: &cloudmount_cache::CacheManager,
    inodes: &cloudmount_vfs::inode::InodeTable,
    graph: &cloudmount_graph::GraphClient,
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
