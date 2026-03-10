## Why

On Windows (CfApi backend), delta sync updates internal caches (memory, SQLite, disk) when remote file changes are detected, but never propagates those metadata changes to the NTFS Cloud Files placeholders. Windows reports file size and attributes from its own placeholder metadata — set once during `fetch_placeholders()` and never refreshed. When a file is later hydrated via `fetch_data()`, Windows requests a byte range based on the stale placeholder size, causing truncated files or size mismatches. Deleted items also remain as ghost placeholders on the filesystem.

## What Changes

- Add a callback/notification mechanism so `run_delta_sync` (in `cloudmount-cache`) can inform `cloudmount-vfs` about items that changed or were deleted, without creating a direct crate dependency from cache → vfs.
- After delta sync processes changed items (eTag mismatch), update the corresponding CfApi placeholder metadata (size, timestamps) and dehydrate the placeholder so the next access triggers a fresh `fetch_data()` with the correct file size.
- After delta sync processes deleted items, remove the placeholder file from the NTFS mount directory.
- Wire the notification bridge in the app orchestration layer (`cloudmount-app`) where both caches and mount handles are accessible.

## Capabilities

### New Capabilities
- `cfapi-placeholder-sync`: Post-delta-sync placeholder metadata update, dehydration, and deletion for Windows CfApi mounts.

### Modified Capabilities
- `cache-layer`: Delta sync must return structured change results (changed items, deleted item IDs) so the caller can propagate updates to platform-specific layers.

## Impact

- **Crates affected**: `cloudmount-cache` (sync return type), `cloudmount-vfs` (new public function for placeholder updates), `cloudmount-app` (wiring the notification bridge in `start_delta_sync`).
- **Platform scope**: Windows only (`#[cfg(target_os = "windows")]`). FUSE backends are unaffected.
- **APIs**: `run_delta_sync` signature changes from `Result<()>` to `Result<DeltaSyncResult>` containing changed/deleted item lists. New public function in `cloudmount-vfs` for applying placeholder updates given a mount path and list of changes.
- **Dependencies**: No new crate dependencies. Uses existing `cloud_filter::placeholder::{Placeholder, UpdateOptions}` APIs already imported.
- **Risk**: Low — CfApi `Placeholder::update()` and `std::fs::remove_file()` are well-understood operations. The notification bridge is a simple function call, not a new async channel.
