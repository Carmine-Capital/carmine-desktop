## Why

The Windows Cloud Files API (CfApi) backend has structural problems that make it unreliable for large document libraries. CfApi hands content ownership to Windows after hydration — the indexer, antivirus, and thumbnail generators trigger hydration storms on large folder trees. When delta sync detects remote changes, dehydrating placeholders fails with `E_HANDLE` if any process holds the file, and the stale content is never retried because the cache eTag was already updated. These are architectural limitations of CfApi, not fixable bugs. WinFsp provides a FUSE-like kernel driver where every read/write goes through our code, giving us full control over caching, content freshness, and indexer behavior.

## What Changes

- **BREAKING**: Replace the CfApi Windows backend (`cfapi.rs`, ~1700 lines) with a WinFsp backend (`winfsp_fs.rs`) using the `winfsp` crate (GPLv3, `winfsp-rs` 0.12). Windows binary becomes GPL-licensed; source remains MIT.
- **BREAKING**: Windows mounts no longer appear as Cloud Files sync roots in Explorer's navigation pane. They appear as regular drive letters or directory mount points (like FUSE mounts on Linux).
- Remove `cloud-filter` crate dependency and all CfApi-specific code paths: placeholder sync, oplock-based dehydration, sync root registration, post-upload placeholder conversion, filesystem watcher thread, periodic timer thread.
- Remove CfApi-specific delta sync handling in `main.rs` (`apply_delta_placeholder_updates`). Delta sync still updates caches; the WinFsp backend serves fresh content on next read via dirty-inode checks (same as FUSE).
- Add `winfsp` and `winfsp-sys` as workspace dependencies (Windows-only, `cfg`-gated).
- WinFsp backend delegates to `CoreOps` for all filesystem operations — same pattern as `fuse_fs.rs`.
- Update Windows mount lifecycle in `main.rs`: `start_mount`/`stop_mount` create/destroy WinFsp filesystem instances instead of CfApi sync roots.
- Add `DeltaSyncObserver` implementation for WinFsp (invalidate open file handles on remote change, same as FUSE's `FuseDeltaObserver`).
- Update installer/packaging to bundle or require WinFsp driver installation.
- Include WinFsp attribution in UI About dialog: "WinFsp - Windows File System Proxy, Copyright (C) Bill Zissimopoulos" (required by FLOSS exception).

## Capabilities

### New Capabilities
- `winfsp-filesystem`: WinFsp backend implementation — `FileSystemContext` trait, mount/unmount lifecycle, read/write/readdir delegation to `CoreOps`, drive letter or directory mount point assignment, `DeltaSyncObserver` for open handle invalidation.

### Modified Capabilities
- `virtual-filesystem`: Windows mount scenarios change from CfApi sync roots to WinFsp filesystem instances. Remove CfApi-specific requirements (sync root ID, display name, icon, placeholder creation, TOCTOU-safe placeholder population, CfApi callback error handling, CfApi closed callback skip, lossless path handling). Add WinFsp-specific requirements (drive letter assignment, `FileSystemContext` trait implementation, WinFsp mount options).

### Obsoleted Capabilities
The following specs are entirely CfApi-specific and have no equivalent in WinFsp:
- `cfapi-placeholder-sync`: Placeholder dehydration/deletion after delta sync — WinFsp has no placeholders; dirty-inode checks handle freshness.
- `cfapi-local-change-watcher`: Filesystem watcher for local changes — WinFsp callbacks handle writes directly.
- `cfapi-periodic-timer`: Deferred operation timer — not needed without CfApi deferred processing model.
- `cfapi-post-upload-conversion`: Post-upload placeholder conversion — no placeholders to convert.

## Impact

- **Code**: `crates/cloudmount-vfs/src/cfapi.rs` (~1700 lines) replaced by `winfsp_fs.rs`. `crates/cloudmount-app/src/main.rs` mount lifecycle changes for Windows. CfApi-specific tests removed/replaced.
- **Dependencies**: Remove `cloud-filter` crate. Add `winfsp` (GPLv3) + `winfsp-sys` as Windows-only workspace dependencies. Add `winfsp` to build-dependencies for delay-load linking.
- **Licensing**: Windows binary distribution changes from MIT to GPLv3 due to `winfsp-rs` static linking. Source files remain MIT. Must comply with WinFsp FLOSS exception (attribution in UI/docs).
- **User experience**: Windows mounts appear as drive letters (e.g., `Z:\`) or junction points instead of Explorer cloud sync roots. No sync status badges or "Free up space" context menu. Files always show fresh content on open. No indexing storms on large libraries.
- **Installer**: Must detect and install WinFsp driver if not present. Can bundle unmodified WinFsp MSI per FLOSS exception terms.
- **Existing in-progress changes**: `fix-windows-cfapi-local-sync` (15/16 tasks complete) targets CfApi code that will be replaced. Should be completed or abandoned before this change begins.
