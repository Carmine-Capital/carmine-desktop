# Test Report: fix-vfs-ux

**Run**: run-cloud-mount-023 | **Date**: 2026-03-09

## Test Results

- **Passed**: 144 (all)
- **Failed**: 0
- **Ignored**: 15 (FUSE integration + live Graph API — expected)

## Build & Lint

- `cargo build --all-targets` — clean
- `cargo clippy --all-targets --all-features` — zero warnings
- `cargo fmt --all -- --check` — no formatting issues

## New Tests

| Test | File | Status |
|------|------|--------|
| Conflict name preserves extension | `crates/cloudmount-vfs/tests/open_file_table_tests.rs` | PASS |
| Conflict name no extension | `crates/cloudmount-vfs/tests/open_file_table_tests.rs` | PASS |
| Conflict name multiple dots | `crates/cloudmount-vfs/tests/open_file_table_tests.rs` | PASS |
| Conflict name hidden file | `crates/cloudmount-vfs/tests/open_file_table_tests.rs` | PASS |
| get_drive with quota | `crates/cloudmount-graph/tests/graph_tests.rs` | PASS |
| get_drive without quota | `crates/cloudmount-graph/tests/graph_tests.rs` | PASS |

## Acceptance Criteria Validation

| Criterion | Status |
|-----------|--------|
| Desktop users get notification when conflict detected | Done — `VfsEvent::ConflictDetected` sent via unbounded channel, receiver task calls `notify::conflict_detected()` |
| Conflict files preserve the original file extension | Done — `conflict_name()` inserts `.conflict.{ts}` before final extension; 4 unit tests cover edge cases |
| statfs reports actual OneDrive quota (or reasonable fallback on failure) | Done — `get_quota()` with 60s cached `DriveQuota`; falls back to large values on error |
| Network errors map to specific errno values, not just EIO | Done — `PermissionDenied`/`TimedOut`/`QuotaExceeded` variants map to EACCES/ETIMEDOUT/ENOSPC |
| Rename to existing file checks for conflicts before overwriting | Done — `has_server_conflict()` checks eTag + pending writes before destination deletion |
| Server-side copy does not block FUSE thread for more than a few seconds | Done — `COPY_MAX_POLL_DURATION_SECS` reduced 300 to 10; timeout triggers `copy_file_range_fallback` |
| New Graph API quota endpoint has tests with wiremock | Done — 2 wiremock tests for `get_drive()` (with/without quota) |
