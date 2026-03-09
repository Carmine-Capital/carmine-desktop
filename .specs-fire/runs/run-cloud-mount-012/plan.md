# Implementation Plan: Fix cache reliability

**Work Item:** fix-cache-reliability
**Intent:** fix-comprehensive-review
**Mode:** confirm
**Run:** run-cloud-mount-012

---

## Approach

Five targeted fixes in the cache crate, ordered from highest to lowest impact. Each fix is isolated — no cascading redesign needed.

## Fix 1: `set_interval()` no-op (sync.rs)

**Problem:** The spawned delta-sync task captures `interval_secs: u64` by value on line 31. Calling `set_interval()` mutates the `AtomicU64` on `Self`, but the spawned task never reads it.

**Fix:** Clone `Arc`-wrapped `AtomicU64` into the spawned task. Read via `.load(Ordering::Relaxed)` each loop iteration.

- Wrap `interval_secs` field as `Arc<AtomicU64>`
- Clone the `Arc` into the spawned closure
- Replace `Duration::from_secs(interval_secs)` with `Duration::from_secs(interval.load(Ordering::Relaxed))`

## Fix 2: `DiskCache::new` panics (disk.rs)

**Problem:** Four `.expect()` calls in constructor — process crashes on locked/corrupted/unwritable DB.

**Fix:** Change signature to `fn new(...) -> cloudmount_core::Result<Self>`. Replace all `.expect()` with `?` + `Error::Cache`.

**Propagation:**
- `manager.rs:31` — add `?` to `DiskCache::new(...)` call (already returns `Result`)
- `tests/cache_tests.rs` — add `?` to all 10 test call sites
- `crates/cloudmount-app/tests/integration_tests.rs:266` — add `?`

## Fix 3: Non-atomic file writes (disk.rs, writeback.rs)

**Problem:** `fs::write(&path, content)` on crash leaves corrupt partial files.

**Fix:** Write to `path.with_extension("tmp")`, then `fs::rename` to final path.

**Files:**
- `disk.rs:109` — `DiskCache::put()` content write
- `writeback.rs:64` — `WriteBackBuffer::persist()` pending write

## Fix 4: SQLite busy_timeout (disk.rs, sqlite.rs)

**Problem:** `SqliteStore` and `DiskCache` open independent connections to the same DB. Concurrent writes → `SQLITE_BUSY`.

**Fix:** Add `PRAGMA busy_timeout = 5000;` to both connection initialization pragmas.

- `sqlite.rs:16` — append to existing `execute_batch`
- `disk.rs:17` — append to existing `execute_batch`

## Fix 5: TOCTOU path.exists() checks (disk.rs, writeback.rs)

**Problem:** `path.exists()` before file ops has race windows.

**Fix:**
- `disk.rs:133` (`remove`): Remove `if path.exists()` guard. Use `fs::remove_file` and ignore `NotFound`.
- `disk.rs:148` (`clear`): Remove `if self.base_dir.exists()` guard. Handle `NotFound` from `remove_dir_all`.
- `disk.rs:234` (eviction): Remove `if path.exists()` guard. Already ignores errors with `let _ =`.
- `writeback.rs:74` (`remove`): Remove `if path.exists()` guard. Ignore `NotFound`.
- `writeback.rs:84` (`list_pending`): Remove `if !self.pending_dir.exists()` guard. Return empty vec on `NotFound` from `read_dir`.

---

## Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-cache/src/sync.rs` | Arc<AtomicU64> shared with spawned task |
| `crates/cloudmount-cache/src/disk.rs` | Return Result, atomic writes, busy_timeout, remove TOCTOU |
| `crates/cloudmount-cache/src/writeback.rs` | Atomic writes, remove TOCTOU |
| `crates/cloudmount-cache/src/sqlite.rs` | Add busy_timeout pragma |
| `crates/cloudmount-cache/src/manager.rs` | Propagate DiskCache::new `?` |
| `crates/cloudmount-cache/tests/cache_tests.rs` | Update DiskCache::new calls to unwrap Result |
| `crates/cloudmount-app/tests/integration_tests.rs` | Update DiskCache::new call |

## Files to Create

(none)

## Tests

- All 23 existing cache tests must continue passing
- Integration test in cloudmount-app updated for new signature
- No new tests needed — existing tests validate the same behavior with the new signatures
