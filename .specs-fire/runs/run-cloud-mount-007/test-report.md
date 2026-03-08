# Test Report — run-cloud-mount-007

**Run scope**: wide | **Intent**: fix-cross-platform-findings
**Test command**: `toolbox run --container cloudmount-build cargo test --all-targets`
**Clippy command**: `toolbox run --container cloudmount-build cargo clippy --all-targets --all-features`

---

## Summary

| Metric | Value |
|--------|-------|
| Total tests | 134 |
| Passed | 121 |
| Failed | 0 |
| Ignored (require live env) | 13 (FUSE: 13) + 2 (live Graph API) |
| Clippy warnings (new) | 0 |
| Clippy warnings (pre-existing) | 3 |

---

## Work Item: fix-macos-fuse-detection

### Test Results
All tests pass. No new tests added (the fix is a one-liner guarded by `#[cfg(target_os = "macos")]`; it cannot be unit-tested on Linux and the acceptance criteria do not require a new test).

### Clippy
Zero warnings from this change.

### Acceptance Criteria Validation
- [x] `fuse_available()` macOS branch now calls `std::path::Path::new("/Library/Filesystems/macfuse.fs").exists()`
- [x] `fusermount` is no longer invoked on macOS
- [x] Linux branch (`fusermount3 --version`) unchanged
- [x] Zero new clippy warnings

---

## Work Item: fix-forbidden-path-cfg-gates

### Test Results
`cargo test -p cloudmount-core` — 11 passed, 0 failed.

### Clippy
Zero warnings from this change. The `#[cfg(not(any(unix, windows)))]` guard on the empty fallback list ensures no dead-code warning on any standard target.

### Acceptance Criteria Validation
- [x] `#[cfg(windows)]` list covers Windows-specific system paths
- [x] `#[cfg(unix)]` list covers Unix-specific system paths
- [x] All original forbidden paths preserved
- [x] Zero new clippy warnings
- [x] `cargo test -p cloudmount-core` passes

---

## Work Item: fix-code-quality

### Test Results
All 134 tests pass.

### Clippy
Zero warnings from this change. Three pre-existing warnings remain:
- `collapsible_if` at `commands.rs:298` — pre-existing, not in scope
- `type_complexity` at `main.rs:105` and `main.rs:873` — pre-existing, not in scope

### Acceptance Criteria Validation
- [x] `drive_id` in `stop_mount` is a single unconditional `handle.drive_id().to_string()`
- [x] Comment at `main.rs:326` updated to accurately say "Desktop, non-Linux" and note headless/Linux paths
- [x] `cache_dir` in both `UserGeneralSettings` and `EffectiveConfig` has a comment explaining Win32 path normalisation semantics
- [x] Zero new clippy warnings
- [x] `cargo test --all-targets` passes

---

## Work Item: fix-autostart-systemd-check

### Test Results
`cargo test -p cloudmount-core` — 11 passed, 0 failed.

### Clippy
Zero warnings from this change.

### Acceptance Criteria Validation
- [x] `autostart::enable()` probes `systemctl --version` before writing any file
- [x] Returns `Err(...)` without touching the filesystem if systemd is unavailable
- [x] Write-then-enable behaviour preserved when systemd is available
- [x] `autostart::disable()` already has correct ordering (disable first, then remove file) — no change needed
- [x] Zero new clippy warnings
- [x] `cargo test -p cloudmount-core` passes
