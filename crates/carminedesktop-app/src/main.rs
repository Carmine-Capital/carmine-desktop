#![cfg_attr(
    all(not(debug_assertions), feature = "desktop"),
    windows_subsystem = "windows"
)]

#[cfg(feature = "desktop")]
mod commands;
#[cfg(feature = "desktop")]
mod notify;
#[cfg(feature = "desktop")]
mod shell_integration;
#[cfg(feature = "desktop")]
mod tray;
#[cfg(feature = "desktop")]
mod update;

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[cfg(not(target_os = "windows"))]
use carminedesktop_core::config::{AccountMetadata, derive_mount_point};
use carminedesktop_core::config::{
    EffectiveConfig, UserConfig, config_file_path, expand_mount_point,
};

#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use std::sync::Arc;

#[cfg(any(feature = "desktop", not(target_os = "windows")))]
type OpenerFn = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

#[cfg(any(feature = "desktop", not(target_os = "windows")))]
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

#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use carminedesktop_auth::AuthManager;
#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use carminedesktop_cache::CacheManager;
#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use carminedesktop_cache::sync::run_delta_sync;
#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use carminedesktop_core::config::MountConfig;
// cache_dir is used in start_mount (FUSE on Linux/macOS) and in desktop start_mount (Windows).
// The cfg union covers both headless unix builds and desktop builds (any platform).
#[cfg(any(target_os = "linux", target_os = "macos", feature = "desktop"))]
use carminedesktop_core::config::cache_dir;
#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use carminedesktop_graph::GraphClient;
#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use carminedesktop_vfs::inode::InodeTable;
#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use tokio_util::sync::CancellationToken;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use carminedesktop_vfs::MountHandle;

#[cfg(any(feature = "desktop", not(target_os = "windows")))]
use std::sync::atomic::AtomicBool;

#[cfg(feature = "desktop")]
use std::collections::HashMap;
#[cfg(feature = "desktop")]
use std::sync::Mutex;

/// Per-mount cache entry: `(CacheManager, InodeTable, DeltaSyncObserver)` keyed by drive_id.
#[cfg(feature = "desktop")]
type MountCacheEntry = (
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn carminedesktop_core::DeltaSyncObserver>>,
);

/// Snapshot row used by the delta-sync loop.
#[cfg(feature = "desktop")]
type SyncSnapshotRow = (
    String,
    String,
    String,
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn carminedesktop_core::DeltaSyncObserver>>,
);

#[allow(dead_code)] // Used conditionally across platform×feature combos; referenced by tests on all platforms
const CLIENT_ID: &str = "8ebe3ef7-f509-4146-8fef-c9b5d7c22252";

/// Annotated default configuration printed by `--print-default-config`.
const DEFAULT_CONFIG_TOML: &str = "\
# Carmine Desktop configuration
# Location: see --print-config-path

[general]
# Start Carmine Desktop on login (systemd user unit / launchd / registry)
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

/// Carmine Desktop — mount Microsoft OneDrive and SharePoint as local filesystems.
#[derive(Parser, Debug)]
#[command(
    version,
    about,
    after_help = "\
SIGNALS (Unix only):
  SIGHUP    Trigger re-authentication in headless mode.
            Useful when a refresh token expires on a remote server.

EXAMPLES:
  carminedesktop --headless                    Run without GUI
  carminedesktop --print-default-config        Show annotated default configuration
  kill -HUP $(pidof carminedesktop)            Re-authenticate a running instance"
)]
struct CliArgs {
    /// Azure AD client ID
    #[arg(long, env = "CARMINEDESKTOP_CLIENT_ID")]
    client_id: Option<String>,

    /// Azure AD tenant ID
    #[arg(long, env = "CARMINEDESKTOP_TENANT_ID")]
    tenant_id: Option<String>,

    /// Config file path
    #[arg(long, env = "CARMINEDESKTOP_CONFIG")]
    config: Option<std::path::PathBuf>,

    /// Log level (trace/debug/info/warn/error)
    #[arg(long, env = "CARMINEDESKTOP_LOG_LEVEL")]
    log_level: Option<String>,

    /// Run without GUI (even if desktop feature is enabled)
    #[arg(long)]
    headless: bool,

    /// Print annotated default configuration and exit
    #[arg(long)]
    print_default_config: bool,

    /// Open a mounted file in SharePoint Online (used by Explorer context menu)
    #[arg(long)]
    open_online: Option<String>,

    /// Open a file: if on Carmine Desktop drive, open online; otherwise fall through to OS handler
    /// (used by Windows file associations)
    #[arg(long)]
    open: Option<String>,

    /// Positional passthrough values (e.g. `carminedesktop://...` deep-link URL on Linux/Windows)
    #[arg(hide = true)]
    _passthrough: Vec<String>,
}

#[allow(dead_code)] // Fields read conditionally across platform×feature combos; referenced by tests
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
    pub mounts: Mutex<HashMap<String, carminedesktop_vfs::WinFspMountHandle>>,
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

#[cfg(any(feature = "desktop", not(target_os = "windows")))]
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
    show_error_dialog("Carmine Desktop \u{2014} Configuration Error", msg);
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

    // WinFsp driver required — Windows installer should bundle or require WinFsp.
    // See https://winfsp.dev/ for MSI installer.
    //
    // We also add the WinFsp bin directory to PATH so the delay-loaded
    // winfsp-x64.dll can be found when the process is launched by Explorer
    // (e.g. via the "Open Online" context menu) rather than from a shell
    // where the user has manually extended PATH.
    #[cfg(target_os = "windows")]
    {
        let winfsp_bin_dir = (|| -> Option<String> {
            // WinFsp registers under SOFTWARE\WinFsp on native-bitness installs,
            // but under SOFTWARE\WOW6432Node\WinFsp when the 32-bit installer
            // is used on 64-bit Windows. Check both.
            let reg_keys = [r"HKLM\SOFTWARE\WinFsp", r"HKLM\SOFTWARE\WOW6432Node\WinFsp"];
            for key in reg_keys {
                let Ok(output) = std::process::Command::new("reg")
                    .args(["query", key, "/v", "InstallDir"])
                    .output()
                else {
                    continue;
                };
                if !output.status.success() {
                    continue;
                }
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Some(idx) = line.find("REG_SZ") {
                        let install_dir = line[idx + "REG_SZ".len()..].trim();
                        let bin_dir = std::path::Path::new(install_dir).join("bin");
                        if bin_dir.join("winfsp-x64.dll").exists() {
                            return Some(bin_dir.to_string_lossy().into_owned());
                        }
                    }
                }
            }
            None
        })();

        match winfsp_bin_dir {
            Some(bin_dir) => {
                // Prepend WinFsp bin to PATH so the delay-loaded DLL is found.
                let current_path = std::env::var("PATH").unwrap_or_default();
                if !current_path
                    .split(';')
                    .any(|p| p.eq_ignore_ascii_case(&bin_dir))
                {
                    // SAFETY: called in main() before any threads are spawned.
                    unsafe {
                        std::env::set_var("PATH", format!("{bin_dir};{current_path}"));
                    }
                }
            }
            None => {
                return Err(
                    "WinFsp driver not found. Install WinFsp from https://winfsp.dev/ to enable filesystem mounts.".to_string(),
                );
            }
        }
    }

    Ok(())
}

#[cfg(any(feature = "desktop", not(target_os = "windows")))]
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

    // Configure tracing: CLI --log-level > CARMINEDESKTOP_LOG_LEVEL (already handled by clap env) > RUST_LOG > "info"
    let filter = if let Some(ref level) = args.log_level {
        EnvFilter::new(level)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };
    tracing_subscriber::fmt().with_env_filter(filter).init();

    tracing::info!("Carmine Desktop starting");

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
    let app_name = "Carmine Desktop".to_string();
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
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // When a second instance is launched (e.g. via Explorer context menu),
            // its argv is forwarded here. Check for --open-online or --open <path>.
            if let Some(pos) = argv.iter().position(|a| a == "--open-online")
                && let Some(path) = argv.get(pos + 1).cloned()
            {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = commands::open_online(handle, path).await {
                        tracing::error!("open-online from context menu failed: {e}");
                    }
                });
            } else if let Some(pos) = argv.iter().position(|a| a == "--open")
                && let Some(path) = argv.get(pos + 1).cloned()
            {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = commands::open_file(handle, path).await {
                        tracing::error!("open from file association failed: {e}");
                    }
                });
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_deep_link::init())
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
            commands::open_online,
            commands::open_file,
            commands::get_file_handlers,
            commands::redetect_file_handlers,
            commands::save_file_handler_override,
            commands::clear_file_handler_override,
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

            // Register deep link handler for runtime URL dispatches (macOS).
            // On Windows/Linux, the OS spawns a new instance with the URL as a
            // CLI arg; get_current() in setup_after_launch handles that case.
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                let dl_handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    let handle = dl_handle.clone();
                    let urls = event.urls();
                    tauri::async_runtime::spawn(async move {
                        handle_deep_link_urls(&handle, urls).await;
                    });
                });
            }

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

    // Register carminedesktop:// protocol handler (Linux/Windows runtime registration).
    // macOS uses Info.plist-based registration handled by the deep-link plugin.
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_deep_link::DeepLinkExt;
        if let Err(e) = app.deep_link().register("carminedesktop") {
            tracing::warn!("failed to register carminedesktop:// deep link: {e}");
        }
    }

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
                    carminedesktop_core::config::autostart::set_enabled(auto_start, &exe_path)
                {
                    tracing::warn!("auto-start sync failed: {e}");
                }
            }
            Err(e) => {
                tracing::warn!("failed to resolve exe path for auto-start sync: {e}");
            }
        }

        // Reconcile file association registration with config.
        {
            let register_file_assoc = {
                let config = state.effective_config.lock().unwrap();
                config.register_file_associations
            };
            if register_file_assoc {
                if let Err(e) = shell_integration::register_file_associations() {
                    tracing::warn!("file association registration failed: {e}");
                }
            } else if shell_integration::are_file_associations_registered()
                && let Err(e) = shell_integration::unregister_file_associations()
            {
                tracing::warn!("file association unregistration failed: {e}");
            }
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if !fuse_available() {
            notify::fuse_unavailable(app);
        }
        start_all_mounts(app);

        // Reconcile Explorer navigation pane registration with config.
        // Placed after start_all_mounts so WinFsp is serving before Explorer
        // is notified via SHChangeNotify (on first registration).
        #[cfg(target_os = "windows")]
        {
            let (nav_pane_enabled, cloud_root) = {
                let config = state.effective_config.lock().unwrap();
                let root = expand_mount_point(&format!("~/{}", config.root_dir));
                (config.explorer_nav_pane, root)
            };
            if nav_pane_enabled {
                if let Err(e) =
                    shell_integration::ensure_nav_pane(std::path::Path::new(&cloud_root))
                {
                    tracing::warn!("Explorer navigation pane registration failed: {e}");
                }
            } else if shell_integration::is_nav_pane_registered()
                && let Err(e) = shell_integration::unregister_nav_pane()
            {
                tracing::warn!("Explorer navigation pane unregistration failed: {e}");
            }
        }

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

    // Check if the app was launched via a deep link (Windows/Linux pass the URL
    // as a CLI argument; the plugin reads it via get_current()).
    {
        use tauri_plugin_deep_link::DeepLinkExt;
        match app.deep_link().get_current() {
            Ok(Some(urls)) => {
                let handle = app.clone();
                tokio::spawn(async move {
                    handle_deep_link_urls(&handle, urls).await;
                });
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!("failed to read startup deep link: {e}");
            }
        }
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

// ---------------------------------------------------------------------------
// Deep link handler
// ---------------------------------------------------------------------------

/// Process deep link URLs dispatched by the OS (e.g. `carminedesktop://open-online?path=...`).
#[cfg(feature = "desktop")]
async fn handle_deep_link_urls(app: &tauri::AppHandle, urls: Vec<url::Url>) {
    for url in urls {
        handle_deep_link_url(app, url).await;
    }
}

/// Handle a single deep link URL.
///
/// Supported actions:
/// - `carminedesktop://open-online?path=<percent-encoded-path>`: resolve the local
///   mount path to its SharePoint URL and open it.
///
/// Invalid paths or unrecognized actions produce a desktop notification.
#[cfg(feature = "desktop")]
async fn handle_deep_link_url(app: &tauri::AppHandle, url: url::Url) {
    if url.scheme() != "carminedesktop" {
        return;
    }

    match url.host_str() {
        Some("open-online") => {
            let path = url
                .query_pairs()
                .find(|(k, _)| k == "path")
                .map(|(_, v)| v.into_owned());

            match path {
                Some(p) => {
                    tracing::info!("deep link: open-online path={p}");
                    if let Err(e) = commands::open_online(app.clone(), p).await {
                        tracing::error!("deep link open-online failed: {e}");
                        notify::deep_link_failed(app, &e);
                    }
                }
                None => {
                    let msg = "Missing file path in deep link URL";
                    tracing::warn!("deep link: {msg} — {url}");
                    notify::deep_link_failed(app, msg);
                }
            }
        }
        Some(action) => {
            tracing::warn!("unrecognized deep link action: {action}");
        }
        None => {
            tracing::warn!("deep link has no action: {url}");
        }
    }
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

    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for mount_config in &mounts_config {
        if let Err(e) = start_mount(app, mount_config) {
            tracing::error!("failed to start mount '{}': {e}", mount_config.name);
            failed += 1;
        } else {
            succeeded += 1;
        }
    }

    if succeeded > 0 || failed > 0 {
        notify::mounts_summary(app, succeeded, failed);
    }
    tray::update_tray_menu(app);
}

/// Shared mount setup resources extracted from platform-specific `start_mount` functions.
#[cfg(feature = "desktop")]
struct MountContext {
    drive_id: String,
    mountpoint: String,
    cache: Arc<CacheManager>,
    inodes: Arc<InodeTable>,
    event_tx: tokio::sync::mpsc::UnboundedSender<carminedesktop_vfs::core_ops::VfsEvent>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<carminedesktop_vfs::core_ops::VfsEvent>,
    rt: tokio::runtime::Handle,
}

/// Validate the drive, set up cache/inodes/event channel — shared by FUSE and WinFsp mounts.
#[cfg(feature = "desktop")]
fn start_mount_common(
    app: &tauri::AppHandle,
    mount_config: &MountConfig,
) -> Result<Option<MountContext>, String> {
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
            Err(carminedesktop_core::Error::GraphApi { status: 404, .. }) => {
                tracing::warn!(
                    "mount '{}' drive not found (404), removing from config",
                    mount_config.name
                );
                remove_mount_from_config(app, &mount_config.id);
                notify::mount_not_found(app, &mount_config.name);
                return Ok(None);
            }
            Err(carminedesktop_core::Error::GraphApi { status: 403, .. }) => {
                tracing::warn!(
                    "mount '{}' access denied (403), skipping",
                    mount_config.name
                );
                notify::mount_access_denied(app, &mount_config.name);
                return Ok(None);
            }
            Err(e) => {
                tracing::warn!(
                    "transient error validating mount '{}': {e}, skipping",
                    mount_config.name
                );
                return Ok(None);
            }
        }
    }

    let mountpoint = expand_mount_point(&mount_config.mount_point);
    // Safety net: strip trailing separators to prevent WinFsp STATUS_ACCESS_VIOLATION.
    // Preserve bare roots like "/" by keeping the original if stripping would empty it.
    let mountpoint = {
        let trimmed = mountpoint.trim_end_matches(['/', '\\']);
        if trimmed.is_empty() {
            mountpoint
        } else {
            trimmed.to_string()
        }
    };

    // FUSE requires the mount directory to exist; WinFsp creates it itself.
    #[cfg(not(target_os = "windows"))]
    std::fs::create_dir_all(&mountpoint).map_err(|e| format!("create mountpoint failed: {e}"))?;

    #[cfg(target_os = "windows")]
    {
        // Ensure the parent directory exists (e.g. ~/Cloud/).
        if let Some(parent) = std::path::Path::new(&mountpoint).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create mountpoint parent failed: {e}"))?;
        }
        // Remove stale directory left over from a previous run.
        let mp = std::path::Path::new(&mountpoint);
        if mp.exists() {
            let _ = std::fs::remove_dir_all(mp);
        }
    }

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
    let cache = Arc::new(
        CacheManager::new(effective_cache_dir, db_path, max_cache_bytes, metadata_ttl)
            .map_err(|e| e.to_string())?,
    );
    let max_inode = cache.sqlite.max_inode().unwrap_or(0);
    let inodes = Arc::new(InodeTable::new_starting_after(max_inode));

    let rt = state
        .tokio_handle
        .get()
        .cloned()
        .unwrap_or_else(|| tokio::runtime::Handle::current());

    let (event_tx, event_rx) =
        tokio::sync::mpsc::unbounded_channel::<carminedesktop_vfs::core_ops::VfsEvent>();

    Ok(Some(MountContext {
        drive_id: drive_id.to_string(),
        mountpoint,
        cache,
        inodes,
        event_tx,
        event_rx,
        rt,
    }))
}

/// Spawn a task that forwards VFS events to desktop notifications.
#[cfg(feature = "desktop")]
fn spawn_event_forwarder(
    rt: &tokio::runtime::Handle,
    app: &tauri::AppHandle,
    mut event_rx: tokio::sync::mpsc::UnboundedReceiver<carminedesktop_vfs::core_ops::VfsEvent>,
) {
    let app_handle = app.clone();
    rt.spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                carminedesktop_vfs::core_ops::VfsEvent::ConflictDetected {
                    file_name,
                    conflict_name,
                } => {
                    notify::conflict_detected(&app_handle, &file_name, &conflict_name);
                }
                carminedesktop_vfs::core_ops::VfsEvent::WritebackFailed { file_name } => {
                    notify::writeback_failed(&app_handle, &file_name);
                }
                carminedesktop_vfs::core_ops::VfsEvent::UploadFailed { file_name, reason } => {
                    notify::upload_failed(&app_handle, &file_name, &reason);
                }
                carminedesktop_vfs::core_ops::VfsEvent::FileLocked { file_name } => {
                    notify::file_locked(&app_handle, &file_name);
                }
                carminedesktop_vfs::core_ops::VfsEvent::CollabGateTimeout { path } => {
                    let file_name = std::path::Path::new(&path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&path)
                        .to_string();
                    tracing::warn!("CollabGate timeout for {path}, opened locally");
                    notify::collab_gate_timeout(&app_handle, &file_name);
                }
                carminedesktop_vfs::core_ops::VfsEvent::CollabOpenOnlineBackground { path } => {
                    // Fire-and-forget: open the file online without blocking the VFS.
                    // This is used when file associations are NOT registered, so the
                    // VFS proceeds with local open while we asynchronously launch Office.
                    let handle = app_handle.clone();
                    tokio::spawn(async move {
                        if let Err(e) = commands::open_online(handle, path.clone()).await {
                            tracing::warn!("background open-online failed for {path}: {e}");
                        }
                    });
                }
            }
        }
    });
}

/// Open a file online — try desktop Office via URI scheme, fall back to browser.
#[cfg(feature = "desktop")]
async fn handle_collab_open_online(
    app: &tauri::AppHandle,
    request: &carminedesktop_core::types::CollabOpenRequest,
) -> Result<(), String> {
    // Delegate to the shared open_online command which handles Office URI +
    // browser fallback.
    commands::open_online(app.clone(), request.path.clone()).await
}

/// Spawn a task that listens on the CollabGate channel and handles requests.
///
/// For each request: unconditionally open online. Falls back to OpenLocally on error.
#[cfg(feature = "desktop")]
fn spawn_collab_handler(
    rt: &tokio::runtime::Handle,
    app: &tauri::AppHandle,
    mut collab_rx: tokio::sync::mpsc::Receiver<(
        carminedesktop_core::types::CollabOpenRequest,
        tokio::sync::oneshot::Sender<carminedesktop_core::types::CollabOpenResponse>,
    )>,
) {
    use carminedesktop_core::types::CollabOpenResponse;

    let app_handle = app.clone();
    rt.spawn(async move {
        while let Some((request, reply_tx)) = collab_rx.recv().await {
            let file_name = std::path::Path::new(&request.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&request.path)
                .to_string();

            // On Windows, respond OpenOnline first (unblocking the VFS so
            // Excel processes STATUS_CANCELLED), then wait briefly before
            // launching the Office URI to avoid duplicate-workbook collisions.
            #[cfg(target_os = "windows")]
            {
                let _ = reply_tx.send(CollabOpenResponse::OpenOnline);
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                match handle_collab_open_online(&app_handle, &request).await {
                    Ok(()) => {
                        tracing::debug!("CollabGate: opened {file_name} online (deferred)");
                    }
                    Err(e) => {
                        // Cannot fall back to OpenLocally — VFS already
                        // unblocked with OpenOnline. Notify the user instead.
                        tracing::warn!(
                            "CollabGate: deferred open of {file_name} failed: {e}"
                        );
                        notify::collab_open_failed(&app_handle, &file_name, &e);
                    }
                }
            }

            // On Linux/macOS, open online first then respond (current behavior).
            #[cfg(not(target_os = "windows"))]
            {
                match handle_collab_open_online(&app_handle, &request).await {
                    Ok(()) => {
                        tracing::debug!("CollabGate: opened {file_name} online");
                        let _ = reply_tx.send(CollabOpenResponse::OpenOnline);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "CollabGate: failed to open {file_name} online: {e}, falling back to local"
                        );
                        notify::collab_open_failed(&app_handle, &file_name, &e);
                        let _ = reply_tx.send(CollabOpenResponse::OpenLocally);
                    }
                }
            }
        }
    });
}

#[cfg(all(feature = "desktop", any(target_os = "linux", target_os = "macos")))]
fn start_mount(app: &tauri::AppHandle, mount_config: &MountConfig) -> Result<(), String> {
    use tauri::Manager;

    let Some(ctx) = start_mount_common(app, mount_config)? else {
        return Ok(());
    };

    if !carminedesktop_vfs::cleanup_stale_mount(&ctx.mountpoint) {
        return Err(format!(
            "stale FUSE mount at {} could not be cleaned up — run `fusermount -u {}` manually",
            ctx.mountpoint, ctx.mountpoint
        ));
    }

    let state = app.state::<AppState>();

    // Spawn the async sync processor for this mount
    let (sync_handle, sync_join) = carminedesktop_vfs::spawn_sync_processor(
        carminedesktop_vfs::SyncProcessorDeps {
            graph: state.graph.clone(),
            cache: ctx.cache.clone(),
            inodes: ctx.inodes.clone(),
            drive_id: ctx.drive_id.clone(),
            event_tx: Some(ctx.event_tx.clone()),
        },
        carminedesktop_vfs::SyncProcessorConfig::default(),
        &ctx.rt,
    );

    // Read collab config and create the CollabGate channel
    let collab_config = {
        let cfg = state.effective_config.lock().map_err(|e| e.to_string())?;
        cfg.collaborative_open.clone()
    };
    let (collab_tx, collab_rx) = tokio::sync::mpsc::channel(8);

    let file_associations_registered = shell_integration::are_file_associations_registered();

    let mut handle = MountHandle::mount(
        state.graph.clone(),
        ctx.cache.clone(),
        ctx.inodes.clone(),
        ctx.drive_id.clone(),
        &ctx.mountpoint,
        ctx.rt.clone(),
        Some(ctx.event_tx),
        Some(sync_handle),
        Some(collab_tx),
        Some(collab_config),
        file_associations_registered,
    )
    .map_err(|e| e.to_string())?;

    handle.set_sync_join(sync_join);

    spawn_event_forwarder(&ctx.rt, app, ctx.event_rx);
    spawn_collab_handler(&ctx.rt, app, collab_rx);

    let observer = Some(handle.delta_observer());
    state
        .mount_caches
        .lock()
        .unwrap()
        .insert(ctx.drive_id.clone(), (ctx.cache, ctx.inodes, observer));

    state
        .mounts
        .lock()
        .unwrap()
        .insert(mount_config.id.clone(), handle);

    tracing::info!(
        "mount '{}' started at {}",
        mount_config.name,
        ctx.mountpoint
    );
    tray::update_tray_menu(app);

    Ok(())
}

#[cfg(all(feature = "desktop", target_os = "windows"))]
fn start_mount(app: &tauri::AppHandle, mount_config: &MountConfig) -> Result<(), String> {
    use tauri::Manager;

    let Some(ctx) = start_mount_common(app, mount_config)? else {
        return Ok(());
    };

    let state = app.state::<AppState>();

    // Spawn the async sync processor for this mount
    let (sync_handle, sync_join) = carminedesktop_vfs::spawn_sync_processor(
        carminedesktop_vfs::SyncProcessorDeps {
            graph: state.graph.clone(),
            cache: ctx.cache.clone(),
            inodes: ctx.inodes.clone(),
            drive_id: ctx.drive_id.clone(),
            event_tx: Some(ctx.event_tx.clone()),
        },
        carminedesktop_vfs::SyncProcessorConfig::default(),
        &ctx.rt,
    );

    // Read collab config and create the CollabGate channel
    let collab_config = {
        let cfg = state.effective_config.lock().map_err(|e| e.to_string())?;
        cfg.collaborative_open.clone()
    };
    let (collab_tx, collab_rx) = tokio::sync::mpsc::channel(8);

    let file_associations_registered = shell_integration::are_file_associations_registered();

    let mut handle = carminedesktop_vfs::WinFspMountHandle::mount(
        state.graph.clone(),
        ctx.cache.clone(),
        ctx.inodes.clone(),
        ctx.drive_id.clone(),
        &ctx.mountpoint,
        ctx.rt.clone(),
        Some(ctx.event_tx),
        Some(sync_handle),
        Some(collab_tx),
        Some(collab_config),
        file_associations_registered,
    )
    .map_err(|e| e.to_string())?;

    handle.set_sync_join(sync_join);

    spawn_event_forwarder(&ctx.rt, app, ctx.event_rx);
    spawn_collab_handler(&ctx.rt, app, collab_rx);

    let observer = Some(handle.delta_observer());
    state
        .mount_caches
        .lock()
        .unwrap()
        .insert(ctx.drive_id.clone(), (ctx.cache, ctx.inodes, observer));

    state
        .mounts
        .lock()
        .unwrap()
        .insert(mount_config.id.clone(), handle);

    tracing::info!(
        "mount '{}' started at {}",
        mount_config.name,
        ctx.mountpoint
    );
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
    let delta_cancel = cancel.clone();

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
                    .map(|(drive_id, (c, i, obs))| {
                        let (mount_id, mount_name) = config
                            .mounts
                            .iter()
                            .find(|m| m.drive_id.as_deref() == Some(drive_id.as_str()))
                            .map(|m| (m.id.clone(), m.name.clone()))
                            .unwrap_or_else(|| (drive_id.clone(), drive_id.clone()));
                        (
                            drive_id.clone(),
                            mount_id,
                            mount_name,
                            c.clone(),
                            i.clone(),
                            obs.clone(),
                        )
                    })
                    .collect()
            };

            for (drive_id, mount_id, mount_name, cache, inodes, observer) in &snapshot {
                let inodes = inodes.clone();
                let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
                    Arc::new(move |item_id: &str| inodes.allocate(item_id));
                match run_delta_sync(
                    &graph,
                    cache,
                    drive_id,
                    &inode_allocator,
                    observer.as_deref(),
                )
                .await
                {
                    Ok(_result) => {
                        // Clear 403 state so the user is notified if access is lost again.
                        notified_403.remove(drive_id.as_str());
                    }
                    Err(carminedesktop_core::Error::GraphApi { status: 404, .. }) => {
                        tracing::warn!(
                            "mount '{mount_name}' drive not found during delta sync (404), removing"
                        );
                        let _ = stop_mount(&app_handle, mount_id);
                        remove_mount_from_config(&app_handle, mount_id);
                        notify::mount_orphaned(&app_handle, mount_name);
                    }
                    Err(carminedesktop_core::Error::GraphApi { status: 403, .. }) => {
                        if notified_403.insert(drive_id.clone()) {
                            tracing::warn!(
                                "mount '{mount_name}' access denied during delta sync (403)"
                            );
                            notify::mount_access_denied(&app_handle, mount_name);
                        }
                    }
                    Err(carminedesktop_core::Error::Auth(ref msg))
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
                _ = delta_cancel.cancelled() => break,
                _ = tokio::time::sleep(wait) => {}
            }
        }
    });

    // Note: retry_pending_writes task removed — the SyncProcessor's tick-based
    // retry with exponential backoff handles all upload retries.
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
        .map(|(c, _, _)| c.clone())
    {
        Some(c) => c,
        None => return, // No mounts active; nothing to recover.
    };

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let recovered =
            carminedesktop_vfs::recover_pending_writes(&cache, &graph, "crash recovery").await;
        if recovered > 0 {
            let path = carminedesktop_core::config::config_dir()
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

// On Windows the function body is cfg-gated as #[cfg(not(target_os = "windows"))];
// full Windows headless support with WinFsp is planned but not yet implemented.
#[cfg_attr(target_os = "windows", allow(unused_variables, unused_mut))]
fn run_headless(
    mut user_config: UserConfig,
    mut effective: EffectiveConfig,
    overrides: RuntimeOverrides,
) {
    #[cfg(not(target_os = "windows"))]
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    #[cfg(not(target_os = "windows"))]
    rt.block_on(async {
        let opener: OpenerFn =
            Arc::new(|url: &str| {
                if carminedesktop_auth::oauth::has_display() {
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
        type HeadlessMountEntry = (
            String,
            Arc<CacheManager>,
            Arc<InodeTable>,
            Option<Arc<dyn carminedesktop_core::DeltaSyncObserver>>,
        );
        let mut mount_entries: Vec<HeadlessMountEntry> = Vec::new();
        let mut mount_handles: Vec<MountHandle> = Vec::new();

        let mounts_config: Vec<MountConfig> = effective
            .mounts
            .iter()
            .filter(|m| m.enabled)
            .cloned()
            .collect();

        let rt_handle = tokio::runtime::Handle::current();

        let effective_cache_dir = effective
            .cache_dir
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(cache_dir);
        let max_cache_bytes = parse_cache_size(&effective.cache_max_size);
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

                if !carminedesktop_vfs::cleanup_stale_mount(&mountpoint) {
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
                    None, // no sync processor in headless mode
                    None, // no collab channel in headless mode
                    None,
                    false, // no file associations in headless mode
                ) {
                    Ok(handle) => {
                        tracing::info!("mount '{}' started at {mountpoint}", mount_config.name);
                        let observer = Some(handle.delta_observer());
                        mount_entries.push((
                            drive_id.to_string(),
                            mount_cache,
                            mount_inodes,
                            observer,
                        ));
                        mount_handles.push(handle);
                    }
                    Err(e) => {
                        tracing::error!("failed to start mount '{}': {e}", mount_config.name);
                    }
                }
            }

        }

        let mount_count = mount_entries.len();

        // Crash recovery (non-blocking — runs in background after mounts are started)
        if let Some((_, recovery_cache, _, _)) = mount_entries.first() {
            let recovery_graph = graph.clone();
            let recovery_cache = recovery_cache.clone();
            tokio::spawn(async move {
                carminedesktop_vfs::recover_pending_writes(
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
                    for (drive_id, cache, inodes, observer) in &sync_entries {
                        let inodes = inodes.clone();
                        let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
                            Arc::new(move |item_id: &str| inodes.allocate(item_id));
                        let obs_ref = observer.as_deref();
                        match run_delta_sync(&sync_graph, cache, drive_id, &inode_allocator, obs_ref).await {
                            Ok(_result) => {}
                            Err(carminedesktop_core::Error::Auth(ref msg))
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

        tracing::info!("Carmine Desktop headless mode running \u{2014} {mount_count} mount(s) active");

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
            let hup_cache = mount_entries.first().map(|(_, c, _, _)| c.clone());
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
                                    carminedesktop_vfs::recover_pending_writes(
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
            "carminedesktop-app",
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
        let args = CliArgs::try_parse_from(["carminedesktop-app"]).unwrap();

        assert!(args.client_id.is_none());
        assert!(args.tenant_id.is_none());
        assert!(args.config.is_none());
        assert!(args.log_level.is_none());
        assert!(!args.headless);
    }

    #[test]
    fn test_cli_args_accepts_deep_link_passthrough() {
        let args = CliArgs::try_parse_from([
            "carminedesktop-app",
            "carminedesktop://open-online?path=%2Fhome%2Fnyxa%2FCloud%2FOneDrive%2Ftest.docx",
        ])
        .unwrap();

        assert_eq!(args._passthrough.len(), 1);
    }

    #[test]
    fn test_preflight_checks_succeeds() {
        // preflight_checks no longer validates client ID (it's hardcoded)
        // On Linux/macOS it only warns about FUSE; on Windows it checks for WinFsp.
        // Non-Windows: just verify it doesn't panic and returns a Result.
        #[cfg(not(target_os = "windows"))]
        {
            let _result = preflight_checks();
        }
    }

    #[test]
    fn test_client_id_constant() {
        // CLIENT_ID is the official Carmine Desktop Azure AD app registration
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
}
