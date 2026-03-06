#![cfg(feature = "desktop")]

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

pub fn mount_success(app: &AppHandle, name: &str, path: &str) {
    send(
        app,
        "Mount Ready",
        &format!("{name} is now available at {path}"),
    );
}

pub fn auth_expired(app: &AppHandle) {
    send(
        app,
        "Sign-in Expired",
        "Sign-in expired. Click to re-authenticate.",
    );
}

fn send(app: &AppHandle, title: &str, body: &str) {
    if let Ok(notification) = app.notification().builder().title(title).body(body).show() {
        let _ = notification;
    } else {
        tracing::warn!("failed to send notification: {title}");
    }
}
