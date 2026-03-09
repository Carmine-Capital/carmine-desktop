## Why

Windows Cloud Files API sync root registration fails for all OneDrive for Business and SharePoint drives. The sync root ID format uses `!` as a component separator (`provider!SID!account`), but Microsoft Graph drive IDs contain `!` in the `b!` prefix (e.g., `b!-RIj2DuyvEy...`). Passing the raw drive_id as the `account_name` parameter produces a malformed 4-component sync root ID, causing `StorageFolder::GetFolderFromPathAsync` to fail with a negative max path length (`-210`). This blocks all Windows CfApi mounting.

## What Changes

- Sanitize the `account_name` passed to `build_sync_root_id()` by replacing `!` characters, matching the existing pattern used for cache DB paths (`main.rs:775`)
- Use user display name or email as `account_name` when available (the Cloud Files API spec recommends "the account name of the user on the remote"), falling back to sanitized drive_id

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `virtual-filesystem`: CfApi sync root ID construction must sanitize `account_name` to exclude `!` separator characters

## Impact

- `crates/cloudmount-vfs/src/cfapi.rs` — `build_sync_root_id` and `CfMountHandle::mount` signature
- `crates/cloudmount-app/src/main.rs` — Windows `start_mount` call site (pass sanitized account name)
- No dependency changes
- **Migration**: Users who somehow had a successful prior registration with a malformed ID would need to unregister the old sync root. Not a concern here since this is the first Windows test.
