//! Shell integration — file type associations for Office documents.
//!
//! Registers Carmine Desktop as a candidate handler for Office file types
//! (.docx, .xlsx, .pptx, .doc, .xls, .ppt) under HKCU using the Windows 10/11
//! Capabilities + RegisteredApplications + OpenWithProgids model.
//!
//! Carmine Desktop is **never** written as the default handler for an
//! extension — it only appears in the "Open with" list and Settings >
//! Default Apps. Users who want Carmine to handle Office files double-click
//! must select it manually from the Default Apps panel.

#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::RegValue;
#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, RegType};

/// Office file extensions we register as handlers for.
pub const OFFICE_EXTENSIONS: &[&str] = &[".docx", ".xlsx", ".pptx", ".doc", ".xls", ".ppt"];

/// Icon files bundled alongside the executable for Windows shell integration.
///
/// Each entry maps a file extension (with leading dot) to the `.ico` filename
/// in the `icons/` subdirectory next to the executable. Referenced in
/// `DefaultIcon` registry values as an absolute path to the `.ico` file.
#[cfg(target_os = "windows")]
const ICON_FILES: &[(&str, &str)] = &[
    (".doc", "doc.ico"),
    (".docx", "doc.ico"),
    (".xls", "xls.ico"),
    (".xlsx", "xls.ico"),
    (".ppt", "ppt.ico"),
    (".pptx", "ppt.ico"),
    (".pdf", "pdf.ico"),
];

/// ProgID prefix for our file type handlers.
#[cfg(target_os = "windows")]
pub(crate) const PROGID_PREFIX: &str = "CarmineDesktop.OfficeFile";

/// Registry path for the Carmine Desktop capabilities key.
#[cfg(target_os = "windows")]
const CAPABILITIES_PATH: &str = r"Software\CarmineDesktop\Capabilities";

/// Registry verb ID for the "Make available offline" context menu entry.
#[cfg(target_os = "windows")]
const CONTEXT_MENU_OFFLINE: &str = "CarmineDesktop.MakeOffline";
/// Registry verb ID for the "Free up space" context menu entry.
#[cfg(target_os = "windows")]
const CONTEXT_MENU_FREE_SPACE: &str = "CarmineDesktop.FreeSpace";

/// Register Carmine Desktop as a candidate handler for Office file types.
///
/// For each extension:
/// 1. Creates the ProgID key (`HKCU\Software\Classes\CarmineDesktop.OfficeFile.{ext}`)
///    with `DefaultIcon` and `shell\open\command` pointing to `Carmine Desktop.exe --open "%1"`
/// 2. Adds the ProgID to `HKCU\Software\Classes\{ext}\OpenWithProgids` so we
///    appear in the Windows "Open with" list
///
/// We **never** write the extension default (`HKCU\Software\Classes\{ext}\@`)
/// or set `UserChoice`. To become the default, the user must explicitly pick
/// Carmine Desktop in Settings > Default Apps.
///
/// Also writes Capabilities + RegisteredApplications so we appear in Settings.
///
/// # Errors
/// Returns an error if registry operations fail.
#[cfg(target_os = "windows")]
pub fn register_file_associations() -> carminedesktop_core::Result<()> {
    let exe_path = std::env::current_exe().map_err(|e| {
        carminedesktop_core::Error::Config(format!("failed to get current exe path: {e}"))
    })?;
    let exe_str = exe_path.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_READ | KEY_WRITE)?;

    for ext in OFFICE_EXTENSIONS {
        let progid = format!("{PROGID_PREFIX}{ext}");

        // Open or create the extension key (e.g. .docx). We never touch its
        // default value — that belongs to whatever app owns the association.
        let (ext_key, _) = classes.create_subkey(ext)?;

        // Create the ProgID key (e.g. CarmineDesktop.OfficeFile.docx)
        let (progid_key, _) = classes.create_subkey(&progid)?;

        // Set the display name
        let display_name = format!("Office Document (Carmine Desktop){ext}");
        progid_key.set_value("", &display_name)?;

        // Set DefaultIcon to the bundled .ico file (if mapped).
        // In dev builds the icon may not exist — skip gracefully.
        if let Some(&(_, icon_name)) = ICON_FILES.iter().find(|(e, _)| e == ext) {
            let exe_dir = exe_path.parent().ok_or_else(|| {
                carminedesktop_core::Error::Config(
                    "current exe has no parent directory".to_string(),
                )
            })?;
            let icon_path = exe_dir.join("icons").join(icon_name);
            if icon_path.exists() {
                let (icon_key, _) = progid_key.create_subkey("DefaultIcon")?;
                icon_key.set_value("", &icon_path.to_string_lossy().as_ref())?;
            }
        }

        // Create shell\open\command with the handler command
        let (shell_key, _) = progid_key.create_subkey("shell")?;
        let (open_key, _) = shell_key.create_subkey("open")?;
        let (command_key, _) = open_key.create_subkey("command")?;

        // Command: "C:\path\to\CarmineDesktop.exe" --open "%1"
        let command = format!("\"{exe_str}\" --open \"%1\"");
        command_key.set_value("", &command)?;

        // Add to OpenWithProgids so Carmine Desktop appears in the "Open with" dialog
        // on Windows 10/11. The value name is the ProgID; the data is empty (REG_NONE).
        let (owp_key, _) = ext_key.create_subkey("OpenWithProgids")?;
        owp_key.set_raw_value(
            &progid,
            &RegValue {
                bytes: vec![],
                vtype: RegType::REG_NONE,
            },
        )?;

        tracing::info!("registered file association candidate for {ext}");
    }

    // Register application capabilities (Windows 10/11 modern file association model).
    // This makes Carmine Desktop visible in Settings > Default Apps and "Open with" dialogs.
    let (cap_key, _) = hkcu.create_subkey(CAPABILITIES_PATH)?;
    cap_key.set_value(
        "ApplicationDescription",
        &"Mounts SharePoint and OneDrive as local drives",
    )?;
    cap_key.set_value("ApplicationName", &"Carmine Desktop")?;

    let (fa_key, _) = cap_key.create_subkey("FileAssociations")?;
    for ext in OFFICE_EXTENSIONS {
        fa_key.set_value(ext, &format!("{PROGID_PREFIX}{ext}"))?;
    }

    // Point RegisteredApplications to our capabilities key.
    let (ra_key, _) = hkcu.create_subkey(r"Software\RegisteredApplications")?;
    ra_key.set_value("CarmineDesktop", &CAPABILITIES_PATH)?;

    // Notify the shell that file associations have changed.
    notify_shell_change();

    Ok(())
}

/// Unregister Carmine Desktop file associations.
///
/// Deletes only the keys we own:
/// - `HKCU\Software\Classes\CarmineDesktop.OfficeFile.{ext}` ProgID trees
/// - The Carmine Desktop entry under each `{ext}\OpenWithProgids`
/// - Capabilities + RegisteredApplications entries
///
/// We never overwrite the per-extension default, so there is no previous
/// handler to restore.
///
/// # Errors
/// Returns an error if registry operations fail.
#[cfg(target_os = "windows")]
pub fn unregister_file_associations() -> carminedesktop_core::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = match hkcu.open_subkey_with_flags(r"Software\Classes", KEY_READ | KEY_WRITE) {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!("failed to open HKCU\\Software\\Classes: {e}");
            return Ok(()); // Nothing to unregister
        }
    };

    for ext in OFFICE_EXTENSIONS {
        let progid = format!("{PROGID_PREFIX}{ext}");

        // Drop our entry from OpenWithProgids (leave the key itself — other apps may share it)
        if let Ok(ext_key) = classes.open_subkey_with_flags(ext, KEY_READ | KEY_WRITE)
            && let Ok(owp_key) = ext_key.open_subkey_with_flags("OpenWithProgids", KEY_WRITE)
        {
            let _ = owp_key.delete_value(&progid);
        }

        // Delete our ProgID key tree
        if let Err(e) = classes.delete_subkey_all(&progid) {
            tracing::debug!("failed to delete ProgID {progid}: {e}");
        }

        tracing::info!("unregistered file association for {ext}");
    }

    // Remove Capabilities and RegisteredApplications entries
    if let Err(e) = hkcu.delete_subkey_all(CAPABILITIES_PATH) {
        tracing::debug!("failed to delete Capabilities key: {e}");
    }
    // Also remove the parent CarmineDesktop key if empty
    let _ = hkcu.delete_subkey(r"Software\CarmineDesktop");

    if let Ok(ra_key) = hkcu.open_subkey_with_flags(r"Software\RegisteredApplications", KEY_WRITE) {
        let _ = ra_key.delete_value("CarmineDesktop");
    }

    // Notify the shell that file associations have changed
    notify_shell_change();

    Ok(())
}

/// Check if Carmine Desktop file associations are currently registered.
///
/// Returns `true` if at least one Office extension has Carmine Desktop in its
/// `OpenWithProgids` or as its default handler, AND the `RegisteredApplications`
/// entry exists. This covers both the legacy and modern Windows 10/11 models.
#[cfg(target_os = "windows")]
pub fn are_file_associations_registered() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Check modern registration: RegisteredApplications entry
    if hkcu
        .open_subkey_with_flags(r"Software\RegisteredApplications", KEY_READ)
        .ok()
        .and_then(|ra| ra.get_value::<String, _>("CarmineDesktop").ok())
        .is_some()
    {
        return true;
    }

    // Fallback: check legacy per-extension default
    let Ok(classes) = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_READ) else {
        return false;
    };

    for ext in OFFICE_EXTENSIONS {
        let Ok(ext_key) = classes.open_subkey_with_flags(ext, KEY_READ) else {
            continue;
        };
        if let Ok(progid) = ext_key.get_value::<String, _>("")
            && progid.starts_with(PROGID_PREFIX)
        {
            return true;
        }
    }

    false
}

/// Notify Windows Shell that file associations have changed.
///
/// Calls `SHChangeNotify` to refresh Explorer's cached associations and
/// broadcasts `WM_SETTINGCHANGE` so Explorer refreshes file associations
/// immediately without requiring a reboot.
#[cfg(target_os = "windows")]
fn notify_shell_change() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    use windows::Win32::UI::WindowsAndMessaging::{
        HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
    };

    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);

        // Broadcast WM_SETTINGCHANGE so Explorer refreshes file associations
        // immediately without requiring a reboot.
        let _ = SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            5000,
            None,
        );
    }
}

// ---------------------------------------------------------------------------
// Offline Office open support
// ---------------------------------------------------------------------------

/// Fallback ProgID for Office 2013+ documents, keyed by dotted extension.
///
/// Used when the user hasn't set a `file_handler_overrides` entry and the
/// system's `OpenWithProgids` list yields nothing usable.
#[cfg(target_os = "windows")]
pub fn default_office_progid(ext: &str) -> Option<&'static str> {
    match ext.to_ascii_lowercase().as_str() {
        ".docx" => Some("Word.Document.12"),
        ".doc" => Some("Word.Document.8"),
        ".xlsx" => Some("Excel.Sheet.12"),
        ".xls" => Some("Excel.Sheet.8"),
        ".pptx" => Some("PowerPoint.Show.12"),
        ".ppt" => Some("PowerPoint.Show.8"),
        _ => None,
    }
}

/// Return the first `OpenWithProgids` entry for `ext` that is **not** one of
/// ours (`CarmineDesktop.OfficeFile*`). Looks in `HKEY_CLASSES_ROOT` which is
/// the merged HKCU+HKLM view Windows uses for shell lookups.
#[cfg(target_os = "windows")]
pub fn find_non_carmine_progid(ext: &str) -> Option<String> {
    use winreg::enums::HKEY_CLASSES_ROOT;

    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let ext_key = hkcr.open_subkey_with_flags(ext, KEY_READ).ok()?;
    let owp_key = ext_key
        .open_subkey_with_flags("OpenWithProgids", KEY_READ)
        .ok()?;
    owp_key
        .enum_values()
        .filter_map(|v| v.ok())
        .map(|(name, _)| name)
        .find(|name| !name.starts_with(PROGID_PREFIX))
}

/// Launch `path` using the handler registered under `progid`, bypassing the
/// per-user default association. Equivalent to invoking `ShellExecuteEx` with
/// `SEE_MASK_CLASSNAME` and the given class — lets us route around Carmine's
/// own default-handler registration when opening cached files offline.
#[cfg(target_os = "windows")]
pub fn open_with_progid(path: &std::path::Path, progid: &str) -> carminedesktop_core::Result<()> {
    use windows::Win32::UI::Shell::{SEE_MASK_CLASSNAME, SHELLEXECUTEINFOW, ShellExecuteExW};
    use windows::core::{HSTRING, PCWSTR};

    // Keep HSTRINGs alive across the ShellExecuteExW call — SHELLEXECUTEINFOW
    // holds raw pointers into their buffers.
    let path_h = HSTRING::from(path.as_os_str());
    let progid_h = HSTRING::from(progid);
    let verb_h = HSTRING::from("open");

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_CLASSNAME;
    info.lpVerb = PCWSTR(verb_h.as_ptr());
    info.lpFile = PCWSTR(path_h.as_ptr());
    info.lpClass = PCWSTR(progid_h.as_ptr());
    info.nShow = 1; // SW_SHOWNORMAL

    unsafe {
        ShellExecuteExW(&mut info).map_err(|e| {
            carminedesktop_core::Error::Other(anyhow::anyhow!(
                "ShellExecuteExW failed for progid '{progid}': {e}"
            ))
        })
    }
}

// ---------------------------------------------------------------------------
// Windows Explorer navigation pane integration
// ---------------------------------------------------------------------------

/// CLSID for the Carmine Desktop navigation pane entry in Windows Explorer.
///
/// This GUID identifies our virtual shell folder in the registry. It is used
/// to create the CLSID key tree, the Desktop\NameSpace pin, and the
/// HideDesktopIcons entry.
#[cfg(target_os = "windows")]
const NAV_PANE_CLSID: &str = "{E4B3F4A1-7C2D-4A8E-B5D6-9F1E2A3C4B5D}";

/// CLSID of the delegate folder class used by Windows Shell to display
/// a filesystem folder as a virtual shell namespace extension.
#[cfg(target_os = "windows")]
const DELEGATE_FOLDER_CLSID: &str = "{0E5AAE11-A475-4c5b-AB00-C66DE400274E}";

/// Register Carmine Desktop in the Windows Explorer navigation pane.
///
/// Creates three registry key trees under HKCU:
/// 1. CLSID definition with shell folder properties pointing to `cloud_root`
/// 2. Desktop\NameSpace entry to pin the folder in Explorer
/// 3. HideDesktopIcons entry to prevent a desktop shortcut
///
/// Calls `SHChangeNotify` afterwards so Explorer picks up the change.
///
/// # Errors
/// Returns an error if any registry operation fails.
#[cfg(target_os = "windows")]
pub fn register_nav_pane(cloud_root: &std::path::Path) -> carminedesktop_core::Result<()> {
    let exe_path = std::env::current_exe().map_err(|e| {
        carminedesktop_core::Error::Config(format!("failed to get current exe path: {e}"))
    })?;
    let exe_str = exe_path.to_string_lossy();
    let target = cloud_root.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // 1. CLSID key tree
    let clsid_path = format!(r"Software\Classes\CLSID\{NAV_PANE_CLSID}");
    let (clsid_key, _) = hkcu.create_subkey(&clsid_path)?;
    clsid_key.set_value("", &"Carmine Desktop")?;
    // Required on Windows 10+ for the entry to appear in the navigation pane
    clsid_key.set_value("System.IsPinnedToNameSpaceTree", &1u32)?;

    // DefaultIcon
    let (icon_key, _) = clsid_key.create_subkey("DefaultIcon")?;
    icon_key.set_value("", &format!("{exe_str},0"))?;

    // InProcServer32 — REG_EXPAND_SZ so that %SystemRoot% is expanded by COM
    let (inproc_key, _) = clsid_key.create_subkey("InProcServer32")?;
    let shell32_path = r"%SystemRoot%\system32\shell32.dll";
    let wide: Vec<u16> = shell32_path
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let bytes: Vec<u8> = wide.iter().flat_map(|&w| w.to_le_bytes()).collect();
    inproc_key.set_raw_value(
        "",
        &RegValue {
            bytes,
            vtype: RegType::REG_EXPAND_SZ,
        },
    )?;
    inproc_key.set_value("ThreadingModel", &"Both")?;

    // Instance
    let (instance_key, _) = clsid_key.create_subkey("Instance")?;
    instance_key.set_value("CLSID", &DELEGATE_FOLDER_CLSID)?;

    // Instance\InitPropertyBag
    let (bag_key, _) = instance_key.create_subkey("InitPropertyBag")?;
    bag_key.set_value("TargetFolderPath", &target.as_ref())?;
    bag_key.set_value("Attributes", &0x11u32)?;

    // ShellFolder
    let (shell_folder_key, _) = clsid_key.create_subkey("ShellFolder")?;
    shell_folder_key.set_value("FolderValueFlags", &0x28u32)?;
    // SFGAO_HASSUBFOLDER | SFGAO_FILESYSTEM | SFGAO_FOLDER |
    // SFGAO_FILESYSANCESTOR | SFGAO_STORAGEANCESTOR |
    // SFGAO_HASPROPSHEET | SFGAO_STORAGE | SFGAO_CANLINK | SFGAO_CANCOPY
    // SFGAO_HASSUBFOLDER (0x80000000) enables tree expansion in the nav pane.
    // SFGAO_ISSLOW is intentionally NOT set: the root directory is a plain
    // NTFS folder (not a WinFsp mount) so enumeration is instant. Including
    // ISSLOW causes Explorer to skip nav-pane child enumeration, hiding the
    // mounted drives from the tree. Google Drive and Dropbox use the same
    // value (0xF080004D) for their delegate-folder nav-pane entries.
    shell_folder_key.set_value("Attributes", &0xF080004Du32)?;

    // shell\open\command
    let (shell_key, _) = clsid_key.create_subkey("shell")?;
    let (open_key, _) = shell_key.create_subkey("open")?;
    let (command_key, _) = open_key.create_subkey("command")?;
    command_key.set_value("", &format!("\"{exe_str}\""))?;

    // 2. Desktop\NameSpace pin
    let ns_path = format!(
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{NAV_PANE_CLSID}"
    );
    let (ns_key, _) = hkcu.create_subkey(&ns_path)?;
    ns_key.set_value("", &"Carmine Desktop")?;

    // 3. HideDesktopIcons — prevent desktop shortcut
    let hide_path =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel"
            .to_string();
    let (hide_key, _) = hkcu.create_subkey(&hide_path)?;
    hide_key.set_value(NAV_PANE_CLSID, &1u32)?;

    notify_shell_change();

    tracing::info!(
        "registered Explorer navigation pane entry pointing to {}",
        cloud_root.display()
    );
    Ok(())
}

/// Unregister Carmine Desktop from the Windows Explorer navigation pane.
///
/// Removes the three registry key trees created by [`register_nav_pane`].
/// Missing keys are silently ignored (logged at debug level) so that this
/// function is safe to call even if the pane was never registered.
///
/// Calls `SHChangeNotify` afterwards so Explorer picks up the change.
///
/// # Errors
/// Returns an error only if a key exists but cannot be deleted.
#[cfg(target_os = "windows")]
pub fn unregister_nav_pane() -> carminedesktop_core::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // 1. Remove CLSID key tree
    let clsid_parent_path = r"Software\Classes\CLSID";
    match hkcu.open_subkey_with_flags(clsid_parent_path, KEY_READ | KEY_WRITE) {
        Ok(clsid_parent) => match clsid_parent.delete_subkey_all(NAV_PANE_CLSID) {
            Ok(()) => tracing::debug!("removed CLSID key for nav pane"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("CLSID key for nav pane not found, skipping");
            }
            Err(e) => return Err(e.into()),
        },
        Err(e) => {
            tracing::debug!("could not open CLSID parent key: {e}, skipping");
        }
    }

    // 2. Remove Desktop\NameSpace entry
    let ns_parent_path = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace";
    match hkcu.open_subkey_with_flags(ns_parent_path, KEY_READ | KEY_WRITE) {
        Ok(ns_parent) => match ns_parent.delete_subkey_all(NAV_PANE_CLSID) {
            Ok(()) => tracing::debug!("removed Desktop\\NameSpace entry for nav pane"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("Desktop\\NameSpace entry for nav pane not found, skipping");
            }
            Err(e) => return Err(e.into()),
        },
        Err(e) => {
            tracing::debug!("could not open NameSpace parent key: {e}, skipping");
        }
    }

    // 3. Remove HideDesktopIcons value
    let hide_path =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel";
    match hkcu.open_subkey_with_flags(hide_path, KEY_READ | KEY_WRITE) {
        Ok(hide_key) => match hide_key.delete_value(NAV_PANE_CLSID) {
            Ok(()) => tracing::debug!("removed HideDesktopIcons value for nav pane"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("HideDesktopIcons value for nav pane not found, skipping");
            }
            Err(e) => return Err(e.into()),
        },
        Err(e) => {
            tracing::debug!("could not open HideDesktopIcons key: {e}, skipping");
        }
    }

    notify_shell_change();

    tracing::info!("unregistered Explorer navigation pane entry");
    Ok(())
}

/// Check whether the Carmine Desktop navigation pane entry is registered.
///
/// Returns `true` if the CLSID key exists under `HKCU\Software\Classes\CLSID`.
#[cfg(target_os = "windows")]
pub fn is_nav_pane_registered() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid_path = format!(r"Software\Classes\CLSID\{NAV_PANE_CLSID}");
    hkcu.open_subkey_with_flags(&clsid_path, KEY_READ).is_ok()
}

/// Update the target folder path and icon for an existing navigation pane entry.
///
/// This is cheaper than a full unregister + register cycle: it only writes the
/// `TargetFolderPath` value and refreshes the `DefaultIcon` (the exe path may
/// have changed after an update).
///
/// Calls `SHChangeNotify` afterwards so Explorer picks up the change.
///
/// # Errors
/// Returns an error if the CLSID key does not exist or cannot be written.
#[cfg(target_os = "windows")]
pub fn update_nav_pane_target(cloud_root: &std::path::Path) -> carminedesktop_core::Result<()> {
    let exe_path = std::env::current_exe().map_err(|e| {
        carminedesktop_core::Error::Config(format!("failed to get current exe path: {e}"))
    })?;
    let exe_str = exe_path.to_string_lossy();
    let target = cloud_root.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid_path = format!(r"Software\Classes\CLSID\{NAV_PANE_CLSID}");
    let clsid_key = hkcu.open_subkey_with_flags(&clsid_path, KEY_READ | KEY_WRITE)?;

    // Update TargetFolderPath
    let bag_key =
        clsid_key.open_subkey_with_flags(r"Instance\InitPropertyBag", KEY_READ | KEY_WRITE)?;
    bag_key.set_value("TargetFolderPath", &target.as_ref())?;

    // Update DefaultIcon (exe path may have changed)
    let icon_key = clsid_key.open_subkey_with_flags("DefaultIcon", KEY_READ | KEY_WRITE)?;
    icon_key.set_value("", &format!("{exe_str},0"))?;

    notify_shell_change();

    tracing::info!(
        "updated Explorer navigation pane target to {}",
        cloud_root.display()
    );
    Ok(())
}

/// Ensure the navigation pane entry is registered and up-to-date.
///
/// Compares the current `TargetFolderPath` in the registry against `cloud_root`.
/// If the CLSID key exists and the target matches, this is a no-op — avoiding
/// the costly `SHChangeNotify(SHCNE_ASSOCCHANGED)` that a full
/// [`register_nav_pane`] call would trigger.
///
/// If the entry is missing or the target differs, delegates to
/// [`register_nav_pane`] (which sends the notification).
#[cfg(target_os = "windows")]
pub fn ensure_nav_pane(cloud_root: &std::path::Path) -> carminedesktop_core::Result<()> {
    let target = cloud_root.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid_path = format!(r"Software\Classes\CLSID\{NAV_PANE_CLSID}");

    if let Ok(clsid_key) = hkcu.open_subkey_with_flags(&clsid_path, KEY_READ)
        && let Ok(bag) = clsid_key.open_subkey_with_flags(r"Instance\InitPropertyBag", KEY_READ)
        && let Ok(existing_target) = bag.get_value::<String, _>("TargetFolderPath")
        && existing_target == target.as_ref()
        && let Ok(sf) = clsid_key.open_subkey_with_flags("ShellFolder", KEY_READ)
        && let Ok(attrs) = sf.get_value::<u32, _>("Attributes")
        && attrs == 0xF080004Du32
    {
        tracing::debug!("nav pane already registered with correct target and attributes, skipping");
        return Ok(());
    }

    register_nav_pane(cloud_root)
}

/// Register "Make available offline" and "Free up space" context menu
/// verbs under `HKCU\Software\Classes\Directory\shell\`.
///
/// The verbs are scoped to VFS mount paths via the `AppliesTo` AQS filter
/// so they only appear on folders within Carmine Desktop mounts.
///
/// Calls `SHChangeNotify` afterwards so Explorer picks up the change.
#[cfg(target_os = "windows")]
pub fn register_context_menu(mount_paths: &[String]) -> carminedesktop_core::Result<()> {
    if mount_paths.is_empty() {
        return Ok(());
    }

    let exe_path = std::env::current_exe().map_err(|e| {
        carminedesktop_core::Error::Config(format!("failed to get current exe path: {e}"))
    })?;
    let exe_str = exe_path.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dir_shell = hkcu.create_subkey(r"Software\Classes\Directory\shell")?.0;

    // Build AppliesTo AQS filter: OR-join mount paths
    let applies_to = mount_paths
        .iter()
        .map(|p| format!("System.ItemPathDisplay:~<\"{}\"", p))
        .collect::<Vec<_>>()
        .join(" OR ");

    // Register "Make available offline"
    {
        let (verb_key, _) = dir_shell.create_subkey(CONTEXT_MENU_OFFLINE)?;
        verb_key.set_value("MUIVerb", &"Make available offline")?;
        verb_key.set_value("AppliesTo", &applies_to)?;
        verb_key.set_value("Icon", &format!("{exe_str},0"))?;
        let (cmd_key, _) = verb_key.create_subkey("command")?;
        cmd_key.set_value("", &format!("\"{}\" --offline-pin \"%V\"", exe_str))?;
    }

    // Register "Free up space"
    {
        let (verb_key, _) = dir_shell.create_subkey(CONTEXT_MENU_FREE_SPACE)?;
        verb_key.set_value("MUIVerb", &"Free up space")?;
        verb_key.set_value("AppliesTo", &applies_to)?;
        verb_key.set_value("Icon", &format!("{exe_str},0"))?;
        let (cmd_key, _) = verb_key.create_subkey("command")?;
        cmd_key.set_value("", &format!("\"{}\" --offline-unpin \"%V\"", exe_str))?;
    }

    notify_shell_change();
    tracing::info!("registered offline context menu verbs");
    Ok(())
}

/// Remove the offline context menu verbs from the registry.
///
/// Missing keys are silently ignored (idempotent).
#[cfg(target_os = "windows")]
pub fn unregister_context_menu() -> carminedesktop_core::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dir_shell = match hkcu
        .open_subkey_with_flags(r"Software\Classes\Directory\shell", KEY_READ | KEY_WRITE)
    {
        Ok(k) => k,
        Err(e) => {
            tracing::debug!("could not open Directory\\shell: {e}, skipping");
            return Ok(());
        }
    };

    for verb in [CONTEXT_MENU_OFFLINE, CONTEXT_MENU_FREE_SPACE] {
        match dir_shell.delete_subkey_all(verb) {
            Ok(()) => tracing::debug!("removed context menu verb {verb}"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::debug!("context menu verb {verb} not found, skipping");
            }
            Err(e) => {
                tracing::warn!("failed to remove context menu verb {verb}: {e}");
            }
        }
    }

    notify_shell_change();
    tracing::info!("unregistered offline context menu verbs");
    Ok(())
}

/// Update the `AppliesTo` filter on existing context menu verbs
/// without a full unregister/register cycle.
#[cfg(target_os = "windows")]
#[allow(dead_code)] // Reserved for future use when mount paths change dynamically
pub fn update_context_menu_paths(mount_paths: &[String]) -> carminedesktop_core::Result<()> {
    if mount_paths.is_empty() {
        return unregister_context_menu();
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dir_shell =
        hkcu.open_subkey_with_flags(r"Software\Classes\Directory\shell", KEY_READ | KEY_WRITE)?;

    let applies_to = mount_paths
        .iter()
        .map(|p| format!("System.ItemPathDisplay:~<\"{}\"", p))
        .collect::<Vec<_>>()
        .join(" OR ");

    for verb in [CONTEXT_MENU_OFFLINE, CONTEXT_MENU_FREE_SPACE] {
        if let Ok(verb_key) = dir_shell.open_subkey_with_flags(verb, KEY_READ | KEY_WRITE) {
            verb_key.set_value("AppliesTo", &applies_to)?;
        }
    }

    notify_shell_change();
    Ok(())
}

// ---------------------------------------------------------------------------
// Linux shell integration — no-op stubs
// ---------------------------------------------------------------------------
//
// Linux file association support has been removed. These stubs ensure the
// public API compiles on Linux without the linux module.

#[cfg(target_os = "linux")]
pub fn register_file_associations() -> carminedesktop_core::Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn unregister_file_associations() -> carminedesktop_core::Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn are_file_associations_registered() -> bool {
    false
}

#[cfg(not(target_os = "windows"))]
pub fn register_context_menu(_mount_paths: &[String]) -> carminedesktop_core::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub fn unregister_context_menu() -> carminedesktop_core::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)] // Reserved for future use when mount paths change dynamically
pub fn update_context_menu_paths(_mount_paths: &[String]) -> carminedesktop_core::Result<()> {
    Ok(())
}

// ---------------------------------------------------------------------------
// macOS shell integration — Launch Services / duti
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos {
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Mapping from file extensions (without dot) to UTI types.
    /// macOS uses UTIs (Uniform Type Identifiers) for Launch Services.
    const UTI_MAP: &[(&str, &str)] = &[
        ("docx", "org.openxmlformats.wordprocessingml.document"),
        ("xlsx", "org.openxmlformats.spreadsheetml.sheet"),
        ("pptx", "org.openxmlformats.presentationml.presentation"),
        ("doc", "com.microsoft.word.doc"),
        ("xls", "com.microsoft.excel.xls"),
        ("ppt", "com.microsoft.powerpoint.ppt"),
    ];

    /// Our macOS bundle identifier (must match tauri.conf.json).
    const BUNDLE_ID: &str = "com.carmine-capital.desktop";

    /// JSON file where we store previous handlers for restoration.
    const PREVIOUS_HANDLERS_FILE: &str = "previous_handlers.json";

    /// Path to our previous handlers JSON: ~/Library/Application Support/carminedesktop/previous_handlers.json
    fn previous_handlers_path() -> carminedesktop_core::Result<PathBuf> {
        carminedesktop_core::config::config_dir().map(|d| d.join(PREVIOUS_HANDLERS_FILE))
    }

    /// Load previous handlers from JSON file.
    fn load_previous_handlers() -> HashMap<String, String> {
        let Ok(path) = previous_handlers_path() else {
            return HashMap::new();
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return HashMap::new();
        };
        serde_json::from_str(&content).unwrap_or_default()
    }

    /// Save previous handlers to JSON file.
    fn save_previous_handlers(
        handlers: &HashMap<String, String>,
    ) -> carminedesktop_core::Result<()> {
        let path = previous_handlers_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                carminedesktop_core::Error::Config(format!(
                    "failed to create config dir {}: {e}",
                    parent.display()
                ))
            })?;
        }
        let json = serde_json::to_string_pretty(handlers).map_err(|e| {
            carminedesktop_core::Error::Config(format!(
                "failed to serialize previous handlers: {e}"
            ))
        })?;
        std::fs::write(&path, json).map_err(|e| {
            carminedesktop_core::Error::Config(format!(
                "failed to write previous handlers to {}: {e}",
                path.display()
            ))
        })?;
        Ok(())
    }

    /// Get the UTI for an extension (without the leading dot).
    fn uti_for_ext(ext_no_dot: &str) -> Option<&'static str> {
        UTI_MAP
            .iter()
            .find(|(e, _)| e.eq_ignore_ascii_case(ext_no_dot))
            .map(|(_, u)| *u)
    }

    /// Query the current default handler for an extension using `duti -x`.
    ///
    /// `duti -x <ext>` outputs something like:
    /// ```text
    /// Microsoft Word.app
    /// /Applications/Microsoft Office/Microsoft Word.app
    /// com.microsoft.Word
    /// ```
    /// We want the bundle ID (last line).
    fn duti_query_default(ext_no_dot: &str) -> Option<String> {
        let output = std::process::Command::new("duti")
            .args(["-x", ext_no_dot])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        // The bundle ID is typically the last non-empty line
        stdout
            .lines()
            .rev()
            .find(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && trimmed.contains('.')
            })
            .map(|s| s.trim().to_string())
    }

    /// Set the default handler for a UTI using `duti -s`.
    ///
    /// `duti -s <bundle_id> <uti> all` sets the handler for all roles.
    fn duti_set_default(bundle_id: &str, uti: &str) -> Result<(), String> {
        let status = std::process::Command::new("duti")
            .args(["-s", bundle_id, uti, "all"])
            .status()
            .map_err(|e| format!("failed to run duti: {e}"))?;
        if !status.success() {
            return Err(format!(
                "duti -s {bundle_id} {uti} all failed with {status}"
            ));
        }
        Ok(())
    }

    /// Check if `duti` is available on the system.
    fn has_duti() -> bool {
        std::process::Command::new("duti")
            .arg("-h")
            .output()
            .is_ok()
    }

    /// Refresh Launch Services database using `lsregister`.
    fn refresh_launch_services() {
        // lsregister lives in the LaunchServices framework
        let lsregister = "/System/Library/Frameworks/CoreServices.framework\
            /Versions/A/Frameworks/LaunchServices.framework\
            /Versions/A/Support/lsregister";
        // Re-register the running app bundle (if running as .app)
        if let Ok(exe) = std::env::current_exe() {
            // Walk up to find the .app bundle
            let mut path = exe.as_path();
            while let Some(parent) = path.parent() {
                if parent.extension().is_some_and(|ext| ext == "app") {
                    let _ = std::process::Command::new(lsregister)
                        .args(["-f", &parent.to_string_lossy()])
                        .status();
                    break;
                }
                path = parent;
            }
        }
    }

    /// Register Carmine Desktop as the handler for Office file types on macOS.
    ///
    /// 1. Saves current default handlers to a JSON file
    /// 2. Registers as default via duti (if available)
    /// 3. Refreshes Launch Services
    pub fn register() -> carminedesktop_core::Result<()> {
        if !has_duti() {
            tracing::warn!(
                "duti not found — file association registration requires duti. \
                 Install with: brew install duti"
            );
            return Err(carminedesktop_core::Error::Config(
                "duti is required for macOS file association registration. \
                 Install with: brew install duti"
                    .into(),
            ));
        }

        let mut previous = load_previous_handlers();

        for ext in super::OFFICE_EXTENSIONS {
            let ext_no_dot = &ext[1..]; // strip leading dot
            let Some(uti) = uti_for_ext(ext_no_dot) else {
                tracing::warn!("no UTI mapping for extension {ext}");
                continue;
            };

            // Save the current handler if we haven't already and it's not us
            if !previous.contains_key(ext_no_dot)
                && let Some(current) = duti_query_default(ext_no_dot)
                && current != BUNDLE_ID
            {
                previous.insert(ext_no_dot.to_string(), current.clone());
                tracing::debug!("saved previous handler for {ext}: {current}");
            }

            match duti_set_default(BUNDLE_ID, uti) {
                Ok(()) => {
                    tracing::info!("registered file association for {ext} ({uti})");
                }
                Err(e) => {
                    tracing::warn!("failed to set default handler for {ext} ({uti}): {e}");
                }
            }
        }

        save_previous_handlers(&previous)?;
        refresh_launch_services();

        Ok(())
    }

    /// Unregister Carmine Desktop file associations and restore previous handlers.
    pub fn unregister() -> carminedesktop_core::Result<()> {
        if !has_duti() {
            tracing::warn!("duti not found — cannot restore file associations");
            // Still clean up our saved handlers file
            if let Ok(handlers_path) = previous_handlers_path() {
                let _ = std::fs::remove_file(handlers_path);
            }
            return Ok(());
        }

        let previous = load_previous_handlers();

        for ext in super::OFFICE_EXTENSIONS {
            let ext_no_dot = &ext[1..];
            let Some(uti) = uti_for_ext(ext_no_dot) else {
                continue;
            };

            // Only restore if we're currently the handler
            if let Some(current) = duti_query_default(ext_no_dot)
                && current == BUNDLE_ID
                && let Some(prev) = previous.get(ext_no_dot)
            {
                match duti_set_default(prev, uti) {
                    Ok(()) => {
                        tracing::debug!("restored previous handler for {ext}: {prev}");
                    }
                    Err(e) => {
                        tracing::warn!("failed to restore handler for {ext}: {e}");
                    }
                }
            }
        }

        // Clean up previous handlers file
        if let Ok(handlers_path) = previous_handlers_path() {
            let _ = std::fs::remove_file(handlers_path);
        }

        refresh_launch_services();
        Ok(())
    }

    /// Check if Carmine Desktop file associations are currently registered.
    ///
    /// Returns `true` if Carmine Desktop is the default handler for at least one
    /// Office extension.
    pub fn is_registered() -> bool {
        if !has_duti() {
            return false;
        }

        for ext in super::OFFICE_EXTENSIONS {
            let ext_no_dot = &ext[1..];
            if let Some(current) = duti_query_default(ext_no_dot)
                && current == BUNDLE_ID
            {
                return true;
            }
        }

        false
    }

    /// Get the previous handler's bundle ID for an extension.
    ///
    /// Returns the saved bundle ID (e.g. "com.microsoft.Word") if one was
    /// stored during registration.
    pub fn get_previous(ext: &str) -> Option<String> {
        let ext_no_dot = ext.strip_prefix('.').unwrap_or(ext);
        let handlers = load_previous_handlers();
        handlers.get(ext_no_dot).cloned()
    }

    /// Well-known bundle IDs for Office suites, grouped by extension category.
    const WELL_KNOWN_SPREADSHEET_BUNDLES: &[&str] =
        &["com.microsoft.Excel", "org.libreoffice.script"];

    const WELL_KNOWN_WORD_BUNDLES: &[&str] = &["com.microsoft.Word", "org.libreoffice.script"];

    const WELL_KNOWN_PRESENTATION_BUNDLES: &[&str] =
        &["com.microsoft.Powerpoint", "org.libreoffice.script"];

    /// Map an extension to its well-known bundle IDs.
    fn well_known_bundles_for_ext(ext: &str) -> &'static [&'static str] {
        let ext_no_dot = ext.strip_prefix('.').unwrap_or(ext);
        match ext_no_dot.to_ascii_lowercase().as_str() {
            "xlsx" | "xls" => WELL_KNOWN_SPREADSHEET_BUNDLES,
            "docx" | "doc" => WELL_KNOWN_WORD_BUNDLES,
            "pptx" | "ppt" => WELL_KNOWN_PRESENTATION_BUNDLES,
            _ => &[],
        }
    }

    /// Discover an Office application handler at runtime for the given extension.
    ///
    /// Fallback when `get_previous()` returns `None`. Checks well-known bundle
    /// IDs and verifies they are installed via `mdfind`.
    ///
    /// Returns the first bundle ID whose application is installed.
    pub fn discover(ext: &str) -> Option<String> {
        for &bundle_id in well_known_bundles_for_ext(ext) {
            if bundle_id == BUNDLE_ID {
                continue;
            }
            if resolve_app_path(bundle_id).is_some() {
                tracing::debug!(
                    "discover_office_handler({ext}): found installed bundle: {bundle_id}"
                );
                return Some(bundle_id.to_string());
            }
        }

        tracing::debug!("discover_office_handler({ext}): no handler discovered");
        None
    }

    /// Resolve a bundle ID to the application path using `mdfind`.
    ///
    /// Returns the .app bundle path (e.g. "/Applications/Microsoft Word.app").
    pub fn resolve_app_path(bundle_id: &str) -> Option<String> {
        let output = std::process::Command::new("mdfind")
            .args([&format!("kMDItemCFBundleIdentifier == '{bundle_id}'")])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .find(|line| line.trim().ends_with(".app"))
            .map(|s| s.trim().to_string())
    }
}

// macOS public API delegates to the macos module
#[cfg(target_os = "macos")]
pub fn register_file_associations() -> carminedesktop_core::Result<()> {
    macos::register()
}

#[cfg(target_os = "macos")]
pub fn unregister_file_associations() -> carminedesktop_core::Result<()> {
    macos::unregister()
}

#[cfg(target_os = "macos")]
pub fn are_file_associations_registered() -> bool {
    macos::is_registered()
}

/// Resolve a macOS bundle ID to its .app path (macOS only).
///
/// Used by `open_file` to find the application to launch.
#[cfg(target_os = "macos")]
pub fn resolve_app_path(bundle_id: &str) -> Option<String> {
    macos::resolve_app_path(bundle_id)
}

// ---------------------------------------------------------------------------
// Windows Explorer navigation pane tests
// ---------------------------------------------------------------------------
//
// These tests live here (instead of crates/carminedesktop-app/tests/) because
// carminedesktop-app is a binary crate — integration tests cannot import its
// private modules. The functions under test (`register_nav_pane`, etc.) are
// module-private, so inline #[cfg(test)] is the only viable option.

#[cfg(test)]
#[cfg(target_os = "windows")]
mod tests {
    use super::*;

    #[test]
    fn test_shell_integration_register_and_unregister_nav_pane() -> carminedesktop_core::Result<()>
    {
        let cloud_root = std::env::temp_dir().join("carminedesktop_test_cloud");
        std::fs::create_dir_all(&cloud_root).ok();

        // Register
        register_nav_pane(&cloud_root)?;
        assert!(is_nav_pane_registered());

        // Verify CLSID key exists with correct default value
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let clsid_path = format!(r"Software\Classes\CLSID\{NAV_PANE_CLSID}");
        let clsid_key = hkcu.open_subkey_with_flags(&clsid_path, KEY_READ)?;
        let default_val: String = clsid_key.get_value("")?;
        assert_eq!(default_val, "Carmine Desktop");

        // Verify IsPinnedToNameSpaceTree
        let pinned: u32 = clsid_key.get_value("System.IsPinnedToNameSpaceTree")?;
        assert_eq!(pinned, 1);

        // Verify TargetFolderPath
        let prop_bag = clsid_key.open_subkey_with_flags(r"Instance\InitPropertyBag", KEY_READ)?;
        let target: String = prop_bag.get_value("TargetFolderPath")?;
        assert_eq!(target, cloud_root.to_string_lossy().as_ref());

        // Verify Desktop\NameSpace entry
        let ns_path = format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\{NAV_PANE_CLSID}"
        );
        let ns_key = hkcu.open_subkey_with_flags(&ns_path, KEY_READ)?;
        let ns_val: String = ns_key.get_value("")?;
        assert_eq!(ns_val, "Carmine Desktop");

        // Verify InProcServer32 is REG_EXPAND_SZ and ThreadingModel is set
        let inproc_key = clsid_key.open_subkey_with_flags("InProcServer32", KEY_READ)?;
        let inproc_val = inproc_key.get_raw_value("")?;
        assert_eq!(inproc_val.vtype, RegType::REG_EXPAND_SZ);
        let threading: String = inproc_key.get_value("ThreadingModel")?;
        assert_eq!(threading, "Both");

        // Verify ShellFolder attributes include SFGAO_HASSUBFOLDER (no SFGAO_ISSLOW)
        let shell_folder = clsid_key.open_subkey_with_flags("ShellFolder", KEY_READ)?;
        let attrs: u32 = shell_folder.get_value("Attributes")?;
        assert_eq!(attrs, 0xF080004D);

        // Verify HideDesktopIcons value
        let hide_path =
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel";
        let hide_key = hkcu.open_subkey_with_flags(hide_path, KEY_READ)?;
        let hide_val: u32 = hide_key.get_value(NAV_PANE_CLSID)?;
        assert_eq!(hide_val, 1);

        // Unregister
        unregister_nav_pane()?;
        assert!(!is_nav_pane_registered());

        // Cleanup
        let _ = std::fs::remove_dir(&cloud_root);

        Ok(())
    }

    #[test]
    fn test_shell_integration_update_nav_pane_target() -> carminedesktop_core::Result<()> {
        let cloud_root = std::env::temp_dir().join("carminedesktop_test_cloud_update");
        std::fs::create_dir_all(&cloud_root).ok();

        register_nav_pane(&cloud_root)?;

        let new_root = std::env::temp_dir().join("carminedesktop_test_cloud_new");
        update_nav_pane_target(&new_root)?;

        // Verify updated path
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let prop_bag = hkcu.open_subkey_with_flags(
            format!(r"Software\Classes\CLSID\{NAV_PANE_CLSID}\Instance\InitPropertyBag"),
            KEY_READ,
        )?;
        let target: String = prop_bag.get_value("TargetFolderPath")?;
        assert_eq!(target, new_root.to_string_lossy().as_ref());

        // Cleanup
        unregister_nav_pane()?;
        let _ = std::fs::remove_dir(&cloud_root);

        Ok(())
    }

    #[test]
    fn test_shell_integration_unregister_nav_pane_missing_keys() -> carminedesktop_core::Result<()>
    {
        // Ensure not registered
        let _ = unregister_nav_pane();
        assert!(!is_nav_pane_registered());

        // Should not error when keys don't exist
        unregister_nav_pane()?;

        Ok(())
    }
}
