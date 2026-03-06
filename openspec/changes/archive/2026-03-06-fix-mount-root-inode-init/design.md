## Context

The VFS layer uses a FUSE inode table where inode 1 (`ROOT_INODE`) is the reserved root of the mounted filesystem. All kernel operations on the mountpoint begin with `getattr(1)` or `readdir(1)`. These resolve through `InodeTable::get_item_id(1)`, which only returns a value if `set_root` has been called.

In tests, `inodes.set_root("root-id")` is called explicitly before mounting. In the real app, this call is absent — so inode 1 has no item ID, and every VFS operation on the mountpoint root returns `ENOENT` or an empty result. All mounts are currently non-functional.

The Graph API exposes a dedicated endpoint for fetching the root of a drive: `GET /drives/{drive-id}/root`. This returns a `DriveItem` whose `id` field is what `set_root` expects.

## Goals / Non-Goals

**Goals:**
- Fix root inode initialization so all mounts work (OneDrive + SharePoint libraries)
- Initialize the root before `spawn_mount2` / CfApi registration so no VFS operation can race against an uninitialized root
- Seed the root item into memory + SQLite caches to avoid a redundant Graph call on first `getattr`
- Propagate root resolution failure as a hard mount error (skip the mount, log, continue with others)

**Non-Goals:**
- Subfolder mounts (mounting a subdirectory within a library, not the library root) — separate change
- Changing how delta sync seeds inodes (it already works correctly for subsequent items)
- Retry logic for the root fetch — auth-level retry is already handled by the Graph client

## Decisions

### D1: Fetch root in `MountHandle::mount` (VFS layer), not in `start_mount` (app layer)

The VFS layer already owns the inode table and drive ID. Putting the root fetch here means both FUSE and CfApi paths share the same initialization, and the contract is enforced at the type level: if `MountHandle::mount` returns `Ok`, the root is guaranteed to be seeded.

Alternative considered: fetch in `start_mount` (app layer) and pass the root item in. This would make the VFS layer simpler but scatter initialization logic across callers, and would require changes to both desktop and headless paths.

### D2: Use `graph.get_item(drive_id, "root")` — no new Graph method needed

The existing `GraphClient::get_item` already accepts an item ID string. Microsoft Graph accepts the special alias `"root"` as an item ID for the drive root (`/drives/{id}/items/root`). No new method is needed.

Alternative considered: add a dedicated `get_drive_root(drive_id)` method. More explicit, but unnecessary — the alias works and keeps the surface area small.

### D3: Seed root into memory cache only at mount time; let SQLite be seeded by delta sync

Memory cache seeding ensures the first `getattr(1)` is instant. SQLite seeding at mount time would require plumbing the SQLite store into `MountHandle::mount`, which already receives `CacheManager`. Since `CacheManager` includes `sqlite`, we can seed both — and should, so the root survives a memory cache eviction between mount and first delta sync.

### D4: Mount fails hard if root cannot be resolved

If `get_item(drive_id, "root")` fails, returning an `Err` from `MountHandle::mount` is correct: the FUSE session never starts, the mount point is left as an empty directory, and the error propagates to `start_mount` which logs and skips. This matches the existing skip-on-error behavior for other mount failures.

## Risks / Trade-offs

- **Extra Graph call at mount time** → Adds ~100-500ms latency to mount startup. Acceptable: this is a one-time cost, and the alternative is a broken mount.
- **Root fetch uses auth tokens** → If tokens expired between restore and mount, the fetch fails and the mount is skipped. This is correct behavior; the auth degradation path already handles token expiry during sync.

## Migration Plan

No migration needed. The change is additive: `MountHandle::mount` gains an async pre-step, which is already handled by `rt.block_on()` (the same pattern used throughout `core_ops.rs`). Existing cached data (SQLite, disk) is unaffected.
