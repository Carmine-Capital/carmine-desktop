use carminedesktop_core::config::{
    expand_mount_point, AccountMetadata, EffectiveConfig, MountConfig, UserConfig,
    UserGeneralSettings,
};
use std::env;
use std::path::PathBuf;

fn create_temp_config_file() -> PathBuf {
    let temp_dir = env::temp_dir();
    let file_name = format!("test_config_{}.toml", uuid::Uuid::new_v4());
    temp_dir.join(file_name)
}

#[test]
fn test_user_config_load_empty() -> carminedesktop_core::Result<()> {
    let user = UserConfig::load("")?;

    assert!(user.general.is_none());
    assert!(user.mounts.is_empty());
    assert!(user.accounts.is_empty());

    Ok(())
}

#[test]
fn test_effective_config_user_only() -> carminedesktop_core::Result<()> {
    let user_toml = r#"
[general]
auto_start = true
cache_max_size = "12GB"
sync_interval_secs = 150
metadata_ttl_secs = 200
cache_dir = "/custom/cache"
log_level = "warn"
notifications = false
"#;

    let user = UserConfig::load(user_toml)?;
    let effective = EffectiveConfig::build(&user);

    assert!(effective.auto_start);
    assert_eq!(effective.cache_max_size, "12GB");
    assert_eq!(effective.sync_interval_secs, 150);
    assert_eq!(effective.metadata_ttl_secs, 200);
    assert_eq!(effective.cache_dir, Some("/custom/cache".to_string()));
    assert_eq!(effective.log_level, "warn");
    assert!(!effective.notifications);

    Ok(())
}

#[test]
fn test_effective_config_defaults() -> carminedesktop_core::Result<()> {
    let user = UserConfig::load("")?;
    let effective = EffectiveConfig::build(&user);

    assert!(effective.auto_start);
    assert_eq!(effective.cache_max_size, "5GB");
    assert_eq!(effective.sync_interval_secs, 60);
    assert_eq!(effective.metadata_ttl_secs, 60);
    assert_eq!(effective.log_level, "info");
    assert!(effective.notifications);
    assert_eq!(effective.root_dir, "Cloud");

    Ok(())
}

#[cfg(unix)]
#[test]
fn test_expand_mount_point_home() {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());

    let template = "{home}/OneDrive";
    let expanded = expand_mount_point(template);

    assert!(expanded.starts_with(&home));
    assert!(!expanded.contains("{home}"));
    assert!(expanded.ends_with("/OneDrive"));
}

#[test]
fn test_expand_mount_point_tilde() {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());

    let expanded = expand_mount_point("~/Cloud/OneDrive");
    assert!(!expanded.contains('~'));
    assert!(std::path::Path::new(&expanded).ends_with("Cloud/OneDrive"));
    assert!(expanded.starts_with(&home));
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_expand_mount_point_no_placeholder() {
    let path = "/mnt/carminedesktop";
    let expanded = expand_mount_point(path);

    assert_eq!(expanded, path);
}

#[test]
fn test_reset_setting() -> carminedesktop_core::Result<()> {
    let user_toml = r#"
[general]
auto_start = true
cache_max_size = "10GB"
sync_interval_secs = 120
metadata_ttl_secs = 300
cache_dir = "/custom/cache"
log_level = "debug"
notifications = false
"#;

    let mut user = UserConfig::load(user_toml)?;

    assert!(user.general.as_ref().unwrap().auto_start.is_some());
    user.reset_setting("auto_start");
    assert!(user.general.as_ref().unwrap().auto_start.is_none());

    assert!(user.general.as_ref().unwrap().cache_max_size.is_some());
    user.reset_setting("cache_max_size");
    assert!(user.general.as_ref().unwrap().cache_max_size.is_none());

    assert!(user.general.as_ref().unwrap().sync_interval_secs.is_some());
    user.reset_setting("sync_interval_secs");
    assert!(user.general.as_ref().unwrap().sync_interval_secs.is_none());

    assert!(user.general.as_ref().unwrap().metadata_ttl_secs.is_some());
    user.reset_setting("metadata_ttl_secs");
    assert!(user.general.as_ref().unwrap().metadata_ttl_secs.is_none());

    assert!(user.general.as_ref().unwrap().cache_dir.is_some());
    user.reset_setting("cache_dir");
    assert!(user.general.as_ref().unwrap().cache_dir.is_none());

    assert!(user.general.as_ref().unwrap().log_level.is_some());
    user.reset_setting("log_level");
    assert!(user.general.as_ref().unwrap().log_level.is_none());

    assert!(user.general.as_ref().unwrap().notifications.is_some());
    user.reset_setting("notifications");
    assert!(user.general.as_ref().unwrap().notifications.is_none());

    Ok(())
}

#[test]
fn test_user_config_save_and_load_roundtrip() -> carminedesktop_core::Result<()> {
    let config_path = create_temp_config_file();

    let user = UserConfig {
        general: Some(UserGeneralSettings {
            auto_start: Some(true),
            cache_max_size: Some("8GB".to_string()),
            sync_interval_secs: Some(120),
            metadata_ttl_secs: Some(300),
            cache_dir: Some("/custom/cache".to_string()),
            log_level: Some("debug".to_string()),
            notifications: Some(false),
            root_dir: None,

            register_file_associations: None,
            file_handler_overrides: None,
            explorer_nav_pane: None,
            offline_ttl_secs: None,
            offline_max_folder_size: None,
        }),
        mounts: vec![MountConfig {
            id: "user-mount1".to_string(),
            name: "My Drive".to_string(),
            mount_type: "drive".to_string(),
            mount_point: "/mnt/mydrive".to_string(),
            enabled: true,
            account_id: Some("account-123".to_string()),
            drive_id: Some("drive-456".to_string()),
            site_id: None,
            site_name: None,
            library_name: None,
        }],
        accounts: vec![AccountMetadata {
            id: "account-123".to_string(),
            email: Some("user@example.com".to_string()),
            display_name: Some("John Doe".to_string()),
            tenant_id: Some("tenant-456".to_string()),
        }],
    };

    user.save_to_file(&config_path)?;
    assert!(config_path.exists());

    let loaded = UserConfig::load_from_file(&config_path)?;

    assert!(loaded.general.is_some());
    let general = loaded.general.unwrap();
    assert_eq!(general.auto_start, Some(true));
    assert_eq!(general.cache_max_size, Some("8GB".to_string()));
    assert_eq!(general.sync_interval_secs, Some(120));
    assert_eq!(general.metadata_ttl_secs, Some(300));
    assert_eq!(general.cache_dir, Some("/custom/cache".to_string()));
    assert_eq!(general.log_level, Some("debug".to_string()));
    assert_eq!(general.notifications, Some(false));

    assert_eq!(loaded.mounts.len(), 1);
    assert_eq!(loaded.mounts[0].id, "user-mount1");
    assert_eq!(loaded.mounts[0].name, "My Drive");

    assert_eq!(loaded.accounts.len(), 1);
    assert_eq!(
        loaded.accounts[0].email,
        Some("user@example.com".to_string())
    );

    let _ = std::fs::remove_file(&config_path);

    Ok(())
}

#[test]
fn test_mount_config_account_id_stored() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("")?;
    user.add_onedrive_mount("drive-abc", "/mnt/onedrive", Some("acc-1".to_string()))?;

    assert_eq!(user.mounts.len(), 1);
    assert_eq!(user.mounts[0].account_id, Some("acc-1".to_string()));

    let config_path = create_temp_config_file();
    user.save_to_file(&config_path)?;
    let loaded = UserConfig::load_from_file(&config_path)?;
    assert_eq!(loaded.mounts[0].account_id, Some("acc-1".to_string()));
    let _ = std::fs::remove_file(&config_path);

    Ok(())
}

#[test]
fn test_mount_config_account_id_none_compat() -> carminedesktop_core::Result<()> {
    // TOML without account_id field should deserialize as None
    let toml = r#"
[[mounts]]
id = "od-1"
name = "OneDrive"
type = "drive"
mount_point = "/mnt/od"
enabled = true
drive_id = "drive-1"
"#;
    let user = UserConfig::load(toml)?;
    assert_eq!(user.mounts.len(), 1);
    assert_eq!(user.mounts[0].account_id, None);

    Ok(())
}

#[test]
fn test_old_config_fields_silently_ignored() -> carminedesktop_core::Result<()> {
    // mount_overrides and dismissed_packaged_mounts from old configs are silently dropped by serde
    let old_config_toml = r#"
[general]
auto_start = true

[[mounts]]
id = "od-1"
name = "OneDrive"
type = "drive"
mount_point = "/mnt/od"
enabled = true
drive_id = "drive-1"

[[mount_overrides]]
id = "pkg-1"
enabled = false

dismissed_packaged_mounts = ["pkg-2"]
"#;

    let user = UserConfig::load(old_config_toml)?;
    assert!(user.general.as_ref().unwrap().auto_start == Some(true));
    assert_eq!(user.mounts.len(), 1);
    // mount_overrides and dismissed_packaged_mounts are dropped — config still loads fine

    Ok(())
}

// --- expand_mount_point path normalization tests ---

#[test]
fn test_expand_mount_point_strips_trailing_forward_slash() {
    let expanded = expand_mount_point("~/Cloud/MyDrive/");
    assert!(
        !expanded.ends_with('/'),
        "trailing / should be stripped, got: {expanded}"
    );
    assert!(
        expanded.ends_with("MyDrive"),
        "path should end with MyDrive, got: {expanded}"
    );
}

#[test]
fn test_expand_mount_point_strips_trailing_backslash() {
    let expanded = expand_mount_point("~/Cloud/MyDrive\\");
    assert!(
        !expanded.ends_with('\\'),
        "trailing \\ should be stripped, got: {expanded}"
    );
}

#[test]
fn test_expand_mount_point_no_trailing_sep_unchanged() {
    let expanded = expand_mount_point("~/Cloud/MyDrive");
    assert!(
        expanded.ends_with("MyDrive"),
        "path without trailing sep should end with MyDrive, got: {expanded}"
    );
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_expand_mount_point_literal_path_strips_trailing_slash() {
    // Literal path (no ~ or {home}) — still normalized
    let expanded = expand_mount_point("/mnt/carminedesktop/");
    assert_eq!(
        expanded, "/mnt/carminedesktop",
        "trailing / on literal path should be stripped"
    );
}

#[test]
#[cfg(not(target_os = "windows"))]
fn test_expand_mount_point_literal_path_no_trailing_unchanged() {
    let expanded = expand_mount_point("/mnt/carminedesktop");
    assert_eq!(expanded, "/mnt/carminedesktop");
}

#[test]
fn test_expand_mount_point_home_placeholder_strips_trailing_slash() {
    let expanded = expand_mount_point("{home}/Cloud/");
    assert!(
        !expanded.ends_with('/'),
        "trailing / after {{home}} expansion should be stripped, got: {expanded}"
    );
}

#[test]
fn test_add_onedrive_mount_strips_trailing_sep() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("")?;
    user.add_onedrive_mount("drive-abc", "/mnt/onedrive/", Some("acc-1".to_string()))?;

    assert_eq!(user.mounts.len(), 1);
    assert!(
        !user.mounts[0].mount_point.ends_with('/'),
        "stored mount_point should not have trailing /, got: {}",
        user.mounts[0].mount_point
    );

    Ok(())
}

#[test]
fn test_add_sharepoint_mount_strips_trailing_sep() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("")?;
    user.add_sharepoint_mount(
        "site-1",
        "drive-1",
        "MySite",
        "Documents",
        "/mnt/sharepoint/",
        None,
    )?;

    assert_eq!(user.mounts.len(), 1);
    assert!(
        !user.mounts[0].mount_point.ends_with('/'),
        "stored mount_point should not have trailing /, got: {}",
        user.mounts[0].mount_point
    );

    Ok(())
}

// --- explorer_nav_pane config tests ---

#[test]
fn test_config_explorer_nav_pane_default() -> carminedesktop_core::Result<()> {
    let user = UserConfig::load("")?;
    let effective = EffectiveConfig::build(&user);

    // Default: true on Windows, false elsewhere
    #[cfg(target_os = "windows")]
    assert!(effective.explorer_nav_pane);
    #[cfg(not(target_os = "windows"))]
    assert!(!effective.explorer_nav_pane);

    Ok(())
}

#[test]
fn test_config_explorer_nav_pane_explicit_true() -> carminedesktop_core::Result<()> {
    let user = UserConfig::load("[general]\nexplorer_nav_pane = true")?;
    let effective = EffectiveConfig::build(&user);
    assert!(effective.explorer_nav_pane);

    Ok(())
}

#[test]
fn test_config_explorer_nav_pane_explicit_false() -> carminedesktop_core::Result<()> {
    let user = UserConfig::load("[general]\nexplorer_nav_pane = false")?;
    let effective = EffectiveConfig::build(&user);
    assert!(!effective.explorer_nav_pane);

    Ok(())
}

#[test]
fn test_config_explorer_nav_pane_reset() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("[general]\nexplorer_nav_pane = true")?;
    assert_eq!(user.general.as_ref().unwrap().explorer_nav_pane, Some(true));

    user.reset_setting("explorer_nav_pane");
    assert!(user.general.as_ref().unwrap().explorer_nav_pane.is_none());

    // After reset, effective config should use platform default
    let effective = EffectiveConfig::build(&user);
    #[cfg(target_os = "windows")]
    assert!(effective.explorer_nav_pane);
    #[cfg(not(target_os = "windows"))]
    assert!(!effective.explorer_nav_pane);

    Ok(())
}

#[test]
fn test_config_explorer_nav_pane_roundtrip() -> carminedesktop_core::Result<()> {
    let config_path = create_temp_config_file();

    let user = UserConfig {
        general: Some(UserGeneralSettings {
            explorer_nav_pane: Some(true),
            ..UserGeneralSettings::default()
        }),
        mounts: vec![],
        accounts: vec![],
    };

    user.save_to_file(&config_path)?;
    let loaded = UserConfig::load_from_file(&config_path)?;
    assert_eq!(
        loaded.general.as_ref().unwrap().explorer_nav_pane,
        Some(true)
    );

    let _ = std::fs::remove_file(&config_path);

    Ok(())
}

// --- Issue #4: Library toggle config tests ---

#[test]
fn test_config_add_sharepoint_mount_creates_correct_config() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("")?;
    user.accounts.push(AccountMetadata {
        id: "acc-1".to_string(),
        email: Some("user@contoso.com".to_string()),
        display_name: None,
        tenant_id: None,
    });

    user.add_sharepoint_mount(
        "site-contoso-123",
        "drive-lib-456",
        "Contoso Team",
        "Documents",
        "/mnt/contoso/Documents",
        Some("acc-1".to_string()),
    )?;

    assert_eq!(user.mounts.len(), 1);
    let m = &user.mounts[0];
    assert!(
        m.id.starts_with("sp-"),
        "sharepoint mount id should start with sp-"
    );
    assert_eq!(m.name, "Contoso Team - Documents");
    assert_eq!(m.mount_type, "sharepoint");
    assert_eq!(m.mount_point, "/mnt/contoso/Documents");
    assert!(m.enabled);
    assert_eq!(m.account_id, Some("acc-1".to_string()));
    assert_eq!(m.drive_id, Some("drive-lib-456".to_string()));
    assert_eq!(m.site_id, Some("site-contoso-123".to_string()));
    assert_eq!(m.site_name, Some("Contoso Team".to_string()));
    assert_eq!(m.library_name, Some("Documents".to_string()));

    // Persist and reload
    let config_path = create_temp_config_file();
    user.save_to_file(&config_path)?;
    let loaded = UserConfig::load_from_file(&config_path)?;
    assert_eq!(loaded.mounts.len(), 1);
    assert_eq!(loaded.mounts[0].name, "Contoso Team - Documents");
    assert_eq!(loaded.mounts[0].drive_id, Some("drive-lib-456".to_string()));
    assert_eq!(
        loaded.mounts[0].site_id,
        Some("site-contoso-123".to_string())
    );

    let _ = std::fs::remove_file(&config_path);
    Ok(())
}

#[test]
fn test_config_remove_mount_persists() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("")?;

    user.add_sharepoint_mount(
        "site-1",
        "drive-1",
        "Site A",
        "Docs",
        "/mnt/a/Docs",
        Some("acc-1".to_string()),
    )?;
    user.add_onedrive_mount("drive-od", "/mnt/onedrive", Some("acc-1".to_string()))?;
    assert_eq!(user.mounts.len(), 2);

    let mount_id = user.mounts[0].id.clone();
    let removed = user.remove_mount(&mount_id);
    assert!(removed);
    assert_eq!(user.mounts.len(), 1);
    assert_eq!(user.mounts[0].mount_type, "drive"); // only OneDrive remains

    // Persist and reload
    let config_path = create_temp_config_file();
    user.save_to_file(&config_path)?;
    let loaded = UserConfig::load_from_file(&config_path)?;
    assert_eq!(loaded.mounts.len(), 1);
    assert_eq!(loaded.mounts[0].mount_type, "drive");

    let _ = std::fs::remove_file(&config_path);
    Ok(())
}

#[test]
fn test_config_has_mount_for_drive() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("")?;

    // No mounts yet
    assert!(!user.has_mount_for_drive("drive-1"));

    user.add_sharepoint_mount("site-1", "drive-1", "Site A", "Docs", "/mnt/a/Docs", None)?;

    assert!(user.has_mount_for_drive("drive-1"));
    assert!(!user.has_mount_for_drive("drive-2"));

    Ok(())
}

#[test]
fn test_config_add_duplicate_drive_returns_error() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("")?;

    user.add_sharepoint_mount("site-1", "drive-1", "Site A", "Docs", "/mnt/a/Docs", None)?;

    // Adding same drive_id again should fail
    let result =
        user.add_sharepoint_mount("site-1", "drive-1", "Site A", "Docs", "/mnt/b/Docs", None);
    assert!(
        result.is_err(),
        "adding a duplicate drive_id should return an error"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("already mounted"),
        "error should mention 'already mounted', got: {err}"
    );

    // Config should still have only one mount
    assert_eq!(user.mounts.len(), 1);

    Ok(())
}

#[test]
fn test_config_add_onedrive_duplicate_drive_returns_error() -> carminedesktop_core::Result<()> {
    let mut user = UserConfig::load("")?;

    user.add_onedrive_mount("drive-od", "/mnt/onedrive", None)?;

    let result = user.add_onedrive_mount("drive-od", "/mnt/onedrive2", None);
    assert!(
        result.is_err(),
        "adding duplicate OneDrive drive_id should return an error"
    );

    assert_eq!(user.mounts.len(), 1);
    Ok(())
}

#[test]
fn test_config_add_mount_no_account() -> carminedesktop_core::Result<()> {
    // When no account is configured, account_id is None — this should still work
    let mut user = UserConfig::load("")?;
    assert!(user.accounts.is_empty());

    user.add_sharepoint_mount("site-1", "drive-1", "Site A", "Docs", "/mnt/a/Docs", None)?;

    assert_eq!(user.mounts.len(), 1);
    assert_eq!(user.mounts[0].account_id, None);

    Ok(())
}
