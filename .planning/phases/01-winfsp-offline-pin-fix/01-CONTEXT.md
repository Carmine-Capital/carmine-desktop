# Phase 1: WinFsp Offline Pin Fix - Context

**Gathered:** 2026-03-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Resolve the deployment-blocking File Explorer hang on Windows when navigating offline-pinned mounts. Users must be able to pin a folder for offline access, lose network connectivity, and browse the pinned folder in File Explorer without any hang, crash, or 30-second stall. Reconnection must resume normal sync without remount or app restart. This is a pure bug fix phase — no new features, no UI work.

</domain>

<decisions>
## Implementation Decisions

### Failure experience
- **Immediate clean error for non-cached items** — When a user tries to open a file that isn't in disk cache while offline, return an appropriate NTSTATUS code immediately (no retry attempts). Explorer shows a standard error dialog, no retry loops or hangs.
- **Same error regardless of pin status** — No distinction between "pinned but partially downloaded" and "completely non-cached" content. If the file content isn't on disk, same immediate error. Simple, predictable behavior.
- **VFS-path timeout + fast offline detection** — Add a short timeout (3-5s) on all Graph API calls made from VFS callback paths. The first timeout sets the offline flag via `set_offline()`, protecting all subsequent calls. Explorer waits at most 3-5 seconds per path segment during the transition window.
- **Standard Explorer error messaging is sufficient** — No custom error messaging needed. Users understand "offline means limited access." The fix goal is no-crash, not richer error reporting. Custom offline error categories deferred to Phase 2/3 observability work.

### Offline directory behavior
- **Show all known items in directory listings** — When offline, return everything from SQLite/memory cache in `list_children`. User sees a familiar directory structure. Non-cached files fail on open with an immediate error. Consistent with how the official OneDrive client works offline.
- **Empty listing for directories with no cached metadata** — If a directory was never browsed and has no cached metadata, return an empty listing (no hang, no error). If this occurs inside a pinned folder, it indicates a pin completeness bug that the metadata population fix should prevent.
- **Populate full directory tree in SQLite during pin_folder** — `recursive_download()` must persist all children metadata (folders and files) into SQLite during pinning, not just download file content to disk cache. This ensures offline directory listings are complete for all paths inside a pinned folder.
- **Protect pinned items from memory cache eviction** — Add eviction protection for pinned items in memory cache, mirroring what `DiskCache` already does via `is_protected()`. Prevents the `find_child` fallthrough to Graph API that causes the hang in the first place. Both TTL expiry and LRU eviction should skip pinned entries.

### Claude's Discretion
- Specific NTSTATUS code selection for offline errors (whichever avoids Explorer retry loops)
- Graph API timeout duration within the 3-5s range
- Whether to add `tracing::debug!` instrumentation to WinFsp `FileSystemContext` methods for investigation
- Whether to bundle the `tracing-appender` log rotation fix (`max_log_files(31)`) into this phase
- Memory cache eviction protection implementation approach (filter callback vs. attribute check)
- How `pin_folder` metadata population integrates with the existing `recursive_download()` flow
- Investigation approach for confirming root cause before implementing fixes

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### VFS and offline behavior
- `crates/carminedesktop-vfs/src/core_ops.rs` — All shared VFS logic. `resolve_path` (line 624), `find_child` (line 686), `set_offline` (line 531), `is_offline` checks throughout. The three-tier cache fallthrough and `rt.block_on()` calls are the hang source.
- `crates/carminedesktop-vfs/src/winfsp_fs.rs` — WinFsp backend. `vfs_err_to_ntstatus()` (line 115), `read_directory` (line 450), `open` (line 325), `get_security_by_name` (line 279). No offline checks at this layer.

### Cache and pinning
- `crates/carminedesktop-cache/src/offline.rs` — `OfflineManager` and `recursive_download()` (line 193). Downloads content to disk cache only, does not populate SQLite metadata or memory cache.
- `crates/carminedesktop-cache/src/pin_store.rs` — `PinStore` with `is_pinned()` (line 81), `is_protected()` (line 168). SQLite-backed pin persistence. `is_protected()` walks parent chain — used by disk cache eviction filter.
- `crates/carminedesktop-cache/src/memory.rs` — `MemoryCache` with `MAX_ENTRIES = 10_000`, `maybe_evict()` (line 137). No pin awareness — all entries subject to TTL and LRU eviction.
- `crates/carminedesktop-cache/src/manager.rs` — `CacheManager` facade. Disk cache eviction filter wired at line 41-43 via `set_eviction_filter()`. Memory cache has no equivalent.

### Graph client (timeout gap)
- `crates/carminedesktop-graph/src/client.rs` — `GraphClient` uses `reqwest::Client::new()` (line 39) with no custom timeout. All requests have no connection or request timeout, relying on OS TCP stack defaults (30-120s).

### Mount lifecycle
- `crates/carminedesktop-app/src/main.rs` — `start_mount_common()` creates the `offline_flag` (line 1252). `start_delta_sync()` sets offline on network error (line 1620) and clears on success (line 1570).
- `crates/carminedesktop-app/src/commands.rs` — `list_offline_pins` (line 443), `remove_offline_pin` (line 503). Pin creation is CLI-only via `--offline-pin` flag.

### Codebase analysis
- `.planning/codebase/ARCHITECTURE.md` — Dependency graph, data flow, offline mode description
- `.planning/codebase/CONCERNS.md` — Performance bottlenecks (`OpenFileTable` O(n) scan, memory cache eviction), fragile areas (lock ordering, `.lock().unwrap()` pattern)
- `.planning/research/SUMMARY.md` — Phase 1 section with recommended fix approach, pitfall analysis, root cause hypothesis

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `PinStore::is_protected()` — Parent-chain walk already implemented for disk cache eviction. Can be reused or adapted for memory cache eviction protection.
- `DiskCache::set_eviction_filter()` — Existing pattern for plugging eviction protection into a cache tier. Memory cache needs an equivalent mechanism.
- `CoreOps::is_offline()` / `set_offline()` — `AtomicBool`-based offline flag already exists and is checked in `find_child`, `list_children`, `read_content`, `open_file`.
- `VfsError::TimedOut` → `STATUS_IO_TIMEOUT` mapping already exists in `vfs_err_to_ntstatus()`.

### Established Patterns
- **Platform abstraction via CoreOps:** Both FUSE and WinFsp delegate to `CoreOps`. Fixes in `CoreOps` apply to both platforms automatically.
- **Three-tier cache fallthrough:** `find_child` checks memory → SQLite → Graph API. The fix must ensure pinned items never reach the Graph API tier when offline.
- **`rt.block_on()` bridge:** All VFS callbacks use `rt.block_on()` to call async code. This is an architectural constraint — timeouts must be applied at the async level inside the `block_on` call.
- **`AtomicBool` with `Relaxed` ordering:** Offline flag uses relaxed atomics. Acceptable for a flag that transitions monotonically until delta sync clears it.

### Integration Points
- `CacheManager::new()` — Where disk cache eviction filter is wired. Memory cache protection should be wired here too.
- `OfflineManager::pin_folder()` / `recursive_download()` — Must be extended to populate SQLite metadata during download.
- `GraphClient` request methods — Must add timeout wrappers for VFS-path calls without breaking non-VFS callers (delta sync, auth, etc.).
- `CoreOps::find_child()` line 722 — Offline check location. Must remain the primary gate for Graph API access.

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. The fix should prioritize stability and predictability over cleverness. The goal is "it doesn't crash" for org-wide Windows deployment.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 01-winfsp-offline-pin-fix*
*Context gathered: 2026-03-18*
