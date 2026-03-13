//! Windows shell integration — file type associations for Office documents.
//!
//! Registers CloudMount as the handler for Office file types (.docx, .xlsx, .pptx,
//! .doc, .xls, .ppt) using per-user registry keys (HKCU\Software\Classes).
//!
//! The previous default handler ProgID is saved so that files NOT on a CloudMount
//! drive can be opened with the original handler, avoiding infinite loops.

#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};

/// Office file extensions we register as handlers for.
#[cfg(target_os = "windows")]
const OFFICE_EXTENSIONS: &[&str] = &[".docx", ".xlsx", ".pptx", ".doc", ".xls", ".ppt"];

/// Registry value name where we store the previous default handler ProgID.
#[cfg(target_os = "windows")]
const PREVIOUS_HANDLER_VALUE: &str = "CloudMount.PreviousHandler";

/// ProgID prefix for our file type handlers.
#[cfg(target_os = "windows")]
const PROGID_PREFIX: &str = "CloudMount.OfficeFile";

/// Register CloudMount as the handler for Office file types.
///
/// For each extension:
/// 1. Reads the current default handler ProgID (if any)
/// 2. Saves it under `CloudMount.PreviousHandler` for fallback
/// 3. Sets our ProgID as the new default
/// 4. Creates the ProgID key with shell\open\command pointing to CloudMount.exe --open "%1"
///
/// # Errors
/// Returns an error if registry operations fail.
#[cfg(target_os = "windows")]
pub fn register_file_associations() -> crate::cloudmount_core::Result<()> {
    let exe_path = std::env::current_exe().map_err(|e| {
        cloudmount_core::Error::Config(format!("failed to get current exe path: {e}"))
    })?;
    let exe_str = exe_path.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_READ | KEY_WRITE)?;

    for ext in OFFICE_EXTENSIONS {
        let progid = format!("{PROGID_PREFIX}{ext}");

        // Open or create the extension key (e.g. .docx)
        let (ext_key, _) = classes.create_subkey(ext)?;

        // Save the previous handler if one exists and we haven't already saved it
        if ext_key
            .get_value::<String, _>(PREVIOUS_HANDLER_VALUE)
            .is_err()
        {
            if let Ok(prev) = ext_key.get_value::<String, _>("") {
                if !prev.is_empty() && !prev.starts_with(PROGID_PREFIX) {
                    ext_key.set_value(PREVIOUS_HANDLER_VALUE, &prev)?;
                    tracing::debug!("saved previous handler for {ext}: {prev}");
                }
            }
        }

        // Set our ProgID as the default handler
        ext_key.set_value("", &progid)?;

        // Create the ProgID key (e.g. CloudMount.OfficeFile.docx)
        let (progid_key, _) = classes.create_subkey(&progid)?;

        // Set the display name
        let display_name = format!("Office Document (CloudMount){ext}");
        progid_key.set_value("", &display_name)?;

        // Create shell\open\command with the handler command
        let (shell_key, _) = progid_key.create_subkey("shell")?;
        let (open_key, _) = shell_key.create_subkey("open")?;
        let (command_key, _) = open_key.create_subkey("command")?;

        // Command: "C:\path\to\CloudMount.exe" --open "%1"
        let command = format!("\"{exe_str}\" --open \"%1\"");
        command_key.set_value("", &command)?;

        tracing::info!("registered file association for {ext}");
    }

    // Notify the shell that file associations have changed
    notify_shell_change();

    Ok(())
}

/// Unregister CloudMount file associations and restore previous handlers.
///
/// For each extension:
/// 1. Reads the saved previous handler ProgID
/// 2. Restores it as the default (or removes the default if none was saved)
/// 3. Deletes our ProgID key
/// 4. Removes the CloudMount.PreviousHandler value
///
/// # Errors
/// Returns an error if registry operations fail.
#[cfg(target_os = "windows")]
pub fn unregister_file_associations() -> cloudmount_core::Result<()> {
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
                    tracing::debug!("skipping {ext}: not currently handled by CloudMount");
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
/// is NOT on a CloudMount drive, avoiding infinite loops.
///
/// # Arguments
/// * `ext` - The file extension including the dot (e.g. ".docx")
///
/// # Returns
/// The previous handler ProgID if one was saved, or `None`.
#[cfg(target_os = "windows")]
pub fn get_previous_handler(ext: &str) -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu
        .open_subkey_with_flags(r"Software\Classes", KEY_READ)
        .ok()?;
    let ext_key = classes.open_subkey_with_flags(ext, KEY_READ).ok()?;
    ext_key.get_value(PREVIOUS_HANDLER_VALUE).ok()
}

/// Get the command line for a ProgID's shell\open\command.
///
/// Used to invoke the previous handler directly.
#[cfg(target_os = "windows")]
pub fn get_progid_command(progid: &str) -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu
        .open_subkey_with_flags(r"Software\Classes", KEY_READ)
        .ok()?;
    let command_path = format!(r"{progid}\shell\open\command");
    let command_key = classes
        .open_subkey_with_flags(&command_path, KEY_READ)
        .ok()?;
    command_key.get_value("").ok()
}

/// Check if CloudMount file associations are currently registered.
///
/// Returns `true` if at least one Office extension has CloudMount as its handler.
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

// ---------------------------------------------------------------------------
// Non-Windows stubs
// ---------------------------------------------------------------------------

// These stubs exist to provide a consistent API across platforms. They are
// never called on non-Windows but are needed for the module to compile.

/// Register file associations (no-op on non-Windows).
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn register_file_associations() -> cloudmount_core::Result<()> {
    Ok(())
}

/// Unregister file associations (no-op on non-Windows).
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn unregister_file_associations() -> cloudmount_core::Result<()> {
    Ok(())
}

/// Get the previous handler for an extension (always `None` on non-Windows).
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn get_previous_handler(_ext: &str) -> Option<String> {
    None
}

/// Get the command line for a ProgID (always `None` on non-Windows).
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn get_progid_command(_progid: &str) -> Option<String> {
    None
}

/// Check if file associations are registered (always `false` on non-Windows).
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub fn are_file_associations_registered() -> bool {
    false
}
