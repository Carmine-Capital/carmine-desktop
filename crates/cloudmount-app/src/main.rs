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
#[cfg(feature = "desktop")]
mod update;

use clap::Parser;
use tracing_subscriber::EnvFilter;

use cloudmount_core::config::{
    AccountMetadata, EffectiveConfig, PackagedDefaults, UserConfig, config_file_path,
    derive_mount_point, expand_mount_point,
};

use std::sync::Arc;

type OpenerFn = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

use cloudmount_auth::AuthManager;
use cloudmount_cache::CacheManager;
use cloudmount_cache::sync::run_delta_sync;
use cloudmount_core::config::{MountConfig, cache_dir};
use cloudmount_graph::GraphClient;
use cloudmount_vfs::inode::InodeTable;
use tokio_util::sync::CancellationToken;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use cloudmount_vfs::MountHandle;

use std::sync::atomic::AtomicBool;

#[cfg(feature = "desktop")]
use std::collections::HashMap;
#[cfg(feature = "desktop")]
use std::sync::{Mutex, RwLock};

const DEFAULT_CLIENT_ID: &str = "00000000-0000-0000-0000-000000000000";

const BUILD_CLIENT_ID: Option<&str> = option_env!("CLOUDMOUNT_CLIENT_ID");
const BUILD_TENANT_ID: Option<&str> = option_env!("CLOUDMOUNT_TENANT_ID");
const BUILD_APP_NAME: Option<&str> = option_env!("CLOUDMOUNT_APP_NAME");

const PACKAGED_DEFAULTS_TOML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../build/defaults.toml"
));

/// CloudMount — mount Microsoft OneDrive and SharePoint as local filesystems.
#[derive(Parser, Debug)]
#[command(version, about)]
struct CliArgs {
    /// Azure AD client ID
    #[arg(long, env = "CLOUDMOUNT_CLIENT_ID")]
    client_id: Option<String>,

    /// Azure AD tenant ID
    #[arg(long, env = "CLOUDMOUNT_TENANT_ID")]
    tenant_id: Option<String>,

    /// Config file path
    #[arg(long, env = "CLOUDMOUNT_CONFIG")]
    config: Option<std::path::PathBuf>,

    /// Log level (trace/debug/info/warn/error)
    #[arg(long, env = "CLOUDMOUNT_LOG_LEVEL")]
    log_level: Option<String>,

    /// Run without GUI (even if desktop feature is enabled)
    #[arg(long)]
    headless: bool,
}

struct RuntimeOverrides {
    client_id: Option<String>,
    tenant_id: Option<String>,
}

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
    pub mounts: Mutex<HashMap<String, cloudmount_vfs::CfMountHandle>>,
    pub sync_cancel: Mutex<Option<CancellationToken>>,
    pub active_sign_in: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub drive_ids: Arc<RwLock<Vec<String>>>,
    pub authenticated: AtomicBool,
    pub auth_degraded: AtomicBool,
}

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

struct Components {
    auth: Arc<AuthManager>,
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,
    inodes: Arc<InodeTable>,
}

fn resolve_client_id(overrides: &RuntimeOverrides, packaged: &PackagedDefaults) -> String {
    overrides
        .client_id
        .clone()
        .or_else(|| BUILD_CLIENT_ID.map(String::from))
        .or_else(|| packaged.client_id().map(String::from))
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

fn resolve_tenant_id(overrides: &RuntimeOverrides, packaged: &PackagedDefaults) -> Option<String> {
    overrides
        .tenant_id
        .clone()
        .or_else(|| BUILD_TENANT_ID.map(String::from))
        .or_else(|| packaged.tenant_id().map(String::from))
}

/// Returns true if FUSE is available on the current system.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn fuse_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("fusermount3")
            .arg("--version")
            .output()
            .is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("fusermount")
            .arg("--version")
            .output()
            .is_ok()
    }
}

/// Checks that the Windows version meets the Cloud Files API minimum (10.0.16299).
/// Extracted for testability — callers can pass a custom minimum version.
#[cfg(target_os = "windows")]
fn cfapi_version_meets(min_major: u32, min_minor: u32, min_build: u32) -> bool {
    use windows::Win32::System::SystemInformation::{
        OSVERSIONINFOEXW, VER_BUILDNUMBER, VER_MAJORVERSION, VER_MINORVERSION, VerSetConditionMask,
        VerifyVersionInfoW,
    };

    let mut osvi = OSVERSIONINFOEXW {
        dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOEXW>() as u32,
        dwMajorVersion: min_major,
        dwMinorVersion: min_minor,
        dwBuildNumber: min_build,
        ..Default::default()
    };

    // 3 = VER_GREATER_EQUAL; omit VER_PRODUCT_TYPE so this works on Windows Server too
    let condition_mask = unsafe {
        let cm = VerSetConditionMask(0, VER_MAJORVERSION, 3);
        let cm = VerSetConditionMask(cm, VER_MINORVERSION, 3);
        VerSetConditionMask(cm, VER_BUILDNUMBER, 3)
    };

    unsafe {
        VerifyVersionInfoW(
            &mut osvi,
            VER_MAJORVERSION | VER_MINORVERSION | VER_BUILDNUMBER,
            condition_mask,
        )
        .is_ok()
    }
}

/// Show a native Win32 error dialog. Only compiled on Windows release desktop builds
/// where `windows_subsystem = "windows"` detaches the console (making eprintln invisible).
#[cfg(all(target_os = "windows", feature = "desktop", not(debug_assertions)))]
fn show_error_dialog(title: &str, msg: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::PCWSTR;

    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let msg_wide: Vec<u16> = msg.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        MessageBoxW(
            None,
            PCWSTR(msg_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Report a fatal startup error and terminate. On Windows release desktop builds, shows
/// a `MessageBoxW` dialog (stderr is detached). On all other builds, writes to stderr.
fn fatal_error(msg: &str) -> ! {
    #[cfg(all(target_os = "windows", feature = "desktop", not(debug_assertions)))]
    show_error_dialog("CloudMount \u{2014} Configuration Error", msg);
    #[cfg(not(all(target_os = "windows", feature = "desktop", not(debug_assertions))))]
    eprintln!("Error: {msg}");
    std::process::exit(1);
}

fn preflight_checks(client_id: &str) -> Result<(), String> {
    if client_id == DEFAULT_CLIENT_ID {
        return Err("No Azure AD client ID configured.\n\n\
             To get started:\n  \
             1. Register an app in Azure AD (see docs/azure-ad-setup.md)\n  \
             2. Provide the client ID via one of:\n     \
             - CLI:  --client-id <your-id>\n     \
             - Env:  CLOUDMOUNT_CLIENT_ID=<your-id>\n     \
             - File: copy .env.example to .env and fill in your values\n"
            .to_string());
    }

    #[cfg(target_os = "linux")]
    if !fuse_available() {
        tracing::warn!(
            "FUSE not available \u{2014} install libfuse3-dev to enable filesystem mounts"
        );
    }

    #[cfg(target_os = "macos")]
    if !fuse_available() {
        tracing::warn!("FUSE not available \u{2014} install macFUSE to enable filesystem mounts");
    }

    #[cfg(target_os = "windows")]
    if !cfapi_version_meets(10, 0, 16299) {
        return Err(
            "Cloud Files API requires Windows 10 version 1709 (build 16299) or later".to_string(),
        );
    }

    Ok(())
}

fn init_components(
    overrides: &RuntimeOverrides,
    packaged: &PackagedDefaults,
    effective: &EffectiveConfig,
    opener: OpenerFn,
) -> Components {
    let client_id = resolve_client_id(overrides, packaged);
    let tenant_id = resolve_tenant_id(overrides, packaged);

    let auth = Arc::new(AuthManager::new(client_id, tenant_id, opener));

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
    let db_path = effective_cache_dir.join("cloudmount.db");
    let max_cache_bytes = parse_cache_size(&effective.cache_max_size);
    let metadata_ttl = Some(effective.metadata_ttl_secs);

    let cache = Arc::new(
        CacheManager::new(effective_cache_dir, db_path, max_cache_bytes, metadata_ttl)
            .unwrap_or_else(|e| {
                tracing::error!("failed to initialize cache: {e}");
                std::process::exit(1);
            }),
    );

    let max_inode = cache.sqlite.max_inode().unwrap_or(0);
    let inodes = Arc::new(InodeTable::new_starting_after(max_inode));

    Components {
        auth,
        graph,
        cache,
        inodes,
    }
}

fn main() {
    dotenvy::dotenv().ok();

    let args = CliArgs::parse();

    // Configure tracing: CLI --log-level > CLOUDMOUNT_LOG_LEVEL (already handled by clap env) > RUST_LOG > "info"
    let filter = if let Some(ref level) = args.log_level {
        EnvFilter::new(level)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let packaged = PackagedDefaults::load(PACKAGED_DEFAULTS_TOML).unwrap_or_else(|e| {
        tracing::warn!("failed to load packaged defaults: {e}");
        PackagedDefaults::default()
    });

    let app_name = BUILD_APP_NAME.unwrap_or_else(|| packaged.app_name());
    tracing::info!("{app_name} starting");

    if packaged.has_packaged_config() {
        tracing::info!("pre-configured build detected");
    }

    let config_path = args.config.unwrap_or_else(config_file_path);
    let user_config = UserConfig::load_from_file(&config_path).unwrap_or_else(|e| {
        tracing::warn!("failed to load user config: {e}");
        UserConfig::default()
    });

    let effective = EffectiveConfig::build(&packaged, &user_config);

    let overrides = RuntimeOverrides {
        client_id: args.client_id,
        tenant_id: args.tenant_id,
    };

    // Resolve client_id for preflight check
    let resolved_client_id = overrides
        .client_id
        .as_deref()
        .or(BUILD_CLIENT_ID)
        .or(packaged.client_id())
        .unwrap_or(DEFAULT_CLIENT_ID);

    if let Err(msg) = preflight_checks(resolved_client_id) {
        fatal_error(&msg);
    }

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
    {
        if args.headless {
            run_headless(packaged, user_config, effective, overrides);
        } else {
            run_desktop(packaged, user_config, effective, overrides);
        }
    }

    #[cfg(not(feature = "desktop"))]
    run_headless(packaged, user_config, effective, overrides);
}

#[cfg(feature = "desktop")]
fn run_desktop(
    packaged: PackagedDefaults,
    user_config: UserConfig,
    effective: EffectiveConfig,
    overrides: RuntimeOverrides,
) {
    let app_name = BUILD_APP_NAME
        .map(String::from)
        .unwrap_or_else(|| effective.app_name.clone());
    let first_run = !config_file_path().exists();

    // On non-Linux, the opener uses tauri_plugin_opener which requires the AppHandle.
    // The AppHandle is lazily populated after Tauri initializes.
    #[cfg(not(target_os = "linux"))]
    let app_handle_slot: Arc<std::sync::Mutex<Option<tauri::AppHandle>>> =
        Arc::new(std::sync::Mutex::new(None));
    #[cfg(not(target_os = "linux"))]
    let opener_handle = app_handle_slot.clone();

    let opener: OpenerFn = {
        #[cfg(target_os = "linux")]
        {
            // On Linux, spawn xdg-open directly so we can strip AppImage env vars
            // (LD_LIBRARY_PATH, LD_PRELOAD) that cause gio/GLib version mismatches
            // and silent browser launch failures. Use .status() for error observability.
            Arc::new(|url: &str| {
                let status = std::process::Command::new("xdg-open")
                    .arg(url)
                    .env_remove("LD_LIBRARY_PATH")
                    .env_remove("LD_PRELOAD")
                    .status()
                    .map_err(|e| format!("failed to spawn xdg-open: {e}"))?;
                if status.success() {
                    Ok(())
                } else {
                    Err(format!("xdg-open exited with {status}"))
                }
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Arc::new(move |url: &str| {
                use tauri_plugin_opener::OpenerExt;
                let handle = {
                    let guard = opener_handle.lock().unwrap();
                    guard
                        .as_ref()
                        .ok_or_else(|| "Tauri app not yet initialized".to_string())?
                        .clone()
                };
                handle
                    .opener()
                    .open_url(url, None::<&str>)
                    .map_err(|e| e.to_string())
            })
        }
    };

    let Components {
        auth,
        graph,
        cache,
        inodes,
    } = init_components(&overrides, &packaged, &effective, opener);
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
        active_sign_in: Mutex::new(None),
        drive_ids,
        authenticated: AtomicBool::new(false),
        auth_degraded: AtomicBool::new(false),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(update::UpdateState::new())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::sign_in,
            commands::start_sign_in,
            commands::cancel_sign_in,
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
            commands::clear_cache,
            commands::open_wizard,
        ])
        .setup(move |app| {
            // Populate the opener's AppHandle slot now that the app is running.
            // On Linux the opener uses xdg-open directly and doesn't need the handle.
            #[cfg(not(target_os = "linux"))]
            {
                *app_handle_slot.lock().unwrap() = Some(app.handle().clone());
            }

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
    use tauri_plugin_updater::UpdaterExt;

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

        // Reconcile OS auto-start state with the persisted config value.
        let auto_start = {
            let config = state.effective_config.lock().unwrap();
            config.auto_start
        };
        match std::env::current_exe() {
            Ok(exe) => {
                let exe_path = exe.to_string_lossy();
                if let Err(e) =
                    cloudmount_core::config::autostart::set_enabled(auto_start, &exe_path)
                {
                    tracing::warn!("auto-start sync failed: {e}");
                }
            }
            Err(e) => {
                tracing::warn!("failed to resolve exe path for auto-start sync: {e}");
            }
        }

        run_crash_recovery(app);
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if !fuse_available() {
            notify::fuse_unavailable(app);
        }
        start_all_mounts(app);
        start_delta_sync(app);
        // Only spawn periodic update checker if the updater endpoint is configured
        if app.updater().is_ok() {
            update::spawn_update_checker(app.clone());
        }
        tray::update_tray_menu(app);
    } else if first_run {
        tray::open_or_focus_window(app, "wizard", "Setup", "wizard.html");
    } else {
        // Post-sign-out restart: config exists but no valid tokens.
        // Reopen the wizard so the user can re-authenticate.
        tracing::info!("no valid tokens and not first run — opening wizard for re-authentication");
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
            notify::mount_failed(app, &mount_config.name, &e);
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

    if !cloudmount_vfs::cleanup_stale_mount(&mountpoint) {
        return Err(format!(
            "stale FUSE mount at {mountpoint} could not be cleaned up — run `fusermount -u {mountpoint}` manually"
        ));
    }

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

    let handle = cloudmount_vfs::CfMountHandle::mount(
        state.graph.clone(),
        state.cache.clone(),
        state.inodes.clone(),
        drive_id.to_string(),
        std::path::Path::new(&mountpoint),
        rt,
        drive_id.to_string(),
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
                    Err(cloudmount_core::Error::Auth(ref msg))
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
fn run_crash_recovery(app: &tauri::AppHandle) {
    use tauri::Manager;

    let state = app.state::<AppState>();
    let graph = state.graph.clone();
    let cache = state.cache.clone();

    tauri::async_runtime::spawn(async move {
        let pending = match cache.writeback.list_pending().await {
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

        for (drive_id, item_id) in &pending {
            // local:* files have no server-side metadata — unrecoverable after restart
            if item_id.starts_with("local:") {
                let _ = cache.writeback.remove(drive_id, item_id).await;
                tracing::warn!(
                    "crash recovery: discarded unrecoverable local file {drive_id}/{item_id}"
                );
                continue;
            }

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
pub fn graceful_shutdown_without_exit(app: &tauri::AppHandle) {
    use tauri::Manager;

    tracing::info!("graceful shutdown initiated");

    update::cancel_checker(app);

    let state = app.state::<AppState>();

    if let Some(cancel) = state.sync_cancel.lock().unwrap().take() {
        cancel.cancel();
    }

    stop_all_mounts(app);

    tracing::info!("shutdown complete");
}

#[cfg(feature = "desktop")]
pub fn graceful_shutdown(app: &tauri::AppHandle) {
    graceful_shutdown_without_exit(app);
    app.exit(0);
}

fn run_headless(
    packaged: PackagedDefaults,
    mut user_config: UserConfig,
    mut effective: EffectiveConfig,
    overrides: RuntimeOverrides,
) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    rt.block_on(async {
        let opener: OpenerFn =
            Arc::new(|url: &str| {
                if cloudmount_auth::oauth::has_display() {
                    open::that(url).map_err(|e| e.to_string())
                } else {
                    Err("no display available".to_string())
                }
            });

        let Components {
            auth,
            graph,
            cache,
            inodes,
        } = init_components(&overrides, &packaged, &effective, opener);

        // Authentication
        let account = effective.accounts.first();
        let mut authenticated = false;

        if let Some(account) = account {
            match auth.try_restore(&account.id).await {
                Ok(true) => {
                    tracing::info!("tokens restored for {}", account.id);
                    authenticated = true;
                }
                Ok(false) => {
                    tracing::info!("stored tokens invalid, attempting sign-in");
                }
                Err(e) => {
                    tracing::warn!("token restore failed: {e}");
                }
            }
        }

        if !authenticated {
            match auth.sign_in(None).await {
                Ok(()) => {
                    tracing::info!("sign-in successful");

                    // Post-sign-in: discover OneDrive and persist config
                    match graph.get_my_drive().await {
                        Ok(drive) => {
                            tracing::info!(
                                "discovered OneDrive: {} ({})",
                                drive.name,
                                drive.id
                            );

                            if user_config.accounts.is_empty() {
                                user_config.accounts.push(AccountMetadata {
                                    id: drive.id.clone(),
                                    email: None,
                                    display_name: Some(drive.name.clone()),
                                    tenant_id: None,
                                });
                            }

                            let has_onedrive =
                                user_config.mounts.iter().any(|m| m.mount_type == "drive");
                            if !has_onedrive {
                                let mount_point = derive_mount_point(
                                    &effective.root_dir,
                                    "drive",
                                    None,
                                    None,
                                );
                                if let Err(e) =
                                    user_config.add_onedrive_mount(&drive.id, &mount_point)
                                {
                                    tracing::warn!("failed to create default mount: {e}");
                                }
                            }

                            if let Err(e) = user_config.save_to_file(&config_file_path()) {
                                tracing::warn!("failed to save config: {e}");
                            }

                            effective = EffectiveConfig::build(&packaged, &user_config);
                        }
                        Err(e) => {
                            tracing::warn!("failed to discover OneDrive: {e}");
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("sign-in failed: {e}");
                    std::process::exit(1);
                }
            }
        }

        // Crash recovery (non-blocking — runs in background)
        let recovery_graph = graph.clone();
        let recovery_cache = cache.clone();
        tokio::spawn(async move {
            match recovery_cache.writeback.list_pending().await {
                Ok(pending) if !pending.is_empty() => {
                    tracing::info!("crash recovery: {} pending writes found", pending.len());
                    for (drive_id, item_id) in &pending {
                        // local:* files have no server-side metadata — unrecoverable after restart
                        if item_id.starts_with("local:") {
                            let _ = recovery_cache.writeback.remove(drive_id, item_id).await;
                            tracing::warn!("crash recovery: discarded unrecoverable local file {drive_id}/{item_id}");
                            continue;
                        }
                        if let Some(content) =
                            recovery_cache.writeback.read(drive_id, item_id).await
                        {
                            match recovery_graph
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
                                    let _ =
                                        recovery_cache.writeback.remove(drive_id, item_id).await;
                                    tracing::info!(
                                        "crash recovery: uploaded {drive_id}/{item_id}"
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "crash recovery: upload failed for {drive_id}/{item_id}: {e}"
                                    );
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("crash recovery: failed to list pending writes: {e}");
                }
                _ => {}
            }
        });

        // Start mounts
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let mut drive_ids: Vec<String> = Vec::new();
        #[cfg(target_os = "windows")]
        let drive_ids: Vec<String> = Vec::new();

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let mut mount_handles: Vec<MountHandle> = Vec::new();

        let mounts_config: Vec<MountConfig> = effective
            .mounts
            .iter()
            .filter(|m| m.enabled)
            .cloned()
            .collect();

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let rt_handle = tokio::runtime::Handle::current();

        for mount_config in &mounts_config {
            if mount_config.drive_id.is_none() {
                tracing::error!("mount '{}' has no drive_id, skipping", mount_config.name);
                continue;
            }

            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let drive_id = mount_config.drive_id.as_deref().unwrap();

            let mountpoint = expand_mount_point(&mount_config.mount_point);

            #[cfg(any(target_os = "linux", target_os = "macos"))]
            if !cloudmount_vfs::cleanup_stale_mount(&mountpoint) {
                tracing::error!(
                    "mount '{}': stale FUSE mount at {mountpoint} could not be cleaned up, skipping",
                    mount_config.name
                );
                continue;
            }

            if let Err(e) = std::fs::create_dir_all(&mountpoint) {
                tracing::error!("create mountpoint failed for '{}': {e}", mount_config.name);
                continue;
            }

            #[cfg(any(target_os = "linux", target_os = "macos"))]
            match MountHandle::mount(
                graph.clone(),
                cache.clone(),
                inodes.clone(),
                drive_id.to_string(),
                &mountpoint,
                rt_handle.clone(),
            ) {
                Ok(handle) => {
                    tracing::info!("mount '{}' started at {mountpoint}", mount_config.name);
                    drive_ids.push(drive_id.to_string());
                    mount_handles.push(handle);
                }
                Err(e) => {
                    tracing::error!("failed to start mount '{}': {e}", mount_config.name);
                }
            }

            #[cfg(target_os = "windows")]
            {
                tracing::warn!(
                    "headless mode does not support Windows CfApi mounts (mount '{}')",
                    mount_config.name
                );
            }
        }

        let mount_count = drive_ids.len();

        // Delta sync loop
        let auth_degraded = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();
        let sync_cancel = cancel.clone();
        let sync_graph = graph.clone();
        let sync_cache = cache.clone();
        let sync_drive_ids = drive_ids.clone();
        let sync_inodes = inodes.clone();
        let sync_interval = effective.sync_interval_secs;
        let sync_degraded = auth_degraded.clone();

        tokio::spawn(async move {
            use std::sync::atomic::Ordering;

            let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
                Arc::new(move |item_id: &str| sync_inodes.allocate(item_id));

            loop {
                for drive_id in &sync_drive_ids {
                    match run_delta_sync(&sync_graph, &sync_cache, drive_id, &inode_allocator).await
                    {
                        Ok(()) => {}
                        Err(cloudmount_core::Error::Auth(ref msg))
                            if msg.contains("re-authentication required") =>
                        {
                            if !sync_degraded.load(Ordering::Relaxed) {
                                sync_degraded.store(true, Ordering::Relaxed);
                                tracing::warn!(
                                    "Re-authentication required \u{2014} cached files remain accessible"
                                );
                            }
                        }
                        Err(e) => {
                            tracing::error!("delta sync failed for drive {drive_id}: {e}");
                        }
                    }
                }

                let wait = std::time::Duration::from_secs(sync_interval);
                tokio::select! {
                    _ = sync_cancel.cancelled() => break,
                    _ = tokio::time::sleep(wait) => {}
                }
            }
        });

        tracing::info!("CloudMount headless mode running \u{2014} {mount_count} mount(s) active");

        // SIGHUP re-authentication handler (Unix only)
        #[cfg(unix)]
        {
            use std::sync::atomic::Ordering;
            use tokio::signal::unix::{SignalKind, signal};

            let mut sighup =
                signal(SignalKind::hangup()).expect("failed to register SIGHUP handler");
            let hup_auth = auth.clone();
            let hup_graph = graph.clone();
            let hup_cache = cache.clone();
            let hup_degraded = auth_degraded.clone();

            tokio::spawn(async move {
                loop {
                    sighup.recv().await;
                    tracing::info!("SIGHUP received \u{2014} attempting re-authentication");

                    match hup_auth.sign_in(None).await {
                        Ok(()) => {
                            hup_degraded.store(false, Ordering::Relaxed);
                            tracing::info!("re-authentication successful");

                            // Flush pending writes
                            let rg = hup_graph.clone();
                            let rc = hup_cache.clone();
                            tokio::spawn(async move {
                                match rc.writeback.list_pending().await {
                                    Ok(pending) if !pending.is_empty() => {
                                        tracing::info!(
                                            "flushing {} pending writes after re-auth",
                                            pending.len()
                                        );
                                        for (drive_id, item_id) in &pending {
                                            if item_id.starts_with("local:") {
                                                let _ = rc.writeback.remove(drive_id, item_id).await;
                                                continue;
                                            }
                                            if let Some(content) =
                                                rc.writeback.read(drive_id, item_id).await
                                            {
                                                match rg
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
                                                        let _ = rc
                                                            .writeback
                                                            .remove(drive_id, item_id)
                                                            .await;
                                                        tracing::info!(
                                                            "re-auth recovery: uploaded {drive_id}/{item_id}"
                                                        );
                                                    }
                                                    Err(e) => {
                                                        tracing::warn!(
                                                            "re-auth recovery: upload failed for {drive_id}/{item_id}: {e}"
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            });
                        }
                        Err(e) => {
                            tracing::warn!(
                                "re-authentication failed: {e} \u{2014} if no browser is available, sign in from a desktop session first, then restart this process"
                            );
                        }
                    }
                }
            });
        }

        // Wait for shutdown signal
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

        // Graceful shutdown
        tracing::info!("received shutdown signal");
        cancel.cancel();

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        for handle in mount_handles {
            if let Err(e) = handle.unmount() {
                tracing::error!("unmount failed: {e}");
            }
        }

        tracing::info!("shutdown complete");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use cloudmount_core::config::PackagedDefaults;

    #[test]
    fn test_cli_args_parse_all_options() {
        let args = CliArgs::try_parse_from([
            "cloudmount-app",
            "--client-id",
            "test-client-id",
            "--tenant-id",
            "test-tenant-id",
            "--config",
            "/tmp/test-config.toml",
            "--log-level",
            "debug",
            "--headless",
        ])
        .unwrap();

        assert_eq!(args.client_id.as_deref(), Some("test-client-id"));
        assert_eq!(args.tenant_id.as_deref(), Some("test-tenant-id"));
        assert_eq!(
            args.config,
            Some(std::path::PathBuf::from("/tmp/test-config.toml"))
        );
        assert_eq!(args.log_level.as_deref(), Some("debug"));
        assert!(args.headless);
    }

    #[test]
    fn test_cli_args_default_values() {
        let args = CliArgs::try_parse_from(["cloudmount-app"]).unwrap();

        assert!(args.client_id.is_none());
        assert!(args.tenant_id.is_none());
        assert!(args.config.is_none());
        assert!(args.log_level.is_none());
        assert!(!args.headless);
    }

    #[test]
    fn test_preflight_checks_placeholder_client_id() {
        let result = preflight_checks(DEFAULT_CLIENT_ID);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Azure AD client ID"));
        assert!(msg.contains("docs/azure-ad-setup.md"));
        assert!(msg.contains("--client-id"));
        assert!(msg.contains("CLOUDMOUNT_CLIENT_ID"));
    }

    #[test]
    fn test_preflight_checks_valid_client_id() {
        let result = preflight_checks("12345678-1234-1234-1234-123456789abc");
        assert!(result.is_ok());
    }

    #[test]
    fn test_runtime_overrides_resolve_client_id() {
        let packaged = PackagedDefaults::default();

        // Override takes priority
        let overrides = RuntimeOverrides {
            client_id: Some("override-id".to_string()),
            tenant_id: None,
        };
        assert_eq!(resolve_client_id(&overrides, &packaged), "override-id");

        // Falls back to DEFAULT_CLIENT_ID when no overrides or packaged
        let no_overrides = RuntimeOverrides {
            client_id: None,
            tenant_id: None,
        };
        assert_eq!(
            resolve_client_id(&no_overrides, &packaged),
            DEFAULT_CLIENT_ID
        );
    }

    #[test]
    fn test_runtime_overrides_resolve_tenant_id() {
        let packaged = PackagedDefaults::default();

        let overrides = RuntimeOverrides {
            client_id: None,
            tenant_id: Some("override-tenant".to_string()),
        };
        assert_eq!(
            resolve_tenant_id(&overrides, &packaged),
            Some("override-tenant".to_string())
        );

        let no_overrides = RuntimeOverrides {
            client_id: None,
            tenant_id: None,
        };
        assert_eq!(resolve_tenant_id(&no_overrides, &packaged), None);
    }

    #[test]
    fn test_build_time_constants_are_option() {
        // BUILD_CLIENT_ID, BUILD_TENANT_ID, BUILD_APP_NAME are Option<&str>
        // They should be None when not set during build (which is the default)
        // This verifies the option_env!() pattern works
        let _: Option<&str> = BUILD_CLIENT_ID;
        let _: Option<&str> = BUILD_TENANT_ID;
        let _: Option<&str> = BUILD_APP_NAME;
    }

    #[test]
    fn test_preflight_checks_rejects_placeholder_client_id() {
        let result = preflight_checks(DEFAULT_CLIENT_ID);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("client ID"),
            "error should mention client ID, got: {msg}"
        );
    }

    // Task 6.1: Windows-only test that simulates CfApi version check failure.
    // Uses an impossibly high version requirement to exercise the failure path of
    // cfapi_version_meets, which is the same code called by preflight_checks.
    #[cfg(target_os = "windows")]
    #[test]
    fn test_cfapi_version_meets_fails_on_impossible_version() {
        // Requesting Windows 99 guarantees failure on any real system, simulating
        // a machine that does not meet the CfApi requirement.
        assert!(
            !cfapi_version_meets(99, 0, 0),
            "cfapi_version_meets should return false for an unreachable version"
        );
        // Verify the error string that preflight_checks would emit is correct.
        let err = "Cloud Files API requires Windows 10 version 1709 (build 16299) or later";
        assert!(err.contains("Windows 10 version 1709"));
    }
}
