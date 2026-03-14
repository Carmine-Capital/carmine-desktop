#[cfg(target_os = "linux")]
use carminedesktop_vfs::process_filter::KNOWN_SHELLS;
use carminedesktop_vfs::process_filter::{current_process_name, is_interactive_shell};

#[cfg(target_os = "linux")]
#[test]
fn test_process_filter_known_shells_linux() {
    let expected = ["nautilus", "dolphin", "thunar", "nemo", "pcmanfm", "caja"];
    for name in &expected {
        assert!(
            KNOWN_SHELLS.contains(name),
            "expected {name} in KNOWN_SHELLS"
        );
    }
}

#[test]
fn test_process_filter_current_process_not_shell() {
    // The test runner binary is not a file manager.
    assert!(!is_interactive_shell(std::process::id(), &[]));
}

#[test]
fn test_process_filter_nonexistent_pid() {
    // u32::MAX is virtually guaranteed to not be a running process.
    assert!(!is_interactive_shell(u32::MAX, &[]));
}

#[test]
fn test_process_filter_extra_shells() {
    // Adding the current process name as an extra shell should make it match.
    let Some(name) = current_process_name() else {
        // If we can't resolve our own process name (e.g. unsupported platform),
        // skip this test — the fail-safe behavior is already tested above.
        return;
    };
    let extras = vec![name];
    assert!(is_interactive_shell(std::process::id(), &extras));
}
