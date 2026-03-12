use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

// --- Open Online (SharePoint) ---
const NAUTILUS_OPEN_ONLINE_NAME: &str = "Open Online (SharePoint)";
const KDE_OPEN_ONLINE_HELPER_NAME: &str = "cloudmount-kde-helper";
const KDE_OPEN_ONLINE_MENU_NAME: &str = "cloudmount-open-in-sharepoint.desktop";

const NAUTILUS_OPEN_ONLINE_CONTENT: &str = include_str!("../scripts/open-online.sh");
const KDE_OPEN_ONLINE_HELPER_CONTENT: &str = include_str!("../scripts/cloudmount-kde-helper");
const KDE_OPEN_ONLINE_MENU_CONTENT: &str =
    include_str!("../scripts/cloudmount-open-in-sharepoint.desktop");

// --- Open Locally ---
const NAUTILUS_OPEN_LOCALLY_NAME: &str = "Open Locally";
const KDE_OPEN_LOCALLY_HELPER_NAME: &str = "cloudmount-kde-open-locally";
const KDE_OPEN_LOCALLY_MENU_NAME: &str = "cloudmount-open-locally.desktop";

const NAUTILUS_OPEN_LOCALLY_CONTENT: &str = include_str!("../scripts/open-locally.sh");
const KDE_OPEN_LOCALLY_HELPER_CONTENT: &str =
    include_str!("../scripts/cloudmount-kde-open-locally");
const KDE_OPEN_LOCALLY_MENU_CONTENT: &str =
    include_str!("../scripts/cloudmount-open-locally.desktop");

// Legacy entry name removed during migration
const LEGACY_NAUTILUS_SCRIPT_NAME: &str = "Open in SharePoint";

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

pub fn reconcile_existing_installation() -> Result<(), String> {
    let paths = integration_paths()?;
    if paths.any_asset_exists() {
        install_integrations()?;
    }
    Ok(())
}

struct IntegrationPaths {
    // Open Online (SharePoint)
    nautilus_open_online: PathBuf,
    kde_open_online_helper: PathBuf,
    kde_open_online_menus: [PathBuf; 2],

    // Open Locally
    nautilus_open_locally: PathBuf,
    kde_open_locally_helper: PathBuf,
    kde_open_locally_menus: [PathBuf; 2],

    // Legacy (for cleanup)
    legacy_nautilus_script: PathBuf,
}

impl IntegrationPaths {
    fn from_home(home: PathBuf) -> Self {
        let local_share = home.join(".local/share");
        let local_bin = home.join(".local/bin");
        let nautilus_scripts = local_share.join("nautilus/scripts");
        let kio_menus = local_share.join("kio/servicemenus");
        let kservices5_menus = local_share.join("kservices5/ServiceMenus");

        Self {
            nautilus_open_online: nautilus_scripts.join(NAUTILUS_OPEN_ONLINE_NAME),
            kde_open_online_helper: local_bin.join(KDE_OPEN_ONLINE_HELPER_NAME),
            kde_open_online_menus: [
                kio_menus.join(KDE_OPEN_ONLINE_MENU_NAME),
                kservices5_menus.join(KDE_OPEN_ONLINE_MENU_NAME),
            ],

            nautilus_open_locally: nautilus_scripts.join(NAUTILUS_OPEN_LOCALLY_NAME),
            kde_open_locally_helper: local_bin.join(KDE_OPEN_LOCALLY_HELPER_NAME),
            kde_open_locally_menus: [
                kio_menus.join(KDE_OPEN_LOCALLY_MENU_NAME),
                kservices5_menus.join(KDE_OPEN_LOCALLY_MENU_NAME),
            ],

            legacy_nautilus_script: nautilus_scripts.join(LEGACY_NAUTILUS_SCRIPT_NAME),
        }
    }

    fn fully_installed(&self) -> bool {
        self.nautilus_open_online.exists()
            && self.nautilus_open_locally.exists()
            && self.kde_open_online_helper.exists()
            && self.kde_open_locally_helper.exists()
            && self.kde_open_online_menus.iter().any(|p| p.exists())
            && self.kde_open_locally_menus.iter().any(|p| p.exists())
    }

    fn any_asset_exists(&self) -> bool {
        self.nautilus_open_online.exists()
            || self.nautilus_open_locally.exists()
            || self.kde_open_online_helper.exists()
            || self.kde_open_locally_helper.exists()
            || self.kde_open_online_menus.iter().any(|p| p.exists())
            || self.kde_open_locally_menus.iter().any(|p| p.exists())
            || self.legacy_nautilus_script.exists()
    }
}

fn integration_paths() -> Result<IntegrationPaths, String> {
    let home = dirs::home_dir().ok_or_else(|| "home directory is unavailable".to_string())?;
    Ok(IntegrationPaths::from_home(home))
}

fn install_integrations() -> Result<(), String> {
    let paths = integration_paths()?;

    // Remove legacy "Open in SharePoint" entry if present
    remove_if_exists(&paths.legacy_nautilus_script)?;

    // --- Open Online (SharePoint) ---
    let kde_open_online_menu =
        render_kde_menu_content(&paths.kde_open_online_helper, KDE_OPEN_ONLINE_MENU_CONTENT);
    write_executable_file(&paths.nautilus_open_online, NAUTILUS_OPEN_ONLINE_CONTENT)?;
    write_executable_file(
        &paths.kde_open_online_helper,
        KDE_OPEN_ONLINE_HELPER_CONTENT,
    )?;
    for menu in &paths.kde_open_online_menus {
        write_file(menu, &kde_open_online_menu, 0o755)?;
    }

    // --- Open Locally ---
    let kde_open_locally_menu = render_kde_menu_content(
        &paths.kde_open_locally_helper,
        KDE_OPEN_LOCALLY_MENU_CONTENT,
    );
    write_executable_file(&paths.nautilus_open_locally, NAUTILUS_OPEN_LOCALLY_CONTENT)?;
    write_executable_file(
        &paths.kde_open_locally_helper,
        KDE_OPEN_LOCALLY_HELPER_CONTENT,
    )?;
    for menu in &paths.kde_open_locally_menus {
        write_file(menu, &kde_open_locally_menu, 0o755)?;
    }

    rebuild_kde_service_cache();
    Ok(())
}

fn remove_integrations() -> Result<(), String> {
    let paths = integration_paths()?;

    // Remove legacy entry
    remove_if_exists(&paths.legacy_nautilus_script)?;

    // Remove Open Online entries
    remove_if_exists(&paths.nautilus_open_online)?;
    remove_if_exists(&paths.kde_open_online_helper)?;
    for menu in &paths.kde_open_online_menus {
        remove_if_exists(menu)?;
    }

    // Remove Open Locally entries
    remove_if_exists(&paths.nautilus_open_locally)?;
    remove_if_exists(&paths.kde_open_locally_helper)?;
    for menu in &paths.kde_open_locally_menus {
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

fn render_kde_menu_content(helper_path: &Path, template: &str) -> String {
    let helper = helper_path.to_string_lossy();
    let escaped_helper = helper.replace('"', "\\\"");
    let helper_filename = helper_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    template.replace(
        &format!("Exec={helper_filename} %F"),
        &format!("Exec=\"{escaped_helper}\" %F"),
    )
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
