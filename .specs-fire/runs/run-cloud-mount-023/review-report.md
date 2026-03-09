# Code Review Report: fix-vfs-ux

**Run**: run-cloud-mount-023 | **Date**: 2026-03-09

## Summary

| Category | Auto-fixed | Suggestions | Skipped |
|----------|-----------|-------------|---------|
| Code Quality | 0 | 0 | 0 |
| Security | 0 | 0 | 0 |
| Architecture | 0 | 0 | 0 |

## Files Reviewed

### `crates/cloudmount-vfs/src/core_ops.rs`

**VfsEvent channel**: Uses `tokio::sync::mpsc::UnboundedSender` — appropriate choice since VFS callbacks are synchronous (`rt.block_on`) and cannot tolerate bounded-channel backpressure. Events are rare (only on conflict), so unbounded is safe.

**`conflict_name()` function**: Pure function with `rfind('.')` for last-extension detection. Handles no-extension files, multiple dots (e.g., `archive.tar.gz` splits at `.gz`), and hidden files correctly. Well-tested.

**Quota cache**: `std::sync::Mutex<Option<(Instant, DriveQuota)>>` — correct choice over `tokio::sync::Mutex` since the lock is held only for a cache check (no `.await` inside). 60s TTL matches `DEFAULT_METADATA_TTL_SECS`.

**`from_core_error()` mapping**: Inspects `cloudmount_core::Error` variants to produce specific `VfsError` values. Graph 403 maps to `PermissionDenied`, 404 to `NotFound`, network errors to `TimedOut`, quota exceeded to `QuotaExceeded`. Unmapped errors fall through to `IoError`. Clean pattern.

**Rename conflict check**: `has_server_conflict()` compares eTag and checks pending writes before allowing destination deletion. Downloads and uploads the existing file as a conflict copy if mismatch detected — preserves data safety.

**Copy timeout**: `COPY_MAX_POLL_DURATION_SECS` reduced from 300 to 10. Replaced `std::thread::sleep` with `tokio::time::sleep` — correct for async context inside `rt.block_on`. On timeout, returns `VfsError::TimedOut` which `copy_file_range` catches and falls back to read/write copy.

### `crates/cloudmount-vfs/src/fuse_fs.rs`

New errno mappings follow the existing pattern: `PermissionDenied` to `EACCES`, `TimedOut` to `ETIMEDOUT`, `QuotaExceeded` to `ENOSPC`. `statfs` now calls `CoreOps::get_quota()` and computes block counts from real quota values.

### `crates/cloudmount-graph/src/client.rs`

`get_drive()` follows the established `get_json`/`with_retry` pattern. Returns `Drive` with optional quota fields, matching Graph API v1.0 response shape.

### `crates/cloudmount-app/src/main.rs`

Event channel created during mount setup, receiver task spawned with `tokio::spawn`. Task logs at debug level and dispatches to `notify::conflict_detected()`. Clean teardown — sender drops when mounts stop, receiver task exits on channel close.

### `crates/cloudmount-app/src/notify.rs`

`conflict_detected()` follows the existing `send()` helper pattern. Notification title and body include the conflicting filename for user context.

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| Unbounded channel for VFS events | Conflicts are rare; bounded channel could deadlock sync FUSE callbacks |
| `std::sync::Mutex` for quota cache | No async operations while lock is held; avoids tokio Mutex overhead |
| 10s copy timeout with fallback | Balances server-side copy optimization against FUSE thread blocking |
| `rfind('.')` for extension detection | Simple, correct for common cases; `archive.tar.gz` preserves `.gz` which is the primary OS association |

## Findings

No issues found. All changes follow existing patterns, clippy and formatting checks pass, and the 6 new tests provide adequate coverage for the new helper functions.
