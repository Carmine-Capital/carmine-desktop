---
id: fix-windows-headless-mounts
title: Add Windows mount tracking in headless mode with diagnostic
intent: fix-cross-platform-findings
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-008
completed_at: 2026-03-08T15:05:38.736Z
---

# Work Item: Add Windows mount tracking in headless mode with diagnostic

## Description

Addresses Issue #5 (Medium severity) from the cross-platform review.

In `run_headless()`, `mount_entries` is populated in the Linux/macOS branch
when a FUSE mount succeeds, but the Windows (`#[cfg(target_os = "windows")]`)
branch at line ~1244 only emits a `tracing::warn!` and never pushes to
`mount_entries`. The vec is declared immutable on Windows (`let` not `let mut`).

Downstream code uses `mount_entries` without platform gates:
- `mount_entries.len()` — always 0 on Windows
- `mount_entries.first()` — always `None` on Windows (skips crash recovery)
- `sync_entries = mount_entries.clone()` — empty, so delta sync processes nothing

Fix: populate `mount_entries` in the Windows branch when `CfMountHandle::mount`
succeeds. The tuple type `(String, Arc<CacheManager>, Arc<InodeTable>)` is
platform-agnostic — `CacheManager` and `InodeTable` are shared types. The
`mount_entries` declaration should become `let mut` on Windows as well.

If CfApi mount tracking requires a different handle type that cannot fit the
existing tuple, emit a clear `tracing::warn!` per affected feature (crash
recovery skipped, sync skipped) so the degradation is visible in logs.

## Acceptance Criteria

- [ ] Windows headless branch populates `mount_entries` when `CfMountHandle::mount` succeeds (preferred path)
- [ ] OR: Windows branch emits explicit per-feature warnings (crash recovery, delta sync) rather than a single generic warn
- [ ] `mount_entries` is declared `let mut` on Windows if it is being populated
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings (including `unused_mut` if not populated)
- [ ] `cargo test -p cloudmount-app` passes

## Technical Notes

Key location:
- `crates/cloudmount-app/src/main.rs` — `run_headless()` at line ~1153

The `mount_entries` tuple: `(String, Arc<CacheManager>, Arc<InodeTable>)` where:
- `String` = drive_id
- `Arc<CacheManager>` = the mount's cache manager (platform-neutral)
- `Arc<InodeTable>` = inode table (platform-neutral)

The Windows `start_mount` returns a `CfMountHandle`. To populate `mount_entries`,
extract the `cache_manager` and `inode_table` from the same `AppState` that was
used to mount (they are already stored in `AppState.mounts`).

The SIGHUP handler is already `#[cfg(unix)]` so no Windows change needed there.

## Dependencies

(none)
