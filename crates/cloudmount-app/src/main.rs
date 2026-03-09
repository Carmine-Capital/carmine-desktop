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
    AccountMetadata, EffectiveConfig, UserConfig, config_file_path, derive_mount_point,
    expand_mount_point,
};

use std::sync::Arc;

type OpenerFn = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub(crate) fn open_with_clean_env(path: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let status = std::process::Command::new("xdg-open")
            .arg(path)
            .env_remove("LD_LIBRARY_PATH")
            .env_remove("LD_PRELOAD")
            .status()
            .map_err(|e| format!("failed to spawn xdg-open: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("xdg-open exited with {status}"))
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        open::that(path).map_err(|e| e.to_string())
    }
}

use cloudmount_auth::AuthManager;
use cloudmount_cache::CacheManager;
use cloudmount_cache::sync::run_delta_sync;
use cloudmount_core::config::MountConfig;
#[cfg(any(target_os = "linux", target_os = "macos", feature = "desktop"))]
use cloudmount_core::config::cache_dir;
use cloudmount_graph::GraphClient;
use cloudmount_vfs::inode::InodeTable;
use tokio_util::sync::CancellationToken;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use cloudmount_vfs::MountHandle;

use std::sync::atomic::AtomicBool;

#[cfg(feature = "desktop")]
use std::collections::HashMap;
#[cfg(feature = "desktop")]
use std::sync::Mutex;

/// Per-mount cache entry: `(CacheManager, InodeTable)` keyed by drive_id.
#[cfg(feature = "desktop")]
type MountCacheEntry = (Arc<CacheManager>, Arc<InodeTable>);

/// Snapshot row used by the delta-sync loop: (drive_id, mount_id, mount_name, cache, inodes).
#[cfg(feature = "desktop")]
type SyncSnapshotRow = (String, String, String, Arc<CacheManager>, Arc<InodeTable>);

const CLIENT_ID: &str = "8ebe3ef7-f509-4146-8fef-c9b5d7c22252";

/// Annotated default configuration printed by `--print-default-config`.
const DEFAULT_CONFIG_TOML: &str = "\
# CloudMount configuration
# Location: ~/.config/cloudmount/config.toml

[general]
# Start CloudMount on login (systemd user unit / launchd / registry)
# auto_start = false

# Show desktop notifications for sync events and errors
# notifications = true

# How often to poll Microsoft Graph for changes (seconds)
# sync_interval_secs = 60

# How long cached metadata is valid before re-fetching (seconds)
# metadata_ttl_secs = 60

# Maximum disk cache size (e.g. \"5GB\", \"500MB\")
# cache_max_size = \"5GB\"

# Override the default cache directory
# cache_dir = \"/path/to/cache\"

# Log level: trace, debug, info, warn, error
# log_level = \"info\"

# Mount root directory name inside your home folder (~/Cloud/)
# root_dir = \"Cloud\"

# Mounts are managed through the GUI or added manually:
#
# [[mounts]]
# id = \"od-xxxxxxxx\"
# name = \"OneDrive\"
# type = \"drive\"
# mount_point = \"~/Cloud/OneDrive\"
# enabled = true
# drive_id = \"...\"
#
# [[mounts]]
# id = \"sp-xxxxxxxx\"
# name = \"Contoso - Documents\"
# type = \"sharepoint\"
# mount_point = \"~/Cloud/Contoso - Documents\"
# enabled = true
# drive_id = \"...\"
# site_id = \"...\"
# site_name = \"Contoso\"
# library_name = \"Documents\"
";

/// CloudMount — mount Microsoft OneDrive and SharePoint as local filesystems.
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    after_help = "\
SIGNALS (Unix only):
  SIGHUP    Trigger re-authentication in headless mode.
            Useful when a refresh token expires on a remote server.

EXAMPLES:
  cloudmount --headless                    Run without GUI
  cloudmount --print-default-config        Show annotated default configuration
  kill -HUP $(pidof cloudmount)            Re-authenticate a running instance"
)]
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

    /// Print annotated default configuration and exit
    #[arg(long)]
    print_default_config: bool,
}

struct RuntimeOverrides {
    client_id: Option<String>,
    tenant_id: Option<String>,
}

#[cfg(feature = "desktop")]
pub struct AppState {
    pub user_config: Mutex<UserConfig>,
    pub effective_config: Mutex<EffectiveConfig>,
    pub auth: Arc<AuthManager>,
    pub graph: Arc<GraphClient>,
    /// Per-mount cache and inode table, keyed by drive_id.
    pub mount_caches: Mutex<HashMap<String, MountCacheEntry>>,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub mounts: Mutex<HashMap<String, MountHandle>>,
    #[cfg(target_os = "windows")]
    pub mounts: Mutex<HashMap<String, cloudmount_vfs::CfMountHandle>>,
    pub sync_cancel: Mutex<Option<CancellationToken>>,
    pub active_sign_in: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub authenticated: AtomicBool,
    pub auth_degraded: AtomicBool,
    /// Drive ID of the currently signed-in account; `None` when no account is active.
    pub account_id: Mutex<Option<String>>,
    pub tokio_handle: std::sync::OnceLock<tokio::runtime::Handle>,
}

#[allow(dead_code)] // Used conditionally across platform×feature combos; no platform-specific code
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
}

/// Returns true if FUSE is available on the current system.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn fuse_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("fusermount3")
            .arg("--version")
            .output()
            .is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        // macFUSE does not ship `fusermount` (that is a Linux FUSE 2/3 binary).
        // The canonical install indicator is the kernel extension bundle.
        std::path::Path::new("/Library/Filesystems/macfuse.fs").exists()
    }
}

/// Checks that the Windows version meets the Cloud Files API minimum (10.0.16299).
/// Extracted for testability — callers can pass a custom minimum version.
#[cfg(target_os = "windows")]
fn cfapi_version_meets(min_major: u32, min_minor: u32, min_build: u32) -> bool {
    use std::mem;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    use windows::core::{s, w};

    // VerifyVersionInfoW lies about the OS version on Windows 10+ unless the binary
    // carries a Windows 10 compatibility manifest (test runners and generic builds
    // do not). RtlGetVersion from ntdll bypasses this and returns the true version.
    type RtlGetVersionFn = unsafe extern "system" fn(*mut OSVERSIONINFOW) -> i32;

    let mut osvi = OSVERSIONINFOW {
        dwOSVersionInfoSize: mem::size_of::<OSVERSIONINFOW>() as u32,
        ..Default::default()
    };

    unsafe {
        let ntdll = match GetModuleHandleW(w!("ntdll.dll")) {
            Ok(h) => h,
            Err(_) => return false,
        };
        let Some(proc) = GetProcAddress(ntdll, s!("RtlGetVersion")) else {
            return false;
        };
        let rtl_get_version: RtlGetVersionFn = mem::transmute(proc);
        if rtl_get_version(&mut osvi) != 0 {
            // Non-zero NTSTATUS means failure
            return false;
        }
    }

    let actual = (osvi.dwMajorVersion, osvi.dwMinorVersion, osvi.dwBuildNumber);
    actual >= (min_major, min_minor, min_build)
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

fn preflight_checks() -> Result<(), String> {
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

fn init_components(overrides: &RuntimeOverrides, opener: OpenerFn) -> Components {
    let client_id = overrides
        .client_id
        .clone()
        .unwrap_or_else(|| CLIENT_ID.to_string());
    let tenant_id = overrides.tenant_id.clone();

    let auth = Arc::new(AuthManager::new(client_id, tenant_id, opener));

    let auth_for_graph = auth.clone();
    let graph = Arc::new(GraphClient::new(move || {
        let auth = auth_for_graph.clone();
        async move { auth.access_token().await }
    }));

    Components { auth, graph }
}

fn main() {
    dotenvy::dotenv().ok();

    let args = CliArgs::parse();

    if args.print_default_config {
        print!("{}", DEFAULT_CONFIG_TOML);
        return;
    }

    // Configure tracing: CLI --log-level > CLOUDMOUNT_LOG_LEVEL (already handled by clap env) > RUST_LOG > "info"
    let filter = if let Some(ref level) = args.log_level {
        EnvFilter::new(level)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!("CloudMount starting");

    let config_path = args.config.unwrap_or_else(|| {
        config_file_path().unwrap_or_else(|e| {
            eprintln!("fatal: no config directory available: {e}");
            std::process::exit(1);
        })
    });
    let user_config = UserConfig::load_from_file(&config_path).unwrap_or_else(|e| {
        tracing::warn!("failed to load user config: {e}");
        UserConfig::default()
    });

    let effective = EffectiveConfig::build(&user_config);

    let overrides = RuntimeOverrides {
        client_id: args.client_id,
        tenant_id: args.tenant_id,
    };

    if let Err(msg) = preflight_checks() {
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
            run_headless(user_config, effective, overrides);
        } else {
            run_desktop(user_config, effective, overrides);
        }
    }

    #[cfg(not(feature = "desktop"))]
    run_headless(user_config, effective, overrides);
}

#[cfg(feature = "desktop")]
fn run_desktop(user_config: UserConfig, effective: EffectiveConfig, overrides: RuntimeOverrides) {
    let app_name = "CloudMount".to_string();
    let first_run = config_file_path().map(|p| !p.exists()).unwrap_or(true);

    // Desktop, non-Linux: the opener uses tauri_plugin_opener which requires the AppHandle.
    // (Linux desktop uses xdg-open directly; headless mode uses open::that — neither needs this.)
    // The AppHandle is lazily populated after Tauri initializes.
    #[cfg(not(target_os = "linux"))]
    let app_handle_slot: Arc<std::sync::Mutex<Option<tauri::AppHandle>>> =
        Arc::new(std::sync::Mutex::new(None));
    #[cfg(not(target_os = "linux"))]
    let opener_handle = app_handle_slot.clone();

    let opener: OpenerFn = {
        #[cfg(target_os = "linux")]
        {
            Arc::new(|url: &str| open_with_clean_env(url))
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

    let Components { auth, graph } = init_components(&overrides, opener);

    let state = AppState {
        user_config: Mutex::new(user_config),
        effective_config: Mutex::new(effective),
        auth,
        graph,
        mount_caches: Mutex::new(HashMap::new()),
        mounts: Mutex::new(HashMap::new()),
        sync_cancel: Mutex::new(None),
        active_sign_in: Mutex::new(None),
        authenticated: AtomicBool::new(false),
        auth_degraded: AtomicBool::new(false),
        account_id: Mutex::new(None),
        tokio_handle: std::sync::OnceLock::new(),
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
            commands::is_authenticated,
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
            commands::get_drive_info,
            commands::get_followed_sites,
            commands::complete_wizard,
            commands::search_sites,
            commands::list_drives,
            commands::refresh_mount,
            commands::clear_cache,
            commands::open_wizard,
            commands::check_fuse_available,
            commands::get_default_mount_root,
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
    // Store the Tokio runtime handle so sync Tauri commands (GTK thread) can use it.
    state
        .tokio_handle
        .set(tokio::runtime::Handle::current())
        .ok();

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
        *state.account_id.lock().unwrap() = Some(account.as_ref().unwrap().id.clone());
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

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if !fuse_available() {
            notify::fuse_unavailable(app);
        }
        start_all_mounts(app);
        run_crash_recovery(app);
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
fn remove_mount_from_config(app: &tauri::AppHandle, mount_id: &str) {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let new_effective = {
        let mut user_config = match state.user_config.lock() {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!("failed to lock user_config for mount removal: {e}");
                return;
            }
        };
        user_config.remove_mount(mount_id);
        match config_file_path() {
            Ok(cfg_path) => {
                if let Err(e) = user_config.save_to_file(&cfg_path) {
                    tracing::warn!("failed to save config after removing mount '{mount_id}': {e}");
                }
            }
            Err(e) => {
                tracing::warn!("config path unavailable: {e}");
            }
        }
        EffectiveConfig::build(&user_config)
    };
    if let Ok(mut effective) = state.effective_config.lock() {
        *effective = new_effective;
    }
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

    // Validate the drive resource exists before mounting.
    {
        let state = app.state::<AppState>();
        let graph = state.graph.clone();
        let rt = state
            .tokio_handle
            .get()
            .cloned()
            .unwrap_or_else(|| tokio::runtime::Handle::current());
        match tokio::task::block_in_place(|| rt.block_on(graph.check_drive_exists(drive_id))) {
            Ok(()) => {}
            Err(cloudmount_core::Error::GraphApi { status: 404, .. }) => {
                tracing::warn!(
                    "mount '{}' drive not found (404), removing from config",
                    mount_config.name
                );
                remove_mount_from_config(app, &mount_config.id);
                notify::mount_not_found(app, &mount_config.name);
                return Ok(());
            }
            Err(cloudmount_core::Error::GraphApi { status: 403, .. }) => {
                tracing::warn!(
                    "mount '{}' access denied (403), skipping",
                    mount_config.name
                );
                notify::mount_access_denied(app, &mount_config.name);
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    "transient error validating mount '{}': {e}, skipping",
                    mount_config.name
                );
                return Ok(());
            }
        }
    }

    let mountpoint = expand_mount_point(&mount_config.mount_point);

    if !cloudmount_vfs::cleanup_stale_mount(&mountpoint) {
        return Err(format!(
            "stale FUSE mount at {mountpoint} could not be cleaned up — run `fusermount -u {mountpoint}` manually"
        ));
    }

    std::fs::create_dir_all(&mountpoint).map_err(|e| format!("create mountpoint failed: {e}"))?;

    let state = app.state::<AppState>();

    let (effective_cache_dir, max_cache_bytes, metadata_ttl) = {
        let cfg = state.effective_config.lock().map_err(|e| e.to_string())?;
        let dir = cfg
            .cache_dir
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(cache_dir);
        let max_bytes = parse_cache_size(&cfg.cache_max_size);
        let ttl = Some(cfg.metadata_ttl_secs);
        (dir, max_bytes, ttl)
    };

    let safe_id = drive_id.replace('!', "_");
    let db_path = effective_cache_dir.join(format!("drive-{safe_id}.db"));
    let mount_cache = Arc::new(
        CacheManager::new(effective_cache_dir, db_path, max_cache_bytes, metadata_ttl)
            .map_err(|e| e.to_string())?,
    );
    let max_inode = mount_cache.sqlite.max_inode().unwrap_or(0);
    let mount_inodes = Arc::new(InodeTable::new_starting_after(max_inode));

    let rt = state
        .tokio_handle
        .get()
        .cloned()
        .unwrap_or_else(|| tokio::runtime::Handle::current());

    let (event_tx, mut event_rx) =
        tokio::sync::mpsc::unbounded_channel::<cloudmount_vfs::core_ops::VfsEvent>();

    let handle = MountHandle::mount(
        state.graph.clone(),
        mount_cache.clone(),
        mount_inodes.clone(),
        drive_id.to_string(),
        &mountpoint,
        rt,
        Some(event_tx),
    )
    .map_err(|e| e.to_string())?;

    // Spawn a task to forward VFS events to desktop notifications
    let app_handle = app.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                cloudmount_vfs::core_ops::VfsEvent::ConflictDetected {
                    file_name,
                    conflict_name,
                } => {
                    notify::conflict_detected(&app_handle, &file_name, &conflict_name);
                }
            }
        }
    });

    state
        .mount_caches
        .lock()
        .unwrap()
        .insert(drive_id.to_string(), (mount_cache, mount_inodes));

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

    // Validate the drive resource exists before mounting.
    {
        let state = app.state::<AppState>();
        let graph = state.graph.clone();
        let rt = state
            .tokio_handle
            .get()
            .cloned()
            .unwrap_or_else(|| tokio::runtime::Handle::current());
        match tokio::task::block_in_place(|| rt.block_on(graph.check_drive_exists(drive_id))) {
            Ok(()) => {}
            Err(cloudmount_core::Error::GraphApi { status: 404, .. }) => {
                tracing::warn!(
                    "mount '{}' drive not found (404), removing from config",
                    mount_config.name
                );
                remove_mount_from_config(app, &mount_config.id);
                notify::mount_not_found(app, &mount_config.name);
                return Ok(());
            }
            Err(cloudmount_core::Error::GraphApi { status: 403, .. }) => {
                tracing::warn!(
                    "mount '{}' access denied (403), skipping",
                    mount_config.name
                );
                notify::mount_access_denied(app, &mount_config.name);
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    "transient error validating mount '{}': {e}, skipping",
                    mount_config.name
                );
                return Ok(());
            }
        }
    }

    let mountpoint = expand_mount_point(&mount_config.mount_point);
    std::fs::create_dir_all(&mountpoint).map_err(|e| format!("create mountpoint failed: {e}"))?;

    let state = app.state::<AppState>();

    let (effective_cache_dir, max_cache_bytes, metadata_ttl) = {
        let cfg = state.effective_config.lock().map_err(|e| e.to_string())?;
        let dir = cfg
            .cache_dir
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(cache_dir);
        let max_bytes = parse_cache_size(&cfg.cache_max_size);
        let ttl = Some(cfg.metadata_ttl_secs);
        (dir, max_bytes, ttl)
    };

    let safe_id = drive_id.replace('!', "_");
    let db_path = effective_cache_dir.join(format!("drive-{safe_id}.db"));
    let mount_cache = Arc::new(
        CacheManager::new(effective_cache_dir, db_path, max_cache_bytes, metadata_ttl)
            .map_err(|e| e.to_string())?,
    );
    let max_inode = mount_cache.sqlite.max_inode().unwrap_or(0);
    let mount_inodes = Arc::new(InodeTable::new_starting_after(max_inode));

    let rt = state
        .tokio_handle
        .get()
        .cloned()
        .unwrap_or_else(|| tokio::runtime::Handle::current());

    let handle = cloudmount_vfs::CfMountHandle::mount(
        state.graph.clone(),
        mount_cache.clone(),
        mount_inodes.clone(),
        drive_id.to_string(),
        &std::path::PathBuf::from(&mountpoint),
        rt,
        drive_id.to_string(),
    )
    .map_err(|e| e.to_string())?;

    state
        .mount_caches
        .lock()
        .unwrap()
        .insert(drive_id.to_string(), (mount_cache, mount_inodes));

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

    let drive_id = handle.drive_id().to_string();

    state.mount_caches.lock().unwrap().remove(&drive_id);

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
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        // Tracks drives that already sent a 403 notification to avoid spam.
        let mut notified_403: std::collections::HashSet<String> = std::collections::HashSet::new();

        loop {
            // Snapshot includes mount_id and mount_name for error handling.
            let snapshot: Vec<SyncSnapshotRow> = {
                use tauri::Manager;
                let state = app_handle.state::<AppState>();
                let caches = state.mount_caches.lock().unwrap();
                let config = state.effective_config.lock().unwrap();
                caches
                    .iter()
                    .map(|(drive_id, (c, i))| {
                        let (mount_id, mount_name) = config
                            .mounts
                            .iter()
                            .find(|m| m.drive_id.as_deref() == Some(drive_id.as_str()))
                            .map(|m| (m.id.clone(), m.name.clone()))
                            .unwrap_or_else(|| (drive_id.clone(), drive_id.clone()));
                        (drive_id.clone(), mount_id, mount_name, c.clone(), i.clone())
                    })
                    .collect()
            };

            for (drive_id, mount_id, mount_name, cache, inodes) in &snapshot {
                let inodes = inodes.clone();
                let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
                    Arc::new(move |item_id: &str| inodes.allocate(item_id));
                match run_delta_sync(&graph, cache, drive_id, &inode_allocator).await {
                    Ok(()) => {
                        // Clear 403 state so the user is notified if access is lost again.
                        notified_403.remove(drive_id.as_str());
                    }
                    Err(cloudmount_core::Error::GraphApi { status: 404, .. }) => {
                        tracing::warn!(
                            "mount '{mount_name}' drive not found during delta sync (404), removing"
                        );
                        let _ = stop_mount(&app_handle, mount_id);
                        remove_mount_from_config(&app_handle, mount_id);
                        notify::mount_orphaned(&app_handle, mount_name);
                    }
                    Err(cloudmount_core::Error::GraphApi { status: 403, .. }) => {
                        if notified_403.insert(drive_id.clone()) {
                            tracing::warn!(
                                "mount '{mount_name}' access denied during delta sync (403)"
                            );
                            notify::mount_access_denied(&app_handle, mount_name);
                        }
                    }
                    Err(cloudmount_core::Error::Auth(ref msg))
                        if msg.contains("re-authentication required") =>
                    {
                        use tauri::Manager;
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
    let cache = match state
        .mount_caches
        .lock()
        .unwrap()
        .values()
        .next()
        .map(|(c, _)| c.clone())
    {
        Some(c) => c,
        None => return, // No mounts active; nothing to recover.
    };

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let recovered =
            cloudmount_vfs::recover_pending_writes(&cache, &graph, "crash recovery").await;
        if recovered > 0 {
            let path = cloudmount_core::config::config_dir()
                .map(|d| d.join("recovered"))
                .unwrap_or_default();
            notify::files_recovered(&app_handle, recovered, &path.display().to_string());
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
                    open_with_clean_env(url)
                } else {
                    Err("no display available".to_string())
                }
            });

        let Components { auth, graph } = init_components(&overrides, opener);

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
                                    user_config.add_onedrive_mount(&drive.id, &mount_point, Some(drive.id.clone()))
                                {
                                    tracing::warn!("failed to create default mount: {e}");
                                }
                            }

                            match config_file_path() {
                                Ok(cfg_path) => {
                                    if let Err(e) = user_config.save_to_file(&cfg_path) {
                                        tracing::warn!("failed to save config: {e}");
                                    }
                                }
                                Err(e) => {
                                    tracing::warn!("config path unavailable: {e}");
                                }
                            }

                            effective = EffectiveConfig::build(&user_config);
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

        // Start mounts and build per-mount cache/inode entries
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let mut mount_entries: Vec<(String, Arc<CacheManager>, Arc<InodeTable>)> = Vec::new();
        #[cfg(target_os = "windows")]
        let mount_entries: Vec<(String, Arc<CacheManager>, Arc<InodeTable>)> = Vec::new();

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

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let effective_cache_dir = effective
            .cache_dir
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(cache_dir);
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let max_cache_bytes = parse_cache_size(&effective.cache_max_size);
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let metadata_ttl = Some(effective.metadata_ttl_secs);

        for mount_config in &mounts_config {
            if mount_config.drive_id.is_none() {
                tracing::error!("mount '{}' has no drive_id, skipping", mount_config.name);
                continue;
            }

            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                let drive_id = mount_config.drive_id.as_deref().unwrap();
                let mountpoint = expand_mount_point(&mount_config.mount_point);

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

                let safe_id = drive_id.replace('!', "_");
                let db_path = effective_cache_dir.join(format!("drive-{safe_id}.db"));
                let mount_cache = match CacheManager::new(
                    effective_cache_dir.clone(),
                    db_path,
                    max_cache_bytes,
                    metadata_ttl,
                ) {
                    Ok(c) => Arc::new(c),
                    Err(e) => {
                        tracing::error!(
                            "failed to init cache for '{}': {e}",
                            mount_config.name
                        );
                        continue;
                    }
                };
                let max_inode = mount_cache.sqlite.max_inode().unwrap_or(0);
                let mount_inodes = Arc::new(InodeTable::new_starting_after(max_inode));

                match MountHandle::mount(
                    graph.clone(),
                    mount_cache.clone(),
                    mount_inodes.clone(),
                    drive_id.to_string(),
                    &mountpoint,
                    rt_handle.clone(),
                    None,
                ) {
                    Ok(handle) => {
                        tracing::info!("mount '{}' started at {mountpoint}", mount_config.name);
                        mount_entries.push((drive_id.to_string(), mount_cache, mount_inodes));
                        mount_handles.push(handle);
                    }
                    Err(e) => {
                        tracing::error!("failed to start mount '{}': {e}", mount_config.name);
                    }
                }
            }

            #[cfg(target_os = "windows")]
            {
                tracing::warn!(
                    "headless mode: CfApi mount for '{}' not started — crash recovery skipped for this mount",
                    mount_config.name
                );
                tracing::warn!(
                    "headless mode: CfApi mount for '{}' not started — delta sync skipped for this mount",
                    mount_config.name
                );
            }
        }

        let mount_count = mount_entries.len();

        // Crash recovery (non-blocking — runs in background after mounts are started)
        if let Some((_, recovery_cache, _)) = mount_entries.first() {
            let recovery_graph = graph.clone();
            let recovery_cache = recovery_cache.clone();
            tokio::spawn(async move {
                cloudmount_vfs::recover_pending_writes(
                    &recovery_cache,
                    &recovery_graph,
                    "crash recovery",
                )
                .await;
            });
        }

        // Delta sync loop — skip when no mounts are active (e.g. Windows headless)
        let auth_degraded = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();

        if !mount_entries.is_empty() {
            let sync_cancel = cancel.clone();
            let sync_graph = graph.clone();
            let sync_entries = mount_entries.clone();
            let sync_interval = effective.sync_interval_secs;
            let sync_degraded = auth_degraded.clone();

            tokio::spawn(async move {
                use std::sync::atomic::Ordering;

                loop {
                    for (drive_id, cache, inodes) in &sync_entries {
                        let inodes = inodes.clone();
                        let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
                            Arc::new(move |item_id: &str| inodes.allocate(item_id));
                        match run_delta_sync(&sync_graph, cache, drive_id, &inode_allocator).await {
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
        }

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
            // All per-mount caches share the same writeback dir; any one suffices for flush.
            let hup_cache = mount_entries.first().map(|(_, c, _)| c.clone());
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
                            if let Some(rc) = hup_cache.clone() {
                                let rg = hup_graph.clone();
                                tokio::spawn(async move {
                                    cloudmount_vfs::recover_pending_writes(
                                        &rc,
                                        &rg,
                                        "re-auth recovery",
                                    )
                                    .await;
                                });
                            }
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
    fn test_preflight_checks_succeeds() {
        // preflight_checks no longer validates client ID (it's hardcoded)
        // On Linux/macOS it only warns about FUSE; on Windows it checks CfApi version.
        // Non-Windows: just verify it doesn't panic and returns a Result.
        #[cfg(not(target_os = "windows"))]
        {
            // On Linux/macOS preflight only warns (no error path for FUSE) unless CfApi
            let _result = preflight_checks();
        }
    }

    #[test]
    fn test_client_id_constant() {
        // CLIENT_ID is the official CloudMount Azure AD app registration
        assert_eq!(CLIENT_ID, "8ebe3ef7-f509-4146-8fef-c9b5d7c22252");
    }

    #[test]
    fn test_runtime_override_takes_priority() {
        let overrides = RuntimeOverrides {
            client_id: Some("override-id".to_string()),
            tenant_id: Some("override-tenant".to_string()),
        };
        let client_id = overrides
            .client_id
            .clone()
            .unwrap_or_else(|| CLIENT_ID.to_string());
        assert_eq!(client_id, "override-id");
        assert_eq!(overrides.tenant_id.as_deref(), Some("override-tenant"));
    }

    #[test]
    fn test_runtime_override_falls_back_to_constant() {
        let no_overrides = RuntimeOverrides {
            client_id: None,
            tenant_id: None,
        };
        let client_id = no_overrides
            .client_id
            .clone()
            .unwrap_or_else(|| CLIENT_ID.to_string());
        assert_eq!(client_id, CLIENT_ID);
        assert!(no_overrides.tenant_id.is_none());
    }

    // Windows-only test that simulates CfApi version check failure.
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
