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

pub fn sync_conflict(app: &AppHandle, file_name: &str) {
    send(
        app,
        "Sync Conflict",
        &format!("Conflict detected: {file_name}. A .conflict copy has been created."),
    );
}

pub fn auth_expired(app: &AppHandle) {
    send(
        app,
        "Sign-in Expired",
        "Sign-in expired. Click to re-authenticate.",
    );
}

pub fn network_offline(app: &AppHandle) {
    send(
        app,
        "Offline",
        "Offline — cached files remain accessible. Changes will sync when connectivity returns.",
    );
}

fn send(app: &AppHandle, title: &str, body: &str) {
    if let Ok(notification) = app.notification().builder().title(title).body(body).show() {
        let _ = notification;
    } else {
        tracing::warn!("failed to send notification: {title}");
    }
}
