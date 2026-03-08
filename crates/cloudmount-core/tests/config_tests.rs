use cloudmount_core::config::{
    AccountMetadata, EffectiveConfig, MountConfig, UserConfig, UserGeneralSettings,
    expand_mount_point,
};
use std::env;
use std::path::PathBuf;

fn create_temp_config_file() -> PathBuf {
    let temp_dir = env::temp_dir();
    let file_name = format!("test_config_{}.toml", uuid::Uuid::new_v4());
    temp_dir.join(file_name)
}

#[test]
fn test_user_config_load_empty() -> cloudmount_core::Result<()> {
    let user = UserConfig::load("")?;

    assert!(user.general.is_none());
    assert!(user.mounts.is_empty());
    assert!(user.accounts.is_empty());

    Ok(())
}

#[test]
fn test_effective_config_user_only() -> cloudmount_core::Result<()> {
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
fn test_effective_config_defaults() -> cloudmount_core::Result<()> {
    let user = UserConfig::load("")?;
    let effective = EffectiveConfig::build(&user);

    assert!(!effective.auto_start);
    assert_eq!(effective.cache_max_size, "5GB");
    assert_eq!(effective.sync_interval_secs, 60);
    assert_eq!(effective.metadata_ttl_secs, 60);
    assert_eq!(effective.log_level, "info");
    assert!(effective.notifications);
    assert_eq!(effective.root_dir, "Cloud");

    Ok(())
}

#[test]
fn test_expand_mount_point_home() {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());

    let template = "/home/{home}/OneDrive";
    let expanded = expand_mount_point(template);

    assert!(expanded.contains(&home));
    assert!(!expanded.contains("{home}"));
    assert!(expanded.starts_with("/home/"));
    assert!(expanded.ends_with("/OneDrive"));
}

#[test]
fn test_expand_mount_point_tilde() {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());

    let expanded = expand_mount_point("~/Cloud/OneDrive");
    assert!(!expanded.contains('~'));
    assert!(expanded.ends_with("/Cloud/OneDrive"));
    assert!(expanded.starts_with(&home));
}

#[test]
fn test_expand_mount_point_no_placeholder() {
    let path = "/mnt/cloudmount";
    let expanded = expand_mount_point(path);

    assert_eq!(expanded, path);
}

#[test]
fn test_reset_setting() -> cloudmount_core::Result<()> {
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
fn test_user_config_save_and_load_roundtrip() -> cloudmount_core::Result<()> {
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
fn test_mount_config_account_id_stored() -> cloudmount_core::Result<()> {
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
fn test_mount_config_account_id_none_compat() -> cloudmount_core::Result<()> {
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
fn test_old_config_fields_silently_ignored() -> cloudmount_core::Result<()> {
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
