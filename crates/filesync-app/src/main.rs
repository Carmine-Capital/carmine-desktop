#![cfg_attr(
    all(not(debug_assertions), feature = "desktop"),
    windows_subsystem = "windows"
)]

#[cfg(feature = "desktop")]
mod commands;
#[cfg(feature = "desktop")]
mod notify;
#[cfg(feature = "desktop")]
mod tray;

use tracing_subscriber::EnvFilter;

use filesync_core::config::{
    EffectiveConfig, PackagedDefaults, UserConfig, config_file_path, expand_mount_point,
};

#[cfg(feature = "desktop")]
use filesync_auth::AuthManager;
#[cfg(feature = "desktop")]
use filesync_cache::CacheManager;
#[cfg(feature = "desktop")]
use filesync_cache::sync::run_delta_sync;
#[cfg(feature = "desktop")]
use filesync_core::config::{MountConfig, cache_dir};
#[cfg(feature = "desktop")]
use filesync_graph::GraphClient;
#[cfg(feature = "desktop")]
use filesync_vfs::inode::InodeTable;
#[cfg(feature = "desktop")]
use std::collections::HashMap;
#[cfg(feature = "desktop")]
use std::sync::atomic::AtomicBool;
#[cfg(feature = "desktop")]
use std::sync::{Arc, Mutex, RwLock};
#[cfg(feature = "desktop")]
use tokio_util::sync::CancellationToken;

#[cfg(all(feature = "desktop", any(target_os = "linux", target_os = "macos")))]
use filesync_vfs::MountHandle;

#[cfg(feature = "desktop")]
const DEFAULT_CLIENT_ID: &str = "00000000-0000-0000-0000-000000000000";

const PACKAGED_DEFAULTS_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../build/defaults.toml"
));

#[cfg(feature = "desktop")]
pub struct AppState {
    pub packaged: PackagedDefaults,
    pub user_config: Mutex<UserConfig>,
    pub effective_config: Mutex<EffectiveConfig>,
    pub auth: Arc<AuthManager>,
    pub graph: Arc<GraphClient>,
    pub cache: Arc<CacheManager>,
    pub inodes: Arc<InodeTable>,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub mounts: Mutex<HashMap<String, MountHandle>>,
    #[cfg(target_os = "windows")]
    pub mounts: Mutex<HashMap<String, filesync_vfs::CfMountHandle>>,
    pub sync_cancel: Mutex<Option<CancellationToken>>,
    pub drive_ids: Arc<RwLock<Vec<String>>>,
    pub authenticated: AtomicBool,
    pub auth_degraded: AtomicBool,
}

#[cfg(feature = "desktop")]
fn parse_cache_size(size_str: &str) -> u64 {
    let s = size_str.trim().to_uppercase();
    let (num_part, multiplier) = if let Some(n) = s.strip_suffix("GB") {
        (n.trim(), 1024 * 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("MB") {
        (n.trim(), 1024 * 1024)
    } else if let Some(n) = s.strip_suffix("KB") {
        (n.trim(), 1024)
    } else {
        (s.as_str(), 1u64)
    };
    num_part.parse::<u64>().unwrap_or(5) * multiplier
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let packaged = PackagedDefaults::load(PACKAGED_DEFAULTS_TOML).unwrap_or_else(|e| {
        tracing::warn!("failed to load packaged defaults: {e}");
        PackagedDefaults::default()
    });

    tracing::info!("{} starting", packaged.app_name());

    if packaged.has_packaged_config() {
        tracing::info!("pre-configured build detected");
    }

    let user_config = UserConfig::load_from_file(&config_file_path()).unwrap_or_else(|e| {
        tracing::warn!("failed to load user config: {e}");
        UserConfig::default()
    });

    let effective = EffectiveConfig::build(&packaged, &user_config);

    for mount in &effective.mounts {
        let expanded = expand_mount_point(&mount.mount_point);
        tracing::info!(
            "mount '{}' ({}) → {}",
            mount.name,
            mount.mount_type,
            expanded
        );
    }

    #[cfg(feature = "desktop")]
    run_desktop(packaged, user_config, effective);

    #[cfg(not(feature = "desktop"))]
    run_headless(packaged, user_config, effective);
}

#[cfg(feature = "desktop")]
fn run_desktop(packaged: PackagedDefaults, user_config: UserConfig, effective: EffectiveConfig) {
    let app_name = effective.app_name.clone();
    let first_run = !config_file_path().exists();

    let client_id = packaged
        .client_id()
        .unwrap_or(DEFAULT_CLIENT_ID)
        .to_string();
    let tenant_id = packaged.tenant_id().map(String::from);

    let auth = Arc::new(AuthManager::new(client_id, tenant_id));

    let auth_for_graph = auth.clone();
    let graph = Arc::new(GraphClient::new(move || {
        let auth = auth_for_graph.clone();
        async move { auth.access_token().await }
    }));

    let effective_cache_dir = effective
        .cache_dir
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(cache_dir);
    let db_path = effective_cache_dir.join("filesync.db");
    let max_cache_bytes = parse_cache_size(&effective.cache_max_size);
    let metadata_ttl = Some(effective.metadata_ttl_secs);

    let cache = Arc::new(
        CacheManager::new(effective_cache_dir, db_path, max_cache_bytes, metadata_ttl)
            .unwrap_or_else(|e| {
                tracing::error!("failed to initialize cache: {e}");
                std::process::exit(1);
            }),
    );

    let inodes = Arc::new(InodeTable::new());
    let drive_ids: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));

    let state = AppState {
        packaged,
        user_config: Mutex::new(user_config),
        effective_config: Mutex::new(effective),
        auth,
        graph,
        cache,
        inodes,
        mounts: Mutex::new(HashMap::new()),
        sync_cancel: Mutex::new(None),
        drive_ids,
        authenticated: AtomicBool::new(false),
        auth_degraded: AtomicBool::new(false),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::sign_in,
            commands::sign_out,
            commands::list_mounts,
            commands::add_mount,
            commands::remove_mount,
            commands::toggle_mount,
            commands::get_settings,
            commands::save_settings,
            commands::search_sites,
            commands::list_drives,
            commands::refresh_mount,
        ])
        .setup(move |app| {
            tray::setup(app.handle(), &app_name)?;

            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                setup_after_launch(&handle, first_run).await;
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "wizard" {
                    if let Some(state) = window.app_handle().try_state::<AppState>() {
                        if !state
                            .authenticated
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            window.app_handle().exit(0);
                            return;
                        }
                    }
                }
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(feature = "desktop")]
async fn setup_after_launch(app: &tauri::AppHandle, first_run: bool) {
    use std::sync::atomic::Ordering;
    use tauri::Manager;

    let state = app.state::<AppState>();

    let account = {
        let config = state.effective_config.lock().unwrap();
        config.accounts.first().cloned()
    };

    let restored = if let Some(ref account) = account {
        match state.auth.try_restore(&account.id).await {
            Ok(true) => {
                tracing::info!("tokens restored for {}", account.id);
                true
            }
            Ok(false) => {
                tracing::info!("stored tokens invalid, sign-in required");
                false
            }
            Err(e) => {
                tracing::warn!("token restore failed: {e}");
                false
            }
        }
    } else {
        false
    };

    if restored {
        state.authenticated.store(true, Ordering::Relaxed);
        run_crash_recovery(app).await;
        start_all_mounts(app);
        start_delta_sync(app);
        tray::update_tray_menu(app);
    } else if first_run {
        tray::open_or_focus_window(app, "wizard", "Setup", "wizard.html");
    }

    // Signal handler — graceful shutdown on Ctrl+C / SIGTERM
    let signal_handle = app.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            let mut sigterm =
                signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to register Ctrl+C handler");
        }
        tracing::info!("received shutdown signal");
        graceful_shutdown(&signal_handle);
    });
}

#[cfg(feature = "desktop")]
fn start_all_mounts(app: &tauri::AppHandle) {
    use tauri::Manager;

    let state = app.state::<AppState>();
    let mounts_config: Vec<MountConfig> = {
        let config = state.effective_config.lock().unwrap();
        config
            .mounts
            .iter()
            .filter(|m| m.enabled)
            .cloned()
            .collect()
    };

    for mount_config in &mounts_config {
        if let Err(e) = start_mount(app, mount_config) {
            tracing::error!("failed to start mount '{}': {e}", mount_config.name);
        }
    }
    tray::update_tray_menu(app);
}

#[cfg(all(feature = "desktop", any(target_os = "linux", target_os = "macos")))]
fn start_mount(app: &tauri::AppHandle, mount_config: &MountConfig) -> Result<(), String> {
    use tauri::Manager;

    let drive_id = mount_config
        .drive_id
        .as_deref()
        .ok_or_else(|| format!("mount '{}' has no drive_id", mount_config.name))?;

    let mountpoint = expand_mount_point(&mount_config.mount_point);
    std::fs::create_dir_all(&mountpoint).map_err(|e| format!("create mountpoint failed: {e}"))?;

    let state = app.state::<AppState>();
    let rt = tokio::runtime::Handle::current();

    let handle = MountHandle::mount(
        state.graph.clone(),
        state.cache.clone(),
        state.inodes.clone(),
        drive_id.to_string(),
        &mountpoint,
        rt,
    )
    .map_err(|e| e.to_string())?;

    state.drive_ids.write().unwrap().push(drive_id.to_string());

    state
        .mounts
        .lock()
        .unwrap()
        .insert(mount_config.id.clone(), handle);

    notify::mount_success(app, &mount_config.name, &mountpoint);
    tracing::info!("mount '{}' started at {mountpoint}", mount_config.name);
    tray::update_tray_menu(app);

    Ok(())
}

#[cfg(all(feature = "desktop", target_os = "windows"))]
fn start_mount(app: &tauri::AppHandle, mount_config: &MountConfig) -> Result<(), String> {
    use tauri::Manager;

    let drive_id = mount_config
        .drive_id
        .as_deref()
        .ok_or_else(|| format!("mount '{}' has no drive_id", mount_config.name))?;

    let mountpoint = expand_mount_point(&mount_config.mount_point);
    std::fs::create_dir_all(&mountpoint).map_err(|e| format!("create mountpoint failed: {e}"))?;

    let state = app.state::<AppState>();
    let rt = tokio::runtime::Handle::current();

    let handle = filesync_vfs::CfMountHandle::mount(
        state.graph.clone(),
        state.cache.clone(),
        state.inodes.clone(),
        drive_id.to_string(),
        std::path::Path::new(&mountpoint),
        rt,
    )
    .map_err(|e| e.to_string())?;

    state.drive_ids.write().unwrap().push(drive_id.to_string());

    state
        .mounts
        .lock()
        .unwrap()
        .insert(mount_config.id.clone(), handle);

    notify::mount_success(app, &mount_config.name, &mountpoint);
    tracing::info!("mount '{}' started at {mountpoint}", mount_config.name);
    tray::update_tray_menu(app);

    Ok(())
}

#[cfg(feature = "desktop")]
fn stop_mount(app: &tauri::AppHandle, mount_id: &str) -> Result<(), String> {
    use tauri::Manager;

    let state = app.state::<AppState>();
    let handle = state
        .mounts
        .lock()
        .unwrap()
        .remove(mount_id)
        .ok_or_else(|| format!("mount '{mount_id}' not found"))?;

    let drive_id = {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            handle.drive_id().to_string()
        }
        #[cfg(target_os = "windows")]
        {
            handle.drive_id().to_string()
        }
    };

    state.drive_ids.write().unwrap().retain(|d| d != &drive_id);

    handle.unmount().map_err(|e| e.to_string())?;
    tracing::info!("mount '{mount_id}' stopped");
    tray::update_tray_menu(app);

    Ok(())
}

#[cfg(feature = "desktop")]
fn stop_all_mounts(app: &tauri::AppHandle) {
    use tauri::Manager;

    let state = app.state::<AppState>();
    let mount_ids: Vec<String> = state.mounts.lock().unwrap().keys().cloned().collect();

    for id in &mount_ids {
        if let Err(e) = stop_mount(app, id) {
            tracing::error!("failed to stop mount '{id}': {e}");
        }
    }
    tray::update_tray_menu(app);
}

#[cfg(feature = "desktop")]
fn start_delta_sync(app: &tauri::AppHandle) {
    use tauri::Manager;

    let state = app.state::<AppState>();
    let interval = {
        let config = state.effective_config.lock().unwrap();
        config.sync_interval_secs
    };

    if let Some(old) = state.sync_cancel.lock().unwrap().take() {
        old.cancel();
    }

    let cancel = CancellationToken::new();
    *state.sync_cancel.lock().unwrap() = Some(cancel.clone());

    let graph = state.graph.clone();
    let cache = state.cache.clone();
    let drive_ids = state.drive_ids.clone();
    let inodes = state.inodes.clone();
    let app_handle = app.clone();

    let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
        Arc::new(move |item_id: &str| inodes.allocate(item_id));

    tauri::async_runtime::spawn(async move {
        loop {
            let drives = drive_ids.read().unwrap().clone();
            for drive_id in &drives {
                match run_delta_sync(&graph, &cache, drive_id, &inode_allocator).await {
                    Ok(()) => {}
                    Err(filesync_core::Error::Auth(ref msg))
                        if msg.contains("re-authentication required") =>
                    {
                        let state = app_handle.state::<AppState>();
                        if !state
                            .auth_degraded
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            state
                                .auth_degraded
                                .store(true, std::sync::atomic::Ordering::Relaxed);
                            tracing::warn!("auth degraded: {msg}");
                            notify::auth_expired(&app_handle);
                            tray::update_tray_menu(&app_handle);
                        }
                    }
                    Err(e) => {
                        tracing::error!("delta sync failed for drive {drive_id}: {e}");
                    }
                }
            }

            let wait = std::time::Duration::from_secs(interval);
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(wait) => {}
            }
        }
    });
}

#[cfg(feature = "desktop")]
async fn run_crash_recovery(app: &tauri::AppHandle) {
    use tauri::Manager;

    let state = app.state::<AppState>();
    let pending = match state.cache.writeback.list_pending().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("crash recovery: failed to list pending writes: {e}");
            return;
        }
    };

    if pending.is_empty() {
        return;
    }

    tracing::info!("crash recovery: {} pending writes found", pending.len());

    let graph = state.graph.clone();
    let cache = state.cache.clone();

    tauri::async_runtime::spawn(async move {
        for (drive_id, item_id) in &pending {
            let content = match cache.writeback.read(drive_id, item_id).await {
                Some(c) => c,
                None => continue,
            };

            match graph
                .upload(
                    drive_id,
                    "",
                    Some(item_id.as_str()),
                    item_id,
                    bytes::Bytes::from(content),
                )
                .await
            {
                Ok(_) => {
                    let _ = cache.writeback.remove(drive_id, item_id).await;
                    tracing::info!("crash recovery: uploaded {drive_id}/{item_id}");
                }
                Err(e) => {
                    tracing::warn!("crash recovery: upload failed for {drive_id}/{item_id}: {e}");
                }
            }
        }
    });
}

#[cfg(feature = "desktop")]
pub fn graceful_shutdown(app: &tauri::AppHandle) {
    use tauri::Manager;

    tracing::info!("graceful shutdown initiated");

    let state = app.state::<AppState>();

    if let Some(cancel) = state.sync_cancel.lock().unwrap().take() {
        cancel.cancel();
    }

    stop_all_mounts(app);

    tracing::info!("shutdown complete");
    app.exit(0);
}

#[cfg(not(feature = "desktop"))]
fn run_headless(_packaged: PackagedDefaults, _user_config: UserConfig, effective: EffectiveConfig) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    rt.block_on(async {
        tracing::info!("{} initialized — ready for sign-in", effective.app_name);
    });
}
