#![cfg(feature = "desktop")]

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn fuse_unavailable(app: &AppHandle) {
    #[cfg(target_os = "linux")]
    let body = "Filesystem mounts require FUSE. Run: sudo apt install fuse3 (Debian/Ubuntu) or equivalent for your distribution.";
    #[cfg(target_os = "macos")]
    let body = "Filesystem mounts require macFUSE. Install it from https://github.com/osxfuse/osxfuse/releases.";
    send(app, "FUSE Not Available", body);
}

pub fn mount_failed(app: &AppHandle, name: &str, reason: &str) {
    send(app, "Mount Failed", &format!("{name}: {reason}"));
}

pub fn mount_success(app: &AppHandle, name: &str, path: &str) {
    send(
        app,
        "Mount Ready",
        &format!("{name} is now available at {path}"),
    );
}

pub fn auto_start_failed(app: &AppHandle, reason: &str) {
    send(
        app,
        "Auto-start",
        &format!("Failed to register auto-start: {reason}"),
    );
}

pub fn auth_expired(app: &AppHandle) {
    send(
        app,
        "Sign-in Expired",
        "Sign-in expired. Click to re-authenticate.",
    );
}

pub fn update_ready(app: &AppHandle, version: &str) {
    let app_name = app_display_name(app);
    send(
        app,
        "Update Available",
        &format!("{app_name} v{version} is ready \u{2014} restart to update"),
    );
}

pub fn up_to_date(app: &AppHandle) {
    let app_name = app_display_name(app);
    send(app, "Up to Date", &format!("{app_name} is up to date"));
}

pub fn update_not_configured(app: &AppHandle) {
    send(
        app,
        "Updates",
        "Update checking is not configured for this build",
    );
}

fn app_display_name(app: &AppHandle) -> String {
    use tauri::Manager;
    app.try_state::<crate::AppState>()
        .map(|s| s.packaged.app_name().to_string())
        .unwrap_or_else(|| "CloudMount".to_string())
}

fn send(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        let _ = e;
        tracing::warn!("failed to send notification: {title}");
    }
}
