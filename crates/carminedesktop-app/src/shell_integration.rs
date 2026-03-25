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

#[cfg(target_os = "windows")]
use base64::Engine as _;

/// Office file extensions we register as handlers for.
pub const OFFICE_EXTENSIONS: &[&str] = &[".docx", ".xlsx", ".pptx", ".doc", ".xls", ".ppt"];

/// Check if an extension is one we handle (Office file types).
///
/// Used to determine whether a fallback to `open::that()` is safe or would
/// cause an infinite loop (since we're registered as the handler).
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn is_handled_extension(ext: &str) -> bool {
    OFFICE_EXTENSIONS
        .iter()
        .any(|&e| e.eq_ignore_ascii_case(ext))
}

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

/// Registry value name where we store the previous default handler ProgID.
#[cfg(target_os = "windows")]
const PREVIOUS_HANDLER_VALUE: &str = "CarmineDesktop.PreviousHandler";

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

/// Static experience string used in Windows UserChoice hash computation.
/// Reverse-engineered from Windows Shell — used by SetUserFTA, Firefox, etc.
#[cfg(target_os = "windows")]
const USER_CHOICE_EXPERIENCE: &str =
    "User Choice set via Windows User Experience {D18B6DD5-6124-4341-9318-804003BAFA0B}";

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

        tracing::info!("registered file association for {ext}");
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

    // Set UserChoice keys with valid hashes so Windows 10/11 uses our ProgID
    // immediately, without requiring the user to visit Settings > Default Apps.
    if let Err(e) = set_all_user_choices() {
        tracing::warn!("UserChoice registration failed (non-fatal): {e}");
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
                // We're not the handler, skip restoring default
                tracing::debug!(
                    "skipping {ext} default restore: not currently handled by Carmine Desktop"
                );
            } else {
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

            // Remove from OpenWithProgids
            if let Ok(owp_key) = ext_key.open_subkey_with_flags("OpenWithProgids", KEY_WRITE) {
                let _ = owp_key.delete_value(&progid);
            }
        }

        // Delete our UserChoice key so Windows falls back to the restored handler
        if let Err(e) = delete_user_choice_key(ext) {
            tracing::debug!("failed to delete UserChoice for {ext}: {e}");
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
pub(crate) fn get_user_choice_progid(ext: &str) -> Option<String> {
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
// Windows UserChoice hash computation
// ---------------------------------------------------------------------------
//
// Implements the reverse-engineered UserChoice hash algorithm used by
// Windows 10/11 to validate file association entries in the registry.
// Based on the well-documented algorithm from SetUserFTA, Firefox, and
// other open-source projects.

/// Read a little-endian u32 from a byte slice at the given offset.
#[cfg(target_os = "windows")]
fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

/// First round of the CS64 hash (WordSwap variant).
///
/// Part of the Windows UserChoice hash algorithm. Processes the input
/// data as u32 words with two magic constants derived from the MD5 hash.
#[cfg(target_os = "windows")]
fn cs64_word_swap(data: &[u8], size: usize, md5: &[u8; 16]) -> [u32; 2] {
    // size = number of u32 words to process (already adjusted: even count)
    if size < 2 || (size & 1) != 0 {
        return [0, 0];
    }

    let c0 = (read_u32_le(md5, 0) | 1).wrapping_add(0x69FB0000);
    let c1 = (read_u32_le(md5, 4) | 1).wrapping_add(0x13DB0000);

    let mut o1: u32 = 0;
    let mut o2: u32 = 0;
    let mut ta: usize = 0;
    let mut ts = size;
    let ti = ((size - 2) >> 1) + 1;

    for _ in 0..ti {
        let n = read_u32_le(data, ta * 4).wrapping_add(o1);
        ta += 2;
        ts -= 2;

        let v1_inner = n
            .wrapping_mul(c0)
            .wrapping_sub(0x10FA9605u32.wrapping_mul(n >> 16));
        let v1 = 0x79F8A395u32
            .wrapping_mul(v1_inner)
            .wrapping_add(0x689B6B9Fu32.wrapping_mul(v1_inner >> 16));
        let v2 = 0xEA970001u32
            .wrapping_mul(v1)
            .wrapping_sub(0x3C101569u32.wrapping_mul(v1 >> 16));

        let v3 = read_u32_le(data, (ta - 1) * 4).wrapping_add(v2);
        let v4 = v3
            .wrapping_mul(c1)
            .wrapping_sub(0x3CE8EC25u32.wrapping_mul(v3 >> 16));
        let v5 = 0x59C3AF2Du32
            .wrapping_mul(v4)
            .wrapping_sub(0x2232E0F1u32.wrapping_mul(v4 >> 16));

        o1 = 0x1EC90001u32
            .wrapping_mul(v5)
            .wrapping_add(0x35BD1EC9u32.wrapping_mul(v5 >> 16));
        o2 = o2.wrapping_add(o1.wrapping_add(v2));
    }

    if ts == 1 {
        let n = read_u32_le(data, ta * 4).wrapping_add(o1);
        let v1 = n
            .wrapping_mul(c0)
            .wrapping_sub(0x10FA9605u32.wrapping_mul(n >> 16));
        let v1_processed = 0x79F8A395u32
            .wrapping_mul(v1)
            .wrapping_add(0x689B6B9Fu32.wrapping_mul(v1 >> 16));
        let v2 = 0xEA970001u32
            .wrapping_mul(v1_processed)
            .wrapping_sub(0x3C101569u32.wrapping_mul(v1_processed >> 16));

        let v3 = v2
            .wrapping_mul(c1)
            .wrapping_sub(0x3CE8EC25u32.wrapping_mul(v2 >> 16));
        let v5 = 0x59C3AF2Du32
            .wrapping_mul(v3)
            .wrapping_sub(0x2232E0F1u32.wrapping_mul(v3 >> 16));
        o1 = 0x1EC90001u32
            .wrapping_mul(v5)
            .wrapping_add(0x35BD1EC9u32.wrapping_mul(v5 >> 16));
        o2 = o2.wrapping_add(o1.wrapping_add(v2));
    }

    [o1, o2]
}

/// Second round of the CS64 hash (Reversible variant).
///
/// Part of the Windows UserChoice hash algorithm. Same iteration structure
/// as [`cs64_word_swap`] but with different magic constants.
#[cfg(target_os = "windows")]
fn cs64_reversible(data: &[u8], size: usize, md5: &[u8; 16]) -> [u32; 2] {
    if size < 2 || (size & 1) != 0 {
        return [0, 0];
    }

    let c0 = read_u32_le(md5, 0) | 1;
    let c1 = read_u32_le(md5, 4) | 1;

    let mut o1: u32 = 0;
    let mut o2: u32 = 0;
    let mut ta: usize = 0;
    let mut ts = size;
    let ti = ((size - 2) >> 1) + 1;

    for _ in 0..ti {
        let n = read_u32_le(data, ta * 4).wrapping_add(o1).wrapping_mul(c0);
        let n = 0xB1110000u32
            .wrapping_mul(n)
            .wrapping_sub(0x30674EEFu32.wrapping_mul(n >> 16));
        ta += 2;
        ts -= 2;

        let v1 = 0x5B9F0000u32
            .wrapping_mul(n)
            .wrapping_sub(0x78F7A461u32.wrapping_mul(n >> 16));

        let v1_inner = 0x12CEB96Du32
            .wrapping_mul(v1 >> 16)
            .wrapping_sub(0x46930000u32.wrapping_mul(v1));
        let v2 = 0x1D830000u32
            .wrapping_mul(v1_inner)
            .wrapping_add(0x257E1D83u32.wrapping_mul(v1_inner >> 16));

        let v3 = read_u32_le(data, (ta - 1) * 4).wrapping_add(v2);
        let v4 = 0x16F50000u32
            .wrapping_mul(c1.wrapping_mul(v3))
            .wrapping_sub(0x5D8BE90Bu32.wrapping_mul(c1.wrapping_mul(v3) >> 16));

        let v5_inner = 0x96FF0000u32
            .wrapping_mul(v4)
            .wrapping_sub(0x2C7C6901u32.wrapping_mul(v4 >> 16));
        let v5 = 0x2B890000u32
            .wrapping_mul(v5_inner)
            .wrapping_add(0x7C932B89u32.wrapping_mul(v5_inner >> 16));

        o1 = 0x9F690000u32
            .wrapping_mul(v5)
            .wrapping_sub(0x405B6097u32.wrapping_mul(v5 >> 16));
        o2 = o2.wrapping_add(o1.wrapping_add(v2));
    }

    if ts == 1 {
        let n = read_u32_le(data, ta * 4).wrapping_add(o1);
        let v1 = 0xB1110000u32
            .wrapping_mul(c0.wrapping_mul(n))
            .wrapping_sub(0x30674EEFu32.wrapping_mul(c0.wrapping_mul(n) >> 16));
        let v2 = 0x5B9F0000u32
            .wrapping_mul(v1)
            .wrapping_sub(0x78F7A461u32.wrapping_mul(v1 >> 16));

        let v3_inner = 0x12CEB96Du32
            .wrapping_mul(v2 >> 16)
            .wrapping_sub(0x46930000u32.wrapping_mul(v2));
        let v3 = 0x1D830000u32
            .wrapping_mul(v3_inner)
            .wrapping_add(0x257E1D83u32.wrapping_mul(v3_inner >> 16));

        let v4 = 0x16F50000u32
            .wrapping_mul(c1.wrapping_mul(v3))
            .wrapping_sub(0x5D8BE90Bu32.wrapping_mul(c1.wrapping_mul(v3) >> 16));
        let v5 = 0x96FF0000u32
            .wrapping_mul(v4)
            .wrapping_sub(0x2C7C6901u32.wrapping_mul(v4 >> 16));
        let v5_processed = 0x2B890000u32
            .wrapping_mul(v5)
            .wrapping_add(0x7C932B89u32.wrapping_mul(v5 >> 16));

        o1 = 0x9F690000u32
            .wrapping_mul(v5_processed)
            .wrapping_sub(0x405B6097u32.wrapping_mul(v5_processed >> 16));
        o2 = o2.wrapping_add(o1.wrapping_add(v3));
    }

    [o1, o2]
}

/// Compute the Windows UserChoice hash for a file extension.
///
/// The hash is based on the reverse-engineered algorithm used by Windows 10/11
/// to validate UserChoice registry entries. It takes the file extension, user SID,
/// ProgID, and registry key timestamp as inputs, and produces a Base64-encoded hash.
///
/// Used by SetUserFTA, Firefox, and other applications to programmatically set
/// file type associations on Windows 10/11.
#[cfg(target_os = "windows")]
fn compute_user_choice_hash(ext: &str, sid: &str, progid: &str, timestamp: &str) -> String {
    use md5::{Digest, Md5};

    // 1. Build input string (all lowercase)
    let input = format!("{ext}{sid}{progid}{timestamp}{USER_CHOICE_EXPERIENCE}").to_lowercase();

    // 2. Convert to UTF-16LE with null terminator
    let utf16: Vec<u16> = input.encode_utf16().chain(std::iter::once(0)).collect();
    let utf16_bytes: Vec<u8> = utf16.iter().flat_map(|&w| w.to_le_bytes()).collect();

    // 3. Compute MD5
    let md5_result = Md5::digest(&utf16_bytes);
    let md5_bytes: [u8; 16] = md5_result.into();

    // 4. Compute shifted size (number of u32 words, made even)
    let mut shifted_size = utf16_bytes.len() / 4;
    if (shifted_size & 1) != 0 {
        shifted_size -= 1;
    }

    // 5. Two-round hash
    let [a1, a2] = cs64_word_swap(&utf16_bytes, shifted_size, &md5_bytes);
    let [b1, b2] = cs64_reversible(&utf16_bytes, shifted_size, &md5_bytes);

    // 6. XOR results
    let mut result = [0u8; 8];
    result[..4].copy_from_slice(&(a1 ^ b1).to_le_bytes());
    result[4..].copy_from_slice(&(a2 ^ b2).to_le_bytes());

    // 7. Base64 encode
    base64::engine::general_purpose::STANDARD.encode(result)
}

/// Get the current user's SID as a string (e.g., `S-1-5-21-...`).
///
/// Uses Win32 `GetTokenInformation` + `ConvertSidToStringSidW`.
#[cfg(target_os = "windows")]
fn get_current_user_sid() -> Option<String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = windows::Win32::Foundation::HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            tracing::warn!("get_current_user_sid: OpenProcessToken failed");
            return None;
        }

        // First call to get buffer size
        let mut size = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);

        let mut buffer = vec![0u8; size as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            size,
            &mut size,
        )
        .is_err()
        {
            let _ = CloseHandle(token);
            tracing::warn!("get_current_user_sid: GetTokenInformation failed");
            return None;
        }

        let token_user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let mut sid_string = windows::core::PWSTR::null();
        let result = ConvertSidToStringSidW(token_user.User.Sid, &mut sid_string);
        let _ = CloseHandle(token);

        if result.is_err() {
            tracing::warn!("get_current_user_sid: ConvertSidToStringSidW failed");
            return None;
        }

        let sid = sid_string.to_string().ok()?;
        // Free the SID string allocated by ConvertSidToStringSidW
        windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            sid_string.as_ptr().cast(),
        )));
        Some(sid)
    }
}

/// Get the `LastWriteTime` of a registry key as a raw FILETIME value (u64).
///
/// Uses `winreg::RegKey::query_info()` to read the key metadata.
/// The `RegKeyMetadata.last_write_time` field is a `windows_sys::Win32::Foundation::FILETIME`
/// with `dwLowDateTime` and `dwHighDateTime` fields.
#[cfg(target_os = "windows")]
fn get_registry_key_write_time(key: &RegKey) -> Option<u64> {
    let info = key.query_info().ok()?;
    let ft = &info.last_write_time;
    Some((ft.dwHighDateTime as u64) << 32 | ft.dwLowDateTime as u64)
}

/// Truncate a FILETIME value to the nearest minute and format as hex.
///
/// FILETIME is in 100-nanosecond intervals. One minute = 600,000,000 intervals.
/// The timestamp is formatted as `{high:08x}{low:08x}` matching the format
/// used by Windows for UserChoice hash validation.
#[cfg(target_os = "windows")]
fn format_filetime_truncated(filetime: u64) -> String {
    let truncated = filetime / 600_000_000 * 600_000_000;
    let low = truncated as u32;
    let high = (truncated >> 32) as u32;
    format!("{high:08x}{low:08x}")
}

/// Delete the existing UserChoice registry key for an extension.
///
/// The key at `HKCU\...\FileExts\{ext}\UserChoice` is ACL-protected by
/// Windows to prevent applications from tampering with user choices.
/// This function handles the protection by taking ownership and adjusting
/// the DACL before deleting.
#[cfg(target_os = "windows")]
fn delete_user_choice_key(ext: &str) -> carminedesktop_core::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let parent_path = format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}");

    let parent = match hkcu.open_subkey_with_flags(&parent_path, KEY_READ | KEY_WRITE) {
        Ok(k) => k,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    // Check if UserChoice exists
    if parent
        .open_subkey_with_flags("UserChoice", KEY_READ)
        .is_err()
    {
        return Ok(()); // doesn't exist
    }

    // Try direct delete first
    match parent.delete_subkey_all("UserChoice") {
        Ok(()) => {
            tracing::debug!("deleted UserChoice key for {ext} (direct)");
            return Ok(());
        }
        Err(e) => {
            tracing::debug!(
                "direct delete of UserChoice for {ext} failed: {e}, trying with ACL override"
            );
        }
    }

    // ACL override: take ownership and set a permissive DACL, then retry delete.
    // The UserChoice key is ACL-protected on Windows 10/11 but the current user
    // can take ownership since it's under HKCU.
    acl_override_delete_user_choice(ext, &parent)
}

/// Take ownership of the UserChoice key, set a permissive DACL, and delete it.
#[cfg(target_os = "windows")]
fn acl_override_delete_user_choice(ext: &str, parent: &RegKey) -> carminedesktop_core::Result<()> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::Authorization::{SE_OBJECT_TYPE, SetSecurityInfo};
    use windows::Win32::Security::{DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION};

    // WRITE_DAC | WRITE_OWNER access flags
    const WRITE_DAC: u32 = 0x0004_0000;
    const WRITE_OWNER: u32 = 0x0008_0000;

    // SE_REGISTRY_KEY = 4
    let se_registry_key = SE_OBJECT_TYPE(4);

    // Open UserChoice with WRITE_DAC | WRITE_OWNER
    let uc_key = match parent.open_subkey_with_flags("UserChoice", WRITE_DAC | WRITE_OWNER) {
        Ok(k) => k,
        Err(e) => {
            tracing::warn!("cannot open UserChoice for {ext} with WRITE_DAC|WRITE_OWNER: {e}");
            return Err(e.into());
        }
    };

    // Get current user SID for ownership
    let Some(sid_string) = get_current_user_sid() else {
        return Err(carminedesktop_core::Error::Config(
            "failed to get current user SID for ACL override".into(),
        ));
    };

    // Get the SID as a PSID from the token (re-derive it)
    unsafe {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
        use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

        let mut token = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).is_err() {
            tracing::warn!("ACL override: OpenProcessToken failed");
            return Err(carminedesktop_core::Error::Config(
                "OpenProcessToken failed during ACL override".into(),
            ));
        }

        let mut size = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);
        let mut buffer = vec![0u8; size as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            size,
            &mut size,
        )
        .is_err()
        {
            let _ = CloseHandle(token);
            return Err(carminedesktop_core::Error::Config(
                "GetTokenInformation failed during ACL override".into(),
            ));
        }

        let token_user = &*(buffer.as_ptr() as *const TOKEN_USER);
        let user_sid = token_user.User.Sid;

        // Convert the raw HKEY to a HANDLE for SetSecurityInfo
        let raw_hkey = uc_key.raw_handle();
        let handle = HANDLE(raw_hkey as *mut std::ffi::c_void);

        // Take ownership
        let result = SetSecurityInfo(
            handle,
            se_registry_key,
            OWNER_SECURITY_INFORMATION,
            Some(user_sid),
            None,
            None,
            None,
        );
        if result.is_err() {
            let _ = CloseHandle(token);
            tracing::warn!("SetSecurityInfo (owner) failed for UserChoice {ext}: {result:?}");
            return Err(carminedesktop_core::Error::Config(format!(
                "SetSecurityInfo (owner) failed: {result:?}"
            )));
        }

        // Set a NULL DACL (grants full access to everyone) so we can delete
        let result = SetSecurityInfo(
            handle,
            se_registry_key,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            None, // NULL DACL = full access
            None,
        );
        if result.is_err() {
            let _ = CloseHandle(token);
            tracing::warn!("SetSecurityInfo (DACL) failed for UserChoice {ext}: {result:?}");
            return Err(carminedesktop_core::Error::Config(format!(
                "SetSecurityInfo (DACL) failed: {result:?}"
            )));
        }

        let _ = CloseHandle(token);
        tracing::debug!("ACL override: took ownership of UserChoice for {ext} (SID: {sid_string})");
    }

    // Drop the key handle before deleting
    drop(uc_key);

    // Retry delete
    match parent.delete_subkey_all("UserChoice") {
        Ok(()) => {
            tracing::debug!("deleted UserChoice key for {ext} (after ACL override)");
            Ok(())
        }
        Err(e) => {
            tracing::warn!("delete UserChoice for {ext} failed even after ACL override: {e}");
            Err(e.into())
        }
    }
}

/// Set the Windows UserChoice registry key for a file extension.
///
/// This is the full flow: delete existing → create new → write ProgId →
/// compute hash → write Hash. Makes Carmine Desktop the default handler
/// for the extension without requiring user interaction.
#[cfg(target_os = "windows")]
fn set_user_choice_for_extension(ext: &str, progid: &str) -> carminedesktop_core::Result<()> {
    // Check if UserChoice already points to our ProgID — skip if so
    if get_user_choice_progid(ext).is_some_and(|p| p == progid) {
        tracing::debug!("UserChoice for {ext} already set to {progid}, skipping");
        return Ok(());
    }

    let Some(sid) = get_current_user_sid() else {
        return Err(carminedesktop_core::Error::Config(
            "failed to get current user SID for UserChoice".into(),
        ));
    };

    // Delete existing UserChoice key (handle ACL-protected keys gracefully)
    if let Err(e) = delete_user_choice_key(ext) {
        tracing::warn!("failed to delete existing UserChoice for {ext}: {e}");
        // Continue anyway — creating a new key may still work
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let parent_path = format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{ext}");

    // Open or create the FileExts parent key
    let (parent, _) = hkcu.create_subkey(&parent_path)?;

    // Create UserChoice subkey
    let (uc_key, _) = parent.create_subkey("UserChoice")?;

    // Write ProgId value
    uc_key.set_value("ProgId", &progid)?;

    // Query the key's LastWriteTime (must be done right after writing ProgId)
    let Some(write_time) = get_registry_key_write_time(&uc_key) else {
        return Err(carminedesktop_core::Error::Config(
            "failed to query UserChoice key write time".into(),
        ));
    };

    // Truncate and format the timestamp
    let timestamp = format_filetime_truncated(write_time);

    // Compute hash
    let hash = compute_user_choice_hash(ext, &sid, progid, &timestamp);

    // Write Hash value
    uc_key.set_value("Hash", &hash)?;

    tracing::info!("set UserChoice for {ext}: progid={progid}, timestamp={timestamp}, hash={hash}");

    Ok(())
}

/// Set UserChoice registry keys for all Office extensions.
///
/// Calls [`set_user_choice_for_extension`] for each extension.
/// Errors are non-fatal — logged and continued.
#[cfg(target_os = "windows")]
pub fn set_all_user_choices() -> carminedesktop_core::Result<()> {
    let mut last_error = None;
    for ext in OFFICE_EXTENSIONS {
        let progid = format!("{PROGID_PREFIX}{ext}");
        if let Err(e) = set_user_choice_for_extension(ext, &progid) {
            tracing::warn!("failed to set UserChoice for {ext}: {e}");
            last_error = Some(e);
        }
    }
    // Return Ok even if some failed — the fallback notification is still available
    if let Some(e) = last_error {
        tracing::warn!("some UserChoice registrations failed, last error: {e}");
    }
    Ok(())
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

#[cfg(target_os = "linux")]
pub fn get_previous_handler(_ext: &str) -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
pub fn discover_office_handler(_ext: &str) -> Option<String> {
    None
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

    // -----------------------------------------------------------------------
    // UserChoice hash computation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_compute_user_choice_hash_known_vector() {
        // Known test vector from SetUserFTA documentation
        let hash = compute_user_choice_hash(
            ".3g2",
            "S-1-5-21-819709642-920330688-1657285119-500",
            "WMP11.AssocFile.3G2",
            "01d4d98267246000",
        );
        assert_eq!(hash, "PCCqEmkvW2Y=");
    }

    #[test]
    fn test_format_filetime_truncated() {
        // 0x01d4d98267246000 = 132243528780000000 decimal
        // Truncated to minute: 132243528780000000 / 600_000_000 = 220405881 (truncated)
        // 220405881 * 600_000_000 = 132243528600000000 = 0x01d4d98200000000
        let ft: u64 = 0x01d4d98267246000;
        let formatted = format_filetime_truncated(ft);
        assert_eq!(formatted, "01d4d98200000000");
    }

    #[test]
    fn test_cs64_word_swap_deterministic() {
        let data = b"test data here!!"; // 16 bytes = 4 u32 words
        let md5 = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let result1 = cs64_word_swap(data, 4, &md5);
        let result2 = cs64_word_swap(data, 4, &md5);
        assert_eq!(result1, result2);
    }

    #[test]
    fn test_cs64_reversible_deterministic() {
        let data = b"test data here!!"; // 16 bytes = 4 u32 words
        let md5 = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let result1 = cs64_reversible(data, 4, &md5);
        let result2 = cs64_reversible(data, 4, &md5);
        assert_eq!(result1, result2);
    }

    // -----------------------------------------------------------------------
    // UserChoice key lifecycle integration test
    // -----------------------------------------------------------------------

    #[test]
    fn test_user_choice_set_and_delete() -> carminedesktop_core::Result<()> {
        let test_ext = ".carminetest";
        let test_progid = "CarmineDesktop.Test";

        // Set UserChoice
        set_user_choice_for_extension(test_ext, test_progid)?;

        // Verify ProgId was written
        let progid = get_user_choice_progid(test_ext);
        assert_eq!(progid.as_deref(), Some(test_progid));

        // Verify Hash was written (non-empty)
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let uc_path = format!(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\{test_ext}\UserChoice"
        );
        let uc_key = hkcu.open_subkey_with_flags(&uc_path, KEY_READ)?;
        let hash: String = uc_key.get_value("Hash")?;
        assert!(!hash.is_empty(), "Hash should be non-empty");

        // Delete
        delete_user_choice_key(test_ext)?;

        // Verify deleted
        assert!(
            hkcu.open_subkey_with_flags(&uc_path, KEY_READ).is_err(),
            "UserChoice key should be deleted"
        );

        // Cleanup: delete the parent FileExts key for our test extension
        if let Ok(parent) = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts",
            KEY_READ | KEY_WRITE,
        ) {
            let _ = parent.delete_subkey_all(test_ext);
        }

        Ok(())
    }
}
