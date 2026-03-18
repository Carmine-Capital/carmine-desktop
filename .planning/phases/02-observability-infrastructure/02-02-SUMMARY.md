---
phase: 02-observability-infrastructure
plan: 02
subsystem: app
tags: [ring-buffer, broadcast-channel, event-bridge, tauri-emit, appstate, sync-handle]

# Dependency graph
requires:
  - phase: 02-observability-infrastructure
    plan: 01
    provides: ObsEvent enum (5 variants), DashboardError, ActivityEntry, CacheManagerStats types
provides:
  - ErrorAccumulator ring buffer (100 cap) for dashboard error history
  - ActivityBuffer ring buffer (500 cap) for activity feed
  - spawn_event_bridge() function routing ObsEvent to Tauri emit + ring buffers
  - AppState.obs_tx broadcast channel (cap 256) for system-wide ObsEvent publishing
  - AppState.error_ring, activity_ring for command-queryable error/activity state
  - AppState.last_synced HashMap for per-drive sync timestamp tracking
  - AppState.stale_pins HashSet for delta-sync-driven pin staleness
  - MountCacheEntry with SyncHandle (6th tuple element) for dashboard SyncMetrics access
affects: [02-03-PLAN, 02-04-PLAN]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "VecDeque ring buffer with manual capacity enforcement and oldest-first drain"
    - "tokio::sync::broadcast for multi-consumer ObsEvent fanout"
    - "Event bridge pattern: single async task subscribing to broadcast and routing to emit + ring buffers"
    - "SyncHandle clone stored in MountCacheEntry for dashboard metrics access"

key-files:
  created:
    - crates/carminedesktop-app/src/observability.rs
  modified:
    - crates/carminedesktop-app/src/main.rs
    - crates/carminedesktop-app/src/commands.rs

key-decisions:
  - "Inline #[cfg(test)] tests for ring buffers since app crate is binary-only (no lib.rs for external test imports)"
  - "Lock ordering documented on AppState: user_config > effective_config > mount_caches > mounts > sync_cancel > active_sign_in > account_id > error_ring > activity_ring > last_synced > stale_pins"
  - "SyncHandle cloned before passing to MountHandle::mount, stored as 6th MountCacheEntry element"
  - "Event bridge spawned in Tauri .setup() closure after state is managed"
  - "Broadcast channel capacity 256 (generous for expected event rates, handles Lagged gracefully)"

patterns-established:
  - "observability module pattern: ring buffer structs + event bridge function in dedicated module"
  - "MountCacheEntry 6-tuple: (CacheManager, InodeTable, DeltaSyncObserver, OfflineManager, offline_flag, SyncHandle)"

requirements-completed: []

# Metrics
duration: 10min
completed: 2026-03-18
---

# Phase 2 Plan 02: Ring Buffers, Event Bridge & AppState Extensions Summary

**ErrorAccumulator (100-cap) and ActivityBuffer (500-cap) ring buffers with broadcast event bridge routing ObsEvent to Tauri emit and ring buffers, AppState extended with obs_tx/error_ring/activity_ring/last_synced/stale_pins, MountCacheEntry gains SyncHandle**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-18T12:37:48Z
- **Completed:** 2026-03-18T12:48:36Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- ErrorAccumulator and ActivityBuffer ring buffers with VecDeque backing, oldest-first drain, and capacity enforcement
- Event bridge task subscribing to broadcast channel, routing Error events to ErrorAccumulator, Activity events to ActivityBuffer, all events to Tauri emit("obs-event")
- AppState extended with obs_tx broadcast sender, error_ring, activity_ring, last_synced per-drive timestamps, and stale_pins tracking
- MountCacheEntry expanded to 6-tuple with SyncHandle for dashboard SyncMetrics access
- 7 unit tests for ring buffer push/drain/capacity/ordering behavior
- All 12 MountCacheEntry destructuring sites updated across main.rs and commands.rs

## Task Commits

Each task was committed atomically:

1. **Task 1: Create observability.rs module with ring buffers and event bridge** - `1692a13` (feat)
2. **Task 2: Extend AppState with observability fields and SyncHandle** - `bdc9f1c` (feat)

## Files Created/Modified
- `crates/carminedesktop-app/src/observability.rs` - ErrorAccumulator, ActivityBuffer ring buffers, spawn_event_bridge function, 7 inline unit tests
- `crates/carminedesktop-app/src/main.rs` - Module registration, MountCacheEntry 6-tuple, SyncSnapshotRow 9-tuple, AppState observability fields, broadcast channel + ring buffer initialization, event bridge spawn in setup, SyncHandle clone in both FUSE and WinFsp start_mount, lock ordering comment, all tuple destructuring sites updated
- `crates/carminedesktop-app/src/commands.rs` - 5 MountCacheEntry destructuring sites updated from 5-element to 6-element patterns

## Decisions Made
- Used inline `#[cfg(test)]` tests rather than external integration test file because carminedesktop-app is a binary crate with no lib.rs, making the ring buffer structs inaccessible to external test files
- Documented canonical lock ordering on AppState to prevent deadlocks as observability fields add more Mutex-guarded state
- Cloned SyncHandle before passing to MountHandle::mount in both FUSE and WinFsp variants (SyncHandle is cheap to clone: mpsc sender + watch receiver)
- Spawned event bridge in Tauri .setup() closure (after .manage(state)) rather than in setup_after_launch, ensuring bridge is running before any mount operations begin

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- obs_tx broadcast sender is ready for Plan 03 to publish ObsEvent from delta sync loop and VFS event forwarder
- error_ring and activity_ring are ready for Plan 03's get_recent_errors and get_activity_feed Tauri commands
- last_synced and stale_pins are ready for Plan 03's get_dashboard_status and get_cache_stats commands
- MountCacheEntry SyncHandle is ready for Plan 03 to read SyncMetrics in get_dashboard_status
- Zero clippy warnings workspace-wide confirmed

## Self-Check: PASSED

All 3 files verified present. Both commit hashes verified in git log. All 10 content checks passed (structs, functions, AppState fields, SyncHandle, lock ordering).

---
*Phase: 02-observability-infrastructure*
*Completed: 2026-03-18*
