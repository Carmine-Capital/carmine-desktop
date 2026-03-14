## Context

The VFS layer has three compounding directory performance bottlenecks.

**Linear child lookup.** `CoreOps::find_child()` retrieves the parent's children as a `Vec<u64>`, then iterates every inode, calls `lookup_item()` to load each `DriveItem` from cache, and compares `.name`. This is O(n) per lookup where n is the number of children. Every FUSE `lookup()`, `unlink()`, `rmdir()`, and `rename()` call passes through `find_child`. In a directory with 500 files, a single name lookup touches up to 500 cache entries.

**Scorched-earth invalidation.** `invalidate_parent()` removes the parent entry from memory cache (`self.cache.memory.invalidate(parent_ino)`) and deletes all SQLite children rows (`self.cache.sqlite.delete_children(parent_ino)`). It is called after every `create_file`, `unlink`, `rmdir`, and `rename`. The next operation on that directory triggers a full Graph API `list_children` network call. In batch workflows (e.g., extracting a 100-file archive), this means 100 network round-trips to re-list the same directory.

**Chatty readdir + getattr.** When `ls -l` lists a directory with 100 files, the kernel issues 1 `readdir` call followed by 100 individual `getattr` calls -- 101 FUSE round-trips between kernel and userspace. The FUSE protocol defines `readdirplus` which returns entries with attributes in one response. fuser 0.17 (already a dependency) supports `readdirplus` via the `Filesystem` trait.

## Goals / Non-Goals

**Goals:**
- O(1) child lookup by name using a HashMap in the memory cache
- Surgical parent cache updates (insert/remove one child) instead of full invalidation
- Implement `readdirplus` in the FUSE backend to return entries with attributes in a single call
- Maintain correctness: delta sync and TTL expiration still refresh stale data

**Non-Goals:**
- SQLite schema changes for the children structure (SQLite stores full `DriveItem` rows, not child lists)
- CfApi backend changes (Windows uses a different directory enumeration mechanism)
- Changing the cache eviction algorithm or thresholds
- Prefetching or background population of directory children

## Decisions

### D1: HashMap<String, u64> for children

The `CachedEntry.children` field changes from `Option<Vec<u64>>` to `Option<HashMap<String, u64>>` where the key is the child filename and the value is the child inode.

**Why HashMap over Vec:** The primary operation is "find child by name" -- a HashMap lookup is O(1) versus O(n) linear scan. The memory overhead of a HashMap versus a Vec is modest: for 100 children, a HashMap uses roughly 2-3 KB more than a Vec (100 String keys averaging 20 bytes each, plus HashMap overhead). This is negligible compared to the `DriveItem` entries themselves.

**Why not BTreeMap:** Child entries do not need sorted order. `readdir` iterates all children regardless of order. HashMap has better average-case lookup performance (O(1) vs O(log n)).

**Why not IndexMap:** IndexMap preserves insertion order, which is irrelevant for our use case. It adds an external dependency for no benefit. HashMap is in `std`.

**Why not a secondary index:** Adding a `DashMap<(u64, String), u64>` alongside the existing `Vec<u64>` would maintain two data structures that must stay in sync. The HashMap-as-children approach is simpler and self-consistent.

**Public API changes on MemoryCache:**
- `get_children(parent_inode) -> Option<HashMap<String, u64>>` (was `Option<Vec<u64>>`)
- `insert_with_children(inode, item, children: HashMap<String, u64>)` (was `Vec<u64>`)
- New: `add_child(parent_inode, name: &str, child_inode: u64)` -- inserts one child into existing HashMap
- New: `remove_child(parent_inode, name: &str)` -- removes one child from existing HashMap

### D2: Surgical invalidation replaces invalidate_parent

The current `invalidate_parent` method:
```rust
fn invalidate_parent(&self, parent_ino: u64) {
    self.cache.memory.invalidate(parent_ino);
    let _ = self.cache.sqlite.delete_children(parent_ino);
}
```
This is replaced by targeted cache mutations:

- **create_file / mkdir:** After inserting the new child's `DriveItem` into memory cache, call `self.cache.memory.add_child(parent_ino, &name, child_ino)`. If the parent's children are `None` (not yet populated), the method is a no-op -- the next readdir will populate from SQLite or Graph API.
- **unlink / rmdir:** After deleting the child, call `self.cache.memory.remove_child(parent_ino, &name)`. Same no-op behavior if children are `None`.
- **rename (same directory):** Call `self.cache.memory.remove_child(parent_ino, &old_name)` then `self.cache.memory.add_child(parent_ino, &new_name, child_ino)`.
- **rename (cross-directory move):** Call `remove_child` on source parent, `add_child` on destination parent.

**SQLite children are left alone.** The current code deletes SQLite children rows on every mutation, but this is unnecessary -- delta sync already applies server-side changes to SQLite transactionally. Local mutations (create/delete/rename) immediately hit the Graph API, so the next delta sync will see the server-side state. Removing the `delete_children` calls avoids expensive SQLite writes on every file operation.

**The `invalidate_parent` method is removed entirely.** The `cleanup_deleted_item` helper is updated to call `remove_child` instead.

**Correctness argument:** The memory cache has a 60-second TTL. Even if a surgical update leaves the children map slightly inconsistent with the server (e.g., another client creates a file between our local create and the next delta sync), the TTL will expire and the next access will re-fetch from SQLite (which delta sync keeps current) or the Graph API. This is the same consistency guarantee as today.

### D3: readdirplus implementation

Implement `Filesystem::readdirplus()` in `carminedesktopFs`. The implementation mirrors the existing `readdir()` but uses `ReplyDirectoryPlus` which accepts `FileAttr` alongside each entry:

1. Build the entries list from `self.ops.list_children(ino)` (same as `readdir`).
2. For each entry (including `.` and `..`), compute `FileAttr` via `self.item_to_attr()`.
3. Call `reply.add(ino, offset, name, &TTL, &attr, Generation(0))` for each entry.
4. When `reply.add` returns `true` (buffer full), stop and let the kernel request the next batch.

The `list_children` call already returns `(u64, DriveItem)` pairs containing all metadata needed for `FileAttr`. No additional cache lookups or API calls are required beyond what `readdir` already does.

**Why this works:** When the kernel has `readdirplus` available, it prefers it over `readdir` + per-entry `getattr`. The kernel caches the returned attributes in its dentry/inode caches, so `ls -l` on a 100-entry directory becomes 1 FUSE round-trip instead of 101. The fallback is graceful: if `readdirplus` is not available (older kernels, macOS), the kernel continues using `readdir` + `getattr`.

**Import addition:** `ReplyDirectoryPlus` must be added to the `fuser` import list in `fuse_fs.rs`.

## Risks / Trade-offs

**[HashMap memory overhead]** A `HashMap<String, u64>` storing 500 children names (average 20 bytes each) uses roughly 20 KB more than a `Vec<u64>`. With 10,000 cache entries (MAX_ENTRIES), worst case is ~200 MB if every entry is a large directory. In practice, most entries are files (no children) and directories average 10-50 children. The overhead is negligible.

**[Case sensitivity]** HashMap keys are case-sensitive. OneDrive is case-insensitive for file names. This means `find_child("README.md")` will not match a child stored as `"readme.md"`. However, the current code also compares `item.name == name` which is case-sensitive, so this is not a regression. If case-insensitive matching is needed in the future, the key could be lowercased at insertion time.

**[Stale surgical updates]** If a surgical insert happens after a TTL expiry but before the entry is evicted, the stale entry gets a new child appended to an outdated children map. The TTL check in `get_children` prevents serving this stale data -- the next caller will re-populate from SQLite or Graph API. The new `add_child`/`remove_child` methods must not reset the `inserted_at` timestamp, preserving the TTL guarantee.

**[Rename atomicity]** A cross-directory rename does `remove_child` on the source and `add_child` on the destination as two separate operations. If the process crashes between the two, the destination parent may not have the child in its HashMap. This is benign -- the child's `DriveItem` still exists in memory cache and SQLite, and the next `find_child` or `list_children` on the destination will find it. This is no worse than the current behavior (invalidation + full re-fetch).
