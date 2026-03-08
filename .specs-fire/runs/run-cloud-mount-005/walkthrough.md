---
run: run-cloud-mount-005
intent: feat-mount-validation
generated: 2026-03-08T14:00:00Z
mode: confirm
scope: wide
---

# Implementation Walkthrough: Mount Validation Before Start + Orphaned Mount Detection

## Summary

Two related work items implemented in a single run. Work item 1 adds pre-mount validation (`GET /drives/{drive_id}`) before each FUSE/CfApi mount is started, handling 404 (remove from config), 403 (skip, notify), and transient errors (skip silently). Work item 2 extends the delta sync loop to detect drives that disappear mid-session, stopping and removing orphaned mounts on 404, and deduplicating 403 access-denied notifications.

## Structure Overview

The validation chain now has two layers:
- **Startup** (`start_mount`): single-attempt `check_drive_exists` via `block_in_place` → classified into 404/403/transient → `remove_mount_from_config` helper for 404.
- **Runtime** (`start_delta_sync` loop): per-mount `run_delta_sync` result matched on 404/403 → `stop_mount` + `remove_mount_from_config` for 404, one-time notification for 403 using a local `HashSet`.

Both layers share the same `remove_mount_from_config` helper and `notify::mount_access_denied` function.

## Files Changed

### Created
(none)

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-graph/src/client.rs` | Added `check_drive_exists` — single-attempt `GET /drives/{drive_id}`, no retry |
| `crates/cloudmount-app/src/notify.rs` | Added `mount_not_found`, `mount_access_denied`, `mount_orphaned` notifications |
| `crates/cloudmount-app/src/main.rs` | Added `remove_mount_from_config` helper; validation block in both `start_mount` variants; orphan/403 handling in `start_delta_sync` with expanded snapshot tuple |
| `crates/cloudmount-graph/tests/graph_tests.rs` | Added 3 tests for `check_drive_exists` (200, 404, 403) |

## Key Implementation Details

### 1. Single-Attempt Drive Check (`check_drive_exists`)

Uses the existing `handle_error` helper and a direct token fetch — no `with_retry` wrapper. This is intentional: 404 and 403 are not transient conditions; retrying would be misleading and slow startup.

### 2. `block_in_place` for Sync-Context Async Call

`start_mount` is a sync function called from both sync Tauri command handlers and from Tauri's async runtime. `tokio::task::block_in_place(|| rt.block_on(...))` is the correct pattern for calling async from within a sync function that may run inside a tokio multi-threaded executor. This was already the pattern used by FUSE's VFS operations.

### 3. `remove_mount_from_config` Helper

Locks `user_config`, removes the mount, saves to disk, builds new `effective_config`, then drops the lock before updating `effective_config`. This one-lock-span approach avoids holding two locks simultaneously and keeps the state transition atomic from the user config perspective.

### 4. Snapshot Expansion in Delta Sync

The `start_delta_sync` snapshot was extended from `(drive_id, cache, inodes)` to `(drive_id, mount_id, mount_name, cache, inodes)`. The `mount_name` enables meaningful notifications and `mount_id` is needed for `stop_mount`. Both locks (`mount_caches` and `effective_config`) are taken together during snapshot building and released before the async loop begins.

### 5. 403 Deduplication via Local `HashSet`

`notified_403: HashSet<String>` lives inside the `spawn` closure. On 403, the drive_id is inserted — `HashSet::insert` returns `false` if already present, suppressing duplicate notifications. On `Ok(())`, the entry is cleared so access-restored drives can be re-notified on future 403s.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| New error variants vs pattern matching | Pattern match on `GraphApi { status }` | Zero changes to shared crate; 404/403 already have distinct status codes |
| Deduplication storage | Local HashSet in spawn closure | No AppState changes; simpler than a Mutex-wrapped set on AppState |
| Return `Ok(())` from start_mount on 404/403 | `Ok(())` not `Err(...)` | Prevents `start_all_mounts` from calling `notify::mount_failed` (double notification) |
| `let _ = stop_mount(...)` in delta sync 404 handler | Discard error | Mount may already be stopped or never fully started; soft error is correct |
| `mount_orphaned` vs reusing `mount_not_found` | Separate function | Different message: startup vs mid-session deletion have different UX wording |

## Deviations from Plan

**Item 1**: None — implementation matched the plan exactly.

**Item 2**: The plan mentioned considering `block_in_place` for `stop_mount` in the async context. We did not add it — existing code already calls `stop_mount` from async contexts (e.g., `toggle_mount` Tauri command) without `block_in_place`, establishing the pattern that this is acceptable.

## Dependencies Added

(none)

## How to Verify

1. **Unit tests — check_drive_exists**
   ```bash
   cargo test -p cloudmount-graph check_drive_exists
   ```
   Expected: 3 tests pass (`ok_on_200`, `returns_404_error`, `returns_403_error`)

2. **Full graph test suite**
   ```bash
   cargo test -p cloudmount-graph
   ```
   Expected: 24 tests pass (0 failures)

3. **Clippy clean**
   ```bash
   RUSTFLAGS="-Dwarnings" cargo clippy -p cloudmount-graph -p cloudmount-app --all-targets
   ```
   Expected: no warnings

4. **Manual: startup validation — 404 scenario**
   - Add a mount with a `drive_id` that no longer exists (deleted library)
   - Start the app
   - Expected: notification "'{name}' is no longer accessible and has been removed from your configuration"; mount absent from tray

5. **Manual: startup validation — 403 scenario**
   - Add a mount with a `drive_id` for a library the account cannot access
   - Start the app
   - Expected: notification "No access to '{name}' — check your permissions"; mount not started but still in config

6. **Manual: runtime orphan — 404 scenario**
   - Mount a drive, then delete the SharePoint library from the admin portal
   - Wait for next delta sync cycle
   - Expected: notification "'{name}' was deleted or moved and has been removed from your configuration"; mount stopped

7. **Manual: runtime orphan — 403 spam prevention**
   - Revoke access to a mounted library
   - Wait for several sync cycles
   - Expected: exactly one "No access" notification (not one per cycle)

## Test Coverage

- Tests added: 3
- Status: all passing
- Note: `start_mount` and `start_delta_sync` changes are in Tauri desktop context — covered by manual scenarios above

## Developer Notes

- The `check_drive_exists` wiremock tests use `expect(1)` to assert no retries occur. If a future refactor accidentally adds retry logic, these tests will fail.
- `notified_403` is cleared on `Ok(())` — if a library has its access restored, the user will be notified again on the next 403. This is intentional.
- Both `start_mount` variants (Linux/macOS FUSE + Windows CfApi) are symmetric — they have identical validation blocks. Any future changes must be applied to both `#[cfg]` variants.

---
*Generated by specs.md - fabriqa.ai FIRE Flow — run-cloud-mount-005*
