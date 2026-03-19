---
phase: 01-winfsp-offline-pin-fix
plan: 01
subsystem: vfs
tags: [timeout, tokio, offline-mode, tracing, log-rotation]

requires:
  - phase: none
    provides: none
provides:
  - VFS-path Graph API calls wrapped with 5-second timeout
  - Timeout-triggered offline mode transition
  - Log rotation capped at 31 days
affects: [02-observability-infra, 03-settings-dashboard]

tech-stack:
  added: []
  patterns: [graph_with_timeout helper for VFS callback paths, Builder API for tracing-appender]

key-files:
  created:
    - crates/carminedesktop-vfs/tests/core_ops_tests.rs
  modified:
    - crates/carminedesktop-vfs/src/core_ops.rs
    - crates/carminedesktop-app/src/main.rs

key-decisions:
  - "graph_with_timeout helper centralizes timeout + offline-flag logic for all VFS-path Graph calls"
  - "5-second VFS_GRAPH_TIMEOUT constant prevents Explorer hangs without being too aggressive"
  - "Non-VFS callers (delta sync, uploads, renames) keep existing behavior — no timeout"
  - "tracing-appender Builder API used for max_log_files(31) — caps at ~1 month of logs"

patterns-established:
  - "graph_with_timeout: wrap sync VFS Graph calls with tokio::time::timeout, set_offline on elapsed/network error"

requirements-completed: [BUG-01]

duration: 9min
completed: 2026-03-18
---

# Phase 1 Plan 1: VFS-path Timeout Wrapping & Log Rotation Summary

**5-second VFS-path timeout on all Graph API calls via graph_with_timeout helper, with offline-mode fallback and 31-day log rotation**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-18T09:45:35Z
- **Completed:** 2026-03-18T09:55:15Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- All 6 VFS callback Graph API calls (find_child, list_children, read_content, open_file, has_server_conflict, get_quota) wrapped with 5-second timeout
- Timeout and network errors trigger offline mode via set_offline(), protecting all subsequent calls
- 4 new integration tests verifying timeout behavior, offline flag, and normal operation
- Log rotation capped at 31 files (~1 month) using tracing-appender Builder API
- Zero clippy warnings, all tests pass

## Task Commits

Each task was committed atomically:

1. **Task 1: Add VFS-path timeout wrapping to CoreOps Graph API calls (TDD)**
   - `f1c45e7` (test: add failing tests for VFS-path Graph API timeout)
   - `f032df6` (feat: implement graph_with_timeout helper and wrap all VFS-path calls)
2. **Task 2: Add log rotation with max_log_files(31)** - `d6c0997` (fix: replace rolling::daily with Builder API)

## Files Created/Modified
- `crates/carminedesktop-vfs/tests/core_ops_tests.rs` - 4 integration tests for timeout behavior
- `crates/carminedesktop-vfs/src/core_ops.rs` - VFS_GRAPH_TIMEOUT constant, graph_with_timeout helper, wrapped 6 Graph API call sites
- `crates/carminedesktop-app/src/main.rs` - Replaced rolling::daily with Builder API + max_log_files(31)

## Decisions Made
- Used a centralized `graph_with_timeout` helper rather than inline timeout at each call site — reduces duplication and ensures consistent offline-flag behavior
- Chose 5 seconds as timeout — long enough for typical Graph API responses, short enough to prevent OS-level VFS callback timeouts on Windows (which default to 30-120s)
- Non-VFS callers left unwrapped — delta sync, uploads, renames, and copy operations don't need the same constraints as sync VFS callbacks
- Error logging switched from Display (`{e}`) to Debug (`{e:?}`) format for VfsError variants since VfsError doesn't implement Display

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Ready for plan 01-02 (memory cache eviction protection and SQLite metadata during pin)
- Note: plan 01-02 commits already exist in the repository (executed in a prior session)

---
*Phase: 01-winfsp-offline-pin-fix*
*Completed: 2026-03-18*
