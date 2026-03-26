use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const DEFAULT_CACHE_MAX_SIZE: &str = "5GB";
const DEFAULT_SYNC_INTERVAL_SECS: u64 = 60;
const DEFAULT_METADATA_TTL_SECS: u64 = 60;
const DEFAULT_ROOT_DIR: &str = "Cloud";
const DEFAULT_OFFLINE_TTL_SECS: u64 = 86400; // 1 day
const DEFAULT_OFFLINE_MAX_FOLDER_SIZE: &str = "5GB";
const MIN_OFFLINE_TTL_SECS: u64 = 60; // 1 minute
const MAX_OFFLINE_TTL_SECS: u64 = 604800; // 7 days

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default)]
    pub general: Option<UserGeneralSettings>,
    #[serde(default)]
    pub mounts: Vec<MountConfig>,
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
                "register_file_associations" => g.register_file_associations = None,
                "file_handler_overrides" => g.file_handler_overrides = None,
                "explorer_nav_pane" => g.explorer_nav_pane = None,
                "offline_ttl_secs" => g.offline_ttl_secs = None,
                "offline_max_folder_size" => g.offline_max_folder_size = None,
                _ => {}
            }
        }
    }

    /// Returns true if any mount already uses the given `drive_id`.
    pub fn has_mount_for_drive(&self, drive_id: &str) -> bool {
        self.mounts
            .iter()
            .any(|m| m.drive_id.as_deref() == Some(drive_id))
    }

    /// Returns the mount id for a mount with the given `drive_id`, if any.
    pub fn mount_id_for_drive(&self, drive_id: &str) -> Option<&str> {
        self.mounts
            .iter()
            .find(|m| m.drive_id.as_deref() == Some(drive_id))
            .map(|m| m.id.as_str())
    }

    pub fn add_sharepoint_mount(
        &mut self,
        site_id: &str,
        drive_id: &str,
        site_name: &str,
        library_name: &str,
        mount_point: &str,
        account_id: Option<String>,
    ) -> crate::Result<()> {
        if self.has_mount_for_drive(drive_id) {
            return Err(crate::Error::Config(format!(
                "library is already mounted (drive {drive_id})"
            )));
        }
        validate_mount_point(mount_point, &self.mounts)?;

        let id = format!("sp-{}", uuid::Uuid::new_v4());
        let clean_mount_point = mount_point.trim_end_matches(['/', '\\']).to_string();
        self.mounts.push(MountConfig {
            id,
            name: format!("{site_name} - {library_name}"),
            mount_type: "sharepoint".to_string(),
            mount_point: clean_mount_point,
            enabled: true,
            account_id,
            drive_id: Some(drive_id.to_string()),
            site_id: Some(site_id.to_string()),
            site_name: Some(site_name.to_string()),
            library_name: Some(library_name.to_string()),
        });
        Ok(())
    }

    pub fn add_onedrive_mount(
        &mut self,
        drive_id: &str,
        mount_point: &str,
        account_id: Option<String>,
    ) -> crate::Result<()> {
        if self.has_mount_for_drive(drive_id) {
            return Err(crate::Error::Config(format!(
                "drive is already mounted (drive {drive_id})"
            )));
        }
        validate_mount_point(mount_point, &self.mounts)?;

        let id = format!("od-{}", uuid::Uuid::new_v4());
        let clean_mount_point = mount_point.trim_end_matches(['/', '\\']).to_string();
        self.mounts.push(MountConfig {
            id,
            name: "OneDrive".to_string(),
            mount_type: "drive".to_string(),
            mount_point: clean_mount_point,
            enabled: true,
            account_id,
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
        None
    }

    pub fn reset_all(&mut self) {
        self.general = None;
        self.mounts.clear();
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
    // Stored as String (not PathBuf) so it round-trips cleanly through TOML.
    // Win32 normalises both `/` and `\` separators, so forward-slash paths from
    // TOML are safe on Windows without explicit conversion.
    #[serde(default)]
    pub cache_dir: Option<String>,
    #[serde(default)]
    pub log_level: Option<String>,
    #[serde(default)]
    pub notifications: Option<bool>,
    #[serde(default)]
    pub root_dir: Option<String>,
    /// Register Carmine Desktop as the handler for Office file types on Windows.
    /// When enabled, double-clicking .docx/.xlsx/.pptx files opens them via
    /// Carmine Desktop, which redirects to SharePoint Online for co-authoring.
    #[serde(default)]
    pub register_file_associations: Option<bool>,
    /// Per-extension handler overrides. Keys are extensions (e.g. ".docx"),
    /// values are handler identifiers (ProgID on Windows, .desktop name on
    /// Linux, bundle ID on macOS).
    #[serde(default)]
    pub file_handler_overrides: Option<HashMap<String, String>>,
    /// Show Carmine Desktop in the Windows Explorer navigation pane.
    /// Default: true on Windows, false on other platforms.
    #[serde(default)]
    pub explorer_nav_pane: Option<bool>,
    /// How long pinned folders remain available offline (seconds).
    /// Default: 86400 (1 day). Clamped to [60, 604800].
    #[serde(default)]
    pub offline_ttl_secs: Option<u64>,
    /// Maximum folder size allowed for offline pinning (e.g. "5GB", "500MB").
    /// Default: "5GB".
    #[serde(default)]
    pub offline_max_folder_size: Option<String>,
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

fn default_true() -> bool {
    true
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
    pub auto_start: bool,
    pub cache_max_size: String,
    pub sync_interval_secs: u64,
    pub metadata_ttl_secs: u64,
    /// String (not PathBuf) for TOML round-trip safety. Win32 normalises both
    /// `/` and `\` separators, so forward-slash paths are safe on Windows.
    pub cache_dir: Option<String>,
    pub log_level: String,
    pub notifications: bool,
    pub root_dir: String,
    pub mounts: Vec<MountConfig>,
    pub accounts: Vec<AccountMetadata>,
    /// Register Carmine Desktop as the handler for Office file types on Windows.
    /// Default: true on Windows, false on other platforms.
    pub register_file_associations: bool,
    /// Per-extension handler overrides. Keys are extensions (e.g. ".docx"),
    /// values are handler identifiers (ProgID on Windows, .desktop name on
    /// Linux, bundle ID on macOS). Default: empty.
    pub file_handler_overrides: HashMap<String, String>,
    /// Show Carmine Desktop in Windows Explorer navigation pane.
    /// Default: true on Windows, false on other platforms.
    pub explorer_nav_pane: bool,
    /// How long pinned folders remain available offline (seconds).
    pub offline_ttl_secs: u64,
    /// Maximum folder size allowed for offline pinning.
    pub offline_max_folder_size: String,
}

impl EffectiveConfig {
    pub fn build(user: &UserConfig) -> Self {
        let user_general = user.general.as_ref();

        let auto_start = user_general.and_then(|g| g.auto_start).unwrap_or(true);

        let cache_max_size = user_general
            .and_then(|g| g.cache_max_size.clone())
            .unwrap_or_else(|| DEFAULT_CACHE_MAX_SIZE.to_string());

        let sync_interval_secs = user_general
            .and_then(|g| g.sync_interval_secs)
            .unwrap_or(DEFAULT_SYNC_INTERVAL_SECS);

        let metadata_ttl_secs = user_general
            .and_then(|g| g.metadata_ttl_secs)
            .unwrap_or(DEFAULT_METADATA_TTL_SECS);

        let root_dir = user_general
            .and_then(|g| g.root_dir.clone())
            .unwrap_or_else(|| DEFAULT_ROOT_DIR.to_string());

        let cache_dir = user_general.and_then(|g| g.cache_dir.clone());
        let log_level = user_general
            .and_then(|g| g.log_level.clone())
            .unwrap_or_else(|| "info".to_string());
        let notifications = user_general.and_then(|g| g.notifications).unwrap_or(true);

        // Default: true on Windows and macOS, false on other platforms
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let default_file_assoc = true;
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let default_file_assoc = false;

        let register_file_associations = user_general
            .and_then(|g| g.register_file_associations)
            .unwrap_or(default_file_assoc);

        let file_handler_overrides = user_general
            .and_then(|g| g.file_handler_overrides.clone())
            .unwrap_or_default();

        // Default: true on Windows, false on other platforms
        #[cfg(target_os = "windows")]
        let default_nav_pane = true;
        #[cfg(not(target_os = "windows"))]
        let default_nav_pane = false;

        let explorer_nav_pane = user_general
            .and_then(|g| g.explorer_nav_pane)
            .unwrap_or(default_nav_pane);

        let offline_ttl_secs = user_general
            .and_then(|g| g.offline_ttl_secs)
            .unwrap_or(DEFAULT_OFFLINE_TTL_SECS)
            .clamp(MIN_OFFLINE_TTL_SECS, MAX_OFFLINE_TTL_SECS);

        let offline_max_folder_size = user_general
            .and_then(|g| g.offline_max_folder_size.clone())
            .unwrap_or_else(|| DEFAULT_OFFLINE_MAX_FOLDER_SIZE.to_string());

        Self {
            auto_start,
            cache_max_size,
            sync_interval_secs,
            metadata_ttl_secs,
            cache_dir,
            log_level,
            notifications,
            root_dir,
            mounts: user.mounts.clone(),
            accounts: user.accounts.clone(),
            register_file_associations,
            file_handler_overrides,
            explorer_nav_pane,
            offline_ttl_secs,
            offline_max_folder_size,
        }
    }
}

fn validate_mount_point(mount_point: &str, existing_mounts: &[MountConfig]) -> crate::Result<()> {
    if mount_point.is_empty() {
        return Err(crate::Error::Config("mount point cannot be empty".into()));
    }

    let expanded = expand_mount_point(mount_point);
    let path = std::path::Path::new(&expanded);

    #[cfg(unix)]
    let system_dirs: &[&str] = &[
        "/", "/bin", "/sbin", "/usr", "/etc", "/var", "/dev", "/proc", "/sys", "/boot", "/lib",
        "/lib64", "/tmp",
    ];
    #[cfg(windows)]
    let system_dirs: &[&str] = &[
        "C:\\",
        "C:\\Windows",
        "C:\\Program Files",
        "C:\\Program Files (x86)",
    ];
    #[cfg(not(any(unix, windows)))]
    let system_dirs: &[&str] = &[];
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

    let base = std::path::Path::new(&home).join(root_dir);
    match mount_type {
        "sharepoint" => {
            let site = site_name.unwrap_or("SharePoint");
            let lib = lib_name.unwrap_or("Documents");
            base.join(site).join(lib).to_string_lossy().into_owned()
        }
        _ => base.join("OneDrive").to_string_lossy().into_owned(),
    }
}

/// Normalizes a mount-point path:
/// - On Windows: replaces all `/` with `\`
/// - On all platforms: strips trailing `/` or `\`, but preserves bare Windows
///   drive roots like `C:\` (only strips when path length > 3).
fn normalize_mountpoint(path: String) -> String {
    #[cfg(target_os = "windows")]
    let path = path.replace('/', "\\");

    let trimmed = path.trim_end_matches(['/', '\\']);
    // Preserve bare Windows drive roots: "C:" alone changes meaning, keep "C:\".
    if trimmed.len() < path.len() && trimmed.len() <= 2 && trimmed.ends_with(':') {
        // Re-add a single native separator for drive roots.
        format!("{trimmed}{}", std::path::MAIN_SEPARATOR)
    } else if trimmed.is_empty() && !path.is_empty() {
        // Root "/" — preserve it.
        std::path::MAIN_SEPARATOR.to_string()
    } else {
        trimmed.to_string()
    }
}

pub fn expand_mount_point(template: &str) -> String {
    if !template.contains("{home}") && !template.starts_with("~/") && template != "~" {
        return normalize_mountpoint(template.to_string());
    }

    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string())
        });

    let expanded = if let Some(rest) = template.strip_prefix("~/") {
        std::path::Path::new(&home)
            .join(rest)
            .to_string_lossy()
            .into_owned()
    } else if template == "~" {
        home.clone()
    } else if let Some(rest) = template.strip_prefix("{home}") {
        // {home} at start — rebuild via Path::join so separators are OS-native.
        let rest = rest.trim_start_matches(['/', '\\']);
        if rest.is_empty() {
            home.clone()
        } else {
            std::path::Path::new(&home)
                .join(rest)
                .to_string_lossy()
                .into_owned()
        }
    } else {
        let parts: Vec<&str> = template.split("{home}").collect();
        let mut path = std::path::Path::new(&home).to_path_buf();
        for part in parts.into_iter().skip(1) {
            let part = part.trim_start_matches(['/', '\\']);
            path = path.join(part);
        }
        path.to_string_lossy().into_owned()
    };

    normalize_mountpoint(expanded)
}

pub fn config_dir() -> crate::Result<PathBuf> {
    dirs::config_dir()
        .map(|d| d.join("carminedesktop"))
        .ok_or_else(|| {
            crate::Error::Config(
                "no config directory available (dirs::config_dir returned None)".into(),
            )
        })
}

pub fn config_file_path() -> crate::Result<PathBuf> {
    config_dir().map(|d| d.join("config.toml"))
}

pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .map(|d| d.join("carminedesktop"))
        .unwrap_or_else(|| PathBuf::from(".carminedesktop-cache"))
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
    OfflineTtlChanged(u64),
    OfflineMaxFolderSizeChanged(String),
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
    if old.offline_ttl_secs != new.offline_ttl_secs {
        events.push(ConfigChangeEvent::OfflineTtlChanged(new.offline_ttl_secs));
    }
    if old.offline_max_folder_size != new.offline_max_folder_size {
        events.push(ConfigChangeEvent::OfflineMaxFolderSizeChanged(
            new.offline_max_folder_size.clone(),
        ));
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
        if enabled {
            enable(app_path)
        } else {
            disable()
        }
    }

    #[cfg(target_os = "linux")]
    fn enable(app_path: &str) -> crate::Result<()> {
        // Probe for systemd availability before writing any files.
        // On non-systemd distributions (Alpine/OpenRC, Void/runit, etc.) `systemctl`
        // is absent; writing the .service file first would leave a stale artifact.
        let systemd_available = std::process::Command::new("systemctl")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !systemd_available {
            return Err(crate::Error::Config(
                "systemd is not available on this system".into(),
            ));
        }

        let service_dir = dirs::config_dir()
            .map(|d| d.join("systemd/user"))
            .ok_or_else(|| crate::Error::Config("no config dir".into()))?;
        std::fs::create_dir_all(&service_dir)?;

        let unit = format!(
            "[Unit]\nDescription=Carmine Desktop\n\n[Service]\nExecStart={app_path}\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n"
        );
        std::fs::write(service_dir.join("carminedesktop.service"), unit)?;

        std::process::Command::new("systemctl")
            .args(["--user", "enable", "carminedesktop.service"])
            .output()
            .map_err(|e| crate::Error::Config(format!("systemctl enable failed: {e}")))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn disable() -> crate::Result<()> {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "disable", "carminedesktop.service"])
            .output();

        let service_path =
            dirs::config_dir().map(|d| d.join("systemd/user/carminedesktop.service"));
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
    <string>com.carmine-capital.desktop.agent</string>
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
        std::fs::write(
            plist_dir.join("com.carmine-capital.desktop.agent.plist"),
            plist,
        )?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn disable() -> crate::Result<()> {
        let plist_path = dirs::home_dir()
            .map(|d| d.join("Library/LaunchAgents/com.carmine-capital.desktop.agent.plist"));
        if let Some(path) = plist_path {
            let _ = std::fs::remove_file(path);
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn enable(app_path: &str) -> crate::Result<()> {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu
            .open_subkey_with_flags(
                r"Software\Microsoft\Windows\CurrentVersion\Run",
                KEY_SET_VALUE,
            )
            .map_err(|e| crate::Error::Config(format!("failed to open Run registry key: {e}")))?;
        run_key
            .set_value("Carmine Desktop", &app_path)
            .map_err(|e| {
                crate::Error::Config(format!("failed to set autostart registry value: {e}"))
            })?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn disable() -> crate::Result<()> {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_SET_VALUE,
        ) {
            let _ = run_key.delete_value("Carmine Desktop");
        }
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
