---
phase: 01-winfsp-offline-pin-fix
plan: 02
subsystem: cache
tags: [memory-cache, eviction, sqlite, offline-pin, dashmap]

# Dependency graph
requires:
  - phase: none
    provides: N/A — first plan touching cache internals
provides:
  - Memory cache eviction protection for pinned items (TTL bypass + LRU skip)
  - SQLite metadata population during pin_folder recursive download
  - CacheManager::new accepts drive_id for eviction filter wiring
affects: [01-winfsp-offline-pin-fix, offline-access, cache-behavior]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Memory eviction filter via Arc<dyn Fn(&DriveItem) -> bool> matching DiskCache pattern"
    - "Temporary inode counter (AtomicU64 from 1_000_000) for SQLite population during pin"

key-files:
  created:
    - crates/carminedesktop-cache/tests/test_memory_eviction_protection.rs
  modified:
    - crates/carminedesktop-cache/src/memory.rs
    - crates/carminedesktop-cache/src/manager.rs
    - crates/carminedesktop-cache/src/offline.rs
    - crates/carminedesktop-app/src/main.rs

key-decisions:
  - "Eviction filter takes &DriveItem (not inode) — enables CacheManager to bridge inode-keyed memory cache to item_id-keyed PinStore"
  - "CacheManager::new gains drive_id param — each mount has its own CacheManager, drive_id already available at call sites"
  - "Temporary inodes start at 1_000_000 to avoid collisions with real VFS inodes (start from 2); ON CONFLICT(item_id) preserves existing rows"

patterns-established:
  - "Memory cache eviction filter: same RwLock<Option<Arc<dyn Fn>>> pattern as DiskCache"
  - "TTL bypass for protected entries: refresh inserted_at instead of removing"

requirements-completed: [BUG-01]

# Metrics
duration: 8min
completed: 2026-03-18
---

# Phase 1 Plan 2: Memory Cache Eviction Protection + SQLite Metadata Population Summary

**Pinned items now survive memory cache TTL expiry and LRU eviction; recursive_download populates SQLite with full directory tree metadata during pin for complete offline directory listings**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-18T09:45:35Z
- **Completed:** 2026-03-18T09:54:13Z
- **Tasks:** 2
- **Files modified:** 13 (3 source + 1 new test + 9 test call site updates)

## Accomplishments
- Pinned items in memory cache are never evicted by LRU pressure and survive TTL expiry
- Memory cache eviction filter wired through CacheManager to PinStore::is_protected, mirroring existing DiskCache pattern
- recursive_download now populates SQLite metadata for every item in the pinned folder tree during pin
- Root folder metadata persisted before spawning background download task
- 5 new tests covering eviction protection, TTL bypass, and normal behavior without filter

## Task Commits

Each task was committed atomically:

1. **Task 1: Add eviction protection to MemoryCache** — TDD
   - RED: `90b1565` (test: add failing tests for memory cache eviction protection)
   - GREEN: `7a0ca2b` (feat: add memory cache eviction protection for pinned items)
2. **Task 2: Populate SQLite metadata during recursive_download** — `a4a3255` (feat)

## Files Created/Modified
- `crates/carminedesktop-cache/src/memory.rs` — Added eviction_filter field, set_eviction_filter method, TTL bypass in get/get_children, skip protected in maybe_evict
- `crates/carminedesktop-cache/src/manager.rs` — Added drive_id param to CacheManager::new, wired memory cache eviction filter via PinStore::is_protected
- `crates/carminedesktop-cache/src/offline.rs` — Modified recursive_download to call sqlite.upsert_item for each child, added temp inode counter, persist root folder metadata in pin_folder
- `crates/carminedesktop-app/src/main.rs` — Updated 2 CacheManager::new call sites with drive_id argument
- `crates/carminedesktop-cache/tests/test_memory_eviction_protection.rs` — New: 5 tests for eviction filter behavior
- `crates/carminedesktop-*/tests/*.rs` — Updated CacheManager::new call sites with drive_id argument (9 test files)

## Decisions Made
- Used `&DriveItem` as eviction filter argument (not inode u64) so the filter closure in CacheManager can access `item.id` to call `PinStore::is_protected(drive_id, item_id)` without needing to look up the entry separately
- Added `drive_id: String` parameter to `CacheManager::new` since memory cache needs it for the eviction filter and each mount already has its own CacheManager instance
- Temporary inodes for SQLite population start at 1,000,000 to avoid collisions with VFS inodes; ON CONFLICT(item_id) DO UPDATE in upsert_item means pre-existing entries keep their real inodes

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 1 Plan 2 complete — both memory cache protection and SQLite metadata population are in place
- Combined with Plan 1 (VFS-path timeout), all three root causes of the offline pin hang are addressed
- Phase 1 is complete; ready for Phase 2 (Observability Infrastructure)

## Self-Check: PASSED

All key files verified on disk. All 3 task commits verified in git history.

---
*Phase: 01-winfsp-offline-pin-fix*
*Completed: 2026-03-18*
