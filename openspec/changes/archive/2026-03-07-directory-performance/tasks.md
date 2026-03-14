## 1. Memory cache HashMap migration (memory.rs)

- [x] 1.1 Change `CachedEntry.children` from `Option<Vec<u64>>` to `Option<HashMap<String, u64>>` in `crates/carminedesktop-cache/src/memory.rs`. Add `use std::collections::HashMap;` import.
- [x] 1.2 Update `MemoryCache::get_children()` return type from `Option<Vec<u64>>` to `Option<HashMap<String, u64>>`. Clone the HashMap instead of the Vec.
- [x] 1.3 Update `MemoryCache::insert_with_children()` parameter from `children: Vec<u64>` to `children: HashMap<String, u64>`. Store as `Some(children)` in the `CachedEntry`.
- [x] 1.4 Update `MemoryCache::insert()` to initialize `children: None` (unchanged, but verify HashMap type is consistent).

## 2. New MemoryCache surgical update methods (memory.rs)

- [x] 2.1 Add `MemoryCache::add_child(&self, parent_inode: u64, name: &str, child_inode: u64)`: get_mut the parent entry, if it exists and `children` is `Some`, insert `(name.to_string(), child_inode)` into the HashMap. If entry does not exist or children is `None`, no-op. Do NOT reset `inserted_at` (preserve TTL).
- [x] 2.2 Add `MemoryCache::remove_child(&self, parent_inode: u64, name: &str)`: get_mut the parent entry, if it exists and `children` is `Some`, remove the name key from the HashMap. If entry does not exist or children is `None`, no-op. Do NOT reset `inserted_at`.

## 3. CoreOps find_child / list_children adaptation (core_ops.rs)

- [x] 3.1 Update `CoreOps::find_child()` memory cache branch: instead of iterating `children_inodes` and comparing names, do `children_map.get(name)` to get the child inode in O(1), then call `self.lookup_item(child_inode)` to get the `DriveItem`. If the inode resolves but lookup_item returns `None`, fall through to SQLite/Graph API.
- [x] 3.2 Update `CoreOps::find_child()` Graph API fallback: when building the children map from `graph.list_children` response, construct a `HashMap<String, u64>` (mapping `item.name` to allocated inode) instead of a `Vec<u64>`. Pass this HashMap to `insert_with_children`.
- [x] 3.3 Update `CoreOps::list_children()` memory cache branch: `get_children` now returns `HashMap<String, u64>`. Iterate the HashMap values (inodes) to build the `Vec<(u64, DriveItem)>` result via `lookup_item`.
- [x] 3.4 Update `CoreOps::list_children()` Graph API fallback: after fetching items from the API, build a `HashMap<String, u64>` and call `insert_with_children` to populate the parent's children cache (currently this branch does not populate children at all -- fix this).

## 4. Surgical invalidation -- replace invalidate_parent calls (core_ops.rs)

- [x] 4.1 Update `CoreOps::create_file()`: remove the block that rebuilds the full children Vec and calls `insert_with_children`. Replace with `self.cache.memory.add_child(parent_ino, name, inode)` after inserting the new child's DriveItem.
- [x] 4.2 Update `CoreOps::mkdir()`: remove the block that gets children, pushes inode, and calls `insert_with_children`. Replace with `self.cache.memory.add_child(parent_ino, name, inode)` after inserting the new folder's DriveItem.
- [x] 4.3 Update `CoreOps::unlink()` / `cleanup_deleted_item()`: replace `self.invalidate_parent(parent_ino)` with `self.cache.memory.remove_child(parent_ino, name)`. The `cleanup_deleted_item` helper needs the child name as a parameter, or the `remove_child` call should be done in `unlink` before calling `cleanup_deleted_item`.
- [x] 4.4 Update `CoreOps::rmdir()`: replace `self.invalidate_parent(parent_ino)` with `self.cache.memory.remove_child(parent_ino, name)`.
- [x] 4.5 Update `CoreOps::rename()`: replace `self.invalidate_parent(parent_ino)` with `self.cache.memory.remove_child(parent_ino, name)`. Replace `self.invalidate_parent(new_parent_ino)` with `self.cache.memory.add_child(new_parent_ino, new_name, child_ino)`. For same-directory rename, do both remove and add on the same parent.
- [x] 4.6 Remove the `invalidate_parent` method from `CoreOps` entirely (no remaining callers).
- [x] 4.7 Remove the `self.cache.sqlite.delete_children(parent_ino)` calls that were inside `invalidate_parent`. SQLite children are now kept consistent by delta sync only.

## 5. readdirplus implementation (fuse_fs.rs)

- [x] 5.1 Add `ReplyDirectoryPlus` to the `fuser` import list in `crates/carminedesktop-vfs/src/fuse_fs.rs`.
- [x] 5.2 Implement `Filesystem::readdirplus()` on `carminedesktopFs`: same entry-building logic as `readdir()` (`.`, `..`, then `self.ops.list_children(ino)`), but for each entry compute `FileAttr` via `self.item_to_attr()` and call `reply.add(INodeNo(inode), offset, &name, &TTL, &attr, Generation(0))`. Stop when `reply.add` returns `true` (buffer full). Call `reply.ok()` at the end.
- [x] 5.3 For the `.` and `..` entries in `readdirplus`, look up the directory's own `DriveItem` via `self.ops.lookup_item(ino)` to compute their `FileAttr`. If lookup fails, use a default directory attr.

## 6. Test updates

- [x] 6.1 Update `crates/carminedesktop-cache/tests/cache_tests.rs`: all tests that call `insert_with_children` must pass a `HashMap<String, u64>` instead of `Vec<u64>`. All tests that call `get_children` must expect `HashMap<String, u64>`.
- [x] 6.2 Add tests for `MemoryCache::add_child`: verify child is inserted into existing HashMap, verify no-op when parent not in cache, verify no-op when children is `None`.
- [x] 6.3 Add tests for `MemoryCache::remove_child`: verify child is removed from existing HashMap, verify no-op when parent not in cache, verify no-op when children is `None`, verify removing non-existent name is a no-op.
- [x] 6.4 Update `crates/carminedesktop-app/tests/integration_tests.rs`: verify that `find_child` still works correctly after the HashMap migration (existing tests should pass without changes to test logic, but verify).
- [x] 6.5 Add integration test for surgical invalidation: create a file in a directory, verify the parent's children cache is NOT invalidated (i.e., a subsequent `list_children` does not trigger a Graph API call -- verify via mock server request count).

## 7. Verify

- [x] 7.1 Run `cargo test --all-targets` -- all tests pass.
- [x] 7.2 Run `cargo clippy --all-targets --all-features` -- no warnings.
- [x] 7.3 Run `cargo fmt --all -- --check` -- formatting is correct.
