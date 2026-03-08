# Implementation Plan — run-cloud-mount-005

## Work Item 1: validate-mount-before-start
**Mode**: confirm | **Intent**: feat-mount-validation

### Approach

Add a single-attempt `check_drive_exists` method to `GraphClient`, call it inside `start_mount`
using `tokio::task::block_in_place` (safe in both async and sync call sites), and handle
404/403/transient outcomes before mounting. Return `Ok(())` for all handled cases to prevent
double-notification from `start_all_mounts`'s `mount_failed` path.

### Files to Modify

- `crates/cloudmount-graph/src/client.rs`
  — Add `pub async fn check_drive_exists(&self, drive_id: &str) -> cloudmount_core::Result<()>`
  — Single GET `/drives/{drive_id}`, no `with_retry`, propagates status verbatim

- `crates/cloudmount-app/src/notify.rs`
  — Add `pub fn mount_not_found(app: &AppHandle, name: &str)`
  — Add `pub fn mount_access_denied(app: &AppHandle, name: &str)`

- `crates/cloudmount-app/src/main.rs`
  — In both `start_mount` variants (Linux/macOS + Windows): add validation block after
    extracting `drive_id`, using `block_in_place(|| rt.block_on(check_drive_exists(...)))`
  — Handle 404: remove from config + notify::mount_not_found + return Ok(())
  — Handle 403: notify::mount_access_denied + return Ok(())
  — Handle transient: tracing::warn! + return Ok(())
  — Add private helper `fn remove_mount_from_config(app, mount_id)` that updates
    user_config, saves to disk, rebuilds effective_config

### Files to Create / Append for Tests

- `crates/cloudmount-graph/tests/graph_tests.rs` (append 3 tests)
  — `check_drive_exists_returns_ok_on_200`
  — `check_drive_exists_returns_404_error`
  — `check_drive_exists_returns_403_error`

### Key Decisions

| Decision | Rationale |
|----------|-----------|
| `block_in_place(rt.block_on(...))` | Safe in both async task and sync Tauri command callers |
| Return `Ok(())` for 404/403/transient | Prevents double-notification from start_all_mounts |
| No `with_retry` in `check_drive_exists` | 404/403 are definitive; retry would be misleading |
| Remove from both `user_config` + `effective_config` | Keeps in-memory state consistent |

---

## Work Item 2: handle-orphaned-mount-in-delta-sync
**Mode**: confirm | **Intent**: feat-mount-validation

### Approach

Match on `Error::GraphApi { status: 404/403 }` in the `start_delta_sync` loop in `main.rs`. Extend the snapshot tuple to include `mount_id` and `mount_name` from `effective_config`. Use a local `HashSet<String>` (`notified_403`) in the spawn closure for 403 deduplication.

### Files Modified

- `crates/cloudmount-app/src/notify.rs`
  — Add `pub fn mount_orphaned(app, name)` — sync-context 404 message

- `crates/cloudmount-app/src/main.rs`
  — `start_delta_sync`: expand snapshot to `(drive_id, mount_id, mount_name, cache, inodes)`
  — Add `notified_403: HashSet<String>` before loop
  — Add match arms for 404, 403; clear 403 state on Ok

### Key Decisions

| Decision | Rationale |
|----------|-----------|
| No new error variants | `GraphApi { status }` sufficient; avoids touching shared crate |
| Local `notified_403` HashSet | Simplest deduplication, no AppState changes |
| Clear 403 on Ok | Re-notifies if access is denied again after restoration |
