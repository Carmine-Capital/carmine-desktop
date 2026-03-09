# Plan: fix-headless-windows

## Work Item
Skip empty delta-sync, don't create unused dirs, parse_cache_size cleanup

## Approach

Two remaining fixes in headless Windows mode (the third item — `parse_cache_size` cleanup — was already addressed in run-015):

### 1. Don't create unused mount directories
Move `expand_mount_point`, `cleanup_stale_mount`, and `create_dir_all` inside the existing `#[cfg(any(target_os = "linux", target_os = "macos"))]` block. On Windows headless, CfApi mounts aren't started, so creating directories is wasteful.

### 2. Skip empty delta-sync loop
Wrap the `tokio::spawn` for delta sync in `if !mount_entries.is_empty()`. On Windows headless, `mount_entries` is always empty, so the spawned task loops forever doing nothing.

## Files to Modify
| File | Change |
|------|--------|
| `crates/cloudmount-app/src/main.rs` | Consolidate FUSE mount loop body into single cfg block; guard delta sync spawn |

## Files to Create
None

## Tests
Existing tests must pass — no new tests needed (logic is platform-gated and headless-specific).
