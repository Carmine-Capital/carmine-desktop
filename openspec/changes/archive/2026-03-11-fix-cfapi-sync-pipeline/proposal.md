## Why

On Windows, local file operations in the CloudMount sync root (create, modify, copy-in, internal copy, rename) never synchronize to OneDrive. The sync-pending icon appears in Explorer but uploads never happen, making the Windows mount effectively read-only for all local mutations. Five independent bugs in the CfApi sync pipeline compound to produce this failure.

## What Changes

- Fix the unmodified guard in `stage_writeback_from_disk()` so `local:*` items (files with no server copy) are never skipped as "unmodified"
- Add a dedicated `ReadDirectoryChangesW` filesystem watcher thread that monitors creation, deletion, rename, size, and last-write changes (the cloud-filter crate's built-in watcher only watches attribute changes)
- Add a 500ms periodic timer thread that processes safe-save timeouts, deferred ingest retries, and deferred timeout cleanup independently of CfApi callbacks
- Convert successfully uploaded `local:*` files to CfApi placeholders so future operations route through the optimized callback pipeline
- Always call `ticket.pass()` in the rename callback even on failure, preventing local/remote name divergence

## Capabilities

### New Capabilities
- `cfapi-local-change-watcher`: Dedicated filesystem watcher for the Windows sync root that detects file creation, deletion, rename, size changes, and last-write changes via `ReadDirectoryChangesW` with appropriate notify flags, debounces events, and routes them to `ingest_local_change()`
- `cfapi-periodic-timer`: Background timer thread that periodically processes safe-save timeouts, deferred ingest retries, and deferred timeout cleanup without depending on CfApi callback activity
- `cfapi-post-upload-conversion`: After successful upload of a `local:*` file, converts the file to a CfApi placeholder with the server item blob and marks it in-sync

### Modified Capabilities
- `cfapi-placeholder-sync`: The writeback pipeline requirement changes to never skip `local:*` items in the unmodified check, and the rename callback requirement changes to always acknowledge via `ticket.pass()` regardless of outcome

## Impact

- **Code**: `crates/cloudmount-vfs/src/cfapi.rs` (watcher, timer, unmod fix, ticket fix, placeholder conversion), `crates/cloudmount-vfs/src/core_ops.rs` (placeholder conversion hook after flush_inode)
- **Tests**: New integration tests in `crates/cloudmount-vfs/tests/cfapi_integration.rs` for copy-in, internal copy, and rename sync scenarios
- **Dependencies**: No new external dependencies; uses `windows-sys` already available transitively
- **Platforms**: All changes gated with `#[cfg(target_os = "windows")]`; no impact on FUSE (Linux/macOS)
- **Supersedes**: The existing `fix-windows-cfapi-local-sync` change, which partially addressed this problem
