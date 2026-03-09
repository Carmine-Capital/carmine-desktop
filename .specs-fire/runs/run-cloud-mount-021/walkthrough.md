# Walkthrough: fix-headless-windows

## Summary

Fixed two issues in the headless Windows codepath in `main.rs`: (1) mount directories were created even though Windows headless mode doesn't actually mount, and (2) a delta sync background task was spawned and looped forever with zero entries.

## Changes

| File | Change |
|------|--------|
| `crates/cloudmount-app/src/main.rs` | Consolidated FUSE mount loop body; guarded delta sync spawn |

## Details

### 1. Don't create unused mount directories

**Before**: The headless mount loop computed `mountpoint` and called `create_dir_all()` for every configured mount on all platforms, then entered platform-specific blocks. On Windows headless, the `#[cfg(target_os = "windows")]` block only logged warnings — the directories were created but never used.

**After**: `drive_id`, `mountpoint`, `cleanup_stale_mount()`, and `create_dir_all()` are now inside the existing `#[cfg(any(target_os = "linux", target_os = "macos"))]` block. On Windows, the loop body only runs the drive_id check and the Windows warning block — no filesystem side effects.

### 2. Skip empty delta-sync loop

**Before**: The delta sync task was spawned unconditionally via `tokio::spawn(async move { loop { ... } })`. On Windows headless, `mount_entries` was always empty (defined as `Vec::new()` with no pushes), so the loop iterated over an empty vec every `sync_interval` seconds forever.

**After**: The spawn is wrapped in `if !mount_entries.is_empty()`. The `auth_degraded` and `cancel` variables remain outside the guard since they're referenced by the SIGHUP handler and shutdown path respectively. The clone variables (`sync_cancel`, `sync_graph`, etc.) move inside the guard to avoid unnecessary allocations.

### 3. Already addressed items

The work item title mentioned `parse_cache_size cleanup` and `config_dir fallback` — both were already fixed in prior runs:
- `parse_cache_size` cfg gate cleanup: run-cloud-mount-015 (fix-ci-build-quality)
- `config_dir()` fallback to `.cloudmount`: run-cloud-mount-014 (fix-auth-security)

## Decisions

| Decision | Rationale |
|----------|-----------|
| Consolidate into single cfg block | Cleaner than multiple `#[cfg]` attributes — all FUSE-specific logic in one place |
| Runtime guard (`if !is_empty`) vs compile-time (`#[cfg]`) for delta sync | `mount_entries` can be empty on any platform if all mounts fail — runtime check benefits all platforms |
| Keep `auth_degraded`/`cancel` outside guard | Still needed by SIGHUP handler (unix) and shutdown path respectively |
