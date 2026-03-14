## Why

When a drive is mounted, the FUSE root inode (inode 1) is never associated with the drive root's Graph item ID, so every `getattr` and `readdir` on the mountpoint returns `ENOENT` or empty — making all mounts non-functional. The `InodeTable::set_root` method exists and is used correctly in tests, but is never called in the real mount path.

## What Changes

- At mount time, fetch the drive root item from the Graph API and call `set_root` with its item ID before the FUSE session starts
- Seed the root `DriveItem` into memory and SQLite caches so the first `getattr(1)` is served locally
- Apply this initialization in both the FUSE (`MountHandle::mount`) and CfApi (`CfMountHandle::mount`) paths

## Capabilities

### New Capabilities
- None

### Modified Capabilities
- `virtual-filesystem`: Mount initialization now requires a Graph API call to resolve the drive root item before the filesystem is exposed to the kernel. The root inode must be populated before any VFS operation is served.
- `app-lifecycle`: `start_mount` must handle the async root fetch and propagate failure (mount aborts if root cannot be resolved).

## Impact

- `crates/carminedesktop-vfs/src/mount.rs` — `MountHandle::mount` gains a Graph call before `spawn_mount2`
- `crates/carminedesktop-vfs/src/cfapi.rs` — `CfMountHandle::mount` gains the same initialization
- `crates/carminedesktop-graph/src/client.rs` — may need a `get_drive_root` helper if not already present
- `crates/carminedesktop-app/src/main.rs` — `start_mount` signature may change to propagate initialization errors
- No new dependencies required
