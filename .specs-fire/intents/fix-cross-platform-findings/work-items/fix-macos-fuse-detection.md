---
id: fix-macos-fuse-detection
title: Fix fuse_available() on macOS to probe macFUSE install path
intent: fix-cross-platform-findings
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-007
completed_at: 2026-03-08T14:51:46.504Z
---

# Work Item: Fix fuse_available() on macOS to probe macFUSE install path

## Description

Addresses Issue #3 (High severity) from the cross-platform review.

`fuse_available()` in `main.rs` uses `#[cfg(target_os = "macos")]` to probe
`fusermount --version`. macFUSE (the only FUSE implementation for macOS) does
not ship `fusermount` — that is a Linux FUSE 2/3 binary. The correct macFUSE
indicator is the kernel extension bundle at
`/Library/Filesystems/macfuse.fs/Contents/Resources/mount_macfuse`.

As-is, `fuse_available()` always returns `false` on macOS even when macFUSE is
correctly installed, causing a spurious "FUSE not available" notification on
every authenticated launch.

Fix: replace the `fusermount` probe with a filesystem existence check for
`/Library/Filesystems/macfuse.fs`.

## Acceptance Criteria

- [ ] `fuse_available()` macOS branch checks `std::path::Path::new("/Library/Filesystems/macfuse.fs").exists()` (or equivalent)
- [ ] `fusermount` is no longer invoked on macOS
- [ ] Linux branch (`fusermount --version`) unchanged
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings
- [ ] No spurious "FUSE not available" notification on macOS with macFUSE installed

## Technical Notes

Key location:
- `crates/cloudmount-app/src/main.rs` — `fuse_available()` at line ~148

Current macOS branch:
```rust
#[cfg(target_os = "macos")]
{
    std::process::Command::new("fusermount")
        .arg("--version")
        .output()
        .is_ok()
}
```

Replacement:
```rust
#[cfg(target_os = "macos")]
{
    std::path::Path::new("/Library/Filesystems/macfuse.fs").exists()
}
```

The bundle existence is the canonical macFUSE install indicator used by other
tools (e.g., macFUSE's own uninstaller checks for this path).

## Dependencies

(none)
