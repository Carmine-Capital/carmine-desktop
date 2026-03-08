---
run: run-cloud-mount-001
work_item: per-mount-cache-isolation
intent: fix-multi-mount-inode-collision
generated: 2026-03-08T10:40:00Z
mode: confirm
---

# Implementation Walkthrough: Isolate CacheManager and InodeTable per mount

## Summary

Removed the single shared `CacheManager` and `InodeTable` from `AppState` and replaced them with a per-mount `mount_caches` map keyed by `drive_id`. Each mount now gets its own SQLite database file (`drive-{safe_id}.db`) and its own `InodeTable` starting after the DB's maximum inode, eliminating the `UNIQUE constraint failed: items.inode` crash that occurred when a second mount tried to insert `inode=1` into the already-occupied shared database.

## Structure Overview

The fix is entirely contained within `cloudmount-app`. No lower-level crates (`cloudmount-cache`, `cloudmount-vfs`) required changes — they already accepted `CacheManager` and `InodeTable` as parameters rather than owning them.

**Before**: `AppState` held one `Arc<CacheManager>` (one SQLite DB: `cloudmount.db`) and one `Arc<InodeTable>` shared across all mounts. When mount 2 started, it collided with mount 1's `inode=1` root record.

**After**: `AppState` holds `mount_caches: Mutex<HashMap<drive_id, (Arc<CacheManager>, Arc<InodeTable>)>>`. Each mount's `start_mount` creates fresh instances with an isolated DB. Delta sync, crash recovery, `refresh_mount`, and `clear_cache` all iterate over the per-mount entries. The headless path follows the same pattern using a local `Vec` of mount entries.

## Files Changed

### Created

None.

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-app/src/main.rs` | Removed `cache`, `inodes`, `drive_ids` from `AppState`/`Components`/`init_components`; added `mount_caches`; rewrote `start_mount` (×2), `stop_mount`, `start_delta_sync`, `run_crash_recovery`, `run_headless`; reordered crash recovery after mounts start |
| `crates/cloudmount-app/src/commands.rs` | Updated `refresh_mount` (per-mount lookup) and `clear_cache` (collect-before-stop ordering fix); fixed pre-existing rustfmt divergence |

## Key Implementation Details

### 1. Per-mount DB naming and safe ID

Drive IDs from Microsoft Graph API can contain `!` characters (e.g. `b!GNkOAJ...`). Since `!` is invalid in some filesystem paths, the safe filename is derived by replacing `!` with `_`. The resulting DB name is `drive-{safe_id}.db`, valid on Linux, macOS, and Windows.

### 2. InodeTable starting point

Each new `InodeTable` starts its counter after `max_inode` read from its own SQLite DB. This prevents inode reuse after restarts. Each mount's inode namespace is fully independent — mount 1's inode 42 and mount 2's inode 42 refer to different items in different DBs.

### 3. Delta sync per-mount snapshot

Delta sync takes a snapshot of `mount_caches` at the start of each iteration (not once at startup). This means mounts added after startup (e.g. via `add_mount`) are picked up automatically, and removed mounts are dropped. A fresh `inode_allocator` closure is created per drive per iteration to capture the correct `InodeTable` reference.

### 4. Crash recovery and SIGHUP

All per-mount `CacheManager`s share the same `cache_dir`, so their `WriteBackBuffer` instances all point to the same `{cache_dir}/pending/` directory. Any one of them can list and process all pending writes. Crash recovery takes the first available cache from `mount_caches`. In headless mode, the SIGHUP handler does the same from `mount_entries`.

### 5. Ordering: crash recovery after mounts start

In `setup_after_launch`, crash recovery is now called after `start_all_mounts` (consistent with the `complete_sign_in` path). This ensures `mount_caches` is populated when recovery runs. The previous ordering (recovery before mounts) would have found an empty `mount_caches` and returned early.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Cache isolation key | `drive_id` string | Natural, stable identifier from Graph API; already used to key delta sync and writeback paths |
| DB file naming | `drive-{safe_id}.db` in shared `cache_dir` | Matches existing cache dir convention; separate DB eliminates inode collision |
| Disk content + writeback dir | Shared across all mounts | Already keyed by drive_id internally; no collision possible; simplifies cleanup |
| `mount_caches` vs separate `HashMap<drive_id, CacheManager>` field | Single `Mutex<HashMap>` | Minimal surface area; atomic snapshot for delta sync |
| Re-snapshot mount_caches each delta sync loop | Yes | Picks up dynamically added/removed mounts without restart |
| Remove `drive_ids: Arc<RwLock<Vec<String>>>` | Removed | Fully superseded by `mount_caches.keys()` |
| Crash recovery: use first cache or all caches | First cache only | All caches share the same writeback dir; iterating all would process the same files multiple times |

## Deviations from Plan

**One addition not in the original plan**: The `setup_after_launch` ordering change — moving `run_crash_recovery` to after `start_all_mounts`. This was necessary because crash recovery now requires `mount_caches` to have entries, which only happens after `start_mount` runs.

**One bug found during code review**: `clear_cache` originally collected caches AFTER `stop_all_mounts`, which removes them from `mount_caches`. Fixed by collecting Arc references first, then stopping, then clearing.

## Dependencies Added

None.

## How to Verify

1. **Single mount still works (regression)**
   ```bash
   cargo run -p cloudmount-app
   ```
   Expected: Mounts successfully, delta sync runs, no errors.

2. **Two simultaneous mounts (the bug fix)**
   Configure `~/.config/cloudmount/config.toml` with one OneDrive mount and one SharePoint mount (both with `enabled = true`). Run the app. Previously: `cache error: upsert failed: UNIQUE constraint failed: items.inode`. Now: both mounts start successfully.

3. **Per-mount DB files created**
   ```bash
   ls ~/.cache/cloudmount/drive-*.db
   ```
   Expected: One `drive-{safe_id}.db` file per active mount.

4. **All CI checks pass**
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets --all-features
   cargo test --all-targets
   ```
   Expected: Zero warnings, zero failures.

## Test Coverage

- Tests run: 134
- Tests passing: 119 (15 skipped: FUSE requires kernel module, 2 e2e require live Graph API)
- Status: ✅ All passing

## Developer Notes

- **`!` in drive IDs**: Microsoft Graph API drive IDs for SharePoint often start with `b!` (e.g. `b!GNkOAJfB...`). Always use `drive_id.replace('!', "_")` when constructing filesystem paths from drive IDs.
- **Shared writeback dir**: All per-mount CacheManagers share `{cache_dir}/pending/`. This is intentional — it means any one of them can do crash recovery. Don't "fix" this by giving each mount a separate pending dir, as it would break the shared-writeback assumption in `clear_cache` and crash recovery.
- **`clear_cache` ordering**: Must collect `Arc<CacheManager>` references BEFORE calling `stop_all_mounts()`. `stop_mount` removes entries from `mount_caches`, so collecting after stop yields an empty list.
- **Old `cloudmount.db`**: After upgrading, the old shared `cloudmount.db` is left behind in `cache_dir`. It's harmless — it'll be skipped (not opened by anyone). Users may delete it manually.

---
*Generated by specs.md - fabriqa.ai FIRE Flow Run run-cloud-mount-001*
