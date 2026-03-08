---
id: per-mount-cache-isolation
title: Isolate CacheManager and InodeTable per mount
intent: fix-multi-mount-inode-collision
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-001
completed_at: 2026-03-08T10:40:13.872Z
---

# Work Item: Isolate CacheManager and InodeTable per mount

## Description

Remove the single shared `CacheManager` and `InodeTable` from `AppState` and instead create one instance of each per mount. Each mount's `CacheManager` uses a dedicated SQLite database file named after the drive ID (`drive-{safe_drive_id}.db`). Each mount's `InodeTable` starts its counter from `max_inode + 1` read from its own database.

All consumers of the previously shared `cache` and `inodes` (delta sync, crash recovery, `refresh_mount`, `clear_cache`) are updated to iterate over the per-mount instances.

## Acceptance Criteria

- [ ] `AppState` no longer has `cache: Arc<CacheManager>` or `inodes: Arc<InodeTable>` fields
- [ ] `Components` struct and `init_components()` no longer create a shared `CacheManager` or `InodeTable`
- [ ] `start_mount` (desktop FUSE + CfApi) creates a per-mount `CacheManager` with db path `{cache_dir}/drive-{safe_drive_id}.db` and a fresh `InodeTable::new_starting_after(max_inode)`
- [ ] Per-mount `(Arc<CacheManager>, Arc<InodeTable>)` tuples are stored in a new `AppState` field `mount_caches: Mutex<HashMap<String, (Arc<CacheManager>, Arc<InodeTable>)>>` keyed by drive_id
- [ ] `stop_mount` removes the entry from `mount_caches`
- [ ] `start_delta_sync` iterates over `mount_caches` entries (drive_id → cache + inodes) instead of the removed `drive_ids` + shared cache/inodes
- [ ] `AppState.drive_ids` field is removed (superseded by `mount_caches` keys)
- [ ] `run_crash_recovery` iterates over all caches in `mount_caches`
- [ ] `commands::refresh_mount` looks up the per-mount cache + inodes from `mount_caches` by drive_id
- [ ] `commands::clear_cache` iterates over all entries in `mount_caches` to clear each cache
- [ ] Headless `run_headless()` path creates per-mount `CacheManager` + `InodeTable` for each mount and passes the correct per-mount pair to each `MountHandle::mount()` call and to the delta sync loop
- [ ] Two simultaneous mounts (OneDrive + SharePoint) start without error — the specific `UNIQUE constraint failed: items.inode` error no longer occurs
- [ ] `cargo build --all-targets` passes with zero warnings
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings
- [ ] `cargo test --all-targets` passes

## Technical Notes

**`safe_drive_id`**: Drive IDs may contain `!` and `-` characters (e.g. `b!GNkOAJ...`). Replace `!` with `_` for the filename: `drive_id.replace('!', "_")`. The resulting filename `drive-b_GNkOAJ....db` is valid on all supported platforms.

**`AppState` change summary**:
```rust
// Remove:
pub cache: Arc<CacheManager>,
pub inodes: Arc<InodeTable>,
pub drive_ids: Arc<RwLock<Vec<String>>>,

// Add:
pub mount_caches: Mutex<HashMap<String, (Arc<CacheManager>, Arc<InodeTable>)>>,
// keyed by drive_id
```

**`start_mount` per-mount init**:
```rust
let safe_id = drive_id.replace('!', "_");
let db_path = effective_cache_dir.join(format!("drive-{safe_id}.db"));
let max_cache_bytes = parse_cache_size(&effective.cache_max_size);
let metadata_ttl = Some(effective.metadata_ttl_secs);
let mount_cache = Arc::new(
    CacheManager::new(effective_cache_dir.clone(), db_path, max_cache_bytes, metadata_ttl)?
);
let max_inode = mount_cache.sqlite.max_inode().unwrap_or(0);
let mount_inodes = Arc::new(InodeTable::new_starting_after(max_inode));
```

**`start_delta_sync` update**: Replace iteration over `drive_ids` + shared `cache`/`inodes` with snapshot of `mount_caches`:
```rust
let snapshot: Vec<(String, Arc<CacheManager>, Arc<InodeTable>)> = {
    state.mount_caches.lock().unwrap()
        .iter()
        .map(|(id, (c, i))| (id.clone(), c.clone(), i.clone()))
        .collect()
};
for (drive_id, cache, inodes) in &snapshot { ... }
```

**`effective_cache_dir` in `start_mount`**: The effective cache dir is already available via `state.effective_config`. Add a helper or inline the resolution:
```rust
let effective_cache_dir = {
    let cfg = state.effective_config.lock().unwrap();
    cfg.cache_dir.as_ref().map(PathBuf::from).unwrap_or_else(cache_dir)
};
```

**Headless path**: The headless `run_headless()` builds per-mount cache/inodes inline before each `MountHandle::mount()` call and collects them into a `Vec<(String, Arc<CacheManager>, Arc<InodeTable>)>` for the delta sync loop.

## Dependencies

(none)
