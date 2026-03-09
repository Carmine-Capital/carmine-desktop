---
run: run-cloud-mount-012
work_item: fix-cache-reliability
intent: fix-comprehensive-review
generated: 2026-03-09T19:19:25Z
mode: confirm
---

# Implementation Walkthrough: Fix cache reliability

## Summary

Fixed five reliability issues in the cache crate: the `set_interval()` no-op where the spawned delta-sync task never saw interval changes, `DiskCache::new` panicking on DB errors instead of returning `Result`, non-atomic file writes that could corrupt data on crash, dual SQLite connections lacking `busy_timeout`, and TOCTOU race conditions from `path.exists()` checks before file operations.

## Structure Overview

The cache crate has four tiers (memory, SQLite, disk, writeback) managed by `CacheManager`. Two tiers open independent SQLite connections to the same database: `SqliteStore` for metadata and `DiskCache` for content tracking. The `DeltaSyncTimer` runs a background tokio task that periodically syncs changes from Microsoft Graph. All fixes target the interfaces between these components and the filesystem, hardening them against concurrent access, crashes, and I/O failures.

## Files Changed

### Created

(none)

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-cache/src/sync.rs` | Wrapped `interval_secs` in `Arc<AtomicU64>`, cloned into spawned task so `set_interval()` mutations are visible to the running loop |
| `crates/cloudmount-cache/src/disk.rs` | Changed `new()` to return `Result`; atomic writes via tmp+rename in `put()`; added `busy_timeout=5000` pragma; removed TOCTOU in `remove()`, `clear()`, eviction |
| `crates/cloudmount-cache/src/writeback.rs` | Atomic writes in `persist()`; removed TOCTOU in `remove()` and `list_pending()`; added `.tmp` file filter in `list_pending()` |
| `crates/cloudmount-cache/src/sqlite.rs` | Added `busy_timeout=5000` to pragma batch |
| `crates/cloudmount-cache/src/manager.rs` | Propagated `DiskCache::new` Result with `?` |
| `crates/cloudmount-cache/tests/cache_tests.rs` | Updated 10 `DiskCache::new` call sites to handle `Result` |
| `crates/cloudmount-app/tests/integration_tests.rs` | Updated 1 `DiskCache::new` call site to handle `Result` |

## Key Implementation Details

### 1. set_interval() fix

The `AtomicU64` was wrapped in `Arc` so both the struct field and the spawned task share the same atomic. The task reads via `.load(Ordering::Relaxed)` at the top of each loop iteration, meaning interval changes take effect on the next sleep cycle.

### 2. DiskCache::new error handling

All four `.expect()` calls replaced with `.map_err()` + `?`, mapping to `Error::Cache(String)`. The function now returns `cloudmount_core::Result<Self>`. Since `CacheManager::new` already returned `Result`, the only propagation needed was adding `?` at the call site.

### 3. Atomic write pattern

Both `DiskCache::put()` and `WriteBackBuffer::persist()` now write to `{path}.tmp` then `fs::rename` to the final path. Rename is atomic on POSIX (same filesystem) and effectively atomic on NTFS. This prevents partial/corrupt files on crash.

### 4. SQLite busy_timeout

Added `PRAGMA busy_timeout = 5000;` to both connection initialization sites. This tells SQLite to retry for up to 5 seconds when encountering a locked database, rather than immediately returning `SQLITE_BUSY`.

### 5. TOCTOU elimination

Five `path.exists()` checks removed across `disk.rs` and `writeback.rs`. Each replaced with direct operation + `NotFound` error handling. This closes race windows where the file could be created/deleted between the check and the operation.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| busy_timeout vs shared connection | `busy_timeout` pragma | Simpler than refactoring to `Arc<Mutex<Connection>>`, sufficient for the access pattern |
| .tmp file filter in list_pending | Skip files ending with `.tmp` | Atomic write pattern can leave orphans on crash; they should not be treated as pending uploads |
| Keep `unwrap_or(0)` on migration check | No change | Query failure means the table doesn't exist yet, so 0 is correct |

## Deviations from Plan

- Added `.tmp` file filtering in `writeback.rs:list_pending()` — discovered during code review that the atomic write pattern could leak `.tmp` files into crash recovery listings.

## Dependencies Added

(none)

## How to Verify

1. **Run cache tests**
   ```bash
   toolbox run -c cloudmount-build cargo test -p cloudmount-cache
   ```
   Expected: 35 tests pass

2. **Run clippy**
   ```bash
   toolbox run -c cloudmount-build cargo clippy -p cloudmount-cache --all-targets -- -D warnings
   ```
   Expected: zero warnings

3. **Manual: verify atomic writes**
   Start a mount, copy a large file into it. Kill the process mid-write. Restart. The pending directory should contain either the complete file or no file — never a partial one.

4. **Manual: verify set_interval**
   Start a mount, change the sync interval via config. The delta sync loop should pick up the new interval on its next cycle.

## Test Coverage

- Tests run: 35
- Tests pass: 35
- Coverage: All modified code paths exercised by existing tests
- Status: passing

## Developer Notes

- The `DiskCache` and `SqliteStore` still open separate connections to the same SQLite file. The `busy_timeout` pragma is sufficient for the current access pattern, but if heavy concurrent writes become an issue in the future, consider sharing a single connection pool.
- The `.tmp` file extension is used as a convention for atomic writes. If item IDs could legitimately end with `.tmp`, this filter would cause issues — but Graph API item IDs are opaque alphanumeric strings, so this is safe.
- The `writeback.rs:write()` method now calls `persist()` immediately (changed by an external hook/linter during this session), which means every write is both buffered in memory AND atomically persisted to disk. This is slightly more I/O but safer.

---
*Generated by FIRE Flow Run run-cloud-mount-012*
