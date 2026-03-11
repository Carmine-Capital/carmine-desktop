use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const NAUTILUS_SCRIPT_NAME: &str = "Open in SharePoint";
const KDE_HELPER_NAME: &str = "cloudmount-kde-helper";
const KDE_MENU_NAME: &str = "cloudmount-open-in-sharepoint.desktop";

const NAUTILUS_SCRIPT_CONTENT: &str = include_str!("../scripts/open-in-sharepoint.sh");
const KDE_HELPER_CONTENT: &str = include_str!("../scripts/cloudmount-kde-helper");
const KDE_MENU_CONTENT: &str = include_str!("../scripts/cloudmount-open-in-sharepoint.desktop");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationStatus {
    Installed,
    NotInstalled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegrationAction {
    Installed,
    Removed,
}

pub fn status() -> IntegrationStatus {
    if integration_paths().is_ok_and(|paths| paths.fully_installed()) {
        IntegrationStatus::Installed
    } else {
        IntegrationStatus::NotInstalled
    }
}

pub fn toggle() -> Result<IntegrationAction, String> {
    match status() {
        IntegrationStatus::Installed => {
            remove_integrations()?;
            Ok(IntegrationAction::Removed)
        }
        IntegrationStatus::NotInstalled => {
            install_integrations()?;
            Ok(IntegrationAction::Installed)
        }
    }
}

struct IntegrationPaths {
    nautilus_script: PathBuf,
    kde_helper: PathBuf,
    kde_menus: [PathBuf; 2],
}

impl IntegrationPaths {
    fn from_home(home: PathBuf) -> Self {
        let local_share = home.join(".local/share");
        let local_bin = home.join(".local/bin");

        Self {
            nautilus_script: local_share
                .join("nautilus/scripts")
                .join(NAUTILUS_SCRIPT_NAME),
            kde_helper: local_bin.join(KDE_HELPER_NAME),
            kde_menus: [
                local_share.join("kio/servicemenus").join(KDE_MENU_NAME),
                local_share
                    .join("kservices5/ServiceMenus")
                    .join(KDE_MENU_NAME),
            ],
        }
    }

    fn fully_installed(&self) -> bool {
        self.nautilus_script.exists()
            && self.kde_helper.exists()
            && self.kde_menus.iter().any(|path| path.exists())
    }
}

fn integration_paths() -> Result<IntegrationPaths, String> {
    let home = dirs::home_dir().ok_or_else(|| "home directory is unavailable".to_string())?;
    Ok(IntegrationPaths::from_home(home))
}

fn install_integrations() -> Result<(), String> {
    let paths = integration_paths()?;

    write_executable_file(&paths.nautilus_script, NAUTILUS_SCRIPT_CONTENT)?;
    write_executable_file(&paths.kde_helper, KDE_HELPER_CONTENT)?;
    for menu in &paths.kde_menus {
        write_file(menu, KDE_MENU_CONTENT, 0o755)?;
    }

    rebuild_kde_service_cache();
    Ok(())
}

fn remove_integrations() -> Result<(), String> {
    let paths = integration_paths()?;

    remove_if_exists(&paths.nautilus_script)?;
    remove_if_exists(&paths.kde_helper)?;
    for menu in &paths.kde_menus {
        remove_if_exists(menu)?;
    }

    rebuild_kde_service_cache();
    Ok(())
}

fn write_executable_file(path: &Path, content: &str) -> Result<(), String> {
    write_file(path, content, 0o755)
}

fn write_file(path: &Path, content: &str, mode: u32) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("no parent directory for {}", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
    std::fs::write(path, content)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|e| format!("failed to chmod {}: {e}", path.display()))?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("failed to remove {}: {e}", path.display())),
    }
}

fn rebuild_kde_service_cache() {
    for command in ["kbuildsycoca6", "kbuildsycoca5"] {
        match Command::new(command).status() {
            Ok(status) => {
                if !status.success() {
                    tracing::warn!("{command} exited with status {status}");
                }
                return;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                tracing::warn!("failed to run {command}: {error}");
                return;
            }
        }
    }
}
