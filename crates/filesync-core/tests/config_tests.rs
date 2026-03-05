use filesync_core::config::{
    AccountMetadata, EffectiveConfig, MountConfig, MountOverride, PackagedDefaults, UserConfig,
    UserGeneralSettings, expand_mount_point,
};
use std::env;
use std::path::PathBuf;

fn create_temp_config_file() -> PathBuf {
    let temp_dir = env::temp_dir();
    let file_name = format!("test_config_{}.toml", uuid::Uuid::new_v4());
    temp_dir.join(file_name)
}

#[test]
fn test_packaged_defaults_load_empty_returns_default() -> filesync_core::Result<()> {
    let toml_str = "# This is all comments\n# No actual config\n";
    let defaults = PackagedDefaults::load(toml_str)?;

    assert!(defaults.tenant.is_none());
    assert!(defaults.branding.is_none());
    assert!(defaults.defaults.is_none());
    assert!(defaults.mounts.is_empty());
    assert_eq!(defaults.app_name(), "FileSync");
    assert!(defaults.tenant_id().is_none());
    assert!(defaults.client_id().is_none());
    assert!(!defaults.has_packaged_config());

    Ok(())
}

#[test]
fn test_packaged_defaults_load_full_config() -> filesync_core::Result<()> {
    let toml_str = r#"
[tenant]
id = "12345678-1234-1234-1234-123456789012"
client_id = "abcdef-client-id"

[branding]
app_name = "MyOrg FileSync"

[defaults]
auto_start = true
cache_max_size = "10GB"
sync_interval_secs = 120
metadata_ttl_secs = 300

[[mounts]]
id = "mount1"
name = "My Drive"
type = "drive"
mount_point = "/mnt/mydrive"
enabled = true
drive_id = "drive-123"

[[mounts]]
id = "mount2"
name = "Shared Library"
type = "sharepoint"
mount_point = "/mnt/sharepoint"
enabled = false
site_id = "site-456"
library_name = "Documents"
"#;

    let defaults = PackagedDefaults::load(toml_str)?;

    assert!(defaults.tenant.is_some());
    assert_eq!(
        defaults.tenant_id(),
        Some("12345678-1234-1234-1234-123456789012")
    );
    assert_eq!(defaults.client_id(), Some("abcdef-client-id"));

    assert!(defaults.branding.is_some());
    assert_eq!(defaults.app_name(), "MyOrg FileSync");

    assert!(defaults.defaults.is_some());
    let pkg_defaults = defaults.defaults.as_ref().unwrap();
    assert_eq!(pkg_defaults.auto_start, Some(true));
    assert_eq!(pkg_defaults.cache_max_size, Some("10GB".to_string()));
    assert_eq!(pkg_defaults.sync_interval_secs, Some(120));
    assert_eq!(pkg_defaults.metadata_ttl_secs, Some(300));

    assert_eq!(defaults.mounts.len(), 2);
    assert_eq!(defaults.mounts[0].id, "mount1");
    assert_eq!(defaults.mounts[0].name, "My Drive");
    assert_eq!(defaults.mounts[0].mount_type, "drive");
    assert_eq!(defaults.mounts[0].drive_id, Some("drive-123".to_string()));
    assert!(defaults.mounts[0].enabled);

    assert_eq!(defaults.mounts[1].id, "mount2");
    assert!(!defaults.mounts[1].enabled);
    assert_eq!(defaults.mounts[1].site_id, Some("site-456".to_string()));

    assert!(defaults.has_packaged_config());

    Ok(())
}

#[test]
fn test_user_config_load_empty() -> filesync_core::Result<()> {
    let user = UserConfig::load("")?;

    assert!(user.general.is_none());
    assert!(user.mounts.is_empty());
    assert!(user.mount_overrides.is_empty());
    assert!(user.dismissed_packaged_mounts.is_empty());
    assert!(user.accounts.is_empty());

    Ok(())
}

#[test]
fn test_effective_config_packaged_only() -> filesync_core::Result<()> {
    let packaged_toml = r#"
[tenant]
id = "tenant-123"
client_id = "client-456"

[branding]
app_name = "OrgName"

[defaults]
auto_start = true
cache_max_size = "8GB"
sync_interval_secs = 90
metadata_ttl_secs = 120
"#;

    let packaged = PackagedDefaults::load(packaged_toml)?;
    let user = UserConfig::load("")?;

    let effective = EffectiveConfig::build(&packaged, &user);

    assert_eq!(effective.app_name, "OrgName");
    assert_eq!(effective.tenant_id, Some("tenant-123".to_string()));
    assert_eq!(effective.client_id, Some("client-456".to_string()));
    assert!(effective.auto_start);
    assert_eq!(effective.cache_max_size, "8GB");
    assert_eq!(effective.sync_interval_secs, 90);
    assert_eq!(effective.metadata_ttl_secs, 120);
    assert_eq!(effective.log_level, "info");
    assert!(effective.notifications);

    Ok(())
}

#[test]
fn test_effective_config_user_overrides_packaged() -> filesync_core::Result<()> {
    let packaged_toml = r#"
[defaults]
auto_start = false
cache_max_size = "5GB"
sync_interval_secs = 60
"#;

    let user_toml = r#"
[general]
auto_start = true
cache_max_size = "15GB"
sync_interval_secs = 180
log_level = "debug"
notifications = false
"#;

    let packaged = PackagedDefaults::load(packaged_toml)?;
    let user = UserConfig::load(user_toml)?;

    let effective = EffectiveConfig::build(&packaged, &user);

    assert!(effective.auto_start);
    assert_eq!(effective.cache_max_size, "15GB");
    assert_eq!(effective.sync_interval_secs, 180);
    assert_eq!(effective.log_level, "debug");
    assert!(!effective.notifications);

    Ok(())
}

#[test]
fn test_effective_config_user_only() -> filesync_core::Result<()> {
    let packaged = PackagedDefaults::default();
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
    let effective = EffectiveConfig::build(&packaged, &user);

    assert_eq!(effective.app_name, "FileSync");
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
fn test_mount_union_merge() -> filesync_core::Result<()> {
    let packaged_toml = r#"
[[mounts]]
id = "pkg-mount1"
name = "Packaged Drive"
type = "drive"
mount_point = "/mnt/pkg1"
enabled = true
drive_id = "drive-pkg1"

[[mounts]]
id = "pkg-mount2"
name = "Packaged SharePoint"
type = "sharepoint"
mount_point = "/mnt/pkg2"
enabled = true
site_id = "site-pkg2"
"#;

    let user_toml = r#"
[[mounts]]
id = "user-mount1"
name = "User Drive"
type = "drive"
mount_point = "/mnt/user1"
enabled = true
account_id = "account-123"
drive_id = "drive-user1"
"#;

    let packaged = PackagedDefaults::load(packaged_toml)?;
    let user = UserConfig::load(user_toml)?;

    let effective = EffectiveConfig::build(&packaged, &user);

    assert_eq!(effective.mounts.len(), 3);

    let pkg1 = effective
        .mounts
        .iter()
        .find(|m| m.id == "pkg-mount1")
        .unwrap();
    assert_eq!(pkg1.name, "Packaged Drive");
    assert_eq!(pkg1.mount_point, "/mnt/pkg1");

    let pkg2 = effective
        .mounts
        .iter()
        .find(|m| m.id == "pkg-mount2")
        .unwrap();
    assert_eq!(pkg2.name, "Packaged SharePoint");

    let user1 = effective
        .mounts
        .iter()
        .find(|m| m.id == "user-mount1")
        .unwrap();
    assert_eq!(user1.name, "User Drive");
    assert_eq!(user1.mount_point, "/mnt/user1");

    Ok(())
}

#[test]
fn test_mount_override_applies() -> filesync_core::Result<()> {
    let packaged_toml = r#"
[[mounts]]
id = "mount1"
name = "Original Name"
type = "drive"
mount_point = "/mnt/original"
enabled = true
drive_id = "drive-123"
"#;

    let user_toml = r#"
[[mount_overrides]]
id = "mount1"
name = "Overridden Name"
mount_point = "/mnt/overridden"
enabled = false
"#;

    let packaged = PackagedDefaults::load(packaged_toml)?;
    let user = UserConfig::load(user_toml)?;

    let effective = EffectiveConfig::build(&packaged, &user);

    assert_eq!(effective.mounts.len(), 1);
    let mount = &effective.mounts[0];
    assert_eq!(mount.id, "mount1");
    assert_eq!(mount.name, "Overridden Name");
    assert_eq!(mount.mount_point, "/mnt/overridden");
    assert!(!mount.enabled);
    assert_eq!(mount.drive_id, Some("drive-123".to_string()));

    Ok(())
}

#[test]
fn test_dismissed_mounts_excluded() -> filesync_core::Result<()> {
    let packaged_toml = r#"
[[mounts]]
id = "mount1"
name = "Mount 1"
type = "drive"
mount_point = "/mnt/1"
enabled = true

[[mounts]]
id = "mount2"
name = "Mount 2"
type = "drive"
mount_point = "/mnt/2"
enabled = true

[[mounts]]
id = "mount3"
name = "Mount 3"
type = "drive"
mount_point = "/mnt/3"
enabled = true
"#;

    let user_toml = r#"
dismissed_packaged_mounts = ["mount2"]
"#;

    let packaged = PackagedDefaults::load(packaged_toml)?;
    let user = UserConfig::load(user_toml)?;

    let effective = EffectiveConfig::build(&packaged, &user);

    assert_eq!(effective.mounts.len(), 2);
    assert!(effective.mounts.iter().any(|m| m.id == "mount1"));
    assert!(!effective.mounts.iter().any(|m| m.id == "mount2"));
    assert!(effective.mounts.iter().any(|m| m.id == "mount3"));

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
fn test_expand_mount_point_no_placeholder() {
    let path = "/mnt/filesync";
    let expanded = expand_mount_point(path);

    assert_eq!(expanded, path);
}

#[test]
fn test_reset_setting() -> filesync_core::Result<()> {
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
fn test_restore_default_mounts() -> filesync_core::Result<()> {
    let user_toml = r#"
dismissed_packaged_mounts = ["mount1", "mount2", "mount3"]
"#;

    let mut user = UserConfig::load(user_toml)?;

    assert_eq!(user.dismissed_packaged_mounts.len(), 3);
    user.restore_default_mounts();
    assert!(user.dismissed_packaged_mounts.is_empty());

    Ok(())
}

#[test]
fn test_user_config_save_and_load_roundtrip() -> filesync_core::Result<()> {
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
        mount_overrides: vec![MountOverride {
            id: "pkg-mount1".to_string(),
            name: Some("Renamed Mount".to_string()),
            mount_point: Some("/mnt/renamed".to_string()),
            enabled: Some(false),
        }],
        dismissed_packaged_mounts: vec!["old-mount1".to_string(), "old-mount2".to_string()],
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

    assert_eq!(loaded.mount_overrides.len(), 1);
    assert_eq!(loaded.mount_overrides[0].id, "pkg-mount1");
    assert_eq!(
        loaded.mount_overrides[0].name,
        Some("Renamed Mount".to_string())
    );

    assert_eq!(loaded.dismissed_packaged_mounts.len(), 2);
    assert!(
        loaded
            .dismissed_packaged_mounts
            .contains(&"old-mount1".to_string())
    );

    assert_eq!(loaded.accounts.len(), 1);
    assert_eq!(
        loaded.accounts[0].email,
        Some("user@example.com".to_string())
    );

    let _ = std::fs::remove_file(&config_path);

    Ok(())
}
