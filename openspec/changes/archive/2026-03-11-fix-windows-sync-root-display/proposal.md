## Why

On Windows, every CfApi sync root is registered with the hardcoded display name `"CloudMount"` and a generic `imageres.dll,0` icon, so all mounts appear identically in File Explorer's navigation pane — indistinguishable black squares all labeled "CloudMount" — regardless of which OneDrive or SharePoint library they represent. Users cannot tell their mounts apart without clicking into each one.

## What Changes

- The `register_sync_root()` function in `cloudmount-vfs` accepts a `display_name` parameter instead of hardcoding `PROVIDER_NAME`.
- `CfMountHandle::mount()` threads the mount's user-visible name through to `register_sync_root()` as the display name.
- The sync root icon is resolved at runtime to the running application executable (`std::env::current_exe(),0`) so File Explorer shows the CloudMount app icon rather than a blank system document icon.
- Already-registered sync roots (from prior launches with the wrong display name) are re-registered on mount to apply the corrected name and icon.

## Capabilities

### New Capabilities

_(none — this is a bug fix)_

### Modified Capabilities

- `virtual-filesystem`: The Windows mount scenario gains requirements for sync root display name (must match the user-visible mount name) and icon (must reference the application executable).

## Impact

- `crates/cloudmount-vfs/src/cfapi.rs`: `register_sync_root()` signature change; always re-registers to update stale registrations.
- `crates/cloudmount-app/src/main.rs`: `start_mount()` passes display name to `CfMountHandle::mount()`.
- `crates/cloudmount-vfs/src/cfapi.rs` (`CfMountHandle::mount`): new `display_name: String` parameter.
- No API, config, or cross-platform changes; Linux/macOS are unaffected.
