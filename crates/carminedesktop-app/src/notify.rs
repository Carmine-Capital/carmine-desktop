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

pub fn mounts_summary(app: &AppHandle, succeeded: usize, failed: usize) {
    let body = match (succeeded, failed) {
        (s, 0) if s > 0 => format!("{s} drive{} mounted", if s == 1 { "" } else { "s" }),
        (s, f) if s > 0 && f > 0 => format!(
            "{s} drive{} mounted, {f} failed",
            if s == 1 { "" } else { "s" }
        ),
        (0, f) if f > 0 => format!("Failed to mount {f} drive{}", if f == 1 { "" } else { "s" }),
        _ => return,
    };
    send(app, "Mounts Ready", &body);
}

pub fn mount_not_found(app: &AppHandle, name: &str) {
    send(
        app,
        "Mount Removed",
        &format!("'{name}' is no longer accessible and has been removed from your configuration"),
    );
}

pub fn mount_orphaned(app: &AppHandle, name: &str) {
    send(
        app,
        "Mount Removed",
        &format!("'{name}' was deleted or moved and has been removed from your configuration"),
    );
}

pub fn mount_access_denied(app: &AppHandle, name: &str) {
    send(
        app,
        "Mount Skipped",
        &format!("No access to '{name}' \u{2014} check your permissions"),
    );
}

pub fn auto_start_failed(app: &AppHandle, reason: &str) {
    send(
        app,
        "Auto-start",
        &format!("Failed to register auto-start: {reason}"),
    );
}

pub fn sign_out_failed(app: &AppHandle, reason: &str) {
    send(
        app,
        "Sign Out Failed",
        &format!("Sign out encountered an error: {reason}"),
    );
}

pub fn auth_expired(app: &AppHandle) {
    send(
        app,
        "Sign-in Expired",
        "Sign-in expired. Open Carmine Desktop to re-authenticate.",
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

pub fn update_check_failed(app: &AppHandle) {
    send(
        app,
        "Update Check Failed",
        "Could not check for updates. Try again later.",
    );
}

pub fn update_not_configured(app: &AppHandle) {
    send(
        app,
        "Updates",
        "Update checking is not configured for this build",
    );
}

pub fn conflict_detected(app: &AppHandle, file_name: &str, conflict_name: &str) {
    send(
        app,
        "Sync Conflict",
        &format!(
            "'{file_name}' was modified on another device. Your version was saved as '{conflict_name}'."
        ),
    );
}

pub fn writeback_failed(app: &AppHandle, file_name: &str) {
    send(
        app,
        "Save Failed",
        &format!("Failed to save changes to '{file_name}'. Your edits may be lost."),
    );
}

pub fn upload_failed(app: &AppHandle, file_name: &str, reason: &str) {
    send(
        app,
        "Upload Failed",
        &format!("Failed to upload '{file_name}': {reason}"),
    );
}

pub fn delete_failed(app: &AppHandle, file_name: &str, reason: &str) {
    send(
        app,
        "Delete Failed",
        &format!("Failed to delete '{file_name}': {reason}"),
    );
}

pub fn file_locked(app: &AppHandle, file_name: &str) {
    send(
        app,
        "File Locked",
        &format!(
            "'{file_name}' is being edited online. Local changes will be saved as a separate copy."
        ),
    );
}

pub fn deep_link_failed(app: &AppHandle, reason: &str) {
    send(
        app,
        "Open in SharePoint",
        &format!("Could not open file: {reason}"),
    );
}

pub fn files_recovered(app: &AppHandle, count: usize, path: &str) {
    send(
        app,
        "Files Recovered",
        &format!(
            "{count} unsaved file(s) recovered to {path}. These files were not uploaded before the last shutdown."
        ),
    );
}

pub fn offline_pin_complete(app: &AppHandle, folder_name: &str) {
    send(
        app,
        "Available Offline",
        &format!("'{folder_name}' is now available offline"),
    );
}

pub fn offline_pin_rejected(app: &AppHandle, folder_name: &str, reason: &str) {
    send(
        app,
        "Offline Unavailable",
        &format!("Cannot make '{folder_name}' available offline: {reason}"),
    );
}

pub fn offline_pin_failed(app: &AppHandle, folder_name: &str, reason: &str) {
    send(
        app,
        "Offline Error",
        &format!("Failed to download '{folder_name}' for offline use: {reason}"),
    );
}

pub fn offline_unpin_complete(app: &AppHandle, folder_name: &str) {
    send(
        app,
        "Space Freed",
        &format!("'{folder_name}' is no longer pinned for offline use"),
    );
}

fn app_display_name(_app: &AppHandle) -> String {
    "Carmine Desktop".to_string()
}

pub(crate) fn send(app: &AppHandle, title: &str, body: &str) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!("failed to send notification '{title}': {e}");
    }
}
