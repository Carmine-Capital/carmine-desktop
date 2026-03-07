## Why

When any CfApi callback (`fetch_data`, `delete`, `rename`, `dehydrate`) returns `Err`, the `cloud-filter` crate's `proxy.rs` calls `CfExecute` with a failure status and `.unwrap()` the result. If `CfExecute` itself fails — which it does when `ERROR_CLOUD_FILE_INVALID_REQUEST` (0x8007017C) is returned — the `.unwrap()` panics across the `extern "system"` FFI boundary, producing `STATUS_STACK_BUFFER_OVERRUN` and crashing the entire process. The existing `fix-cfapi-toctou-crash` change fixed `fetch_placeholders`; `fetch_data` and the ticket-passing callbacks share the same crash mechanism and are still exposed.

A secondary cause: the integration test fixture creates file placeholders with `mark_in_sync()` but zero on-disk bytes, creating a contradictory OS state that causes `CfExecute(TRANSFER_DATA)` to fail with the same HRESULT when the OS fires `fetch_data` for those files.

## What Changes

- **`fetch_data` callback**: Replace the propagate-on-error pattern with catch-log-return-Ok for all failure paths (`resolve_path` returning None, `read_range_direct` failing, `write_at` failing). A failed hydration surfaces as a read error to the calling application — not a process crash.
- **`delete`, `rename`, `dehydrate` callbacks**: Replace `ticket.pass().map_err(|_| ...)?` with a non-fatal pattern: log the error and return `Ok(())`. Side effects (Graph API calls, cache invalidation) have already occurred by the time `ticket.pass()` is called; returning `Err` only triggers the proxy crash without adding recovery value.
- **Test fixture `create_root_placeholders`**: Remove `mark_in_sync()` from file placeholders. Directories may keep it (structural, no content needed). File placeholders must be created dehydrated so the OS knows to fire `fetch_data` and will accept `CfExecute(TRANSFER_DATA)` writes.

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `virtual-filesystem`: CfApi callback error behaviour changes — callbacks must not propagate `Err` to the proxy; errors are logged and surfaced as graceful I/O failures rather than process termination.

## Impact

- `crates/cloudmount-vfs/src/cfapi.rs` — `fetch_data`, `dehydrate`, `delete`, `rename` callbacks
- `crates/cloudmount-vfs/tests/cfapi_integration.rs` — `create_root_placeholders` fixture helper
- No new dependencies
- No API surface changes — internal error-handling behaviour only
