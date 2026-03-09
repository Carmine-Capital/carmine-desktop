use std::sync::Mutex;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;
use tokio_util::sync::CancellationToken;

const STARTUP_DELAY_SECS: u64 = 10;
const CHECK_INTERVAL_SECS: u64 = 4 * 60 * 60; // 4 hours

pub struct UpdateState {
    pub pending_version: Mutex<Option<String>>,
    pub cancel: Mutex<Option<CancellationToken>>,
}

impl UpdateState {
    pub fn new() -> Self {
        Self {
            pending_version: Mutex::new(None),
            cancel: Mutex::new(None),
        }
    }
}

/// Check the configured endpoint for an update. If found, download and install it
/// (the update takes effect on next restart). Returns `Some(version)` if an update
/// was downloaded, `None` if already up to date.
pub async fn check_for_update(app: &AppHandle) -> Result<Option<String>, String> {
    use tauri::Manager;

    // If an update is already pending, skip
    if let Some(state) = app.try_state::<UpdateState>()
        && state.pending_version.lock().unwrap().is_some()
    {
        return Ok(None);
    }

    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;

    let Some(update) = update else {
        return Ok(None);
    };

    let version = update.version.clone();

    // download_and_install downloads the bundle, verifies the signature, and stages
    // the update for installation on next app restart.
    update
        .download_and_install(
            |chunk_length, content_length| {
                tracing::debug!(
                    "update download: {chunk_length} bytes (total: {content_length:?})"
                );
            },
            || {
                tracing::debug!("update download finished");
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    // Record pending version for tray menu display
    if let Some(state) = app.try_state::<UpdateState>() {
        *state.pending_version.lock().unwrap() = Some(version.clone());
    }

    Ok(Some(version))
}

/// Spawn a background task that checks for updates periodically.
/// First check runs after a 10-second delay, then every 4 hours.
pub fn spawn_update_checker(app: AppHandle) {
    use tauri::Manager;

    let cancel = CancellationToken::new();

    if let Some(state) = app.try_state::<UpdateState>() {
        *state.cancel.lock().unwrap() = Some(cancel.clone());
    }

    tauri::async_runtime::spawn(async move {
        // Wait before first check to let mounts initialize
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(std::time::Duration::from_secs(STARTUP_DELAY_SECS)) => {}
        }

        loop {
            match check_for_update(&app).await {
                Ok(Some(version)) => {
                    tracing::info!("update v{version} downloaded and ready to install");
                    crate::notify::update_ready(&app, &version);
                    crate::tray::update_tray_menu(&app);
                    // Update downloaded — stop periodic checking
                    return;
                }
                Ok(None) => {
                    tracing::debug!("up to date (v{})", env!("CARGO_PKG_VERSION"));
                }
                Err(e) => {
                    tracing::warn!("update check failed: {e}");
                }
            }

            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(std::time::Duration::from_secs(CHECK_INTERVAL_SECS)) => {}
            }
        }
    });
}

/// Perform graceful shutdown then restart to apply the pending update.
pub fn install_and_relaunch(app: &AppHandle) {
    crate::graceful_shutdown_without_exit(app);
    app.restart();
}

/// Handle a manual "Check for Updates" click from the tray menu.
pub async fn handle_manual_check(app: &AppHandle) {
    use tauri::Manager;

    // If already pending, do nothing — tray already shows "Restart to Update"
    if let Some(state) = app.try_state::<UpdateState>()
        && state.pending_version.lock().unwrap().is_some()
    {
        return;
    }

    // Try to create the updater — fails if endpoints/pubkey are not configured
    if app.updater().is_err() {
        crate::notify::update_not_configured(app);
        return;
    }

    match check_for_update(app).await {
        Ok(Some(version)) => {
            tracing::info!("update v{version} downloaded and ready to install");
            crate::notify::update_ready(app, &version);
            crate::tray::update_tray_menu(app);
        }
        Ok(None) => {
            crate::notify::up_to_date(app);
        }
        Err(e) => {
            tracing::warn!("manual update check failed: {e}");
            crate::notify::update_check_failed(app);
        }
    }
}

/// Cancel the update checker background task.
pub fn cancel_checker(app: &AppHandle) {
    use tauri::Manager;

    if let Some(state) = app.try_state::<UpdateState>()
        && let Some(cancel) = state.cancel.lock().unwrap().take()
    {
        cancel.cancel();
    }
}
