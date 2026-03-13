//! Process filtering for CollabGate interactive shell detection.
//!
//! Determines whether a given PID belongs to an interactive file manager
//! (Explorer, Nautilus, Finder, etc.) so the VFS can decide whether to
//! show a collaborative-open dialog or silently proceed with a local open.
//!
//! Non-interactive processes (antivirus, indexers, build tools) bypass the
//! dialog. On resolution failure the function returns `false` (fail-safe to
//! local open).

/// Known interactive shell / file-manager process names on Linux.
#[cfg(target_os = "linux")]
pub const KNOWN_SHELLS: &[&str] = &["nautilus", "dolphin", "thunar", "nemo", "pcmanfm", "caja"];

/// Known interactive shell / file-manager process names on Windows.
#[cfg(target_os = "windows")]
pub const KNOWN_SHELLS: &[&str] = &["explorer.exe"];

/// Known interactive shell / file-manager process names on macOS.
#[cfg(target_os = "macos")]
pub const KNOWN_SHELLS: &[&str] = &["Finder"];

/// Fallback for unsupported platforms — empty list.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub const KNOWN_SHELLS: &[&str] = &[];

/// Returns `true` if `pid` belongs to a known interactive file-manager process.
///
/// Checks the resolved process name against [`KNOWN_SHELLS`] and any
/// user-configured `extra_shells` entries. Comparison is case-insensitive.
///
/// Returns `false` on any failure (process exited, permission denied,
/// unsupported platform) — fail-safe to local open.
pub fn is_interactive_shell(pid: u32, extra_shells: &[String]) -> bool {
    let Some(name) = resolve_process_name(pid) else {
        return false;
    };

    if is_known_shell(&name, extra_shells) {
        return true;
    }

    // On Windows, Explorer launches the associated app (e.g. Excel) which then
    // calls CreateFile. The caller PID is Excel, not Explorer. Check the parent
    // process to catch this indirection.
    #[cfg(target_os = "windows")]
    if let Some(parent_pid) = resolve_parent_pid(pid)
        && let Some(parent_name) = resolve_process_name(parent_pid)
        && is_known_shell(&parent_name, extra_shells)
    {
        return true;
    }

    false
}

/// Check whether a process name matches any known interactive shell.
fn is_known_shell(name: &str, extra_shells: &[String]) -> bool {
    let name_lower = name.to_lowercase();

    for &known in KNOWN_SHELLS {
        if name_lower == known.to_lowercase() {
            return true;
        }
    }

    for extra in extra_shells {
        if name_lower == extra.to_lowercase() {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// Platform-specific PID → process-name resolution
// ---------------------------------------------------------------------------

/// Linux: read `/proc/<pid>/exe` symlink, extract the filename component.
#[cfg(target_os = "linux")]
fn resolve_process_name(pid: u32) -> Option<String> {
    let link = format!("/proc/{pid}/exe");
    let path = std::fs::read_link(link).ok()?;
    let file_name = path.file_name()?;
    Some(file_name.to_string_lossy().into_owned())
}

/// macOS: use `proc_pidpath` to obtain the executable path, then extract the
/// filename component.
#[cfg(target_os = "macos")]
fn resolve_process_name(pid: u32) -> Option<String> {
    use std::ffi::CStr;

    // MAXPATHLEN on macOS is 1024; proc_pidpath needs a buffer of that size.
    let mut buf = vec![0u8; libc::MAXPATHLEN as usize];

    // Safety: buf is large enough, pid is a valid i32 range for proc_pidpath.
    let ret = unsafe {
        libc::proc_pidpath(
            pid as i32,
            buf.as_mut_ptr().cast::<libc::c_void>(),
            buf.len() as u32,
        )
    };

    if ret <= 0 {
        return None;
    }

    let c_str = CStr::from_bytes_until_nul(&buf).ok()?;
    let path = std::path::Path::new(c_str.to_str().ok()?);
    let file_name = path.file_name()?;
    Some(file_name.to_string_lossy().into_owned())
}

/// Windows: open the process with limited query rights, read the image name.
#[cfg(target_os = "windows")]
fn resolve_process_name(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };

    // Safety: OpenProcess with limited info rights is safe for any valid PID.
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };

    if handle.is_null() {
        return None;
    }

    let result = (|| {
        let mut buf = [0u16; 260]; // MAX_PATH
        let mut len = buf.len() as u32;

        // Safety: handle is valid, buf is properly sized, len is in/out.
        let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut len) };

        if ok == 0 {
            return None;
        }

        let path_str = String::from_utf16_lossy(&buf[..len as usize]);
        let path = std::path::Path::new(&path_str);
        let file_name = path.file_name()?;
        Some(file_name.to_string_lossy().into_owned())
    })();

    // Safety: handle is a valid, open process handle.
    unsafe { CloseHandle(handle) };

    result
}

/// Unsupported platforms: always return `None` → `is_interactive_shell` returns
/// `false`.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn resolve_process_name(_pid: u32) -> Option<String> {
    None
}

// ---------------------------------------------------------------------------
// Windows: parent-PID resolution via Toolhelp API
// ---------------------------------------------------------------------------

/// Windows: resolve the parent PID of a given process using the Toolhelp API.
///
/// Enumerates all processes via `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)`,
/// finds the entry whose `th32ProcessID` matches `pid`, and returns its
/// `th32ParentProcessID`. Returns `None` on failure (snapshot error, PID not
/// found, or parent PID is 0).
#[cfg(target_os = "windows")]
fn resolve_parent_pid(pid: u32) -> Option<u32> {
    use std::mem;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    // Safety: TH32CS_SNAPPROCESS with 0 takes a snapshot of all processes.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }

    let result = (|| {
        let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
        entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

        // Safety: snapshot is a valid handle, entry is properly sized.
        if unsafe { Process32FirstW(snapshot, &mut entry) } == 0 {
            return None;
        }

        loop {
            if entry.th32ProcessID == pid {
                let parent = entry.th32ParentProcessID;
                return if parent != 0 { Some(parent) } else { None };
            }

            // Safety: snapshot is valid, entry is properly sized.
            if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
                break;
            }
        }

        None
    })();

    // Safety: snapshot is a valid handle from CreateToolhelp32Snapshot.
    unsafe { CloseHandle(snapshot) };

    result
}

// ---------------------------------------------------------------------------
// Helper: resolve process name for the current process (used by tests)
// ---------------------------------------------------------------------------

/// Resolves the process name for the current process.
/// Exposed for testing the extra_shells feature.
pub fn current_process_name() -> Option<String> {
    resolve_process_name(std::process::id())
}
