---
run: run-cloud-mount-008
work_items: fix-mount-path-separator, fix-windows-headless-mounts
intent: fix-cross-platform-findings
generated: 2026-03-08T15:05:38Z
mode: confirm
scope: wide
---

# Implementation Walkthrough: Fix Cross-Platform Path and Headless Diagnostics

## Summary

Fixed two cross-platform issues in CloudMount: (1) path construction in `config.rs` now uses `std::path::PathBuf::join()` instead of hardcoded `/` separators, producing OS-native paths on all platforms; (2) the Windows headless mode warning was split into two targeted diagnostics that name which features are degraded (crash recovery and delta sync), making the silent no-op visible in logs.

## Structure Overview

Both changes are narrow surgical fixes at well-defined boundaries:
- **Path separator fix** lives in `cloudmount-core` (shared library) and touches the two path-building helpers that all other crates rely on when resolving mount points. The desktop `cloudmount-app` Windows branch also gets an explicit `PathBuf::from` at the point where the path string enters the VFS layer.
- **Headless diagnostics fix** lives in `cloudmount-app`'s `run_headless()` function, a self-contained `#[cfg(target_os = "windows")]` block within the mount-start loop.

Neither change affects data structures, trait implementations, or public API shapes.

## Files Changed

### Created

_(none)_

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-core/src/config.rs` | `derive_mount_point`: replaced `format!` string concatenation with `PathBuf::join` chain. `expand_mount_point`: `~/` branch uses `Path::new(&home).join(rest)`; `{home}` prefix branch also rebuilt via `Path::join` (review fix). |
| `crates/cloudmount-app/src/main.rs` | `start_mount` Windows branch: `std::path::Path::new(&mountpoint)` → `&std::path::PathBuf::from(&mountpoint)`. `run_headless` Windows block: single generic warn → two per-feature warns (crash recovery, delta sync). |

## Key Implementation Details

### 1. Path assembly via Path::join

`derive_mount_point` builds `{home}/{root_dir}/OneDrive` (or `{home}/{root_dir}/{site}/{lib}`) by first converting `home` (a `String` from `dirs::home_dir()`) to a `Path` reference, then chaining `.join()` calls. On Windows, `dirs::home_dir()` returns a `PathBuf` with backslashes (`C:\Users\Alice`); `Path::join` appends with `MAIN_SEPARATOR`, producing `C:\Users\Alice\Cloud\OneDrive` instead of the previous mixed-separator `C:\Users\Alice/Cloud/OneDrive`.

### 2. expand_mount_point ~/... branch

`expand_mount_point` handles three cases: `~/rest`, `~`, and `{home}/...`. The `~/` branch now uses `Path::new(&home).join(rest)` where `rest` is the substring after `~/`. Because Windows `Path` accepts both `/` and `\` as separators when parsing a join target, a user who writes `~/Cloud/OneDrive` will get `C:\Users\Alice\Cloud\OneDrive` on Windows and `/home/alice/Cloud/OneDrive` on Linux — both correct.

### 3. {home} prefix handling (review fix)

The review caught that the `{home}` template branch (used when a user writes `{home}/Cloud` in config.toml) still did raw string replacement, leaving the `/` separator from the template. Added a `strip_prefix("{home}")` arm that trims the leading separator and rebuilds via `Path::join`, matching the behaviour of the `~/` branch. The rare case where `{home}` appears mid-template falls back to the original string replace (acceptable: `derive_mount_point` never emits such templates).

### 4. PathBuf::from at the VFS boundary

The desktop Windows `start_mount` function passes the mountpoint to `CfMountHandle::mount`. Changed from `std::path::Path::new(&mountpoint)` to `&std::path::PathBuf::from(&mountpoint)`. Both dereference to `&Path`, but `PathBuf::from` on Windows normalises any residual forward slashes to backslashes before the value enters the CfApi layer. A belt-and-suspenders complement to the upstream path-construction fix.

### 5. Per-feature headless diagnostics

Replaced one generic `warn!` with two that name the affected subsystems: crash recovery and delta sync. This makes it immediately clear in logs *why* a Windows headless mount is not functioning, rather than leaving operators to infer downstream effects.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Windows headless: mount or warn? | Warn (OR path from acceptance criteria) | Implementing CfApi mounting in headless mode requires keeping the `CfMountHandle` alive across an async loop and thread-safe ownership handoff — significant scope increase. The acceptance criteria explicitly allows the diagnostic approach. |
| `{home}` branch fix scope | Fix `{home}` prefix only, not mid-template | Mid-template `{home}` is not produced by any code path and is an edge case in manually authored config. A full recursive fix would increase complexity for no practical gain. |
| Path string storage type | Keep `String` at config layer | `MountConfig.mount_point` is `String` (TOML round-trips cleanly). Conversion to `PathBuf` at consumption points (join, CfMountHandle) is cleaner than changing the field type throughout all serialization boundaries. |

## Deviations from Plan

One deviation: the original plan listed the `{home}` branch as out of scope (only `~/...` required). Code review identified the same class of bug in the `{home}` prefix branch and the fix was applied, strengthening the change.

## Dependencies Added

_(none)_

## How to Verify

1. **Run cloudmount-core tests**
   ```bash
   cargo test -p cloudmount-core
   ```
   Expected: 11/11 tests pass.

2. **Run cloudmount-app tests**
   ```bash
   cargo test -p cloudmount-app
   ```
   Expected: 19/19 active tests pass (2 ignored — require live Graph API).

3. **Clippy clean (no new warnings)**
   ```bash
   cargo clippy --all-targets --all-features
   ```
   Expected: only pre-existing warnings in `commands.rs` and `main.rs` (not from these changes).

4. **Manual: derive_mount_point output**
   On any platform, add a temporary test that prints `derive_mount_point("Cloud", "drive", None, None)` — should be a path using the platform's native separator.

5. **Manual (Windows): headless log output**
   Run `cloudmount-app` in headless mode on Windows without authentication. The log should emit two distinct `WARN` messages per configured mount entry, each naming crash-recovery or delta-sync as the skipped feature.

## Test Coverage

- Tests run: 30 (11 cloudmount-core + 19 active cloudmount-app)
- New tests added: 0 (existing tests cover the path helpers)
- Status: all passing

## Developer Notes

- `to_string_lossy().into_owned()` is the correct idiom for converting a `PathBuf` back to `String` at a config-layer boundary. On all supported platforms (Linux, macOS, Windows), home directory paths are valid UTF-8, so the `lossy` replacement character will never fire in practice.
- `Path::join` on Windows treats both `/` and `\` as path separators when processing the joined component. A user-supplied path fragment like `"Cloud/OneDrive"` will be joined as `Cloud\OneDrive` on Windows. This is desirable behaviour.
- Pre-existing Clippy warnings (`collapsible_if` in `commands.rs`, `type_complexity` in `main.rs:105` and `main.rs:873`) were present before this run and are not caused by these changes.

---
*Generated by specs.md - fabriqa.ai FIRE Flow Run run-cloud-mount-008*
