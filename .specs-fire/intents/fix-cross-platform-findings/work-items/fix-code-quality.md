---
id: fix-code-quality
title: Collapse redundant drive_id cfg branches, fix inaccurate comment,
  document cache_dir rationale
intent: fix-cross-platform-findings
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-007
completed_at: 2026-03-08T14:51:46.504Z
---

# Work Item: Collapse redundant drive_id cfg branches, fix inaccurate comment, document cache_dir rationale

## Description

Addresses Issues #4, #6, and #8 (Info/Low severity) from the cross-platform review.

**Issue #6** (`main.rs:822-830`): `stop_mount` extracts `drive_id` with two
identical cfg branches:
```rust
let drive_id = {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    { handle.drive_id().to_string() }
    #[cfg(target_os = "windows")]
    { handle.drive_id().to_string() }
};
```
Both arms are identical. This also fails to compile on any future target that
is neither Linux/macOS nor Windows. Replace with a single unconditional
expression.

**Issue #4** (`main.rs:326`): Comment says "On non-Linux, the opener uses
`tauri_plugin_opener`" but in headless mode it uses `open::that`. Fix the
comment to be accurate.

**Issue #8** (`config.rs:157,208`): `cache_dir` is `Option<String>` rather than
`Option<PathBuf>`. On Windows, a user could supply a forward-slash path in TOML
that would be stored and used verbatim. This is functionally OK (Win32 accepts
`/` as separator) but inconsistent. Add a brief comment explaining that Win32
normalisation handles both separators, making the String representation safe.

## Acceptance Criteria

- [ ] `drive_id` in `stop_mount` is a single unconditional `handle.drive_id().to_string()` — no cfg branches
- [ ] Comment at `main.rs:326` accurately reflects both desktop and headless opener paths
- [ ] `cache_dir` in `config.rs` has a comment noting Win32 path normalisation semantics (or is converted to `PathBuf` if the change is low-risk)
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings
- [ ] `cargo test --all-targets` passes

## Technical Notes

Key locations:
- `crates/cloudmount-app/src/main.rs` — `stop_mount()` at line ~822
- `crates/cloudmount-app/src/main.rs` — comment at line ~326
- `crates/cloudmount-core/src/config.rs` — `UserGeneralSettings::cache_dir` at line ~157, `EffectiveConfig::cache_dir` at line ~208

The `drive_id()` method exists on both `MountHandle` and `CfMountHandle` and
returns the same type, so the unconditional call will compile on all platforms.

## Dependencies

(none)
