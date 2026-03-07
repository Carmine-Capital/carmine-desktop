## Why

Directory operations have three compounding performance problems:

1. **O(n) child lookup.** `find_child()` in `core_ops.rs` iterates every child inode, loads each `DriveItem` from cache, and compares `.name`. In a directory with 500 files, a single `lookup("readme.md")` touches 500 cache entries on average. Every `ls -l`, `cd`, `open()`, `unlink()`, and `rename()` calls `find_child`.

2. **Aggressive parent invalidation.** After every create, delete, or rename, `invalidate_parent()` wipes the entire parent directory from memory cache AND deletes all SQLite children rows. The very next lookup or readdir for that directory forces a full Graph API `list_children` call over the network. In a workflow like extracting an archive (100 creates in one directory), the parent is invalidated and re-fetched from the network 100 times.

3. **Chatty readdir + getattr round-trips.** `ls -l` on a directory with 100 files issues 1 `readdir` FUSE call followed by 100 individual `getattr` calls -- 101 kernel-to-userspace round-trips. The FUSE protocol's `readdirplus` operation returns entries with attributes in a single call, reducing this to 1 round-trip. fuser 0.17 (already in use) supports `readdirplus`.

These three issues are tightly coupled: the HashMap children structure (#1) enables surgical invalidation (#2), and `readdirplus` (#3) leverages the same `list_children` data that already returns full `DriveItem` metadata.

## What Changes

- **Children data structure**: Change `CachedEntry.children` from `Option<Vec<u64>>` to `Option<HashMap<String, u64>>` (name-to-inode map). `find_child` becomes an O(1) HashMap lookup instead of an O(n) linear scan.
- **Surgical parent updates**: Replace `invalidate_parent()` calls with targeted HashMap mutations -- `insert(name, ino)` on create/mkdir, `remove(name)` on unlink/rmdir, `remove(old) + insert(new)` on rename. SQLite children are left alone (delta sync keeps them consistent).
- **readdirplus**: Implement `Filesystem::readdirplus()` in `fuse_fs.rs`, returning directory entries with full `FileAttr` in a single FUSE response. The data is already available from `CoreOps::list_children`.

## Capabilities

### New Capabilities

_(none -- these are internal performance improvements within existing capabilities)_

### Modified Capabilities

- `cache-layer`: The in-memory cache children structure changes from `Vec<u64>` to `HashMap<String, u64>`. New methods for surgical child insertion and removal. Public API signatures for `get_children` and `insert_with_children` change.
- `virtual-filesystem`: Child lookup becomes O(1). Parent invalidation is replaced by surgical cache updates. `readdirplus` is implemented for FUSE.

## Impact

- **Code**: `crates/cloudmount-cache/src/memory.rs` (CachedEntry struct, get_children, insert_with_children, new surgical methods), `crates/cloudmount-vfs/src/core_ops.rs` (find_child, list_children, create_file, mkdir, unlink, rmdir, rename, remove invalidate_parent), `crates/cloudmount-vfs/src/fuse_fs.rs` (new readdirplus impl, add ReplyDirectoryPlus import).
- **Tests**: `crates/cloudmount-cache/tests/cache_tests.rs` (update for HashMap API), `crates/cloudmount-app/tests/integration_tests.rs` (verify no regressions).
- **Dependencies**: None added. `std::collections::HashMap` is in the standard library.
- **Backwards compatibility**: Internal change only -- no user-facing API, config, or behavior changes. FUSE external behavior is identical (readdirplus is transparently negotiated by the kernel).
