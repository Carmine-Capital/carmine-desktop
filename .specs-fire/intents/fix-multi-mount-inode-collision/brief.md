---
id: fix-multi-mount-inode-collision
title: Fix UNIQUE constraint failure when mounting multiple drives simultaneously
status: completed
created: 2026-03-08T00:00:00Z
completed_at: 2026-03-08T10:40:13.879Z
---

# Intent: Fix UNIQUE constraint failure when mounting multiple drives simultaneously

## Goal

Fix the crash `cache error: upsert failed: UNIQUE constraint failed: items.inode` that prevents a second mount (e.g. a SharePoint library) from starting when a first mount (OneDrive) is already active.

## Users

End users who have configured both a personal OneDrive mount and one or more SharePoint library mounts in their `config.toml`.

## Problem

All mounts share a single `CacheManager` (one SQLite database `cloudmount.db`) and a single `InodeTable`. The FUSE protocol mandates that every filesystem's root directory has inode number 1 (`ROOT_INODE = 1`). When mount 1 (OneDrive) starts, it inserts `{inode=1, item_id="<onedrive_root>"}` into the shared SQLite `items` table. When mount 2 (SharePoint) starts, it tries to insert `{inode=1, item_id="<sharepoint_root>"}` — a different `item_id` but the same inode. The `ON CONFLICT(item_id)` clause in the upsert SQL does not fire (because the item_id is new), so SQLite falls through to a plain INSERT which hits the `inode INTEGER PRIMARY KEY` constraint, failing with `UNIQUE constraint failed: items.inode`.

Additionally, the shared `InodeTable` is also semantically broken for multi-mount: calling `set_root()` for mount 2 overwrites mount 1's `ROOT_INODE=1` mapping in memory, corrupting mount 1's root lookup.

## Success Criteria

- Two or more mounts (any combination of OneDrive + SharePoint) start successfully without errors
- Each mount has its own isolated inode namespace (no cross-mount inode collisions)
- Delta sync, crash recovery, `refresh_mount`, and `clear_cache` commands all work correctly across multiple active mounts
- No regressions: single-mount setups continue to work
- CI passes (zero warnings, zero test failures)

## Constraints

- Must not add new crate dependencies
- Must preserve the existing `MountHandle`/`CfMountHandle` API signatures in `cloudmount-vfs`
- The fix applies to both the desktop (Tauri) and headless code paths in `main.rs`

## Notes

Root cause confirmed by code inspection of:
- `crates/cloudmount-vfs/src/inode.rs` — `ROOT_INODE = 1` hardcoded
- `crates/cloudmount-vfs/src/mount.rs` — `upsert_item(ROOT_INODE, ...)` on mount init
- `crates/cloudmount-cache/src/sqlite.rs` — `ON CONFLICT(item_id)` does not cover `inode` PK collision
- `crates/cloudmount-app/src/main.rs` — single `cache` + `inodes` shared across all mounts in `AppState`
