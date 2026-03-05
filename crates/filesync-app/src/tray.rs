#![cfg(feature = "desktop")]

use std::sync::Mutex;

use tauri::{
    AppHandle, Manager, WebviewUrl, WebviewWindowBuilder,
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub struct TrayState(pub Mutex<tauri::tray::TrayIcon>);

pub fn setup(app: &AppHandle, app_name: &str) -> tauri::Result<()> {
    let open_item = MenuItemBuilder::with_id("open_folder", "Open Mount Folder").build(app)?;
    let settings_item = MenuItemBuilder::with_id("settings", "Settings\u{2026}").build(app)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let signout_item = MenuItemBuilder::with_id("sign_out", "Sign Out").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", &format!("Quit {app_name}")).build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&open_item, &settings_item, &sep, &signout_item, &quit_item])
        .build()?;

    let tray = TrayIconBuilder::with_id("filesync-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip(app_name)
        .menu(&menu)
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
                open_or_focus_window(tray.app_handle(), "settings", "Settings", "settings.html");
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
                let expanded = filesync_core::config::expand_mount_point(&mc.mount_point);
                let _ = open::that(&expanded);
            }
        }
        return;
    }

    match id {
        "add_mount" => {
            open_or_focus_window(app, "wizard", "Setup", "wizard.html");
        }
        "settings" => {
            open_or_focus_window(app, "settings", "Settings", "settings.html");
        }
        "sign_out" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = crate::commands::sign_out(app).await {
                    tracing::error!("sign out failed: {e}");
                }
            });
        }
        "quit" => {
            crate::graceful_shutdown(app);
        }
        _ => {}
    }
}

pub fn open_or_focus_window(app: &AppHandle, label: &str, title: &str, url: &str) {
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        let _ = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
            .title(title)
            .inner_size(800.0, 600.0)
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

    let (mount_entries, app_name, auth_degraded) = {
        let config = app_state.effective_config.lock().unwrap();
        let active_mounts = app_state.mounts.lock().unwrap();
        let entries: Vec<(String, String, String)> = config
            .mounts
            .iter()
            .map(|mc| {
                let status = if active_mounts.contains_key(&mc.id) {
                    "Mounted"
                } else if mc.enabled && mc.drive_id.is_some() {
                    "Unmounted"
                } else {
                    "Error"
                };
                (
                    format!("mount_{}", mc.id),
                    format!("{} \u{2014} {status}", mc.name),
                    mc.id.clone(),
                )
            })
            .collect();
        let name = app_state.packaged.app_name().to_string();
        let degraded = app_state
            .auth_degraded
            .load(std::sync::atomic::Ordering::Relaxed);
        (entries, name, degraded)
    };

    let tooltip = if auth_degraded {
        format!("{app_name} \u{2014} Re-authentication required")
    } else {
        let mounted = mount_entries
            .iter()
            .filter(|(_, label, _)| label.contains("Mounted") && !label.contains("Unmounted"))
            .count();
        if mounted > 0 {
            format!("{app_name} \u{2014} {mounted} drive(s) mounted")
        } else {
            app_name.clone()
        }
    };

    let result: tauri::Result<()> = (|| {
        let mut builder = MenuBuilder::new(app);

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

        let add_mount = MenuItemBuilder::with_id("add_mount", "Add Mount\u{2026}").build(app)?;
        let settings = MenuItemBuilder::with_id("settings", "Settings\u{2026}").build(app)?;
        builder = builder.item(&add_mount).item(&settings);

        let sep2 = PredefinedMenuItem::separator(app)?;
        builder = builder.item(&sep2);

        let sign_out = MenuItemBuilder::with_id("sign_out", "Sign Out").build(app)?;
        let quit = MenuItemBuilder::with_id("quit", &format!("Quit {app_name}")).build(app)?;
        builder = builder.item(&sign_out).item(&quit);

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
