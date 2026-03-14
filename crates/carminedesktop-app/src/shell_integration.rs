//! Shell integration — file type associations for Office documents.
//!
//! Registers Carmine Desktop as the handler for Office file types (.docx, .xlsx, .pptx,
//! .doc, .xls, .ppt).
//!
//! - **Windows**: per-user registry keys (HKCU\Software\Classes)
//! - **Linux**: xdg-mime and .desktop files (~/.local/share/applications/)
//! - **macOS**: no-op stubs (planned for future)
//!
//! The previous default handler is saved so that files NOT on a Carmine Desktop
//! drive can be opened with the original handler, avoiding infinite loops.

#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};

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
            if let Ok(ref current_progid) = current {
                if current_progid != &progid {
                    // We're not the handler, skip
                    tracing::debug!("skipping {ext}: not currently handled by Carmine Desktop");
                    continue;
                }
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
    if let Some(progid) = get_user_choice_progid(ext) {
        if !progid.starts_with(PROGID_PREFIX) && get_progid_command(&progid).is_some() {
            tracing::debug!("get_previous_handler({ext}): found UserChoice fallback: {progid}");
            return Some(progid);
        }
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
        if let Ok(progid) = ext_key.get_value::<String, _>("") {
            if progid.starts_with(PROGID_PREFIX) {
                return true;
            }
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
// Linux shell integration — xdg-mime and .desktop files
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Office file extensions we register as handlers for.
    pub(super) const OFFICE_EXTENSIONS: &[&str] =
        &[".docx", ".xlsx", ".pptx", ".doc", ".xls", ".ppt"];

    /// Mapping from file extensions to MIME types.
    const MIME_MAP: &[(&str, &str)] = &[
        (
            ".docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        (
            ".xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
        (
            ".pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ),
        (".doc", "application/msword"),
        (".xls", "application/vnd.ms-excel"),
        (".ppt", "application/vnd.ms-powerpoint"),
    ];

    /// Our .desktop file name (installed to ~/.local/share/applications/).
    const DESKTOP_FILE_NAME: &str = "carminedesktop-open.desktop";

    /// JSON file where we store previous handlers for restoration.
    const PREVIOUS_HANDLERS_FILE: &str = "previous_handlers.json";

    /// Get the MIME type for an extension.
    pub(super) fn mime_for_ext(ext: &str) -> Option<&'static str> {
        MIME_MAP
            .iter()
            .find(|(e, _)| e.eq_ignore_ascii_case(ext))
            .map(|(_, m)| *m)
    }

    /// Get all MIME types we handle.
    fn all_mime_types() -> Vec<&'static str> {
        MIME_MAP.iter().map(|(_, m)| *m).collect()
    }

    /// Path to our .desktop file: ~/.local/share/applications/carminedesktop-open.desktop
    fn desktop_file_path() -> carminedesktop_core::Result<PathBuf> {
        dirs::data_dir()
            .map(|d| d.join("applications").join(DESKTOP_FILE_NAME))
            .ok_or_else(|| {
                carminedesktop_core::Error::Config(
                    "no data directory available (dirs::data_dir returned None)".into(),
                )
            })
    }

    /// Path to our previous handlers JSON: ~/.config/carminedesktop/previous_handlers.json
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

    /// Query the current default handler for a MIME type via `xdg-mime query default`.
    fn xdg_query_default(mime_type: &str) -> Option<String> {
        let output = std::process::Command::new("xdg-mime")
            .args(["query", "default", mime_type])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let handler = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if handler.is_empty() {
            None
        } else {
            Some(handler)
        }
    }

    /// Set the default handler for a MIME type via `xdg-mime default`.
    fn xdg_set_default(desktop_file: &str, mime_type: &str) -> carminedesktop_core::Result<()> {
        let status = std::process::Command::new("xdg-mime")
            .args(["default", desktop_file, mime_type])
            .status()
            .map_err(|e| {
                carminedesktop_core::Error::Config(format!("failed to run xdg-mime default: {e}"))
            })?;
        if !status.success() {
            return Err(carminedesktop_core::Error::Config(format!(
                "xdg-mime default {desktop_file} {mime_type} failed with {status}"
            )));
        }
        Ok(())
    }

    /// Generate the .desktop file content.
    fn desktop_file_content(exe_path: &str) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Carmine Desktop File Handler\n\
             Comment=Opens Office files through Carmine Desktop for online collaboration\n\
             Exec=\"{exe_path}\" --open %f\n\
             NoDisplay=true\n\
             Terminal=false\n\
             MimeType={mimes};\n",
            mimes = all_mime_types().join(";")
        )
    }

    /// Register Carmine Desktop as the handler for Office file types on Linux.
    ///
    /// 1. Creates a .desktop file at ~/.local/share/applications/
    /// 2. Saves current default handlers to a JSON file
    /// 3. Registers as default via xdg-mime
    pub fn register() -> carminedesktop_core::Result<()> {
        let exe_path = std::env::current_exe().map_err(|e| {
            carminedesktop_core::Error::Config(format!("failed to get current exe path: {e}"))
        })?;
        let exe_str = exe_path.to_string_lossy();

        // Create the .desktop file
        let desktop_path = desktop_file_path()?;
        if let Some(parent) = desktop_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                carminedesktop_core::Error::Config(format!(
                    "failed to create applications dir {}: {e}",
                    parent.display()
                ))
            })?;
        }
        std::fs::write(&desktop_path, desktop_file_content(&exe_str)).map_err(|e| {
            carminedesktop_core::Error::Config(format!(
                "failed to write .desktop file to {}: {e}",
                desktop_path.display()
            ))
        })?;
        tracing::debug!("wrote .desktop file to {}", desktop_path.display());

        // Save current handlers and register ours
        let mut previous = load_previous_handlers();
        for (ext, mime) in MIME_MAP {
            // Only save the previous handler if we haven't already saved one
            // and the current handler is not us
            if !previous.contains_key(*mime)
                && let Some(current) = xdg_query_default(mime)
                && current != DESKTOP_FILE_NAME
            {
                previous.insert((*mime).to_string(), current.clone());
                tracing::debug!("saved previous handler for {ext} ({mime}): {current}");
            }

            if let Err(e) = xdg_set_default(DESKTOP_FILE_NAME, mime) {
                tracing::warn!("failed to set default handler for {ext} ({mime}): {e}");
            } else {
                tracing::info!("registered file association for {ext} ({mime})");
            }
        }
        save_previous_handlers(&previous)?;

        // Update desktop database if available
        let _ = std::process::Command::new("update-desktop-database")
            .arg(
                desktop_path
                    .parent()
                    .unwrap_or(&PathBuf::from("~/.local/share/applications")),
            )
            .status();

        Ok(())
    }

    /// Unregister Carmine Desktop file associations and restore previous handlers.
    pub fn unregister() -> carminedesktop_core::Result<()> {
        let previous = load_previous_handlers();

        for (_ext, mime) in MIME_MAP {
            // Only restore if we're still the current handler
            if let Some(current) = xdg_query_default(mime)
                && current == DESKTOP_FILE_NAME
                && let Some(prev) = previous.get(*mime)
            {
                if let Err(e) = xdg_set_default(prev, mime) {
                    tracing::warn!("failed to restore handler for {mime}: {e}");
                } else {
                    tracing::debug!("restored previous handler for {mime}: {prev}");
                }
            }
            // If no previous handler was saved, just leave it — xdg-mime
            // doesn't have a "remove default" command.
        }

        // Remove the .desktop file
        if let Ok(desktop_path) = desktop_file_path()
            && desktop_path.exists()
        {
            if let Err(e) = std::fs::remove_file(&desktop_path) {
                tracing::warn!(
                    "failed to remove .desktop file at {}: {e}",
                    desktop_path.display()
                );
            } else {
                tracing::debug!("removed .desktop file at {}", desktop_path.display());
            }
        }

        // Clean up previous handlers file
        if let Ok(handlers_path) = previous_handlers_path() {
            let _ = std::fs::remove_file(handlers_path);
        }

        // Update desktop database if available
        if let Ok(desktop_path) = desktop_file_path() {
            let _ = std::process::Command::new("update-desktop-database")
                .arg(
                    desktop_path
                        .parent()
                        .unwrap_or(&PathBuf::from("~/.local/share/applications")),
                )
                .status();
        }

        Ok(())
    }

    /// Check if Carmine Desktop file associations are currently registered.
    ///
    /// Returns `true` if the .desktop file exists AND Carmine Desktop is the
    /// default handler for at least one Office MIME type.
    pub fn is_registered() -> bool {
        let Ok(desktop_path) = desktop_file_path() else {
            return false;
        };
        if !desktop_path.exists() {
            return false;
        }

        // Check if we're the default for at least one MIME type
        for (_ext, mime) in MIME_MAP {
            if let Some(current) = xdg_query_default(mime)
                && current == DESKTOP_FILE_NAME
            {
                return true;
            }
        }

        false
    }

    /// Get the previous handler's .desktop file name for an extension's MIME type.
    ///
    /// Returns the saved .desktop file name (e.g. "libreoffice-writer.desktop")
    /// if one was stored during registration.
    pub fn get_previous(ext: &str) -> Option<String> {
        let mime = mime_for_ext(ext)?;
        let handlers = load_previous_handlers();
        handlers.get(mime).cloned()
    }

    /// Well-known .desktop file names for Office suites.
    const WELL_KNOWN_SPREADSHEET_DESKTOPS: &[&str] = &[
        "libreoffice-calc.desktop",
        "org.onlyoffice.desktopeditors.desktop",
        "wps-office-et.desktop",
    ];

    const WELL_KNOWN_WORD_DESKTOPS: &[&str] = &[
        "libreoffice-writer.desktop",
        "org.onlyoffice.desktopeditors.desktop",
        "wps-office-wps.desktop",
    ];

    const WELL_KNOWN_PRESENTATION_DESKTOPS: &[&str] = &[
        "libreoffice-impress.desktop",
        "org.onlyoffice.desktopeditors.desktop",
        "wps-office-wpp.desktop",
    ];

    /// Map an extension to its well-known .desktop file names.
    fn well_known_desktops_for_ext(ext: &str) -> &'static [&'static str] {
        match ext.to_ascii_lowercase().as_str() {
            ".xlsx" | ".xls" => WELL_KNOWN_SPREADSHEET_DESKTOPS,
            ".docx" | ".doc" => WELL_KNOWN_WORD_DESKTOPS,
            ".pptx" | ".ppt" => WELL_KNOWN_PRESENTATION_DESKTOPS,
            _ => &[],
        }
    }

    /// Parse a `mimeinfo.cache` file and return .desktop names for a MIME type.
    ///
    /// The format is: `mime/type=app1.desktop;app2.desktop;`
    fn parse_mimeinfo_cache(path: &std::path::Path, mime_type: &str) -> Vec<String> {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Vec::new();
        };
        let prefix = format!("{mime_type}=");
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix(&prefix) {
                return rest
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect();
            }
        }
        Vec::new()
    }

    /// Discover an Office application handler at runtime for the given extension.
    ///
    /// Fallback when `get_previous()` returns `None`. Searches:
    /// 1. `mimeinfo.cache` files (user-local then system) for the MIME type,
    ///    skipping our own `carminedesktop-open.desktop`
    /// 2. Well-known .desktop file names for common Office suites
    ///
    /// Returns the first .desktop file name that has a valid `Exec=` line.
    pub fn discover(ext: &str) -> Option<String> {
        let mime_type = mime_for_ext(ext)?;

        // 1. mimeinfo.cache lookup
        let cache_paths: Vec<PathBuf> = {
            let mut paths = Vec::new();
            // User-local first (higher priority)
            if let Some(data_dir) = dirs::data_dir() {
                paths.push(data_dir.join("applications/mimeinfo.cache"));
            }
            paths.push(PathBuf::from("/usr/share/applications/mimeinfo.cache"));
            paths
        };

        for cache_path in &cache_paths {
            let desktops = parse_mimeinfo_cache(cache_path, mime_type);
            for desktop in desktops {
                // Skip our own handler
                if desktop == DESKTOP_FILE_NAME {
                    continue;
                }
                if get_desktop_exec(&desktop).is_some() {
                    tracing::debug!(
                        "discover_office_handler({ext}): found in mimeinfo.cache: {desktop}"
                    );
                    return Some(desktop);
                }
            }
        }

        // 2. Well-known .desktop files
        for &desktop in well_known_desktops_for_ext(ext) {
            if get_desktop_exec(desktop).is_some() {
                tracing::debug!(
                    "discover_office_handler({ext}): found well-known desktop: {desktop}"
                );
                return Some(desktop.to_string());
            }
        }

        tracing::debug!("discover_office_handler({ext}): no handler discovered");
        None
    }

    /// Parse the Exec= line from a .desktop file to get the command template.
    ///
    /// Searches standard XDG application directories for the .desktop file
    /// and extracts the Exec= value.
    pub fn get_desktop_exec(desktop_name: &str) -> Option<String> {
        // Search paths in priority order
        let search_dirs: Vec<PathBuf> = {
            let mut dirs = Vec::new();
            if let Some(data_dir) = dirs::data_dir() {
                dirs.push(data_dir.join("applications"));
            }
            dirs.push(PathBuf::from("/usr/local/share/applications"));
            dirs.push(PathBuf::from("/usr/share/applications"));

            // Also check flatpak exports
            if let Some(data_dir) = dirs::data_dir() {
                dirs.push(data_dir.join("flatpak/exports/share/applications"));
            }
            dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
            dirs
        };

        for dir in &search_dirs {
            let path = dir.join(desktop_name);
            if let Ok(content) = std::fs::read_to_string(&path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(exec) = trimmed.strip_prefix("Exec=") {
                        return Some(exec.to_string());
                    }
                }
            }
        }

        None
    }
}

// Linux public API delegates to the linux module
#[cfg(target_os = "linux")]
pub fn register_file_associations() -> carminedesktop_core::Result<()> {
    linux::register()
}

#[cfg(target_os = "linux")]
pub fn unregister_file_associations() -> carminedesktop_core::Result<()> {
    linux::unregister()
}

#[cfg(target_os = "linux")]
pub fn are_file_associations_registered() -> bool {
    linux::is_registered()
}

#[cfg(target_os = "linux")]
pub fn get_previous_handler(ext: &str) -> Option<String> {
    linux::get_previous(ext)
}

/// Get the Exec= command from a .desktop file (Linux only).
///
/// Used by `open_file` to find the command for the previous handler.
#[cfg(target_os = "linux")]
pub fn get_desktop_exec(desktop_name: &str) -> Option<String> {
    linux::get_desktop_exec(desktop_name)
}

#[cfg(target_os = "linux")]
pub fn is_handled_extension(ext: &str) -> bool {
    linux::OFFICE_EXTENSIONS
        .iter()
        .any(|&e| e.eq_ignore_ascii_case(ext))
}

/// Discover an Office application handler at runtime (Linux).
///
/// Fallback when `get_previous_handler()` returns `None`.
/// Searches mimeinfo.cache and well-known .desktop files.
#[cfg(target_os = "linux")]
pub fn discover_office_handler(ext: &str) -> Option<String> {
    linux::discover(ext)
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
