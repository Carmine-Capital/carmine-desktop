---
phase: 02-observability-infrastructure
plan: 01
subsystem: core
tags: [serde, observability, cache-stats, pin-health, tauri-ipc]

# Dependency graph
requires:
  - phase: 01-winfsp-offline-pin-fix
    provides: CacheManager with pin_store, DiskCache with eviction filter, items/cache_entries SQLite schema
provides:
  - ObsEvent enum (5 variants) for real-time event bus
  - Tauri command response structs (DashboardStatus, DashboardError, ActivityEntry, CacheStatsResponse, PinHealthInfo, WritebackEntry, UploadQueueInfo)
  - CacheManagerStats internal stats struct
  - CacheManager::stats() method for aggregate cache metrics
  - DiskCache::entry_count() accessor
  - MemoryCache::len() and is_empty() accessors
  - PinStore::health() with recursive CTE for subtree file counting
affects: [02-02-PLAN, 02-03-PLAN, 02-04-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Serde tagged union with per-field rename for enum variant fields"
    - "Recursive CTE for subtree file counting across items/cache_entries tables"
    - "CacheManagerStats as internal non-serialized aggregate type"

key-files:
  created:
    - crates/carminedesktop-core/tests/observability_types_tests.rs
    - crates/carminedesktop-cache/tests/cache_stats_tests.rs
  modified:
    - crates/carminedesktop-core/src/types.rs
    - crates/carminedesktop-core/src/lib.rs
    - crates/carminedesktop-cache/src/disk.rs
    - crates/carminedesktop-cache/src/memory.rs
    - crates/carminedesktop-cache/src/pin_store.rs
    - crates/carminedesktop-cache/src/manager.rs

key-decisions:
  - "Per-field serde rename on ObsEvent variants instead of container-level rename_all (serde rename_all on tagged enums only renames variant names, not inner fields)"
  - "PinStore::health() uses recursive CTE joining items and cache_entries tables via the same SQLite database"
  - "stale_pins parameter is a HashSet passed by caller, not computed by PinStore::health() itself"

patterns-established:
  - "ObsEvent tagged union: #[serde(tag = 'type', rename_all = 'camelCase')] with per-field #[serde(rename)] for inner fields"
  - "Cache stat accessors: simple pub fn wrappers around DashMap::len() and SQLite COUNT queries"

requirements-completed: []

# Metrics
duration: 6min
completed: 2026-03-18
---

# Phase 2 Plan 01: Core Observability Types & Cache Stats Summary

**ObsEvent tagged enum with 5 variants, 7 Tauri response structs with camelCase serialization matching UI-SPEC, and cache stat methods (CacheManager::stats, DiskCache::entry_count, MemoryCache::len, PinStore::health with recursive CTE)**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-18T12:26:58Z
- **Completed:** 2026-03-18T12:33:52Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- ObsEvent enum with Error, Activity, SyncStateChanged, OnlineStateChanged, AuthStateChanged variants -- all serialize with correct JSON discriminator and camelCase field names
- All Tauri command response structs (DashboardStatus, DashboardError, ActivityEntry, CacheStatsResponse, PinHealthInfo, WritebackEntry, UploadQueueInfo) defined with camelCase serialization matching UI-SPEC IPC data shape contract
- CacheManager::stats() returns aggregate CacheManagerStats with memory count, disk usage, max size, dirty inode count
- PinStore::health() computes per-pin file count breakdown using recursive CTE across items and cache_entries tables

## Task Commits

Each task was committed atomically (TDD: test -> feat):

1. **Task 1: Define ObsEvent enum and response structs** - `d94e1ce` (test) + `9c77a83` (feat)
2. **Task 2: Add cache stat methods** - `8c6c4c5` (test) + `c0a4e64` (feat)

## Files Created/Modified
- `crates/carminedesktop-core/src/types.rs` - ObsEvent enum, DashboardStatus, DriveStatus, UploadQueueInfo, DashboardError, ActivityEntry, CacheStatsResponse, PinHealthInfo, WritebackEntry, CacheManagerStats
- `crates/carminedesktop-core/src/lib.rs` - Re-exports for all new types
- `crates/carminedesktop-core/tests/observability_types_tests.rs` - 10 tests verifying JSON serialization matches UI-SPEC contract
- `crates/carminedesktop-cache/src/disk.rs` - Added entry_count() accessor
- `crates/carminedesktop-cache/src/memory.rs` - Added len() and is_empty() accessors
- `crates/carminedesktop-cache/src/pin_store.rs` - Added health() method with recursive CTE
- `crates/carminedesktop-cache/src/manager.rs` - Added stats() method returning CacheManagerStats
- `crates/carminedesktop-cache/tests/cache_stats_tests.rs` - 7 tests for cache stat methods and pin health

## Decisions Made
- Used per-field `#[serde(rename = "...")]` on ObsEvent variant fields because serde's container-level `rename_all` on tagged enums only renames variant names, not inner field names
- PinStore::health() uses `is_folder = 0` (actual schema) instead of `folder_child_count IS NULL` (plan's assumption based on outdated interface spec)
- stale_pins is passed as a `HashSet<(String, String)>` parameter to health() rather than computed internally, keeping PinStore network-free

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed serde rename strategy for ObsEvent variant fields**
- **Found during:** Task 1 (ObsEvent serialization tests)
- **Issue:** Plan specified `#[serde(tag = "type", rename_all = "camelCase")]` on the enum, but serde's `rename_all` on enums only renames variant names (the tag value), not fields within variants. Fields serialized with snake_case names (e.g., `drive_id` instead of `driveId`).
- **Fix:** Added explicit `#[serde(rename = "driveId")]` etc. on each field that needs camelCase renaming
- **Files modified:** crates/carminedesktop-core/src/types.rs
- **Verification:** All 10 serialization tests pass confirming correct JSON field names
- **Committed in:** 9c77a83

**2. [Rule 1 - Bug] Adapted PinStore::health() SQL for actual schema**
- **Found during:** Task 2 (PinStore health implementation)
- **Issue:** Plan's SQL used `folder_child_count IS NULL` to identify files, but actual items table uses `is_folder INTEGER` column
- **Fix:** Changed SQL predicate to `is_folder = 0` to match actual schema
- **Files modified:** crates/carminedesktop-cache/src/pin_store.rs
- **Verification:** All 3 PinStore health tests pass with correctly counted files
- **Committed in:** c0a4e64

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Both auto-fixes necessary for correctness. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All observability types are defined and re-exported from carminedesktop-core, ready for Plan 02 (ring buffers, event bridge, AppState extensions) to import
- CacheManager::stats() is ready for the get_cache_stats Tauri command in Plan 03
- PinStore::health() is ready for cache stats response assembly in Plan 03
- Zero clippy warnings workspace-wide confirmed

## Self-Check: PASSED

All 8 files verified present. All 4 commit hashes verified in git log.

---
*Phase: 02-observability-infrastructure*
*Completed: 2026-03-18*
