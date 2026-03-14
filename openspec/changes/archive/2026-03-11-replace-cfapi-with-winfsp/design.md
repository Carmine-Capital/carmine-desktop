## Context

carminedesktop's Windows backend currently uses the Cloud Files API (CfApi) via the `cloud-filter` crate (`cfapi.rs`, ~1700 lines). CfApi has structural problems that are unfixable: hydration storms from the Windows indexer/antivirus, stale content after failed dehydration, and no recovery mechanism when placeholder updates fail with `E_HANDLE`. These are architectural properties of CfApi — once a file is hydrated, Windows owns the content and bypasses carminedesktop.

The Linux/macOS backend uses FUSE (`fuse_fs.rs`, ~605 lines), which delegates every filesystem operation to `CoreOps`. This gives carminedesktop full control over caching, content freshness, and read behavior. WinFsp provides the same model on Windows: a kernel driver routes every I/O operation to userspace callbacks.

The existing architecture already separates concerns cleanly: `CoreOps` handles all filesystem logic (cache, Graph API, writeback, conflict detection), and `fuse_fs.rs` is a thin adapter that maps FUSE trait methods to `CoreOps` calls. The WinFsp backend follows the same pattern — a thin adapter mapping `FileSystemContext` trait methods to `CoreOps`.

### Key structural differences between CfApi and WinFsp

| Aspect | CfApi (current) | WinFsp (target) |
|--------|-----------------|-----------------|
| Content control | Windows owns hydrated content | Every read goes through our callback |
| Addressing | Path-based placeholders | Path-based `FileSystemContext` |
| Delta sync | `apply_delta_placeholder_updates` with dehydration | Dirty-inode checks on next read (same as FUSE) |
| Mount appearance | Explorer sync root with status badges | Drive letter or directory mount point |
| Extra threads | Filesystem watcher + periodic timer | None (WinFsp handles all I/O through callbacks) |
| Delta observer | None (CfApi has no observer) | `WinFspDeltaObserver` marks open handles stale |

## Goals / Non-Goals

**Goals:**

- Replace the CfApi backend with a WinFsp backend that delegates all filesystem operations to `CoreOps`, matching the FUSE pattern
- Provide a `WinFspMountHandle` with the same public API shape as `MountHandle` (mount, unmount, drive_id, delta_observer)
- Implement `DeltaSyncObserver` for WinFsp so delta sync marks open handles stale (currently missing on Windows)
- Remove all CfApi-specific code paths from `main.rs`, including `apply_delta_placeholder_updates` and the Windows-specific delta sync block
- Enable headless mode on Windows (currently blocked because CfApi requires desktop session)
- Detect WinFsp driver availability at startup with actionable error messaging

**Non-Goals:**

- Async `FileSystemContext` — CoreOps methods are synchronous (they internally `block_on`). Async can be explored in a future change if profiling shows the sync bridge is a bottleneck.
- Explorer shell extension (status badges, context menus) — WinFsp mounts appear as regular filesystem volumes. Shell integration is a separate feature if wanted later.
- Automatic WinFsp installation — the installer will bundle WinFsp MSI per FLOSS exception terms, but auto-install logic is out of scope for this change. Preflight checks will detect and report.
- Supporting both CfApi and WinFsp simultaneously — this is a full replacement, not a feature flag toggle. CfApi code is removed.
- Windows Server / headless-only deployment patterns — enabling `--headless` on Windows is in scope, but server-specific concerns (running as a Windows Service) are not.

## Decisions

### D1: Sync `FileSystemContext`, not `AsyncFileSystemContext`

**Choice:** Implement the synchronous `FileSystemContext` trait, bridging to Tokio with `rt.block_on()` inside callbacks — the same pattern as FUSE.

**Why:** `CoreOps` methods are already synchronous. They internally use `rt.block_on()` to call async Graph API and cache operations. Wrapping them in another async layer adds complexity without benefit. The `winfsp` crate's `AsyncFileSystemContext` is designed for filesystems that are natively async (e.g., network passthrough). Our filesystem logic is already built around `CoreOps`' sync interface.

**Alternative considered:** `AsyncFileSystemContext` with `spawn_task` delegating to Tokio. Would require either making CoreOps async (major rearchitecture, out of scope) or double-wrapping sync calls in async blocks (pointless overhead). Rejected.

### D2: Directory mount points by default, drive letters configurable

**Choice:** Mount using directory paths (e.g., `C:\Users\<user>\Cloud\OneDrive`) by default. Users can override with a drive letter (e.g., `Z:`) in their mount config.

**Why:** Consistent with the Linux/macOS FUSE behavior where mounts go under `~/Cloud/`. The existing `MountConfig.mount_point` field already holds a directory path, so no config schema change is needed. Drive letters are limited to 26 and conflict-prone on multi-mount setups.

**Alternative considered:** Drive letters by default (more Windows-native UX). Rejected because it limits scalability for users with many SharePoint libraries and conflicts with existing config structure.

### D3: `WinFspFileContext` bridges path-based WinFsp to inode-based CoreOps

**Choice:** Define a `WinFspFileContext` struct as the `FileSystemContext::FileContext` associated type:

```rust
struct WinFspFileContext {
    ino: u64,
    fh: Option<u64>,   // CoreOps file handle (from open_file), None for directories
    is_dir: bool,
}
```

**Why:** WinFsp is path-based — `open()` receives `\folder\file.txt`. CoreOps is inode-based. The path-to-inode resolution happens in `get_security_by_name()` (called before every `open`/`create`) and in `open()` itself using `CoreOps::resolve_path()`. Once resolved, the inode is stored in the context and used for all subsequent operations (read, write, flush, close).

For files, `fh` holds the CoreOps file handle obtained from `CoreOps::open_file(ino)`. For directories, `fh` is `None` since `readdir`/`list_children` operates on the inode directly.

**Alternative considered:** Store only the path and re-resolve on every operation. Rejected because repeated path resolution is wasteful and introduces TOCTOU issues.

### D4: `WinFspDeltaObserver` marks open handles stale

**Choice:** Implement `DeltaSyncObserver` for WinFsp that calls `OpenFileTable::mark_stale_by_ino()` — similar to `FuseDeltaObserver` but without kernel cache invalidation.

**Why:** WinFsp does not cache file content in kernel memory the way FUSE writeback cache does. Every `read()` call reaches our callback, where `CoreOps::read_handle()` already checks the dirty-inode set and re-downloads content when stale. Marking handles stale is sufficient to ensure the next read returns fresh content.

FUSE's `FuseDeltaObserver` additionally calls `notifier.inval_inode()` to purge the kernel page cache and force `i_size` re-fetch. WinFsp has no equivalent need because it doesn't maintain a parallel kernel cache of file sizes/content.

**Alternative considered:** Using WinFsp's `FspFileSystemNotify` to push change notifications to Explorer. This is a potential enhancement but not required for correctness — Explorer will see updated metadata on the next directory listing since we serve from the delta-sync-updated cache.

### D5: WinFsp driver detection via registry + DLL probe

**Choice:** At startup, `preflight_checks()` replaces the CfApi version check with a WinFsp availability check:
1. Check registry key `HKLM\SOFTWARE\WinFsp\InstallDir` for the installation directory
2. Verify `winfsp-x64.dll` (or `winfsp-x86.dll`) is loadable — the `winfsp-rs` crate uses delay-load linking and will panic at runtime if the DLL is missing

If WinFsp is not found, show an error dialog directing the user to install it. The carminedesktop installer will bundle the unmodified WinFsp MSI per FLOSS exception terms and can install it silently during setup, but the application itself should degrade gracefully if the driver is missing at runtime.

**Alternative considered:** Bundling WinFsp DLL directly (static linking). Not possible — WinFsp is a kernel driver + userspace DLL pair; the driver must be installed system-wide. The winfsp-rs crate links against the DLL at load time.

### D6: `WinFspMountHandle` mirrors `MountHandle` API

**Choice:** Create `WinFspMountHandle` with the same public surface as the FUSE `MountHandle`:

```rust
pub struct WinFspMountHandle {
    host: FileSystemHost<carminedesktopWinFsp>,
    cache: Arc<CacheManager>,
    graph: Arc<GraphClient>,
    drive_id: String,
    rt: Handle,
    mountpoint: String,
    delta_observer: Arc<WinFspDeltaObserver>,
}
```

Methods: `mount()`, `unmount()`, `drive_id()`, `mountpoint()`, `delta_observer()`.

**Why:** The FUSE `MountHandle` API is already clean and well-integrated with `main.rs`. Matching it means `start_mount()` and `stop_mount()` on Windows need only minimal changes (swap type, remove CfApi-specific params like `account_name` and `display_name`). The `mount_caches` entry now stores `Some(observer)` instead of `None`.

**Alternative considered:** A platform-agnostic `MountHandle` trait. Over-engineering for two implementations. The `#[cfg]` gating already cleanly separates the two.

### D7: Remove CfApi-specific delta sync handling in main.rs

**Choice:** Remove the entire `#[cfg(target_os = "windows")]` block inside the delta sync loop that calls `apply_delta_placeholder_updates`. With WinFsp, delta sync updates the cache (memory + SQLite), the observer marks open handles stale, and the next `read()`/`get_file_info()` serves fresh data — exactly like FUSE.

**Why:** The CfApi delta sync block exists because CfApi placeholders are a separate state machine that must be explicitly updated (dehydrated, deleted) after delta sync. WinFsp has no placeholders and no separate state — it serves directly from the cache.

This also removes the dependency on `carminedesktop_cache::resolve_relative_path` and `resolve_deleted_path` in the delta sync path.

### D8: CfApi sync root cleanup on upgrade

**Choice:** On first run after upgrading from a CfApi-based version, detect and unregister any orphaned CfApi sync roots. This is a one-time migration step in `setup_after_launch()`.

**How:** Check if any previously registered sync roots exist (by checking the registry or calling `SyncRootManager::GetCurrentSyncRoots()`). If found, unregister them. Store a migration flag in the config to avoid re-running.

**Why:** Without cleanup, orphaned sync roots appear in Explorer's navigation pane as broken entries pointing to directories that are no longer CfApi-managed.

**Alternative considered:** Leave cleanup to the user. Rejected because orphaned sync roots are confusing and users won't know how to remove them.

### D9: Enable headless mode on Windows

**Choice:** Remove the early-exit block in `run_headless()` that rejects Windows. With WinFsp, headless mode works because `FileSystemHost::start()` blocks on internal threads (or can be signaled to stop), without requiring a desktop session or Explorer integration.

**Why:** CfApi required desktop mode because sync roots interact with Explorer. WinFsp mounts are pure filesystem volumes that work headlessly. This unblocks server and CI/CD use cases.

## Risks / Trade-offs

**[UX regression: no Explorer sync status badges]** → Accepted. CfApi sync roots show overlay icons and "Free up space" context menus. WinFsp mounts appear as regular volumes. Users who relied on sync status indicators will not have them. Mitigation: document the change. A future Explorer shell extension could restore this if demanded.

**[WinFsp driver not installed at runtime]** → Preflight check with actionable error message. The installer bundles WinFsp MSI. If a user installs carminedesktop manually (e.g., portable), they must install WinFsp separately. The delay-load linking in winfsp-rs means the DLL absence is caught early, not as a crash.

**[Antivirus interference with WinFsp driver]** → Some security products flag third-party filesystem drivers. This is a known issue for WinFsp (and FUSE on macOS). Mitigation: WinFsp is signed with a Microsoft-approved certificate. Document known AV exclusion requirements.

**[GPLv3 license on Windows binary]** → Accepted. The `winfsp-rs` crate is GPLv3. Static linking means the Windows binary is GPL-licensed. Source files remain MIT. This was explicitly evaluated and accepted — performance and reliability outweigh licensing cleanliness.

**[Path resolution overhead]** → WinFsp delivers paths, CoreOps uses inodes. Every `open()` requires path-to-inode resolution. `CoreOps::resolve_path()` walks the InodeTable, which is in-memory and fast. The FUSE backend avoids this because the kernel caches inode mappings. In practice, WinFsp's path-based API means slightly more InodeTable lookups than FUSE, but these are O(depth) in-memory operations — negligible compared to Graph API calls.

**[Orphaned CfApi sync roots after upgrade]** → Migration step in `setup_after_launch()` unregisters old sync roots. If migration fails (permissions, corrupt state), log a warning and continue — the orphaned entries are cosmetic, not functional.

**[In-progress `fix-windows-cfapi-local-sync` change]** → This change (15/16 tasks complete) targets CfApi code that will be entirely replaced. It should be completed and archived (or abandoned) before implementation begins, to avoid merge conflicts and wasted effort on code that will be deleted.

## Open Questions

- **WinFsp change notification for Explorer refresh**: Should `WinFspDeltaObserver` call `FspFileSystemNotify` to push directory change notifications? Without it, Explorer may not refresh file lists until the user manually navigates away and back. Needs testing to determine if this is a real UX issue or if Windows polls WinFsp volumes naturally.

- **`copy_file_range` equivalent on WinFsp**: FUSE supports `copy_file_range` for server-side copy. WinFsp has no direct equivalent. Should we silently fall back to read+write copy, or is there a WinFsp extension point?

- **Allocation size semantics**: WinFsp's `create()` and `overwrite()` pass `allocation_size`. CoreOps doesn't track allocation size (files are streamed on demand). Should we report `allocation_size = file_size` rounded up to 4KB, or always 0?
