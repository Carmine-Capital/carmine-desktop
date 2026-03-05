use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_APP_NAME: &str = "CloudMount";
const DEFAULT_CACHE_MAX_SIZE: &str = "5GB";
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 60;
const DEFAULT_METADATA_TTL_SECS: u64 = 60;
const DEFAULT_ROOT_DIR: &str = "Cloud";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackagedDefaults {
    #[serde(default)]
    pub tenant: Option<TenantConfig>,
    #[serde(default)]
    pub branding: Option<BrandingConfig>,
    #[serde(default)]
    pub defaults: Option<DefaultSettings>,
    #[serde(default)]
    pub mounts: Vec<PackagedMount>,
}

impl PackagedDefaults {
    pub fn load(toml_str: &str) -> crate::Result<Self> {
        let stripped = strip_comment_only_toml(toml_str);
        if stripped.is_empty() {
            return Ok(Self::default());
        }
        toml::from_str(&stripped)
            .map_err(|e| crate::Error::Config(format!("failed to parse packaged defaults: {e}")))
    }

    pub fn app_name(&self) -> &str {
        self.branding
            .as_ref()
            .and_then(|b| b.app_name.as_deref())
            .unwrap_or(DEFAULT_APP_NAME)
    }

    pub fn tenant_id(&self) -> Option<&str> {
        self.tenant.as_ref().and_then(|t| t.id.as_deref())
    }

    pub fn client_id(&self) -> Option<&str> {
        self.tenant.as_ref().and_then(|t| t.client_id.as_deref())
    }

    pub fn has_packaged_config(&self) -> bool {
        self.tenant.is_some() || !self.mounts.is_empty()
    }
}

fn strip_comment_only_toml(input: &str) -> String {
    input
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantConfig {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub client_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrandingConfig {
    #[serde(default)]
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultSettings {
    #[serde(default)]
    pub auto_start: Option<bool>,
    #[serde(default)]
    pub cache_max_size: Option<String>,
    #[serde(default)]
    pub sync_interval_secs: Option<u64>,
    #[serde(default)]
    pub metadata_ttl_secs: Option<u64>,
    #[serde(default)]
    pub root_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagedMount {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub mount_type: String,
    pub mount_point: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub drive_id: Option<String>,
    #[serde(default)]
    pub site_id: Option<String>,
    #[serde(default)]
    pub library_name: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default)]
    pub general: Option<UserGeneralSettings>,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
    #[serde(default)]
    pub mount_overrides: Vec<MountOverride>,
    #[serde(default)]
    pub dismissed_packaged_mounts: Vec<String>,
    #[serde(default)]
    pub accounts: Vec<AccountMetadata>,
}

impl UserConfig {
    pub fn load(toml_str: &str) -> crate::Result<Self> {
        if toml_str.trim().is_empty() {
            return Ok(Self::default());
        }
        toml::from_str(toml_str)
            .map_err(|e| crate::Error::Config(format!("failed to parse user config: {e}")))
    }

    pub fn load_from_file(path: &Path) -> crate::Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(content) => match Self::load(&content) {
                Ok(config) => Ok(config),
                Err(_) => {
                    let backup = path.with_extension("toml.bak");
                    let _ = std::fs::copy(path, &backup);
                    tracing::warn!(
                        "config corrupted, backed up to {} and reset",
                        backup.display()
                    );
                    let default = Self::default();
                    let _ = default.save_to_file(path);
                    Ok(default)
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(crate::Error::Config(format!("failed to read config: {e}"))),
        }
    }

    pub fn save_to_file(&self, path: &Path) -> crate::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| crate::Error::Config(format!("failed to serialize config: {e}")))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    pub fn reset_setting(&mut self, key: &str) {
        if let Some(ref mut g) = self.general {
            match key {
                "auto_start" => g.auto_start = None,
                "cache_max_size" => g.cache_max_size = None,
                "sync_interval_secs" => g.sync_interval_secs = None,
                "metadata_ttl_secs" => g.metadata_ttl_secs = None,
                "cache_dir" => g.cache_dir = None,
                "log_level" => g.log_level = None,
                "notifications" => g.notifications = None,
                "root_dir" => g.root_dir = None,
                _ => {}
            }
        }
    }

    pub fn add_sharepoint_mount(
        &mut self,
        site_id: &str,
        drive_id: &str,
        site_name: &str,
        library_name: &str,
        mount_point: &str,
    ) -> crate::Result<()> {
        validate_mount_point(mount_point, &self.mounts)?;

        let id = format!("sp-{}", uuid::Uuid::new_v4());
        self.mounts.push(MountConfig {
            id,
            name: format!("{site_name} - {library_name}"),
            mount_type: "sharepoint".to_string(),
            mount_point: mount_point.to_string(),
            enabled: true,
            account_id: None,
            drive_id: Some(drive_id.to_string()),
            site_id: Some(site_id.to_string()),
            site_name: Some(site_name.to_string()),
            library_name: Some(library_name.to_string()),
        });
        Ok(())
    }

    pub fn add_onedrive_mount(&mut self, drive_id: &str, mount_point: &str) -> crate::Result<()> {
        validate_mount_point(mount_point, &self.mounts)?;

        let id = format!("od-{}", uuid::Uuid::new_v4());
        self.mounts.push(MountConfig {
            id,
            name: "OneDrive".to_string(),
            mount_type: "drive".to_string(),
            mount_point: mount_point.to_string(),
            enabled: true,
            account_id: None,
            drive_id: Some(drive_id.to_string()),
            site_id: None,
            site_name: None,
            library_name: None,
        });
        Ok(())
    }

    pub fn remove_mount(&mut self, id: &str) -> bool {
        let before = self.mounts.len();
        self.mounts.retain(|m| m.id != id);
        self.mounts.len() < before
    }

    pub fn toggle_mount(&mut self, id: &str) -> Option<bool> {
        if let Some(mount) = self.mounts.iter_mut().find(|m| m.id == id) {
            mount.enabled = !mount.enabled;
            return Some(mount.enabled);
        }
        if let Some(ov) = self.mount_overrides.iter_mut().find(|o| o.id == id) {
            let new_val = !ov.enabled.unwrap_or(true);
            ov.enabled = Some(new_val);
            return Some(new_val);
        }
        None
    }

    pub fn restore_default_mounts(&mut self) {
        self.dismissed_packaged_mounts.clear();
    }

    pub fn reset_all(&mut self) {
        self.general = None;
        self.mounts.clear();
        self.mount_overrides.clear();
        self.dismissed_packaged_mounts.clear();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserGeneralSettings {
    #[serde(default)]
    pub auto_start: Option<bool>,
    #[serde(default)]
    pub cache_max_size: Option<String>,
    #[serde(default)]
    pub sync_interval_secs: Option<u64>,
    #[serde(default)]
    pub metadata_ttl_secs: Option<u64>,
    #[serde(default)]
    pub cache_dir: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub notifications: Option<bool>,
    #[serde(default)]
    pub root_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub mount_type: String,
    pub mount_point: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub drive_id: Option<String>,
    #[serde(default)]
    pub site_id: Option<String>,
    #[serde(default)]
    pub site_name: Option<String>,
    #[serde(default)]
    pub library_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountOverride {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub mount_point: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountMetadata {
    pub id: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    pub app_name: String,
    pub tenant_id: Option<String>,
    pub client_id: Option<String>,
    pub auto_start: bool,
    pub cache_max_size: String,
    pub sync_interval_secs: u64,
    pub metadata_ttl_secs: u64,
    pub cache_dir: Option<String>,
    pub log_level: String,
    pub notifications: bool,
    pub root_dir: String,
    pub mounts: Vec<MountConfig>,
    pub accounts: Vec<AccountMetadata>,
}

impl EffectiveConfig {
    pub fn build(packaged: &PackagedDefaults, user: &UserConfig) -> Self {
        let user_general = user.general.as_ref();
        let pkg_defaults = packaged.defaults.as_ref();

        let auto_start = user_general
            .and_then(|g| g.auto_start)
            .or_else(|| pkg_defaults.and_then(|d| d.auto_start))
            .unwrap_or(false);

        let cache_max_size = user_general
            .and_then(|g| g.cache_max_size.clone())
            .or_else(|| pkg_defaults.and_then(|d| d.cache_max_size.clone()))
            .unwrap_or_else(|| DEFAULT_CACHE_MAX_SIZE.to_string());

        let sync_interval_secs = user_general
            .and_then(|g| g.sync_interval_secs)
            .or_else(|| pkg_defaults.and_then(|d| d.sync_interval_secs))
            .unwrap_or(DEFAULT_SYNC_INTERVAL_SECS);

        let metadata_ttl_secs = user_general
            .and_then(|g| g.metadata_ttl_secs)
            .or_else(|| pkg_defaults.and_then(|d| d.metadata_ttl_secs))
            .unwrap_or(DEFAULT_METADATA_TTL_SECS);

        let root_dir = user_general
            .and_then(|g| g.root_dir.clone())
            .or_else(|| pkg_defaults.and_then(|d| d.root_dir.clone()))
            .unwrap_or_else(|| DEFAULT_ROOT_DIR.to_string());

        let cache_dir = user_general.and_then(|g| g.cache_dir.clone());
        let log_level = user_general
            .and_then(|g| g.log_level.clone())
            .unwrap_or_else(|| "info".to_string());
        let notifications = user_general.and_then(|g| g.notifications).unwrap_or(true);

        let mounts = merge_mounts(packaged, user);

        Self {
            app_name: packaged.app_name().to_string(),
            tenant_id: packaged.tenant_id().map(String::from),
            client_id: packaged.client_id().map(String::from),
            auto_start,
            cache_max_size,
            sync_interval_secs,
            metadata_ttl_secs,
            cache_dir,
            log_level,
            notifications,
            root_dir,
            mounts,
            accounts: user.accounts.clone(),
        }
    }
}

fn merge_mounts(packaged: &PackagedDefaults, user: &UserConfig) -> Vec<MountConfig> {
    let mut result: Vec<MountConfig> = Vec::new();

    let overrides: HashMap<&str, &MountOverride> = user
        .mount_overrides
        .iter()
        .map(|o| (o.id.as_str(), o))
        .collect();

    for pm in &packaged.mounts {
        if user.dismissed_packaged_mounts.contains(&pm.id) {
            continue;
        }

        let mut mount = MountConfig {
            id: pm.id.clone(),
            name: pm.name.clone(),
            mount_type: pm.mount_type.clone(),
            mount_point: pm.mount_point.clone(),
            enabled: pm.enabled,
            account_id: None,
            drive_id: pm.drive_id.clone(),
            site_id: pm.site_id.clone(),
            site_name: None,
            library_name: pm.library_name.clone(),
        };

        if let Some(ov) = overrides.get(pm.id.as_str()) {
            if let Some(ref name) = ov.name {
                mount.name = name.clone();
            }
            if let Some(ref mp) = ov.mount_point {
                mount.mount_point = mp.clone();
            }
            if let Some(enabled) = ov.enabled {
                mount.enabled = enabled;
            }
        }

        result.push(mount);
    }

    result.extend(user.mounts.iter().cloned());

    result
}

fn validate_mount_point(mount_point: &str, existing_mounts: &[MountConfig]) -> crate::Result<()> {
    if mount_point.is_empty() {
        return Err(crate::Error::Config("mount point cannot be empty".into()));
    }

    let expanded = expand_mount_point(mount_point);
    let path = std::path::Path::new(&expanded);

    let system_dirs = [
        "/",
        "/bin",
        "/sbin",
        "/usr",
        "/etc",
        "/var",
        "/dev",
        "/proc",
        "/sys",
        "/boot",
        "/lib",
        "/lib64",
        "/tmp",
        "C:\\",
        "C:\\Windows",
        "C:\\Program Files",
        "C:\\Program Files (x86)",
    ];
    if system_dirs.iter().any(|d| path == std::path::Path::new(d)) {
        return Err(crate::Error::Config(format!(
            "cannot use system directory as mount point: {expanded}"
        )));
    }

    if existing_mounts
        .iter()
        .any(|m| expand_mount_point(&m.mount_point) == expanded)
    {
        return Err(crate::Error::Config(format!(
            "mount point already in use: {expanded}"
        )));
    }

    Ok(())
}

/// Derives a mount point path for auto-created mounts.
///
/// - OneDrive: `{home}/{root_dir}/OneDrive`
/// - SharePoint: `{home}/{root_dir}/{site_name}/{lib_name}`
pub fn derive_mount_point(
    root_dir: &str,
    mount_type: &str,
    site_name: Option<&str>,
    lib_name: Option<&str>,
) -> String {
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string())
        });

    match mount_type {
        "sharepoint" => {
            let site = site_name.unwrap_or("SharePoint");
            let lib = lib_name.unwrap_or("Documents");
            format!("{home}/{root_dir}/{site}/{lib}")
        }
        _ => format!("{home}/{root_dir}/OneDrive"),
    }
}

pub fn expand_mount_point(template: &str) -> String {
    if !template.contains("{home}") {
        return template.to_string();
    }

    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string())
        });

    template.replace("{home}", &home)
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("cloudmount"))
        .unwrap_or_else(|| PathBuf::from(".cloudmount"))
}

pub fn config_file_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .map(|d| d.join("cloudmount"))
        .unwrap_or_else(|| PathBuf::from(".cloudmount-cache"))
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigChangeEvent {
    MountAdded {
        id: String,
    },
    MountRemoved {
        id: String,
    },
    MountPointChanged {
        id: String,
        old: String,
        new: String,
    },
    MountToggled {
        id: String,
        enabled: bool,
    },
    CacheMaxSizeChanged(String),
    SyncIntervalChanged(u64),
    MetadataTtlChanged(u64),
    CacheDirChanged(Option<String>),
    AutoStartChanged(bool),
    LogLevelChanged(String),
    NotificationsChanged(bool),
}

pub fn diff_configs(old: &EffectiveConfig, new: &EffectiveConfig) -> Vec<ConfigChangeEvent> {
    let mut events = Vec::new();

    if old.auto_start != new.auto_start {
        events.push(ConfigChangeEvent::AutoStartChanged(new.auto_start));
    }
    if old.cache_max_size != new.cache_max_size {
        events.push(ConfigChangeEvent::CacheMaxSizeChanged(
            new.cache_max_size.clone(),
        ));
    }
    if old.sync_interval_secs != new.sync_interval_secs {
        events.push(ConfigChangeEvent::SyncIntervalChanged(
            new.sync_interval_secs,
        ));
    }
    if old.metadata_ttl_secs != new.metadata_ttl_secs {
        events.push(ConfigChangeEvent::MetadataTtlChanged(new.metadata_ttl_secs));
    }
    if old.cache_dir != new.cache_dir {
        events.push(ConfigChangeEvent::CacheDirChanged(new.cache_dir.clone()));
    }
    if old.log_level != new.log_level {
        events.push(ConfigChangeEvent::LogLevelChanged(new.log_level.clone()));
    }
    if old.notifications != new.notifications {
        events.push(ConfigChangeEvent::NotificationsChanged(new.notifications));
    }

    let old_ids: std::collections::HashSet<&str> =
        old.mounts.iter().map(|m| m.id.as_str()).collect();
    let new_ids: std::collections::HashSet<&str> =
        new.mounts.iter().map(|m| m.id.as_str()).collect();

    for id in new_ids.difference(&old_ids) {
        events.push(ConfigChangeEvent::MountAdded { id: id.to_string() });
    }
    for id in old_ids.difference(&new_ids) {
        events.push(ConfigChangeEvent::MountRemoved { id: id.to_string() });
    }

    for new_mount in &new.mounts {
        if let Some(old_mount) = old.mounts.iter().find(|m| m.id == new_mount.id) {
            if old_mount.mount_point != new_mount.mount_point {
                events.push(ConfigChangeEvent::MountPointChanged {
                    id: new_mount.id.clone(),
                    old: old_mount.mount_point.clone(),
                    new: new_mount.mount_point.clone(),
                });
            }
            if old_mount.enabled != new_mount.enabled {
                events.push(ConfigChangeEvent::MountToggled {
                    id: new_mount.id.clone(),
                    enabled: new_mount.enabled,
                });
            }
        }
    }

    events
}

pub mod autostart {
    pub fn set_enabled(enabled: bool, app_path: &str) -> crate::Result<()> {
        if enabled { enable(app_path) } else { disable() }
    }

    #[cfg(target_os = "linux")]
    fn enable(app_path: &str) -> crate::Result<()> {
        let service_dir = dirs::config_dir()
            .map(|d| d.join("systemd/user"))
            .ok_or_else(|| crate::Error::Config("no config dir".into()))?;
        std::fs::create_dir_all(&service_dir)?;

        let unit = format!(
            "[Unit]\nDescription=CloudMount\n\n[Service]\nExecStart={app_path}\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n"
        );
        std::fs::write(service_dir.join("cloudmount.service"), unit)?;

        std::process::Command::new("systemctl")
            .args(["--user", "enable", "cloudmount.service"])
            .output()
            .map_err(|e| crate::Error::Config(format!("systemctl enable failed: {e}")))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn disable() -> crate::Result<()> {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "cloudmount.service"])
            .output();

        let service_path = dirs::config_dir().map(|d| d.join("systemd/user/cloudmount.service"));
        if let Some(path) = service_path {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn enable(app_path: &str) -> crate::Result<()> {
        let plist_dir = dirs::home_dir()
            .map(|d| d.join("Library/LaunchAgents"))
            .ok_or_else(|| crate::Error::Config("no home dir".into()))?;
        std::fs::create_dir_all(&plist_dir)?;

        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.cloudmount.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>{app_path}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>"#
        );
        std::fs::write(plist_dir.join("com.cloudmount.agent.plist"), plist)?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn disable() -> crate::Result<()> {
        let plist_path =
            dirs::home_dir().map(|d| d.join("Library/LaunchAgents/com.cloudmount.agent.plist"));
        if let Some(path) = plist_path {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn enable(app_path: &str) -> crate::Result<()> {
        std::process::Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "CloudMount",
                "/t",
                "REG_SZ",
                "/d",
                app_path,
                "/f",
            ])
            .output()
            .map_err(|e| crate::Error::Config(format!("reg add failed: {e}")))?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn disable() -> crate::Result<()> {
        let _ = std::process::Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "CloudMount",
                "/f",
            ])
            .output();
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn enable(_app_path: &str) -> crate::Result<()> {
        Err(crate::Error::Config(
            "auto-start not supported on this platform".into(),
        ))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn disable() -> crate::Result<()> {
        Ok(())
    }
}
