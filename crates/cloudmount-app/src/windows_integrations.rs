//! Windows Explorer context menu registration via the registry.
//!
//! Registers two shell entries under `HKCU\Software\Classes\*\shell\`:
//!
//! - `CloudMount.OpenOnline`  -- dispatches a `cloudmount://open-online?path=%1` deep-link
//! - `CloudMount.OpenLocally` -- opens the file with the default system handler
//!
//! Registration happens on the first mount and cleanup on the last unmount.

use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const SHELL_KEY: &str = r"Software\Classes\*\shell";

const OPEN_ONLINE_KEY: &str = "CloudMount.OpenOnline";
const OPEN_ONLINE_LABEL: &str = "Open Online (SharePoint)";

const OPEN_LOCALLY_KEY: &str = "CloudMount.OpenLocally";
const OPEN_LOCALLY_LABEL: &str = "Open Locally";

/// Register both context menu entries in the Windows registry.
pub fn register_context_menus() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("failed to resolve exe path: {e}"))?
        .to_string_lossy()
        .into_owned();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let shell = hkcu
        .open_subkey_with_flags(SHELL_KEY, winreg::enums::KEY_WRITE)
        .or_else(|_| hkcu.create_subkey(SHELL_KEY).map(|(key, _)| key))
        .map_err(|e| format!("failed to open {SHELL_KEY}: {e}"))?;

    // --- Open Online (SharePoint) ---
    register_entry(
        &shell,
        OPEN_ONLINE_KEY,
        OPEN_ONLINE_LABEL,
        &format!("\"{exe}\" --open-online \"%1\""),
    )?;

    // --- Open Locally ---
    register_entry(
        &shell,
        OPEN_LOCALLY_KEY,
        OPEN_LOCALLY_LABEL,
        "cmd /c start \"\" \"%1\"",
    )?;

    tracing::info!("registered Windows Explorer context menu entries");
    Ok(())
}

/// Remove both context menu entries from the Windows registry.
pub fn unregister_context_menus() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let shell = match hkcu.open_subkey_with_flags(SHELL_KEY, winreg::enums::KEY_WRITE) {
        Ok(key) => key,
        Err(_) => return Ok(()), // nothing to clean up
    };

    remove_entry(&shell, OPEN_ONLINE_KEY)?;
    remove_entry(&shell, OPEN_LOCALLY_KEY)?;

    // Clean up legacy key if present
    remove_entry(&shell, "CloudMount.OpenInSharePoint")?;

    tracing::info!("removed Windows Explorer context menu entries");
    Ok(())
}

fn register_entry(
    shell: &RegKey,
    subkey_name: &str,
    label: &str,
    command: &str,
) -> Result<(), String> {
    let (entry, _) = shell
        .create_subkey(subkey_name)
        .map_err(|e| format!("failed to create {subkey_name}: {e}"))?;
    entry
        .set_value("", &label)
        .map_err(|e| format!("failed to set label for {subkey_name}: {e}"))?;
    entry
        .set_value("Icon", &"cloudmount.exe,0")
        .map_err(|e| format!("failed to set icon for {subkey_name}: {e}"))?;

    let (cmd_key, _) = entry
        .create_subkey("command")
        .map_err(|e| format!("failed to create command subkey for {subkey_name}: {e}"))?;
    cmd_key
        .set_value("", &command)
        .map_err(|e| format!("failed to set command for {subkey_name}: {e}"))?;

    Ok(())
}

fn remove_entry(shell: &RegKey, subkey_name: &str) -> Result<(), String> {
    // Delete the command subkey first (registry requires leaf-first deletion)
    if let Ok(entry) = shell.open_subkey_with_flags(subkey_name, winreg::enums::KEY_WRITE) {
        let _ = entry.delete_subkey("command");
    }
    match shell.delete_subkey(subkey_name) {
        Ok(()) => Ok(()),
        Err(ref e)
            if e.to_string().contains("not found") || e.to_string().contains("cannot find") =>
        {
            Ok(())
        }
        Err(e) => Err(format!("failed to remove {subkey_name}: {e}")),
    }
}
