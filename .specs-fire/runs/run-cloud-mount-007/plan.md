# Run Plan — run-cloud-mount-007

**Scope**: wide | **Mode**: autopilot | **Intent**: fix-cross-platform-findings

---

## Work Item 1: fix-macos-fuse-detection

### Approach
Replace the `fusermount --version` probe in the macOS branch of `fuse_available()` with
a filesystem existence check for the macFUSE bundle at
`/Library/Filesystems/macfuse.fs`. macFUSE does not ship `fusermount` — that is a
Linux FUSE 2/3 binary.

### Files to Modify
- `crates/cloudmount-app/src/main.rs` — `fuse_available()` macOS branch (lines ~148-154)

### Tests
- `cargo clippy --all-targets --all-features` — zero warnings
- `cargo test --all-targets` — no regressions

---

## Work Item 2: fix-forbidden-path-cfg-gates

### Approach
Split the single mixed `system_dirs` array in `validate_mount_point` into a
`#[cfg(unix)]` list (Unix paths) and a `#[cfg(windows)]` list (Windows paths),
then form the effective slice using the appropriate platform constant. All
original paths are preserved; no runtime behaviour changes.

### Files to Modify
- `crates/cloudmount-core/src/config.rs` — `validate_mount_point()` (lines ~267-285)

### Tests
- `cargo clippy --all-targets --all-features`
- `cargo test -p cloudmount-core`

---

## Work Item 3: fix-code-quality

Three micro-fixes:

1. **Collapse redundant `drive_id` cfg branches** (`main.rs:821-830`):
   Replace two identical `#[cfg]` arms with a single unconditional
   `handle.drive_id().to_string()`.

2. **Fix inaccurate comment** (`main.rs:326`):
   The comment says "On non-Linux, the opener uses `tauri_plugin_opener`" —
   accurate for desktop but misleading because the headless path uses `open::that`.
   Update to mention "desktop, non-Linux" scope.

3. **Document `cache_dir` String rationale** (`config.rs:157,208`):
   Add a brief comment explaining that Win32 normalises both `/` and `\`
   separators, making `Option<String>` safe for cross-platform use.

### Files to Modify
- `crates/cloudmount-app/src/main.rs` — `stop_mount` drive_id block + comment at ~326
- `crates/cloudmount-core/src/config.rs` — `UserGeneralSettings::cache_dir` + `EffectiveConfig::cache_dir`

### Tests
- `cargo clippy --all-targets --all-features`
- `cargo test --all-targets`

---

## Work Item 4: fix-autostart-systemd-check

### Approach
In `autostart::enable()` (Linux branch), probe for systemd availability by running
`systemctl --version` **before** writing the `.service` file. If the probe fails,
return `Err` without touching the filesystem. This prevents stale `.service` file
artifacts on non-systemd Linux distributions (Alpine, Void, Artix, etc.).

Also verify `autostart::disable()` ordering: the service is disabled first
(`systemctl --user disable`), then the file is removed. Current code already
does this correctly — no change needed there.

### Files to Modify
- `crates/cloudmount-core/src/config.rs` — `autostart::enable()` Linux branch (lines ~466-481)

### Tests
- `cargo clippy --all-targets --all-features`
- `cargo test -p cloudmount-core`
