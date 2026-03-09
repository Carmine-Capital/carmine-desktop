---
id: fix-cache-reliability
title: Fix cache reliability (set_interval, DiskCache panic, atomic writes)
intent: fix-comprehensive-review
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-012
completed_at: 2026-03-09T19:19:25.110Z
---

# Work Item: Fix cache reliability (set_interval, DiskCache panic, atomic writes)

## Description

Fix reliability issues in the cache crate:

1. **set_interval() no-op** (`sync.rs:28-31`): The spawned delta-sync task captures `interval_secs` (u64) by value instead of reading from the `AtomicU64`. Fix: clone the `Arc<AtomicU64>` into the spawned task; read via `interval.load(Ordering::Relaxed)` each iteration.

2. **DiskCache::new panics** (`disk.rs:16-44`): Uses `.expect()` for Connection::open, pragma, and table creation. If DB is locked/corrupted/unwritable, entire process crashes. Fix: return `Result<Self, Error>` and propagate to caller.

3. **Non-atomic file writes** (`disk.rs:109`, `writeback.rs:64`): `fs::write(&path, content)` can leave corrupt files on crash. Fix: write to `path.with_extension("tmp")`, then `fs::rename` to final path. Rename is atomic on Linux/macOS (POSIX) and effectively atomic on Windows (NTFS MOVEFILE_REPLACE_EXISTING).

4. **Dual SQLite connections** (`manager.rs:29-31`): `SqliteStore` and `DiskCache` each open independent connections to the same DB. Under heavy load, concurrent writes can cause `SQLITE_BUSY`. Fix: add `busy_timeout(5000)` to both connections, or share a single `Arc<Mutex<Connection>>`.

5. **TOCTOU on path.exists()** (various): `path.exists()` checks before file operations have race windows. Minor severity. Fix: use `create_dir_all` without pre-check, handle errors from actual operations.

## Acceptance Criteria

- [ ] `set_interval()` changes are visible to the running delta-sync loop
- [ ] `DiskCache::new` returns `Result` — callers handle the error gracefully
- [ ] All disk cache writes use write-to-temp-then-rename pattern
- [ ] SQLite connections have `busy_timeout(5000)` set
- [ ] No `path.exists()` checks before `create_dir_all` or `fs::write`
- [ ] Existing cache tests pass

## Technical Notes

For `DiskCache::new` → `Result`, propagate change through `CacheManager::new` in `manager.rs` and all callers in `main.rs`.

The `busy_timeout` pragma is the simplest fix for dual connections: `conn.execute_batch("PRAGMA busy_timeout = 5000;")?;`.

## Dependencies

(none)
