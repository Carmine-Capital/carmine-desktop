## Why

`fetch_data` in `cfapi.rs` returns `Ok(())` on every error path (path resolution failure, download failure, empty content) without calling any CfExecute operation. Windows then waits the full 60-second timeout before returning error 426 ("cloud operation timed out") to the reading process, causing `cfapi_hydrate_file_on_read` and `cfapi_edit_and_sync_file` CI tests to hang and fail.

## What Changes

- Return `Err(CloudErrorKind::Unsuccessful)` from all error paths in `fetch_data` so the `cloud-filter` proxy immediately calls `Write::fail`, signaling failure to Windows instead of leaving it waiting.
- Use `request.file_blob()` to extract the item ID directly in `fetch_data`, replacing the `resolve_path` network call in the hot hydration path — making hydration faster and eliminating one class of resolution failures.
- Verify `Write::fail` is safe to call before any `write_at` invocation (no prior partial transfer written).

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `virtual-filesystem`: The CfApi `fetch_data` callback error contract changes — errors now signal failure immediately to Windows (via `Write::fail`) rather than silently returning success and waiting for timeout.

## Impact

- `crates/cloudmount-vfs/src/cfapi.rs` — `fetch_data` method rewritten
- No API surface changes; behavior change is internal to the Windows CfApi path only
- `crates/cloudmount-vfs/tests/cfapi_integration.rs` — `cfapi_hydrate_file_on_read` and `cfapi_edit_and_sync_file` tests must pass after fix
- No new dependencies required
