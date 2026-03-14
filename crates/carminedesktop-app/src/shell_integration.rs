//! Shell integration — file type associations for Office documents.
//!
//! Registers Carmine Desktop as the handler for Office file types (.docx, .xlsx, .pptx,
//! .doc, .xls, .ppt).
//!
//! - **Windows**: per-user registry keys (HKCU\Software\Classes)
//! - **macOS**: duti + Launch Services
//! - **Linux**: no-op stubs (file associations removed)
//!
//! The previous default handler is saved so that files NOT on a Carmine Desktop
//! drive can be opened with the original handler, avoiding infinite loops.

#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::RegValue;
#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, RegType};

/// Office file extensions we register as handlers for.
#[cfg(target_os = "windows")]
const OFFICE_EXTENSIONS: &[&str] = &[".docx", ".xlsx", ".pptx", ".doc", ".xls", ".ppt"];

/// Registry value name where we store the previous default handler ProgID.
#[cfg(target_os = "windows")]
const PREVIOUS_HANDLER_VALUE: &str = "CarmineDesktop.PreviousHandler";

/// ProgID prefix for our file type handlers.
#[cfg(target_os = "windows")]
const PROGID_PREFIX: &str = "CarmineDesktop.OfficeFile";

/// Register Carmine Desktop as the handler for Office file types.
///
/// For each extension:
/// 1. Reads the current default handler ProgID (if any)
/// 2. Saves it under `CarmineDesktop.PreviousHandler` for fallback
/// 3. Sets our ProgID as the new default
/// 4. Creates the ProgID key with shell\open\command pointing to CarmineDesktop.exe --open "%1"
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

        // Open or create the extension key (e.g. .docx)
        let (ext_key, _) = classes.create_subkey(ext)?;

        // Save the previous handler if one exists and we haven't already saved it.
        // Check both the extension's default ProgID and the UserChoice key
        // (where Windows stores the user's explicit choice, e.g. after "always open with").
        // Only save ProgIDs that have a valid shell\open\command to avoid fallback failures.
        if ext_key
            .get_value::<String, _>(PREVIOUS_HANDLER_VALUE)
            .is_err()
        {
            // Try 1: extension's default ProgID (HKCU\Software\Classes\{ext} default value)
            let from_ext_default = ext_key
                .get_value::<String, _>("")
                .ok()
                .filter(|prev| !prev.is_empty() && !prev.starts_with(PROGID_PREFIX))
                .filter(|prev| get_progid_command(prev).is_some());

            // Try 2: UserChoice ProgId (HKCU\...\FileExts\{ext}\UserChoice\ProgId)
            let from_user_choice = get_user_choice_progid(ext)
                .filter(|prev| !prev.starts_with(PROGID_PREFIX))
                .filter(|prev| get_progid_command(prev).is_some());

            // Prefer UserChoice (the user's explicit selection) over the extension default
            if let Some(prev) = from_user_choice.or(from_ext_default) {
                ext_key.set_value(PREVIOUS_HANDLER_VALUE, &prev)?;
                tracing::debug!("saved previous handler for {ext}: {prev}");
            }
        }

        // Set our ProgID as the default handler
        ext_key.set_value("", &progid)?;

        // Create the ProgID key (e.g. CarmineDesktop.OfficeFile.docx)
        let (progid_key, _) = classes.create_subkey(&progid)?;

        // Set the display name
        let display_name = format!("Office Document (Carmine Desktop){ext}");
        progid_key.set_value("", &display_name)?;

        // Create shell\open\command with the handler command
        let (shell_key, _) = progid_key.create_subkey("shell")?;
        let (open_key, _) = shell_key.create_subkey("open")?;
        let (command_key, _) = open_key.create_subkey("command")?;

        // Command: "C:\path\to\CarmineDesktop.exe" --open "%1"
        let command = format!("\"{exe_str}\" --open \"%1\"");
        command_key.set_value("", &command)?;

        tracing::info!("registered file association for {ext}");
    }

    // Notify the shell that file associations have changed
    notify_shell_change();

    Ok(())
}

/// Unregister Carmine Desktop file associations and restore previous handlers.
///
/// For each extension:
/// 1. Reads the saved previous handler ProgID
/// 2. Restores it as the default (or removes the default if none was saved)
/// 3. Deletes our ProgID key
/// 4. Removes the CarmineDesktop.PreviousHandler value
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

        // Try to restore the previous handler
        if let Ok(ext_key) = classes.open_subkey_with_flags(ext, KEY_READ | KEY_WRITE) {
            // Check if we're currently the handler
            let current: Result<String, _> = ext_key.get_value("");
            if let Ok(ref current_progid) = current
                && current_progid != &progid
            {
                // We're not the handler, skip
                tracing::debug!("skipping {ext}: not currently handled by Carmine Desktop");
                continue;
            }

            // Restore the previous handler
            if let Ok(prev) = ext_key.get_value::<String, _>(PREVIOUS_HANDLER_VALUE) {
                ext_key.set_value("", &prev)?;
                tracing::debug!("restored previous handler for {ext}: {prev}");
            } else {
                // No previous handler — remove the default value
                let _ = ext_key.delete_value("");
            }

            // Remove the saved previous handler value
            let _ = ext_key.delete_value(PREVIOUS_HANDLER_VALUE);
        }

        // Delete our ProgID key tree
        if let Err(e) = classes.delete_subkey_all(&progid) {
            tracing::debug!("failed to delete ProgID {progid}: {e}");
        }

        tracing::info!("unregistered file association for {ext}");
    }

    // Notify the shell that file associations have changed
    notify_shell_change();

    Ok(())
}

/// Get the previous handler ProgID for an extension.
///
/// This is used by `open_file` to invoke the original handler when a file
/// is NOT on a Carmine Desktop drive, avoiding infinite loops.
///
/// Checks (in order):
/// 1. Saved `CarmineDesktop.PreviousHandler` value
/// 2. `UserChoice\ProgId` from FileExts (runtime fallback)
///
/// # Arguments
/// * `ext` - The file extension including the dot (e.g. ".docx")
///
/// # Returns
/// The previous handler ProgID if one was found, or `None`.
#[cfg(target_os = "windows")]
pub fn get_previous_handler(ext: &str) -> Option<String> {
    // Try saved PreviousHandler first
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(classes) = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_READ)
        && let Ok(ext_key) = classes.open_subkey_with_flags(ext, KEY_READ)
        && let Ok(prev) = ext_key.get_value::<String, _>(PREVIOUS_HANDLER_VALUE)
    {
        tracing::debug!("get_previous_handler({ext}): found saved PreviousHandler: {prev}");
        return Some(prev);
    }

    // Fallback: check UserChoice ProgId (may have been set after registration)
    if let Some(progid) = get_user_choice_progid(ext)
        && !progid.starts_with(PROGID_PREFIX)
        && get_progid_command(&progid).is_some()
    {
        tracing::debug!("get_previous_handler({ext}): found UserChoice fallback: {progid}");
        return Some(progid);
    }

    tracing::debug!("get_previous_handler({ext}): no previous handler found");
    None
}

/// Get the command line for a ProgID's shell\open\command.
///
/// Searches both HKCU and HKLM (in that order) since Microsoft Office ProgIDs
/// (e.g., `Excel.Sheet.12`) are typically registered in HKLM, not HKCU.
#[cfg(target_os = "windows")]
pub fn get_progid_command(progid: &str) -> Option<String> {
    let command_path = format!(r"{progid}\shell\open\command");

    // Try HKCU first (user-level overrides)
    if let Some(cmd) = get_progid_command_from_root(HKEY_CURRENT_USER, &command_path) {
        return Some(cmd);
    }

    // Fall back to HKLM (system-level, where Office ProgIDs typically live)
    get_progid_command_from_root(HKEY_LOCAL_MACHINE, &command_path)
}

/// Helper to read a ProgID command from a specific registry root.
#[cfg(target_os = "windows")]
fn get_progid_command_from_root(root: winreg::HKEY, command_path: &str) -> Option<String> {
    let root_key = RegKey::predef(root);
    let classes = root_key
        .open_subkey_with_flags(r"Software\Classes", KEY_READ)
        .ok()?;
    let command_key = classes
        .open_subkey_with_flags(command_path, KEY_READ)
        .ok()?;
    command_key.get_value("").ok()
}

/// Read the user's chosen ProgID from the FileExts UserChoice registry key.
///
/// Windows stores the user's explicit file association choice at:
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}\UserChoice`
/// under the `ProgId` value. This is where "always open with" selections go,
/// and it takes precedence over `HKCU\Software\Classes\{ext}` in Windows Explorer.
#[cfg(target_os = "windows")]
fn get_user_choice_progid(ext: &str) -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path =
        format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}\UserChoice");
    let key = hkcu.open_subkey_with_flags(&path, KEY_READ).ok()?;
    let progid: String = key.get_value("ProgId").ok()?;
    tracing::debug!("found UserChoice ProgId for {ext}: {progid}");
    if progid.is_empty() {
        None
    } else {
        Some(progid)
    }
}

/// Check if Carmine Desktop file associations are currently registered.
///
/// Returns `true` if at least one Office extension has Carmine Desktop as its handler.
#[cfg(target_os = "windows")]
pub fn are_file_associations_registered() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
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
/// Calls `SHChangeNotify` to refresh Explorer's cached associations.
#[cfg(target_os = "windows")]
fn notify_shell_change() {
    use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};

    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

/// Check if an extension is one we handle (Office file types).
///
/// Used to determine whether a fallback to `open::that()` is safe or would
/// cause an infinite loop (since we're registered as the handler).
#[cfg(target_os = "windows")]
pub fn is_handled_extension(ext: &str) -> bool {
    OFFICE_EXTENSIONS
        .iter()
        .any(|&e| e.eq_ignore_ascii_case(ext))
}

/// Well-known Office ProgIDs to try when no saved previous handler exists.
///
/// Ordered by version (newest first) for each extension group.
#[cfg(target_os = "windows")]
const WELL_KNOWN_PROGIDS: &[(&str, &[&str])] = &[
    (".xlsx", &["Excel.Sheet.12", "Excel.Sheet.8"]),
    (".xls", &["Excel.Sheet.12", "Excel.Sheet.8"]),
    (".docx", &["Word.Document.12", "Word.Document.8"]),
    (".doc", &["Word.Document.12", "Word.Document.8"]),
    (".pptx", &["PowerPoint.Show.12", "PowerPoint.Show.8"]),
    (".ppt", &["PowerPoint.Show.12", "PowerPoint.Show.8"]),
];

/// Discover an Office application handler at runtime for the given extension.
///
/// This is the fallback when `get_previous_handler()` returns `None` — e.g.
/// when the user set "always open with Carmine Desktop" via the Windows "Open with"
/// dialog (bypassing `register_file_associations()`), or when registration ran
/// but the handler was already Carmine Desktop.
///
/// Search order:
/// 1. Well-known Office ProgIDs (HKCU then HKLM) with valid `shell\open\command`
/// 2. HKLM system default ProgID for the extension
/// 3. `OpenWithProgids` under `HKCU\Software\Classes\{ext}`
///
/// Returns the first ProgID that has a valid `shell\open\command`.
#[cfg(target_os = "windows")]
pub fn discover_office_handler(ext: &str) -> Option<String> {
    // 1. Well-known Office ProgIDs
    if let Some((_, progids)) = WELL_KNOWN_PROGIDS
        .iter()
        .find(|(e, _)| e.eq_ignore_ascii_case(ext))
    {
        for &progid in *progids {
            if get_progid_command(progid).is_some() {
                tracing::debug!(
                    "discover_office_handler({ext}): found well-known ProgID: {progid}"
                );
                return Some(progid.to_string());
            }
        }
    }

    // 2. HKLM system default — check HKLM\Software\Classes\{ext} default value
    if let Ok(hklm_classes) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"Software\Classes", KEY_READ)
        && let Ok(ext_key) = hklm_classes.open_subkey_with_flags(ext, KEY_READ)
        && let Ok(progid) = ext_key.get_value::<String, _>("")
        && !progid.is_empty()
        && !progid.starts_with(PROGID_PREFIX)
        && get_progid_command(&progid).is_some()
    {
        tracing::debug!("discover_office_handler({ext}): found HKLM system default: {progid}");
        return Some(progid);
    }

    // 3. OpenWithProgids — check HKCU\Software\Classes\{ext}\OpenWithProgids
    if let Ok(hkcu_classes) =
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(r"Software\Classes", KEY_READ)
        && let Ok(ext_key) = hkcu_classes.open_subkey_with_flags(ext, KEY_READ)
        && let Ok(owp_key) = ext_key.open_subkey_with_flags("OpenWithProgids", KEY_READ)
    {
        // Each value name under OpenWithProgids is a ProgID
        for name in owp_key.enum_values().filter_map(|v| v.ok()).map(|(n, _)| n) {
            if !name.is_empty()
                && !name.starts_with(PROGID_PREFIX)
                && get_progid_command(&name).is_some()
            {
                tracing::debug!(
                    "discover_office_handler({ext}): found OpenWithProgids entry: {name}"
                );
                return Some(name);
            }
        }
    }

    tracing::debug!("discover_office_handler({ext}): no handler discovered");
    None
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
    // SFGAO_FILESYSTEM | SFGAO_FOLDER | SFGAO_FILESYSANCESTOR |
    // SFGAO_STORAGEANCESTOR | SFGAO_ISSLOW | SFGAO_HASPROPSHEET |
    // SFGAO_STORAGE | SFGAO_CANLINK | SFGAO_CANCOPY
    // Notably: SFGAO_HASSUBFOLDER (0x80000000) is omitted to prevent Explorer
    // from eagerly enumerating WinFsp mount children (Graph API calls).
    // SFGAO_ISSLOW (0x00004000) signals slow storage so Explorer avoids
    // aggressive prefetching.
    shell_folder_key.set_value("Attributes", &0x7080404Du32)?;

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
        && let Ok(bag) =
            clsid_key.open_subkey_with_flags(r"Instance\InitPropertyBag", KEY_READ)
        && let Ok(existing_target) = bag.get_value::<String, _>("TargetFolderPath")
        && existing_target == target.as_ref()
    {
        tracing::debug!("nav pane already registered with correct target, skipping");
        return Ok(());
    }

    register_nav_pane(cloud_root)
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

#[cfg(target_os = "linux")]
pub fn get_previous_handler(_ext: &str) -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
pub fn discover_office_handler(_ext: &str) -> Option<String> {
    None
}

// ---------------------------------------------------------------------------
// macOS shell integration — Launch Services / duti
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos {
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Office file extensions we register as handlers for.
    pub(super) const OFFICE_EXTENSIONS: &[&str] =
        &[".docx", ".xlsx", ".pptx", ".doc", ".xls", ".ppt"];

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

        for ext in OFFICE_EXTENSIONS {
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

        for ext in OFFICE_EXTENSIONS {
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

        for ext in OFFICE_EXTENSIONS {
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

#[cfg(target_os = "macos")]
pub fn get_previous_handler(ext: &str) -> Option<String> {
    macos::get_previous(ext)
}

/// Resolve a macOS bundle ID to its .app path (macOS only).
///
/// Used by `open_file` to find the application to launch.
#[cfg(target_os = "macos")]
pub fn resolve_app_path(bundle_id: &str) -> Option<String> {
    macos::resolve_app_path(bundle_id)
}

#[cfg(target_os = "macos")]
pub fn is_handled_extension(ext: &str) -> bool {
    macos::OFFICE_EXTENSIONS
        .iter()
        .any(|&e| e.eq_ignore_ascii_case(ext))
}

/// Discover an Office application handler at runtime (macOS).
///
/// Fallback when `get_previous_handler()` returns `None`.
/// Checks well-known bundle IDs and verifies installation via `mdfind`.
#[cfg(target_os = "macos")]
pub fn discover_office_handler(ext: &str) -> Option<String> {
    macos::discover(ext)
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

        // Verify ShellFolder attributes include SFGAO_ISSLOW and exclude SFGAO_HASSUBFOLDER
        let shell_folder = clsid_key.open_subkey_with_flags("ShellFolder", KEY_READ)?;
        let attrs: u32 = shell_folder.get_value("Attributes")?;
        assert_eq!(attrs, 0x7080404D);

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
