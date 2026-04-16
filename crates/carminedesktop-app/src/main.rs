#![cfg_attr(
    all(not(debug_assertions), feature = "desktop"),
    windows_subsystem = "windows"
)]

#[cfg(feature = "desktop")]
mod commands;
#[cfg(feature = "desktop")]
mod ipc_server;
#[cfg(feature = "desktop")]
mod notify;
#[cfg(feature = "desktop")]
mod observability;
#[cfg(feature = "desktop")]
mod pin_events;
#[cfg(feature = "desktop")]
mod shell_integration;
#[cfg(feature = "desktop")]
mod tray;
#[cfg(feature = "desktop")]
mod update;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use carminedesktop_core::config::{
    EffectiveConfig, UserConfig, config_file_path, expand_mount_point,
};

#[cfg(feature = "desktop")]
use std::sync::Arc;

#[cfg(feature = "desktop")]
type OpenerFn = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

#[cfg(feature = "desktop")]
pub(crate) fn open_with_clean_env(path: &str) -> Result<(), String> {
    open::that(path).map_err(|e| e.to_string())
}

#[cfg(feature = "desktop")]
use carminedesktop_auth::AuthManager;
#[cfg(feature = "desktop")]
use carminedesktop_cache::CacheManager;
#[cfg(feature = "desktop")]
use carminedesktop_cache::sync::run_delta_sync;
#[cfg(feature = "desktop")]
use carminedesktop_core::config::MountConfig;
#[cfg(feature = "desktop")]
use carminedesktop_core::config::cache_dir;
#[cfg(feature = "desktop")]
use carminedesktop_graph::GraphClient;
#[cfg(feature = "desktop")]
use carminedesktop_vfs::inode::InodeTable;
#[cfg(feature = "desktop")]
use tokio_util::sync::CancellationToken;

#[cfg(feature = "desktop")]
use std::sync::atomic::AtomicBool;

#[cfg(feature = "desktop")]
use std::collections::HashMap;
#[cfg(feature = "desktop")]
use std::sync::Mutex;

/// Per-mount cache entry keyed by drive_id.
///
/// `(CacheManager, InodeTable, DeltaSyncObserver, OfflineManager, offline_flag, SyncHandle)`
#[cfg(feature = "desktop")]
type MountCacheEntry = (
    Arc<CacheManager>,
    Arc<InodeTable>,
    Option<Arc<dyn carminedesktop_core::DeltaSyncObserver>>,
    Arc<carminedesktop_cache::OfflineManager>,
    Arc<std::sync::atomic::AtomicBool>,
    Option<carminedesktop_vfs::SyncHandle>,
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
    Arc<carminedesktop_cache::OfflineManager>,
    Arc<std::sync::atomic::AtomicBool>,
    Option<carminedesktop_vfs::SyncHandle>,
);

#[allow(dead_code)] // Read by init_components on desktop; referenced by tests
const CLIENT_ID: &str = "70053421-2c1b-44fe-80f8-d258d0a81133";
#[allow(dead_code)]
const TENANT_ID: &str = "6a658318-4ef7-4de5-a2a6-d3c1698f272a";

/// Annotated default configuration printed by `--print-default-config`.
const DEFAULT_CONFIG_TOML: &str = "\
# Carmine Desktop configuration
# Location: see --print-config-path

[general]
# Start Carmine Desktop on login (Windows registry Run key)
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
EXAMPLES:
  carminedesktop                               Launch the desktop app
  carminedesktop --print-default-config        Show annotated default configuration"
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

    /// Pin a folder for offline use (used by Explorer context menu)
    #[arg(long)]
    offline_pin: Option<String>,

    /// Unpin a folder from offline use (used by Explorer context menu)
    #[arg(long)]
    offline_unpin: Option<String>,

    /// Positional passthrough values (e.g. `carminedesktop://...` deep-link URL).
    #[arg(hide = true)]
    _passthrough: Vec<String>,
}

#[allow(dead_code)] // Fields read by init_components on desktop; referenced by tests
struct RuntimeOverrides {
    client_id: Option<String>,
    tenant_id: Option<String>,
}

// Lock ordering (always acquire in this order to prevent deadlocks):
// user_config > effective_config > mount_caches > mounts > sync_cancel >
// active_sign_in > account_id > error_ring > activity_ring > last_synced > stale_pins
#[cfg(feature = "desktop")]
pub struct AppState {
    pub user_config: Mutex<UserConfig>,
    pub effective_config: Mutex<EffectiveConfig>,
    pub auth: Arc<AuthManager>,
    pub graph: Arc<GraphClient>,
    /// Per-mount cache and inode table, keyed by drive_id.
    pub mount_caches: Mutex<HashMap<String, MountCacheEntry>>,
    pub mounts: Mutex<HashMap<String, carminedesktop_vfs::WinFspMountHandle>>,
    pub sync_cancel: Mutex<Option<CancellationToken>>,
    pub active_sign_in: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub authenticated: AtomicBool,
    pub auth_degraded: AtomicBool,
    /// Drive ID of the currently signed-in account; `None` when no account is active.
    pub account_id: Mutex<Option<String>>,
    pub tokio_handle: std::sync::OnceLock<tokio::runtime::Handle>,
    pub ipc_server: Mutex<Option<ipc_server::IpcServer>>,
    /// Broadcast sender for observability events.
    pub obs_tx: tokio::sync::broadcast::Sender<carminedesktop_core::ObsEvent>,
    /// Ring buffer of recent errors for the dashboard.
    pub error_ring: Arc<Mutex<observability::ErrorAccumulator>>,
    /// Ring buffer of recent activity entries for the dashboard.
    pub activity_ring: Arc<Mutex<observability::ActivityBuffer>>,
    /// Per-drive last successful sync timestamp (ISO 8601).
    pub last_synced: Mutex<HashMap<String, String>>,
    /// Pins known to be stale (drive_id, item_id) -- set by delta sync when changed
    /// items overlap pinned subtrees.
    pub stale_pins: Mutex<std::collections::HashSet<(String, String)>>,
    /// Dirty signal into the pin aggregator (see `pin_events::spawn_aggregator`).
    /// Fed from `DiskCache::on_change` plus explicit pin/unpin commands; drives
    /// debounced `pin:health` / `pin:removed` emissions to the frontend.
    pub pin_tx: tokio::sync::mpsc::UnboundedSender<pin_events::PinDirty>,
    /// Global cache budget shared across every mount's DiskCache.  A single
    /// atomic store here (triggered from `save_settings`) instantly updates
    /// the eviction threshold seen by every mount — one user-configured
    /// value, one source of truth.
    pub cache_budget: Arc<std::sync::atomic::AtomicU64>,
}

#[allow(dead_code)] // Read by start_mount on desktop builds
fn parse_cache_size(size_str: &str) -> u64 {
    // Accept both English (GB/MB/KB/B) and French (Go/Mo/Ko/o) byte-unit
    // suffixes so settings persisted before the French-UI migration keep
    // parsing and the new UI-facing strings round-trip correctly.
    let s = size_str.trim().to_uppercase();
    let (num_part, multiplier) =
        if let Some(n) = s.strip_suffix("GB").or_else(|| s.strip_suffix("GO")) {
            (n.trim(), 1024u64 * 1024 * 1024)
        } else if let Some(n) = s.strip_suffix("MB").or_else(|| s.strip_suffix("MO")) {
            (n.trim(), 1024u64 * 1024)
        } else if let Some(n) = s.strip_suffix("KB").or_else(|| s.strip_suffix("KO")) {
            (n.trim(), 1024u64)
        } else {
            (s.as_str(), 1u64)
        };
    num_part.parse::<u64>().unwrap_or(5) * multiplier
}

#[cfg(feature = "desktop")]
struct Components {
    auth: Arc<AuthManager>,
    graph: Arc<GraphClient>,
}

/// Show a native Win32 error dialog. Only compiled on Windows release desktop builds
/// where `windows_subsystem = "windows"` detaches the console (making eprintln invisible).
#[cfg(all(feature = "desktop", not(debug_assertions)))]
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

/// Report a fatal startup error and terminate. On release desktop builds, shows
/// a `MessageBoxW` dialog (stderr is detached). Otherwise writes to stderr.
fn fatal_error(msg: &str) -> ! {
    #[cfg(all(feature = "desktop", not(debug_assertions)))]
    show_error_dialog("Carmine Desktop \u{2014} Configuration Error", msg);
    #[cfg(not(all(feature = "desktop", not(debug_assertions))))]
    eprintln!("Error: {msg}");
    std::process::exit(1);
}

fn preflight_checks() -> Result<(), String> {
    // WinFsp driver required — Windows installer should bundle or require WinFsp.
    // See https://winfsp.dev/ for MSI installer.
    //
    // We also add the WinFsp bin directory to PATH so the delay-loaded
    // winfsp-x64.dll can be found when the process is launched by Explorer
    // (e.g. via the "Open Online" context menu) rather than from a shell
    // where the user has manually extended PATH.
    let winfsp_bin_dir = (|| -> Option<String> {
        // WinFsp registers under SOFTWARE\WinFsp on native-bitness installs,
        // but under SOFTWARE\WOW6432Node\WinFsp when the 32-bit installer
        // is used on 64-bit Windows. Check both.
        let reg_keys = [r"HKLM\SOFTWARE\WinFsp", r"HKLM\SOFTWARE\WOW6432Node\WinFsp"];
        for key in reg_keys {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let Ok(output) = std::process::Command::new("reg")
                .args(["query", key, "/v", "InstallDir"])
                .creation_flags(CREATE_NO_WINDOW)
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
                    #[cfg(target_arch = "x86_64")]
                    const WINFSP_DLL: &str = "winfsp-x64.dll";
                    #[cfg(target_arch = "aarch64")]
                    const WINFSP_DLL: &str = "winfsp-a64.dll";
                    #[cfg(target_arch = "x86")]
                    const WINFSP_DLL: &str = "winfsp-x86.dll";
                    if bin_dir.join(WINFSP_DLL).exists() {
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

    Ok(())
}

#[cfg(feature = "desktop")]
fn init_components(overrides: &RuntimeOverrides, opener: OpenerFn) -> Components {
    let client_id = overrides
        .client_id
        .clone()
        .unwrap_or_else(|| CLIENT_ID.to_string());
    let tenant_id = overrides
        .tenant_id
        .clone()
        .or_else(|| Some(TENANT_ID.to_string()));

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

    // Log to a file alongside stderr so that GUI builds (windows_subsystem = "windows")
    // have persistent, inspectable logs even though stderr is void.
    let log_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("carminedesktop")
        .join("logs");
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("carminedesktop")
        .filename_suffix("log")
        .max_log_files(31)
        .build(&log_dir)
        .expect("failed to create log appender");

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false),
        )
        .init();

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
    run_desktop(user_config, effective, overrides);

    #[cfg(not(feature = "desktop"))]
    {
        let _ = (user_config, effective, overrides);
        eprintln!("Carmine Desktop requires the 'desktop' feature to run.");
        std::process::exit(1);
    }
}

#[cfg(feature = "desktop")]
fn run_desktop(user_config: UserConfig, effective: EffectiveConfig, overrides: RuntimeOverrides) {
    let app_name = "Carmine Desktop".to_string();
    let first_run = config_file_path().map(|p| !p.exists()).unwrap_or(true);

    // The opener uses tauri_plugin_opener which requires the AppHandle. The
    // AppHandle is lazily populated after Tauri initializes.
    let app_handle_slot: Arc<std::sync::Mutex<Option<tauri::AppHandle>>> =
        Arc::new(std::sync::Mutex::new(None));
    let opener_handle = app_handle_slot.clone();

    let opener: OpenerFn = Arc::new(move |url: &str| {
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
    });

    let Components { auth, graph } = init_components(&overrides, opener);

    let (obs_tx, _) = tokio::sync::broadcast::channel::<carminedesktop_core::ObsEvent>(256);
    let error_ring = Arc::new(Mutex::new(observability::ErrorAccumulator::new(100)));
    let activity_ring = Arc::new(Mutex::new(observability::ActivityBuffer::new(500)));
    let (pin_tx, pin_rx) = tokio::sync::mpsc::unbounded_channel::<pin_events::PinDirty>();
    let cache_budget = Arc::new(std::sync::atomic::AtomicU64::new(parse_cache_size(
        &effective.cache_max_size,
    )));

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
        ipc_server: Mutex::new(None),
        obs_tx,
        error_ring,
        activity_ring,
        last_synced: Mutex::new(HashMap::new()),
        stale_pins: Mutex::new(std::collections::HashSet::new()),
        pin_tx,
        cache_budget,
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
            } else if let Some(pos) = argv.iter().position(|a| a == "--offline-pin")
                && let Some(path) = argv.get(pos + 1).cloned()
            {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = handle_offline_pin(&handle, &path).await;
                });
            } else if let Some(pos) = argv.iter().position(|a| a == "--offline-unpin")
                && let Some(path) = argv.get(pos + 1).cloned()
            {
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = handle_offline_unpin(&handle, &path).await;
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
            commands::list_primary_site_libraries,
            commands::get_primary_site_info,
            commands::refresh_mount,
            commands::clear_cache,
            commands::open_wizard,
            commands::get_default_mount_root,
            commands::open_online,
            commands::open_file,
            commands::get_file_handlers,
            commands::save_file_handler_override,
            commands::clear_file_handler_override,
            commands::prompt_set_default_handler,
            commands::list_offline_pins,
            commands::remove_offline_pin,
            commands::extend_offline_pin,
            commands::get_dashboard_status,
            commands::get_recent_errors,
            commands::get_activity_feed,
            commands::get_cache_stats,
        ])
        .setup(move |app| {
            // Populate the opener's AppHandle slot now that the app is running.
            *app_handle_slot.lock().unwrap() = Some(app.handle().clone());

            tray::setup(app.handle(), &app_name)?;

            // Register deep link handler for runtime URL dispatches.
            // The OS may also spawn a new instance with the URL as a CLI arg;
            // get_current() in setup_after_launch handles that case.
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

            // Spawn observability event bridge
            {
                use tauri::Manager;
                let state = app.state::<AppState>();
                let obs_rx = state.obs_tx.subscribe();
                observability::spawn_event_bridge(
                    app.handle().clone(),
                    obs_rx,
                    state.error_ring.clone(),
                    state.activity_ring.clone(),
                );
            }

            // Spawn the pin aggregator that fans disk-cache writes into
            // granular `pin:health` / `pin:removed` events.
            pin_events::spawn_aggregator(app.handle().clone(), pin_rx);

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
    // Store the Tokio runtime handle so sync Tauri commands can use it.
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

    // Register carminedesktop:// protocol handler.
    {
        use tauri_plugin_deep_link::DeepLinkExt;
        if let Err(e) = app.deep_link().register("carminedesktop") {
            tracing::warn!("failed to register carminedesktop:// deep link: {e}");
        }
    }

    // Reconcile OS auto-start state with the persisted config value.
    // Runs unconditionally (not just when tokens are restored) so that the
    // first manual launch after install overwrites the NSIS-written registry
    // value with the correct exe path from std::env::current_exe().
    {
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
    }

    if restored {
        *state.account_id.lock().unwrap() = Some(account.as_ref().unwrap().id.clone());
        state.authenticated.store(true, Ordering::Relaxed);

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

        start_all_mounts(app);

        // Reconcile Explorer navigation pane registration with config.
        // Placed after start_all_mounts so WinFsp is serving before Explorer
        // is notified via SHChangeNotify (on first registration).
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

        // Register offline context menu verbs
        refresh_offline_context_menu(app);

        {
            let ipc = ipc_server::IpcServer::start(app.clone());
            *state.ipc_server.lock().unwrap() = Some(ipc);
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

    // Check if the app was launched via a deep link (the OS passes the URL
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

    // Signal handler — graceful shutdown on Ctrl+C
    let signal_handle = app.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to register Ctrl+C handler");
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
/// - `carminedesktop://open-offline`: focus the settings window on the Offline panel.
///
/// Invalid paths or unrecognized actions produce a desktop notification.
#[cfg(feature = "desktop")]
async fn handle_deep_link_url(app: &tauri::AppHandle, url: url::Url) {
    use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

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
        Some("open-offline") => {
            tracing::info!("deep link: open-offline");
            if let Some(win) = app.get_webview_window("settings") {
                let _ = win.unminimize();
                let _ = win.show();
                let _ = win.set_focus();
                let _ = win.emit("navigate-to-panel", "offline");
            } else {
                let _ = WebviewWindowBuilder::new(
                    app,
                    "settings",
                    WebviewUrl::App("settings.html?panel=offline".into()),
                )
                .title("Settings")
                .inner_size(800.0, 600.0)
                .min_inner_size(640.0, 480.0)
                .center()
                .build();
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

/// Return the user-facing mount name for a drive, falling back to the drive_id.
#[cfg(feature = "desktop")]
fn mount_display_name(state: &AppState, drive_id: &str) -> String {
    state
        .effective_config
        .lock()
        .ok()
        .and_then(|cfg| {
            cfg.mounts
                .iter()
                .find(|m| m.drive_id.as_deref() == Some(drive_id))
                .map(|m| m.name.clone())
        })
        .unwrap_or_else(|| drive_id.to_string())
}

#[cfg(feature = "desktop")]
async fn handle_offline_pin(app: &tauri::AppHandle, path: &str) -> Result<String, String> {
    use tauri::Manager;

    let state = app.state::<AppState>();
    if !state
        .authenticated
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        notify::offline_pin_rejected(app, path, "connexion requise");
        return Err("sign in required".to_string());
    }

    match resolve_and_pin(app, path).await {
        Ok(folder_name) => {
            notify::offline_pin_started(app, &folder_name);
            Ok(folder_name)
        }
        Err(e) => {
            let folder_name = std::path::Path::new(path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(path);
            notify::offline_pin_failed(app, folder_name, &e);
            Err(e)
        }
    }
}

#[cfg(feature = "desktop")]
async fn handle_offline_unpin(app: &tauri::AppHandle, path: &str) -> Result<String, String> {
    use tauri::Manager;

    let state = app.state::<AppState>();
    if !state
        .authenticated
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return Err("sign in required".to_string());
    }

    match resolve_and_unpin(app, path).await {
        Ok(folder_name) => {
            notify::offline_unpin_complete(app, &folder_name);
            Ok(folder_name)
        }
        Err(e) => {
            tracing::warn!("offline unpin failed for {path}: {e}");
            Err(e)
        }
    }
}

#[cfg(feature = "desktop")]
async fn resolve_and_pin(app: &tauri::AppHandle, path: &str) -> Result<String, String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let (drive_id, item) = commands::resolve_item_for_path(&state, path).await?;

    let offline_mgr = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        let (_, _, _, mgr, _, _) = caches
            .get(&drive_id)
            .ok_or_else(|| format!("no active cache for drive '{drive_id}'"))?;
        mgr.clone()
    };

    let folder_name = if item.name == "root" {
        mount_display_name(&state, &drive_id)
    } else {
        item.name.clone()
    };
    match offline_mgr
        .pin_folder(&item.id, &folder_name)
        .await
        .map_err(|e| e.to_string())?
    {
        carminedesktop_cache::PinResult::Ok => {
            // Surface the new pin in the UI before recursive_download lands —
            // the row shows up with analyzing/partial health and ticks up as
            // files cache in.
            let _ = state.pin_tx.send(pin_events::PinDirty::DriveRefresh {
                drive_id: drive_id.clone(),
            });
            Ok(folder_name)
        }
        carminedesktop_cache::PinResult::Rejected { reason } => {
            notify::offline_pin_rejected(app, &folder_name, &reason);
            Err(reason)
        }
    }
}

#[cfg(feature = "desktop")]
async fn resolve_and_unpin(app: &tauri::AppHandle, path: &str) -> Result<String, String> {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let (drive_id, item) = commands::resolve_item_for_path(&state, path).await?;

    let offline_mgr = {
        let caches = state.mount_caches.lock().map_err(|e| e.to_string())?;
        let (_, _, _, mgr, _, _) = caches
            .get(&drive_id)
            .ok_or_else(|| format!("no active cache for drive '{drive_id}'"))?;
        mgr.clone()
    };

    let folder_name = if item.name == "root" {
        mount_display_name(&state, &drive_id)
    } else {
        item.name.clone()
    };
    offline_mgr
        .unpin_folder(&item.id)
        .map_err(|e| e.to_string())?;
    let _ = state.pin_tx.send(pin_events::PinDirty::DriveRefresh {
        drive_id: drive_id.clone(),
    });
    Ok(folder_name)
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

/// Shared mount setup resources extracted from `start_mount`.
#[cfg(feature = "desktop")]
struct MountContext {
    drive_id: String,
    mountpoint: String,
    cache: Arc<CacheManager>,
    inodes: Arc<InodeTable>,
    offline_manager: Arc<carminedesktop_cache::OfflineManager>,
    offline_flag: Arc<std::sync::atomic::AtomicBool>,
    event_tx: tokio::sync::mpsc::UnboundedSender<carminedesktop_vfs::core_ops::VfsEvent>,
    event_rx: tokio::sync::mpsc::UnboundedReceiver<carminedesktop_vfs::core_ops::VfsEvent>,
    rt: tokio::runtime::Handle,
}

/// Validate the drive, set up cache/inodes/event channel — shared mount setup.
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
            Err(carminedesktop_core::Error::Network(ref msg)) => {
                tracing::warn!(
                    "mount '{}' offline — network unavailable ({msg}), \
                     proceeding with cached data",
                    mount_config.name
                );
                // Continue to mount creation — VFS will serve from cache.
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

    // WinFsp creates the mount directory itself; ensure the parent exists
    // and remove any stale directory left over from a previous run.
    if let Some(parent) = std::path::Path::new(&mountpoint).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create mountpoint parent failed: {e}"))?;
    }
    let mp = std::path::Path::new(&mountpoint);
    if mp.exists() {
        let _ = std::fs::remove_dir_all(mp);
    }

    let state = app.state::<AppState>();

    let (effective_cache_dir, metadata_ttl) = {
        let cfg = state.effective_config.lock().map_err(|e| e.to_string())?;
        let dir = cfg
            .cache_dir
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(cache_dir);
        let ttl = Some(cfg.metadata_ttl_secs);
        (dir, ttl)
    };

    let safe_id = drive_id.replace('!', "_");
    let db_path = effective_cache_dir.join(format!("drive-{safe_id}.db"));
    let cache = Arc::new(
        CacheManager::new_shared(
            effective_cache_dir,
            db_path,
            state.cache_budget.clone(),
            metadata_ttl,
            drive_id.to_string(),
        )
        .map_err(|e| e.to_string())?,
    );

    // Push `pin:health` updates whenever a cache entry lands or is evicted.
    // The aggregator debounces and only emits when the snapshot actually
    // changed, so high-churn writes (recursive pin downloads) stay quiet on
    // the wire.
    {
        let pin_tx = state.pin_tx.clone();
        let drive_id_owned = drive_id.to_string();
        cache
            .disk
            .set_cache_change_handler(Arc::new(move |_did: &str, item_id: &str| {
                let _ = pin_tx.send(pin_events::PinDirty::Cache {
                    drive_id: drive_id_owned.clone(),
                    item_id: item_id.to_string(),
                });
            }));
    }

    let max_inode = cache.sqlite.max_inode().unwrap_or(0);
    let inodes = Arc::new(InodeTable::new_starting_after(max_inode));
    // Seed InodeTable with existing SQLite mappings so VFS and offline
    // download agree on inode values (prevents ghost entries after delta sync).
    if let Ok(pairs) = cache.sqlite.all_inode_pairs() {
        inodes.seed(&pairs);
    }

    let (offline_ttl, offline_max_bytes) = {
        let cfg = state.effective_config.lock().map_err(|e| e.to_string())?;
        (
            cfg.offline_ttl_secs,
            parse_cache_size(&cfg.offline_max_folder_size),
        )
    };

    let offline_manager = Arc::new(carminedesktop_cache::OfflineManager::new(
        cache.pin_store.clone(),
        state.graph.clone(),
        cache.clone(),
        drive_id.to_string(),
        offline_ttl,
        offline_max_bytes,
    ));

    // Wire download error handler for desktop notifications
    {
        let app_for_notify = app.clone();
        offline_manager.set_download_error_handler(Arc::new(
            move |folder_name: &str, error: &str| {
                notify::offline_pin_failed(&app_for_notify, folder_name, error);
            },
        ));
    }

    // Wire download completion handler — fires once recursive_download finishes
    // successfully, replacing the old premature "pin complete" notification.
    {
        let app_for_notify = app.clone();
        offline_manager.set_download_complete_handler(Arc::new(move |folder_name: &str| {
            notify::offline_pin_completed(&app_for_notify, folder_name);
        }));
    }

    // Alias the shared offline flag owned by GraphClient. This way the retry
    // short-circuit in carminedesktop_graph::retry and the VFS/app reads of
    // the per-drive entry in `mount_caches` observe the same atomic — one
    // network transition propagates everywhere instantly.
    let offline_flag = state.graph.offline_flag().clone();

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
        offline_manager,
        offline_flag,
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
    obs_tx: tokio::sync::broadcast::Sender<carminedesktop_core::ObsEvent>,
) {
    use carminedesktop_core::types::ObsEvent;

    let app_handle = app.clone();
    rt.spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let now = chrono::Utc::now().to_rfc3339();
            match event {
                carminedesktop_vfs::core_ops::VfsEvent::ConflictDetected {
                    file_name,
                    conflict_name,
                } => {
                    notify::conflict_detected(&app_handle, &file_name, &conflict_name);
                    let _ = obs_tx.send(ObsEvent::Error {
                        drive_id: None,
                        file_name: Some(file_name.clone()),
                        remote_path: None,
                        error_type: "conflict_detected".to_string(),
                        message: format!("Conflict copy created: {conflict_name}"),
                        action_hint: Some(
                            "A conflict copy was created in the same folder".to_string(),
                        ),
                        timestamp: now.clone(),
                    });
                    let _ = obs_tx.send(ObsEvent::Activity {
                        drive_id: String::new(),
                        file_path: format!("/{file_name}"),
                        activity_type: "conflict".to_string(),
                        timestamp: now,
                    });
                }
                carminedesktop_vfs::core_ops::VfsEvent::WritebackFailed { file_name } => {
                    notify::writeback_failed(&app_handle, &file_name);
                    let _ = obs_tx.send(ObsEvent::Error {
                        drive_id: None,
                        file_name: Some(file_name),
                        remote_path: None,
                        error_type: "writeback_failed".to_string(),
                        message: "Writeback to buffer failed".to_string(),
                        action_hint: Some("Upload failed -- file queued for retry".to_string()),
                        timestamp: now,
                    });
                }
                carminedesktop_vfs::core_ops::VfsEvent::UploadFailed { file_name, reason } => {
                    notify::upload_failed(&app_handle, &file_name, &reason);
                    let _ = obs_tx.send(ObsEvent::Error {
                        drive_id: None,
                        file_name: Some(file_name),
                        remote_path: None,
                        error_type: "upload_failed".to_string(),
                        message: format!("Upload failed: {reason}"),
                        action_hint: Some(
                            "Upload failed -- check file permissions and size".to_string(),
                        ),
                        timestamp: now,
                    });
                }
                carminedesktop_vfs::core_ops::VfsEvent::FileLocked { file_name } => {
                    notify::file_locked(&app_handle, &file_name);
                    let _ = obs_tx.send(ObsEvent::Error {
                        drive_id: None,
                        file_name: Some(file_name),
                        remote_path: None,
                        error_type: "file_locked".to_string(),
                        message: "File is locked by another user".to_string(),
                        action_hint: Some(
                            "File is locked by another user -- try again later".to_string(),
                        ),
                        timestamp: now,
                    });
                }
                carminedesktop_vfs::core_ops::VfsEvent::DeleteFailed { file_name, reason } => {
                    notify::delete_failed(&app_handle, &file_name, &reason);
                    let _ = obs_tx.send(ObsEvent::Error {
                        drive_id: None,
                        file_name: Some(file_name),
                        remote_path: None,
                        error_type: "delete_failed".to_string(),
                        message: format!("Delete failed: {reason}"),
                        action_hint: Some(
                            "Delete failed -- the file may still exist on the server".to_string(),
                        ),
                        timestamp: now,
                    });
                }
            }
        }
    });
}

/// Spawn a task that streams `drive:upload-progress` to the frontend from the
/// sync processor's `SyncMetrics` watch channel.  The watch fires roughly once
/// per second when the processor is active; we debounce to 250 ms so a burst
/// of tick updates collapses into a single emit, and skip emits when the
/// snapshot is byte-for-byte equal to the last one we sent.
#[cfg(feature = "desktop")]
fn spawn_upload_progress_emitter(
    rt: &tokio::runtime::Handle,
    app: &tauri::AppHandle,
    drive_id: String,
    mut metrics_rx: tokio::sync::watch::Receiver<carminedesktop_vfs::SyncMetrics>,
) {
    use carminedesktop_core::types::DriveUploadProgressEvent;
    use carminedesktop_vfs::SyncMetrics;
    use tauri::Emitter;

    const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);
    let app_handle = app.clone();

    rt.spawn(async move {
        let mut last_emitted: Option<SyncMetrics> = None;

        loop {
            // Block until the processor publishes a new SyncMetrics snapshot.
            if metrics_rx.changed().await.is_err() {
                // Processor task ended and dropped the sender — exit.
                break;
            }

            // Debounce: coalesce any follow-up changes that arrive within the
            // window into a single emit.  `timeout` returning Err means we hit
            // the window quiet, which is what we want.
            let _ = tokio::time::timeout(DEBOUNCE, async {
                while metrics_rx.changed().await.is_ok() {}
            })
            .await;

            let metrics = metrics_rx.borrow().clone();
            if last_emitted.as_ref() == Some(&metrics) {
                continue;
            }

            let payload = DriveUploadProgressEvent {
                drive_id: drive_id.clone(),
                queue_depth: metrics.queue_depth,
                in_flight: metrics.in_flight,
                failed_count: metrics.failed_count,
                total_uploaded: metrics.total_uploaded,
                total_failed: metrics.total_failed,
                total_deduplicated: metrics.total_deduplicated,
            };
            if let Err(e) = app_handle.emit("drive:upload-progress", &payload) {
                tracing::warn!("drive:upload-progress emit failed: {e}");
            }
            last_emitted = Some(metrics);
        }
    });
}

/// Rewrite the `AppliesTo` filter on the offline context menu registry keys
/// from the currently-enabled mounts. Call after any change to the set of
/// enabled mount paths (toggle / add / remove) so Explorer picks up the new
/// set without requiring an app restart.
#[cfg(feature = "desktop")]
pub(crate) fn refresh_offline_context_menu(app: &tauri::AppHandle) {
    use tauri::Manager;
    let state = app.state::<AppState>();
    let mount_paths: Vec<String> = {
        let config = state.effective_config.lock().unwrap();
        config
            .mounts
            .iter()
            .filter(|m| m.enabled)
            .map(|m| expand_mount_point(&m.mount_point))
            .collect()
    };
    if let Err(e) = shell_integration::register_context_menu(&mount_paths) {
        tracing::warn!("offline context menu refresh failed: {e}");
    }
}

#[cfg(feature = "desktop")]
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

    // Stream debounced `drive:upload-progress` events for this drive.
    spawn_upload_progress_emitter(
        &ctx.rt,
        app,
        ctx.drive_id.clone(),
        sync_handle.subscribe_metrics(),
    );

    let sync_handle_clone = sync_handle.clone();

    let mut handle = carminedesktop_vfs::WinFspMountHandle::mount(
        state.graph.clone(),
        ctx.cache.clone(),
        ctx.inodes.clone(),
        ctx.drive_id.clone(),
        &ctx.mountpoint,
        ctx.rt.clone(),
        Some(ctx.event_tx),
        Some(sync_handle),
        ctx.offline_flag.clone(),
    )
    .map_err(|e| e.to_string())?;

    handle.set_sync_join(sync_join);

    let obs_tx = state.obs_tx.clone();
    spawn_event_forwarder(&ctx.rt, app, ctx.event_rx, obs_tx);

    let observer = Some(handle.delta_observer());
    state.mount_caches.lock().unwrap().insert(
        ctx.drive_id.clone(),
        (
            ctx.cache,
            ctx.inodes,
            observer,
            ctx.offline_manager,
            ctx.offline_flag,
            Some(sync_handle_clone),
        ),
    );

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

        // Resume incomplete offline pin downloads from previous session.
        let offline_mgrs: Vec<_> = {
            use tauri::Manager;
            let state = app_handle.state::<AppState>();
            let caches = state.mount_caches.lock().unwrap();
            caches
                .values()
                .map(|(_, _, _, offline_mgr, _, _)| offline_mgr.clone())
                .collect()
        };
        for mgr in offline_mgrs {
            if let Err(e) = mgr.resume_incomplete().await {
                tracing::warn!("offline resume failed: {e}");
            }
        }

        loop {
            // Snapshot includes mount_id and mount_name for error handling.
            let snapshot: Vec<SyncSnapshotRow> = {
                use tauri::Manager;
                let state = app_handle.state::<AppState>();
                let caches = state.mount_caches.lock().unwrap();
                let config = state.effective_config.lock().unwrap();
                caches
                    .iter()
                    .map(|(drive_id, (c, i, obs, offline_mgr, offline_flag, _sh))| {
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
                            offline_mgr.clone(),
                            offline_flag.clone(),
                            _sh.clone(),
                        )
                    })
                    .collect()
            };

            for (
                drive_id,
                mount_id,
                mount_name,
                cache,
                inodes,
                observer,
                offline_mgr,
                offline_flag,
                _sync_handle,
            ) in &snapshot
            {
                let inodes = inodes.clone();
                let inode_allocator: Arc<dyn Fn(&str) -> u64 + Send + Sync> =
                    Arc::new(move |item_id: &str| inodes.allocate(item_id));

                // Publish "syncing" state transition before the tick runs.  The
                // Solid frontend dedupes on === so repeat values across ticks
                // are free.
                {
                    use carminedesktop_core::types::ObsEvent;
                    use tauri::Manager;
                    let state = app_handle.state::<AppState>();
                    let _ = state.obs_tx.send(ObsEvent::SyncStateChanged {
                        drive_id: drive_id.clone(),
                        state: "syncing".to_string(),
                    });
                }

                let mut sync_succeeded = false;
                match run_delta_sync(
                    &graph,
                    cache,
                    drive_id,
                    &inode_allocator,
                    observer.as_deref(),
                )
                .await
                {
                    Ok(result) => {
                        sync_succeeded = true;
                        notified_403.remove(drive_id.as_str());
                        // Network is working -- exit offline mode.  Swap so we
                        // only emit the `online=true` transition on the tick
                        // that actually recovered connectivity.
                        let was_offline =
                            offline_flag.swap(false, std::sync::atomic::Ordering::Relaxed);
                        if was_offline {
                            use carminedesktop_core::types::ObsEvent;
                            use tauri::Manager;
                            let state = app_handle.state::<AppState>();
                            let _ = state.obs_tx.send(ObsEvent::OnlineStateChanged {
                                drive_id: drive_id.clone(),
                                online: true,
                            });
                        }

                        // Update last_synced timestamp
                        {
                            use tauri::Manager;
                            let state = app_handle.state::<AppState>();
                            let mut ls = state.last_synced.lock().unwrap();
                            ls.insert(drive_id.clone(), chrono::Utc::now().to_rfc3339());
                        }

                        // Publish activity entries for changed/deleted items
                        {
                            use carminedesktop_core::types::ObsEvent;
                            use tauri::Manager;

                            let now = chrono::Utc::now().to_rfc3339();
                            let mut activity_events: Vec<ObsEvent> = Vec::new();

                            for item in &result.changed_items {
                                // Skip folders: files only per CONTEXT.md decision
                                if item.is_folder() {
                                    continue;
                                }
                                let file_path = item
                                    .parent_reference
                                    .as_ref()
                                    .and_then(|pr| pr.path.as_ref())
                                    .map(|p| {
                                        // Graph parent_reference.path: "/drives/{id}/root:/path"
                                        // Strip prefix to get user-visible path
                                        if let Some(idx) = p.find(":/") {
                                            format!("{}/{}", &p[idx + 1..], &item.name)
                                        } else {
                                            format!("/{}", &item.name)
                                        }
                                    })
                                    .unwrap_or_else(|| format!("/{}", &item.name));

                                activity_events.push(ObsEvent::Activity {
                                    drive_id: drive_id.clone(),
                                    file_path,
                                    activity_type: "synced".to_string(),
                                    timestamp: now.clone(),
                                });
                            }

                            for deleted in &result.deleted_items {
                                // Skip items without a resolved name (not in SQLite)
                                if deleted.name.is_empty() {
                                    continue;
                                }
                                let file_path = deleted
                                    .parent_path
                                    .as_ref()
                                    .map(|p| {
                                        // Strip OData prefix (e.g. "/drive/root:/path")
                                        if let Some(idx) = p.find(":/") {
                                            format!("{}/{}", &p[idx + 1..], &deleted.name)
                                        } else {
                                            format!("{}/{}", p, &deleted.name)
                                        }
                                    })
                                    .unwrap_or_else(|| format!("/{}", &deleted.name));

                                activity_events.push(ObsEvent::Activity {
                                    drive_id: drive_id.clone(),
                                    file_path,
                                    activity_type: "deleted".to_string(),
                                    timestamp: now.clone(),
                                });
                            }

                            // Batch-emit activity entries for efficient frontend delivery
                            if !activity_events.is_empty() {
                                let state = app_handle.state::<AppState>();
                                for event in &activity_events {
                                    let _ = state.obs_tx.send(event.clone());
                                }
                                use tauri::Emitter;
                                let _ = app_handle.emit("activity-batch", &activity_events);
                            }
                        }

                        // Check for stale pins: snapshot pin data, then update stale_pins
                        if !result.changed_items.is_empty() {
                            use tauri::Manager;
                            // Snapshot pin item IDs from cache (under mount_caches lock)
                            let pinned_item_ids: Option<std::collections::HashSet<String>> = {
                                let state = app_handle.state::<AppState>();
                                let caches = state.mount_caches.lock().unwrap();
                                caches.get(drive_id).and_then(|(cache, _, _, _, _, _)| {
                                    cache.pin_store.list_all().ok().map(|pins| {
                                        pins.iter().map(|p| p.item_id.clone()).collect()
                                    })
                                })
                            };
                            // Update stale_pins outside mount_caches lock
                            if let Some(pinned_ids) = pinned_item_ids {
                                let state = app_handle.state::<AppState>();
                                let mut stale = state.stale_pins.lock().unwrap();
                                for item in &result.changed_items {
                                    if let Some(pr) = &item.parent_reference
                                        && let Some(parent_id) = &pr.id
                                        && pinned_ids.contains(parent_id)
                                    {
                                        stale.insert((drive_id.clone(), parent_id.clone()));
                                    }
                                }
                            }
                        }

                        // Re-download changed items in pinned folders
                        if !result.changed_items.is_empty() {
                            let offline_mgr_clone = offline_mgr.clone();
                            let changed = result.changed_items.clone();
                            tokio::spawn(async move {
                                if let Err(e) =
                                    offline_mgr_clone.redownload_changed_items(&changed).await
                                {
                                    tracing::warn!("offline re-download failed: {e}");
                                }
                            });
                        }
                    }
                    Err(carminedesktop_core::Error::GraphApi { status: 404, .. }) => {
                        tracing::warn!(
                            "mount '{mount_name}' drive not found during delta sync (404), removing"
                        );
                        let _ = stop_mount(&app_handle, mount_id);
                        remove_mount_from_config(&app_handle, mount_id);
                        notify::mount_orphaned(&app_handle, mount_name);
                        {
                            use carminedesktop_core::types::ObsEvent;
                            use tauri::Manager;
                            let state = app_handle.state::<AppState>();
                            let _ = state.obs_tx.send(ObsEvent::Error {
                                drive_id: Some(drive_id.clone()),
                                file_name: None,
                                remote_path: None,
                                error_type: "drive_deleted".to_string(),
                                message: format!("Drive '{}' was deleted or not found", mount_name),
                                action_hint: Some(
                                    "This drive was deleted or access was revoked".to_string(),
                                ),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            });
                        }
                    }
                    Err(carminedesktop_core::Error::GraphApi { status: 403, .. }) => {
                        if notified_403.insert(drive_id.clone()) {
                            tracing::warn!(
                                "mount '{mount_name}' access denied during delta sync (403)"
                            );
                            notify::mount_access_denied(&app_handle, mount_name);
                            {
                                use carminedesktop_core::types::ObsEvent;
                                use tauri::Manager;
                                let state = app_handle.state::<AppState>();
                                let _ = state.obs_tx.send(ObsEvent::Error {
                                    drive_id: Some(drive_id.clone()),
                                    file_name: None,
                                    remote_path: None,
                                    error_type: "permission_denied".to_string(),
                                    message: format!("Access denied for drive '{}'", mount_name),
                                    action_hint: Some(
                                        "Check your permissions for this drive".to_string(),
                                    ),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                });
                            }
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
                            let _ = state.obs_tx.send(
                                carminedesktop_core::types::ObsEvent::AuthStateChanged {
                                    degraded: true,
                                },
                            );
                        }
                    }
                    Err(carminedesktop_core::Error::Network(_)) => {
                        tracing::warn!("delta sync for {drive_id}: network unavailable");
                        // Swap so we only emit the `online=false` transition on
                        // the tick that actually lost connectivity; subsequent
                        // failing probes are silent.
                        let was_online =
                            !offline_flag.swap(true, std::sync::atomic::Ordering::Relaxed);
                        if was_online {
                            use carminedesktop_core::types::ObsEvent;
                            use tauri::Manager;
                            let state = app_handle.state::<AppState>();
                            let _ = state.obs_tx.send(ObsEvent::OnlineStateChanged {
                                drive_id: drive_id.clone(),
                                online: false,
                            });
                        }
                    }
                    Err(e) => {
                        tracing::error!("delta sync failed for drive {drive_id}: {e}");
                        {
                            use carminedesktop_core::types::ObsEvent;
                            use tauri::Manager;
                            let state = app_handle.state::<AppState>();
                            let _ = state.obs_tx.send(ObsEvent::Error {
                                drive_id: Some(drive_id.clone()),
                                file_name: None,
                                remote_path: None,
                                error_type: "sync_error".to_string(),
                                message: format!("Delta sync failed: {e}"),
                                action_hint: None,
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            });
                        }
                    }
                }

                // Emit the final sync state transition after each tick.  A
                // Network error does not count as sync "error" state — the
                // drive is offline, not broken — so we keep the previous
                // final state for that case by skipping the emit.
                {
                    use carminedesktop_core::types::ObsEvent;
                    use tauri::Manager;
                    let is_offline = offline_flag.load(std::sync::atomic::Ordering::Relaxed);
                    if !is_offline || sync_succeeded {
                        let final_state = if sync_succeeded {
                            "up_to_date"
                        } else {
                            "error"
                        };
                        let state = app_handle.state::<AppState>();
                        let _ = state.obs_tx.send(ObsEvent::SyncStateChanged {
                            drive_id: drive_id.clone(),
                            state: final_state.to_string(),
                        });
                    }
                }
            }

            // Process expired offline pins (once per cycle)
            for (drive_id, _, _, _, _, _, offline_mgr, _, _) in &snapshot {
                match offline_mgr.process_expired() {
                    Ok(expired) if !expired.is_empty() => {
                        use tauri::Manager;
                        let state = app_handle.state::<AppState>();
                        let _ = state.pin_tx.send(pin_events::PinDirty::DriveRefresh {
                            drive_id: drive_id.clone(),
                        });
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!("offline expiry processing failed: {e}"),
                }
            }

            // When any drive is offline, probe more frequently so we recover
            // connectivity within ~15 s instead of the full configured cadence
            // (default 60 s). Cost is one Graph call per interval while
            // offline — negligible, and the retry short-circuit keeps it cheap.
            const OFFLINE_PROBE_SECS: u64 = 15;
            let any_offline = snapshot
                .iter()
                .any(|row| row.7.load(std::sync::atomic::Ordering::Relaxed));
            let wait = if any_offline {
                std::time::Duration::from_secs(OFFLINE_PROBE_SECS.min(interval))
            } else {
                std::time::Duration::from_secs(interval)
            };
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
        .map(|(c, _, _, _, _, _)| c.clone())
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

    if let Err(e) = shell_integration::unregister_context_menu() {
        tracing::warn!("offline context menu unregistration failed: {e}");
    }

    if let Some(ipc) = state.ipc_server.lock().unwrap().take() {
        ipc.stop();
    }

    stop_all_mounts(app);

    tracing::info!("shutdown complete");
}

#[cfg(feature = "desktop")]
pub fn graceful_shutdown(app: &tauri::AppHandle) {
    graceful_shutdown_without_exit(app);
    app.exit(0);
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
        ])
        .unwrap();

        assert_eq!(args.client_id.as_deref(), Some("test-client-id"));
        assert_eq!(args.tenant_id.as_deref(), Some("test-tenant-id"));
        assert_eq!(
            args.config,
            Some(std::path::PathBuf::from("/tmp/test-config.toml"))
        );
        assert_eq!(args.log_level.as_deref(), Some("debug"));
    }

    #[test]
    fn test_cli_args_default_values() {
        let args = CliArgs::try_parse_from(["carminedesktop-app"]).unwrap();

        assert!(args.client_id.is_none());
        assert!(args.tenant_id.is_none());
        assert!(args.config.is_none());
        assert!(args.log_level.is_none());
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
    fn test_preflight_checks_returns_result() {
        // preflight_checks looks up the WinFsp install dir in the registry.
        // On CI runners (winfsp.msi installed in workflow setup) this returns
        // Ok; in environments without WinFsp it returns an Err. Either way it
        // must not panic.
        let _result = preflight_checks();
    }

    #[test]
    fn test_client_id_constant() {
        // CLIENT_ID is the official Carmine Desktop Azure AD app registration
        assert_eq!(CLIENT_ID, "70053421-2c1b-44fe-80f8-d258d0a81133");
        // TENANT_ID is the Carmine Capital Azure AD tenant
        assert_eq!(TENANT_ID, "6a658318-4ef7-4de5-a2a6-d3c1698f272a");
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
        // tenant_id defaults to TENANT_ID when not overridden
        let tenant_id = no_overrides
            .tenant_id
            .clone()
            .or_else(|| Some(TENANT_ID.to_string()));
        assert_eq!(tenant_id.as_deref(), Some(TENANT_ID));
    }
}
