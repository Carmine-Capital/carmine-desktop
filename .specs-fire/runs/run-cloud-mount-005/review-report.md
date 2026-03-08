# Code Review Report — run-cloud-mount-005

## Work Item: validate-mount-before-start

### Files Reviewed
- `crates/cloudmount-graph/src/client.rs` (modified)
- `crates/cloudmount-app/src/notify.rs` (modified)
- `crates/cloudmount-app/src/main.rs` (modified)
- `crates/cloudmount-graph/tests/graph_tests.rs` (modified)

---

## Summary

| Category | Auto-Fixed | Suggested | Skipped |
|----------|-----------|-----------|---------|
| Code Quality | 0 | 0 | — |
| Security | 0 | 0 | — |
| Architecture | 0 | 0 | — |
| Testing | 0 | 0 | — |

**Outcome**: No issues found. Code is clean and idiomatic.

---

## Findings

### client.rs — `check_drive_exists`
- ✅ Single attempt, no `with_retry` wrapper — correct for definitive errors
- ✅ `handle_error` reused — consistent with all other methods
- ✅ `map(|_| ())` cleanly discards the response body (status check is sufficient)

### notify.rs — `mount_not_found` / `mount_access_denied`
- ✅ Follows `send()` helper pattern exactly
- ✅ Em dash `\u{2014}` consistent with existing `update_ready` notification

### main.rs — `remove_mount_from_config`
- ✅ `#[cfg(feature = "desktop")]` gate matches `AppState` availability
- ✅ Lock span: `user_config` locked, modified, saved, effective built — then dropped before `effective_config` locked (no deadlock)
- ✅ Soft error handling (warn + return) avoids panics in callback context

### main.rs — validation block in both `start_mount` variants
- ✅ `block_in_place(|| rt.block_on(...))` — correct pattern for calling async from sync within tokio multi-threaded runtime
- ✅ Returns `Ok(())` for all classified outcomes (prevents double-notification from `start_all_mounts`)
- ✅ Both FUSE and CfApi variants are symmetric

### graph_tests.rs — 3 new tests
- ✅ `expect(1)` mock count verifies no retry occurs (validates single-attempt contract)
- ✅ Pattern matches against `cloudmount_core::Error::GraphApi { status, .. }` — consistent with existing tests
- ✅ No timing sensitivity — tests are deterministic

---

## Work Item 2: handle-orphaned-mount-in-delta-sync

### Files Reviewed
- `crates/cloudmount-app/src/notify.rs` (modified — added `mount_orphaned`)
- `crates/cloudmount-app/src/main.rs` (modified — `start_delta_sync` snapshot + match arms)

### Findings

### notify.rs — `mount_orphaned`
- ✅ Follows `send()` helper pattern, consistent message style

### main.rs — snapshot expansion
- ✅ Both `mount_caches` and `effective_config` locks released before the loop — no held-lock across await
- ✅ `unwrap_or_else(|| (drive_id.clone(), drive_id.clone()))` fallback handles edge case where drive_id has no matching mount config

### main.rs — `notified_403` HashSet
- ✅ Local to spawn closure — no shared state, no locking needed
- ✅ Cleared on `Ok(())` — correct re-notification semantics
- ✅ `HashSet::insert` returns `bool` — idiomatic deduplication pattern

### main.rs — 404 match arm
- ✅ `stop_mount` + `remove_mount_from_config` + `notify::mount_orphaned` — correct sequence
- ✅ `let _ =` on `stop_mount` — mount may already be stopped if something else removed it; soft error is correct

### Summary

| Category | Auto-Fixed | Suggested | Skipped |
|----------|-----------|-----------|---------|
| Code Quality | 0 | 0 | — |
| Security | 0 | 0 | — |
| Architecture | 0 | 0 | — |
| Testing | 0 | 0 | — |

---

## Verdict

**APPROVED** — No changes needed. All code follows brownfield conventions, passes clippy, and tests verify the acceptance criteria.
