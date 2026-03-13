use std::sync::Mutex;

use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub struct TrayState(pub Mutex<tauri::tray::TrayIcon>);

pub fn setup(app: &AppHandle, app_name: &str) -> tauri::Result<()> {
    let settings_item = MenuItemBuilder::with_id("settings", "Settings\u{2026}").build(app)?;
    let update_item =
        MenuItemBuilder::with_id("check_for_updates", "Check for Updates").build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let signout_item = MenuItemBuilder::with_id("sign_out", "Sign Out").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", format!("Quit {app_name}")).build(app)?;

    let menu = MenuBuilder::new(app).item(&settings_item);
    let menu = menu
        .item(&update_item)
        .item(&sep)
        .item(&signout_item)
        .item(&quit_item)
        .build()?;

    let icon = app
        .default_window_icon()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?
        .clone();

    let tray = TrayIconBuilder::with_id("cloudmount-tray")
        .icon(icon)
        .tooltip(app_name)
        .menu(&menu)
        // Linux AppIndicator backend may not fire TrayIconEvent::Click for
        // left-click. All functionality is accessible via the right-click menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id().as_ref());
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let authenticated = app
                    .try_state::<crate::AppState>()
                    .map(|s| s.authenticated.load(std::sync::atomic::Ordering::Relaxed))
                    .unwrap_or(false);
                if authenticated {
                    open_or_focus_window(app, "settings", "Settings", "settings.html");
                } else {
                    open_or_focus_window(app, "wizard", "Setup", "wizard.html");
                }
            }
        })
        .build(app)?;

    app.manage(TrayState(Mutex::new(tray)));
    Ok(())
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    if let Some(mount_id) = id.strip_prefix("mount_") {
        if let Some(state) = app.try_state::<crate::AppState>() {
            let config = state.effective_config.lock().unwrap();
            if let Some(mc) = config.mounts.iter().find(|m| m.id == mount_id) {
                let expanded = cloudmount_core::config::expand_mount_point(&mc.mount_point);
                let _ = crate::open_with_clean_env(&expanded);
            }
        }
        return;
    }

    match id {
        "sign_in" => {
            open_or_focus_wizard(app, false);
        }
        "add_mount" => {
            open_or_focus_wizard(app, true);
        }
        "settings" => {
            open_or_focus_window(app, "settings", "Settings", "settings.html");
        }
        "check_for_updates" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::update::handle_manual_check(&app).await;
            });
        }
        "restart_to_update" => {
            let app = app.clone();
            std::thread::spawn(move || {
                crate::update::install_and_relaunch(&app);
            });
        }
        "re_authenticate" => {
            open_or_focus_wizard(app, false);
        }
        "sign_out" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
                let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
                app.dialog()
                    .message("Sign out? All mounts will stop.")
                    .title("Sign Out")
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        "Sign Out".to_string(),
                        "Cancel".to_string(),
                    ))
                    .show(move |confirmed| {
                        let _ = tx.send(confirmed);
                    });
                match rx.await {
                    Ok(true) => {
                        if let Err(e) = crate::commands::sign_out(app).await {
                            tracing::error!("sign out failed: {e}");
                        }
                    }
                    Ok(false) | Err(_) => {
                        tracing::debug!("sign-out cancelled by user");
                    }
                }
            });
        }
        "quit" => {
            crate::graceful_shutdown(app);
        }
        _ => {}
    }
}

pub fn open_or_focus_wizard(app: &AppHandle, add_mount: bool) {
    if let Some(win) = app.get_webview_window("wizard") {
        if add_mount {
            let _ = win.emit("navigate-add-mount", ());
        }
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        let _ = WebviewWindowBuilder::new(app, "wizard", WebviewUrl::App("wizard.html".into()))
            .title("Setup")
            .inner_size(800.0, 600.0)
            .min_inner_size(640.0, 480.0)
            .center()
            .build();
    }
}

pub fn open_or_focus_window(app: &AppHandle, label: &str, title: &str, url: &str) {
    if let Some(win) = app.get_webview_window(label) {
        if label == "settings" {
            let _ = win.emit("refresh-settings", ());
        }
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        let _ = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .title(title)
            .inner_size(800.0, 600.0)
            .min_inner_size(640.0, 480.0)
            .center()
            .build();
    }
}

pub fn update_tray_menu(app: &AppHandle) {
    let Some(app_state) = app.try_state::<crate::AppState>() else {
        return;
    };
    let Some(tray_state) = app.try_state::<TrayState>() else {
        return;
    };

    let pending_update_version = app
        .try_state::<crate::update::UpdateState>()
        .and_then(|s| s.pending_version.lock().ok().and_then(|g| g.clone()));

    let (mount_entries, app_name, auth_degraded, authenticated) = {
        let Ok(config) = app_state.effective_config.lock() else {
            tracing::warn!("update_tray_menu: effective_config mutex poisoned, skipping update");
            return;
        };
        let Ok(active_mounts) = app_state.mounts.lock() else {
            tracing::warn!("update_tray_menu: mounts mutex poisoned, skipping update");
            return;
        };
        let entries: Vec<(String, String, bool)> = config
            .mounts
            .iter()
            .map(|mc| {
                let is_mounted = active_mounts.contains_key(&mc.id);
                let status = if is_mounted {
                    "Mounted"
                } else if mc.enabled && mc.drive_id.is_some() {
                    "Unmounted"
                } else {
                    "Error"
                };
                (
                    format!("mount_{}", mc.id),
                    format!("{} \u{2014} {status}", mc.name),
                    is_mounted,
                )
            })
            .collect();
        let name = "CloudMount".to_string();
        let degraded = app_state
            .auth_degraded
            .load(std::sync::atomic::Ordering::Relaxed);
        let auth = app_state
            .authenticated
            .load(std::sync::atomic::Ordering::Relaxed);
        (entries, name, degraded, auth)
    };

    let tooltip = if auth_degraded {
        format!("{app_name} \u{2014} Re-authentication required")
    } else {
        let mounted = mount_entries
            .iter()
            .filter(|(_, _, is_mounted)| *is_mounted)
            .count();
        if mounted > 0 {
            format!("{app_name} \u{2014} {mounted} drive(s) mounted")
        } else {
            app_name.clone()
        }
    };

    let result: tauri::Result<()> = (|| {
        let mut builder = MenuBuilder::new(app);

        if authenticated {
            let mut mount_items = Vec::new();
            for (item_id, label, _) in &mount_entries {
                let item = MenuItemBuilder::with_id(item_id, label).build(app)?;
                mount_items.push(item);
            }
            for item in &mount_items {
                builder = builder.item(item);
            }

            let sep1 = PredefinedMenuItem::separator(app)?;
            builder = builder.item(&sep1);

            let add_mount =
                MenuItemBuilder::with_id("add_mount", "Add Mount\u{2026}").build(app)?;
            builder = builder.item(&add_mount);
        }

        let settings = MenuItemBuilder::with_id("settings", "Settings\u{2026}").build(app)?;
        builder = builder.item(&settings);

        if let Some(ref version) = pending_update_version {
            let update_item = MenuItemBuilder::with_id(
                "restart_to_update",
                format!("Restart to Update (v{version})"),
            )
            .build(app)?;
            builder = builder.item(&update_item);
        } else {
            let update_item =
                MenuItemBuilder::with_id("check_for_updates", "Check for Updates").build(app)?;
            builder = builder.item(&update_item);
        }

        let sep2 = PredefinedMenuItem::separator(app)?;
        builder = builder.item(&sep2);

        let quit = MenuItemBuilder::with_id("quit", format!("Quit {app_name}")).build(app)?;
        if authenticated {
            if auth_degraded {
                let re_auth =
                    MenuItemBuilder::with_id("re_authenticate", "Re-authenticate\u{2026}")
                        .build(app)?;
                builder = builder.item(&re_auth);
            }
            let sign_out = MenuItemBuilder::with_id("sign_out", "Sign Out").build(app)?;
            builder = builder.item(&sign_out).item(&quit);
        } else {
            let sign_in = MenuItemBuilder::with_id("sign_in", "Sign In\u{2026}").build(app)?;
            builder = builder.item(&sign_in).item(&quit);
        }

        let menu = builder.build()?;

        if let Ok(tray) = tray_state.0.lock() {
            let _ = tray.set_menu(Some(menu));
            let _ = tray.set_tooltip(Some(&tooltip));
        }

        Ok(())
    })();

    if let Err(e) = result {
        tracing::error!("failed to rebuild tray menu: {e}");
    }
}
