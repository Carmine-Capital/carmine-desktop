# Test Report — run-cloud-mount-005

## Work Item 1: validate-mount-before-start

### Test Results

| Suite | Passed | Failed | Skipped |
|-------|--------|--------|---------|
| cloudmount-graph (all) | 24 | 0 | 0 |
| cloudmount-core | pass | 0 | 0 |
| cloudmount-auth | pass | 0 | 0 |
| cloudmount-cache | pass | 0 | 0 |

**New tests added**: 3 (in `crates/cloudmount-graph/tests/graph_tests.rs`)
- `check_drive_exists_returns_ok_on_200` ✅
- `check_drive_exists_returns_404_error` ✅
- `check_drive_exists_returns_403_error` ✅

### Acceptance Criteria Validation

- [x] `start_mount` performs `GET /drives/{drive_id}` before mounting via `check_drive_exists`
- [x] 404 response: mount removed from `user_config` via `remove_mount_from_config` + `notify::mount_not_found` sent
- [x] 403 response: mount skipped, config unchanged, `notify::mount_access_denied` sent
- [x] Transient errors: mount skipped with `tracing::warn!`, config unchanged, no notification
- [x] Validation uses a single attempt — `check_drive_exists` has no `with_retry` wrapper
- [x] Both FUSE (Linux/macOS) and CfApi (Windows) `start_mount` variants covered
- [x] `cargo clippy -p cloudmount-graph -p cloudmount-app --all-targets` passes with zero warnings

### Notes

- `--all-features` cannot be tested locally (GTK system libs not installed), but CI will cover it
- Validation returns `Ok(())` for 404/403/transient to prevent double-notification from `start_all_mounts`
- `remove_mount_from_config` rebuilds `effective_config` atomically within a single `user_config` lock span

---

## Work Item 2: handle-orphaned-mount-in-delta-sync

### Test Results

| Suite | Passed | Failed | Skipped |
|-------|--------|--------|---------|
| cloudmount-graph (all) | 24 | 0 | 0 |
| cloudmount-core | pass | 0 | 0 |
| cloudmount-auth | pass | 0 | 0 |
| cloudmount-cache | pass | 0 | 0 |

**New tests added**: 0 (changes are in `main.rs`/`notify.rs` — Tauri app context, not unit-testable in isolation)

**Linter**: `cargo clippy -p cloudmount-app --all-targets` — 0 warnings

### Acceptance Criteria Validation

- [x] `start_delta_sync` matches `Error::GraphApi { status: 404 }` per-mount
- [x] 404: `stop_mount` called, mount removed from config via `remove_mount_from_config`, `notify::mount_orphaned` sent
- [x] 403: sync continues (mount not removed), `notify::mount_access_denied` sent once via `notified_403` deduplication
- [x] Other errors: existing behavior unchanged (logged at error level)
- [x] Per-mount error handling — loop continues for remaining mounts on any error
- [x] `cargo test -p cloudmount-cache` passes
- [x] `cargo clippy` passes with zero warnings

### Notes

- No new `Error` variants added — `GraphApi { status }` carries enough info
- `notified_403` lives inside the spawn closure (no AppState changes needed)
- `notified_403` cleared on `Ok(())` so access-restored drives can be notified again on future 403s
