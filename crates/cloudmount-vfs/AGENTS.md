# cloudmount-vfs

Virtual filesystem exposing OneDrive/SharePoint as local mount. FUSE on Linux/macOS, Cloud Files API (CfApi) on Windows. All platform-gated via `#[cfg]`.

## STRUCTURE

```
src/
├── lib.rs       # Re-exports, platform-conditional module declarations
├── core_ops.rs  # CoreOps — shared business logic (cache, Graph, inode, writeback)
├── fuse_fs.rs   # CloudMountFs — thin FUSE adapter delegating to CoreOps (Linux/macOS)
├── cfapi.rs     # CloudMountCfFilter — CfApi sync filter delegating to CoreOps (Windows)
├── inode.rs     # InodeTable — bidirectional item_id ↔ inode mapping
└── mount.rs     # MountHandle — lifecycle, unmount flush, signal handling (Linux/macOS)
```

## WHERE TO LOOK

| Task | File | Notes |
|------|------|-------|
| Shared VFS logic | `core_ops.rs` | Cache lookups, Graph calls, write-back, conflict detection |
| New FUSE operation | `fuse_fs.rs` | Implement `Filesystem` trait method, delegate to `CoreOps` |
| CfApi callback | `cfapi.rs` | Implement `SyncFilter` trait method, delegate to `CoreOps` |
| Change file permissions/attrs | `fuse_fs.rs` → `item_to_attr` | Dirs=0o755, files=0o644 |
| Modify inode allocation | `inode.rs` → `allocate` | AtomicU64 counter + dual HashMap |
| Change mount options | `fuse_fs.rs` → `mount` | `MountOption::*` in config |
| Modify unmount behavior | `mount.rs` → `flush_pending` | 30s timeout for pending writes |
| Signal handling | `mount.rs` → `shutdown_on_signal` | SIGTERM + Ctrl-C, unmounts all |

## DATA FLOW

**Lookup chain** (every `lookup`/`readdir`/`read`):
1. Memory cache → hit? return
2. SQLite → hit? populate memory, return
3. Graph API (network) → populate both caches, return

**Write flow**:
1. `write()` → read existing from writeback or disk → splice data at offset → save to writeback
2. `flush()`/`fsync()` → `flush_inode()` → conflict check (eTag) → upload → update caches → remove pending

**Create flow**:
1. `create()` → assign temp `local:{nanos}` ID → empty writeback entry → update parent children
2. On flush → upload → `InodeTable::reassign` to real server ID

## CONFLICT DETECTION

In `flush_inode`, before uploading existing files:
1. Compare cached eTag with server eTag (via `graph.get_item`)
2. On mismatch → upload local copy as `{name}.conflict.{timestamp}` to same parent
3. Proceed with normal upload regardless

## CONVENTIONS

- All `Filesystem` trait methods are sync. Bridge to async via `self.rt.block_on()`.
- All `SyncFilter` trait methods are sync. Bridge to async via `self.core.rt().block_on()`.
- Reply `Errno::ENOENT` for missing items, `Errno::EIO` for server/upload errors.
- TTL for all attr/entry replies: 60 seconds (`const TTL`).
- UID/GID from current process via `libc::getuid()`/`libc::getgid()`.
- After child mutations (create, delete, rename): invalidate parent's memory cache entry.
- Mount options: `RW`, `FSName("cloudmount")`, `AutoUnmount`.

## ANTI-PATTERNS

- Do NOT make Filesystem trait methods async — `fuser` requires sync.
- Do NOT hold cache locks across `block_on` calls — deadlock risk.
- Do NOT skip conflict detection in flush — data loss risk.
- Do NOT forget `invalidate(parent_ino)` after child create/delete/rename.
- Do NOT remove writeback entry before successful upload confirmation.
