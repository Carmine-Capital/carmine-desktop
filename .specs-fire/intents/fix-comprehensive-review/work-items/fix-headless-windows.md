---
id: fix-headless-windows
title: Skip empty delta-sync, don't create unused dirs, parse_cache_size cleanup
intent: fix-comprehensive-review
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-021
completed_at: 2026-03-09T19:31:18.537Z
---

# Work Item: Skip empty delta-sync, don't create unused dirs, parse_cache_size cleanup

## Description

Fix headless Windows mode and related cleanup:

1. **Empty delta-sync loop** (`main.rs:1329`): Headless Windows spawns a delta-sync task that loops forever with zero entries. Fix: skip spawning the task when `mount_entries.is_empty()`.

2. **Unused mount directories** (`main.rs:1204`): Headless Windows creates mount directories via `create_dir_all` but never mounts. Fix: gate directory creation behind the platform-specific mount block.

3. **Config dir fallback** (`config.rs:370`): Falls back to `.cloudmount` if `dirs::config_dir()` returns None. Consistent with auth fix but separate location. Fix: return error.

## Acceptance Criteria

- [ ] Headless Windows does not spawn delta-sync when no mounts are active
- [ ] Mount directories only created when mounting will actually occur
- [ ] No unused background tasks running on headless Windows
- [ ] Existing tests pass

## Technical Notes

Simple conditional checks — no architectural changes needed.

## Dependencies

(none)
