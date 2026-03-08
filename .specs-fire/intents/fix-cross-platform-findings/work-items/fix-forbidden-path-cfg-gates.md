---
id: fix-forbidden-path-cfg-gates
title: Split forbidden-path list into platform-gated sets
intent: fix-cross-platform-findings
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-007
completed_at: 2026-03-08T14:51:46.504Z
---

# Work Item: Split forbidden-path list into platform-gated sets

## Description

Addresses Issue #2 (Low severity) from the cross-platform review.

`validate_mount_point` in `config.rs` (line ~267) uses a single `system_dirs`
array containing both Unix paths (`/`, `/bin`, `/sbin`, `/usr`, ...) and Windows
paths (`C:\`, `C:\Windows`, ...) with no `#[cfg]` gate. This is harmless at
runtime (the wrong-platform paths are never matched against valid paths on that
platform), but produces false-positive rejections if someone supplies a nonsense
cross-platform path, and violates the principle that dead code should be gated.

Fix: split into two `#[cfg]`-gated lists and concatenate into one slice for the
check.

## Acceptance Criteria

- [ ] `validate_mount_point` uses a `#[cfg(windows)]` list for Windows-specific system paths (`C:\`, `C:\Windows`, etc.)
- [ ] `validate_mount_point` uses a `#[cfg(unix)]` (or `not(windows)`) list for Unix-specific system paths (`/`, `/bin`, `/usr`, etc.)
- [ ] A shared list (if any) covers paths that apply to all platforms
- [ ] All original forbidden paths are preserved — no regressions in rejection logic
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings
- [ ] `cargo test -p cloudmount-core` passes

## Technical Notes

Key location:
- `crates/cloudmount-core/src/config.rs` — `validate_mount_point()` at line ~267

Suggested pattern:
```rust
#[cfg(unix)]
let unix_dirs: &[&str] = &["/", "/bin", "/sbin", "/usr", "/etc", "/var", "/tmp"];
#[cfg(windows)]
let windows_dirs: &[&str] = &["C:\\", "C:\\Windows", "C:\\Program Files"];

#[cfg(unix)]
let system_dirs = unix_dirs;
#[cfg(windows)]
let system_dirs = windows_dirs;
```

## Dependencies

(none)
