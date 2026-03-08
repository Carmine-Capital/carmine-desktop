# Implementation Plan: Isolate CacheManager and InodeTable per mount

**Run**: run-cloud-mount-001
**Work Item**: per-mount-cache-isolation
**Intent**: fix-multi-mount-inode-collision
**Mode**: confirm
**Date**: 2026-03-08

---

## Problem

All mounts share a single `CacheManager` (one `cloudmount.db`) and a single `InodeTable`. When mount 2 starts, it tries to insert `inode=1` into the shared SQLite `items` table where `inode=1` already exists for mount 1's root → `UNIQUE constraint failed: items.inode`.

Additionally, `InodeTable::set_root()` for mount 2 overwrites mount 1's `ROOT_INODE=1` mapping in memory.

## Approach

Remove the shared `cache: Arc<CacheManager>`, `inodes: Arc<InodeTable>`, and `drive_ids: Arc<RwLock<Vec<String>>>` from `AppState`. Replace them with a single `mount_caches: Mutex<HashMap<String, (Arc<CacheManager>, Arc<InodeTable>)>>` keyed by `drive_id`.

Each mount gets:
- Its own `CacheManager` backed by `drive-{safe_drive_id}.db` in the cache directory
- Its own `InodeTable` starting after `max_inode` from its own DB

All consumers (`start_delta_sync`, `run_crash_recovery`, `refresh_mount`, `clear_cache`, `run_headless`) are updated to iterate over per-mount instances.

**`safe_drive_id`** = `drive_id.replace('!', "_")` — handles OneDrive IDs like `b!GNkOAJ...`

---

## Files to Modify

### 1. `crates/cloudmount-app/src/main.rs`

**`AppState` struct** (desktop-only, line ~99):
- Remove: `pub cache: Arc<CacheManager>`
- Remove: `pub inodes: Arc<InodeTable>`
- Remove: `pub drive_ids: Arc<RwLock<Vec<String>>>`
- Add: `pub mount_caches: Mutex<HashMap<String, (Arc<CacheManager>, Arc<InodeTable>)>>`

**`Components` struct** (line ~132):
- Remove: `cache: Arc<CacheManager>`
- Remove: `inodes: Arc<InodeTable>`

**`init_components()`** (line ~250):
- Remove: cache dir resolution, `CacheManager::new(...)`, `max_inode`, `InodeTable::new_starting_after`
- Only return `auth` and `graph`

**`run_desktop()`** (line ~353):
- Remove: destructuring `cache`, `inodes` from `Components`
- Remove: `let drive_ids: Arc<RwLock<Vec<String>>> = ...`
- Change `AppState` construction: replace `cache`, `inodes`, `drive_ids` with `mount_caches: Mutex::new(HashMap::new())`
- Remove unused `RwLock` import if no longer needed

**`start_mount()` — FUSE variant** (line ~601):
- Resolve `effective_cache_dir` from `state.effective_config`
- Compute `safe_id = drive_id.replace('!', "_")`
- Create per-mount `CacheManager::new(effective_cache_dir.clone(), db_path, ...)`
- Create `InodeTable::new_starting_after(mount_cache.sqlite.max_inode().unwrap_or(0))`
- Pass per-mount `mount_cache.clone()` and `mount_inodes.clone()` to `MountHandle::mount()`
- Insert `(Arc<CacheManager>, Arc<InodeTable>)` into `state.mount_caches` keyed by `drive_id`
- Remove: `state.drive_ids.write().unwrap().push(...)`

**`start_mount()` — CfApi variant** (line ~652):
- Same per-mount cache/inodes creation as FUSE variant
- Pass per-mount instances to `CfMountHandle::mount()`
- Insert into `state.mount_caches`
- Remove: `state.drive_ids.write().unwrap().push(...)`

**`stop_mount()`** (line ~697):
- Remove: `state.drive_ids.write().unwrap().retain(...)`
- Add: `state.mount_caches.lock().unwrap().remove(&drive_id);`

**`start_delta_sync()`** (line ~744):
- Replace: `let cache = state.cache.clone()`, `let drive_ids = state.drive_ids.clone()`, `let inodes = state.inodes.clone()`
- With: snapshot `Vec<(String, Arc<CacheManager>, Arc<InodeTable>)>` from `state.mount_caches`
- Snapshot is taken inside the loop (re-snapshot on each iteration to pick up newly added/removed mounts)
- Each iteration creates per-drive `inode_allocator` closure

**`run_crash_recovery()`** (line ~807):
- Replace: single `let cache = state.cache.clone()`
- With: snapshot all caches from `state.mount_caches`, iterate over each

**`run_headless()`** (line ~892):
- Remove: destructuring `cache`, `inodes` from `init_components()`
- In the mount loop: create per-mount `CacheManager` + `InodeTable` inline
- Collect into `Vec<(String, Arc<CacheManager>, Arc<InodeTable>)>` (indexed by drive_id)
- Update crash recovery to iterate all per-mount caches
- Update delta sync to use the per-mount vec
- Update SIGHUP handler to iterate all per-mount caches for pending write flush

**Imports**:
- Remove `use std::sync::RwLock` (if only used by `drive_ids`)
- Keep all others

### 2. `crates/cloudmount-app/src/commands.rs`

**`refresh_mount`** (line ~482):
- Remove: `let inodes = state.inodes.clone()`
- Replace: `run_delta_sync(&state.graph, &state.cache, ...)`
- With: look up `(cache, inodes)` from `state.mount_caches` by `drive_id`, use per-mount pair

**`clear_cache`** (line ~507):
- Remove: `state.cache.clear().await`
- Replace: snapshot all caches from `state.mount_caches`, call `cache.clear().await` on each

---

## Files to Create

None. This is a targeted refactor of existing files.

---

## Tests

No new test files. Existing tests in `crates/cloudmount-app/tests/integration_tests.rs` and other crates are unaffected (they test `CacheManager` independently).

**Validation gates**:
1. `cargo build --all-targets` — zero warnings (`RUSTFLAGS=-Dwarnings`)
2. `cargo clippy --all-targets --all-features` — zero warnings
3. `cargo test --all-targets` — all tests pass

---

## Key Decisions

| Decision | Rationale |
|----------|-----------|
| `HashMap<String, (Arc<CacheManager>, Arc<InodeTable>)>` keyed by `drive_id` | drive_id is the natural key for per-mount isolation |
| Re-snapshot `mount_caches` on each delta sync loop iteration | Picks up mounts added/removed while sync is running |
| Per-mount DB name: `drive-{safe_id}.db` | Unique per drive, safe filename on all platforms |
| Remove `drive_ids` field entirely | Superseded by `mount_caches.keys()` |
| Create per-mount cache in `start_mount` not `init_components` | Cache dir config may not be known at Components init time |

---

Approve plan? [Y/n/edit]
