# Codebase Concerns

**Analysis Date:** 2026-03-18

## Tech Debt

**Monolithic `main.rs` (2167 lines):**
- Issue: `crates/carminedesktop-app/src/main.rs` contains CLI arg parsing, Tauri setup, all mount lifecycle functions, delta sync loop, headless mode, signal handling, crash recovery, deep link handling, offline pin/unpin, and config helpers — all in one file.
- Files: `crates/carminedesktop-app/src/main.rs`
- Impact: Hard to navigate, high merge conflict risk, cognitive overload when modifying any part. Functions like `start_mount`, `start_mount_common`, `start_all_mounts`, `stop_mount`, `stop_all_mounts`, `start_delta_sync`, `run_crash_recovery`, `graceful_shutdown`, `run_headless` all live together.
- Fix approach: Extract into focused modules: `mount.rs` (start/stop/lifecycle), `sync.rs` (delta sync loop), `deep_link.rs`, `headless.rs`, `recovery.rs`. Keep `main.rs` as orchestrator only.

**Tuple-based type aliases for complex state:**
- Issue: `MountCacheEntry` and `SyncSnapshotRow` are opaque tuples with 5 and 8 elements respectively, accessed by destructuring position (e.g., `(c, i, obs, _, _)`). No named fields.
- Files: `crates/carminedesktop-app/src/main.rs` (lines 90–109)
- Impact: Easy to mix up positional elements. Every callsite must destructure in the correct order. Adding a new field requires updating every destructure site.
- Fix approach: Replace with named structs.

**Duplicated `find_child_by_name` logic:**
- Issue: `CoreOps::find_child` (sync, `rt.block_on`) and `commands::find_child_by_name` (async, `.await`) implement identical three-tier lookup (memory → SQLite → Graph API) but with different calling conventions. Divergence risk if one is updated without the other.
- Files: `crates/carminedesktop-vfs/src/core_ops.rs` (lines 686–760), `crates/carminedesktop-app/src/commands.rs` (lines 1417–1491)
- Impact: Bug fixes or behavior changes must be applied in two places. Both have case-sensitivity handling via `#[cfg]` blocks that must stay synchronized.
- Fix approach: Extract a shared async function in `carminedesktop-vfs` that `CoreOps::find_child` calls via `rt.block_on`.

**Headless mode does not support Windows:**
- Issue: The `run_headless` function body is cfg-gated with `#[cfg(not(target_os = "windows"))]`, leaving a no-op stub on Windows. This is noted in a comment but not tracked.
- Files: `crates/carminedesktop-app/src/main.rs` (lines 1714–1717)
- Impact: `--headless` flag silently does nothing on Windows. Users may expect headless/service mode to work.
- Fix approach: Implement WinFsp headless mounting or clearly error with an informative message.

**Headless mode lacks sync processor:**
- Issue: Headless FUSE mounts pass `None` for `sync_handle` (line 1904), meaning uploads go through the slower inline `flush_inode` path with no async batching, debouncing, or retry backoff.
- Files: `crates/carminedesktop-app/src/main.rs` (line 1904)
- Impact: Worse upload performance and no exponential backoff retry in headless mode compared to desktop mode.
- Fix approach: Wire up `spawn_sync_processor` in headless mode the same way desktop mode does.

**No log rotation or size cap:**
- Issue: `tracing_appender::rolling::daily` creates daily log files in `{data_dir}/carminedesktop/logs/` with no maximum number of files or size limit.
- Files: `crates/carminedesktop-app/src/main.rs` (line 444)
- Impact: Over weeks/months, log directory grows unbounded. On systems with small home partitions, this can cause disk pressure.
- Fix approach: Use `tracing-appender` with a max-files roller, or implement periodic cleanup of old log files.

## Known Bugs

**No explicit bugs found in code comments or issue trackers.** The codebase has no `TODO`, `FIXME`, `HACK`, or `BUG` comments — CI likely enforces this as a convention.

## Security Considerations

**Encrypted token fallback relies on predictable machine password:**
- Risk: When the OS keychain is unavailable, tokens are encrypted with AES-256-GCM using a key derived from `carminedesktop-fallback-{USER}@{config_dir}:{machine_id}`. The machine_id (`/etc/machine-id`, registry `MachineGuid`, or `ioreg` UUID) and username/config_dir are all readable by any local process.
- Files: `crates/carminedesktop-auth/src/storage.rs` (lines 229–238)
- Current mitigation: Argon2id with 64KB memory cost; file permissions 0600 on Unix. Keychain is always attempted first.
- Recommendations: Document that the encrypted file fallback is a defense-in-depth measure, not a substitute for OS keychain. Consider prompting the user to fix keychain issues rather than silently falling back.

**OAuth callback listener binds to all localhost connections:**
- Risk: The PKCE callback server binds to `127.0.0.1:{port}` with port 0 (random). A local attacker could potentially race to connect to the listener, though the code only reads the first connection and the auth code is single-use.
- Files: `crates/carminedesktop-auth/src/oauth.rs` (line 62)
- Current mitigation: PKCE challenge/verifier, single-use auth code, 120-second timeout.
- Recommendations: This is standard for native OAuth2 PKCE flows. Risk is acceptable.

**`unsafe` blocks for `set_var` on Windows:**
- Risk: `std::env::set_var` is called in `preflight_checks` to prepend WinFsp bin to PATH. Since Rust 2024 edition, `set_var` is `unsafe` because it's not thread-safe. The comment claims "called in main() before any threads are spawned" which is correct.
- Files: `crates/carminedesktop-app/src/main.rs` (line 386)
- Current mitigation: Called early in `main()` before Tokio runtime or Tauri starts.
- Recommendations: Verify this invariant is maintained. Consider alternative approaches like passing the path to `Command::new()` directly.

**`unsafe` libc calls for uid/gid:**
- Risk: `libc::getuid()` and `libc::getgid()` are unsafe FFI calls but are well-defined and safe in practice.
- Files: `crates/carminedesktop-vfs/src/fuse_fs.rs` (lines 97–98)
- Current mitigation: Standard POSIX calls. No risk.
- Recommendations: None needed.

## Performance Bottlenecks

**`OpenFileTable::find_by_ino` and `get_content_size_by_ino` are O(n) scans:**
- Problem: These methods iterate over the entire `DashMap` to find entries matching an inode number. With many open files, this degrades linearly.
- Files: `crates/carminedesktop-vfs/src/core_ops.rs` (lines 287–312)
- Cause: The primary key is file handle (`u64`), but lookups by inode require scanning all entries.
- Improvement path: Add a secondary index `DashMap<u64, Vec<u64>>` mapping inode → file handles, or use `DashMap<u64, OpenFile>` keyed by inode with a separate handle → inode lookup.

**Memory cache eviction scans all entries:**
- Problem: `MemoryCache::maybe_evict()` collects all 10,000+ entries into a Vec, sorts them, then removes the oldest. This is called on every `insert`.
- Files: `crates/carminedesktop-cache/src/memory.rs` (lines 137–154)
- Cause: `DashMap` doesn't support ordered iteration. LRU eviction requires sorting all entries.
- Improvement path: Only trigger eviction check when size exceeds threshold (already done), but consider using a proper LRU data structure (e.g., `lru` crate) or batch eviction less frequently.

**`rt.block_on()` in FUSE callbacks blocks the FUSE thread pool:**
- Problem: Every FUSE filesystem operation (`open`, `read`, `write`, `flush`, `readdir`, etc.) calls `rt.block_on()` to bridge sync FUSE callbacks to async Graph API calls. This blocks a FUSE worker thread for the duration of every network request.
- Files: `crates/carminedesktop-vfs/src/core_ops.rs` (throughout — `rt.block_on()` used in `find_child`, `list_children`, `read_content`, `open_file`, `flush_handle`, etc.)
- Cause: FUSE library (`fuser`) requires sync trait method implementations. The Graph API is async.
- Improvement path: This is an inherent architectural constraint (documented in `AGENTS.md`). Mitigations include aggressive caching, streaming downloads, and the sync processor for uploads. No simple fix exists.

**`write_to_buffer` reads full writeback content on every write:**
- Problem: `CoreOps::write_to_buffer` reads the entire file from writeback/disk cache, extends it at the write offset, then writes the entire content back. For large files with small writes, this is extremely wasteful.
- Files: `crates/carminedesktop-vfs/src/core_ops.rs` (lines 975–1003)
- Cause: The writeback buffer stores complete file content, not sparse updates.
- Improvement path: The open file table's in-memory buffer (`write_handle`) avoids this for files opened via `open_file`. `write_to_buffer` is the fallback path for writes without an open handle. Consider if this path is still needed or can be removed.

**Disk cache eviction queries and iterates all entries:**
- Problem: `evict_if_needed` queries all `cache_entries` from SQLite sorted by `last_access`, iterates them sequentially, and deletes files one by one.
- Files: `crates/carminedesktop-cache/src/disk.rs` (lines 265–338)
- Cause: LRU eviction requires full table scan. Each evicted entry incurs a filesystem `remove_file` and a SQLite DELETE.
- Improvement path: Run eviction in a background task with a batch approach. Consider limiting eviction to N entries per cycle to avoid blocking.

## Fragile Areas

**AppState mutex lock ordering:**
- Files: `crates/carminedesktop-app/src/main.rs` (AppState struct, lines 235–255)
- Why fragile: `AppState` contains 7+ `Mutex` fields (`user_config`, `effective_config`, `mount_caches`, `mounts`, `sync_cancel`, `active_sign_in`, `account_id`, `ipc_server`). Many functions lock multiple mutexes in sequence (e.g., `start_delta_sync` locks `mount_caches` then `effective_config` on line 1519–1520). No formal lock ordering is documented.
- Safe modification: Always acquire locks in the same order. Never hold one `AppState` mutex while calling a function that acquires another. Extract values from locks into local variables before acquiring the next lock (the code mostly does this already).
- Test coverage: No unit tests for concurrent access patterns to `AppState`.

**Pervasive `.lock().unwrap()` pattern (56 occurrences):**
- Files: Throughout `crates/carminedesktop-app/src/main.rs`, `crates/carminedesktop-cache/src/sqlite.rs`, `crates/carminedesktop-auth/src/manager.rs`, `crates/carminedesktop-vfs/src/fuse_fs.rs`
- Why fragile: If any code path panics while holding a lock, the mutex becomes poisoned and all subsequent `.lock().unwrap()` calls panic, cascading the failure. Particularly risky in `SqliteStore` where every method uses `.lock().unwrap()`.
- Safe modification: In `commands.rs`, many lock calls correctly use `.map_err(|e| e.to_string())?` — this pattern is safer. Apply it consistently. For internal code paths (FUSE callbacks), panicking on poisoned mutex is acceptable as it indicates a prior panic that already corrupted state.
- Test coverage: No tests simulate poisoned mutex scenarios.

**FUSE `WRITEBACK_CACHE` interaction with metadata freshness:**
- Files: `crates/carminedesktop-vfs/src/core_ops.rs` (lines 1077–1128), `crates/carminedesktop-vfs/src/fuse_fs.rs` (lines 233–246)
- Why fragile: With `FUSE_WRITEBACK_CACHE` enabled, the Linux kernel caches file sizes and ignores `getattr` updates. The code works around this by: (1) refreshing metadata from the server on every `open_file`, (2) calling `inval_inode` via the notifier to force kernel cache invalidation, (3) returning TTL=0 for files with open handles. Missing any of these steps causes stale-size bugs where reads are truncated.
- Safe modification: Always invalidate the kernel cache when metadata changes. Never remove the `open_file` metadata refresh. Test with large files that change size between opens.
- Test coverage: Integration tests in `crates/carminedesktop-vfs/tests/fuse_integration.rs` cover basic FUSE operations but not the `WRITEBACK_CACHE` interaction specifically.

**Conflict detection timing window:**
- Files: `crates/carminedesktop-vfs/src/sync_processor.rs` (lines 532–578), `crates/carminedesktop-vfs/src/core_ops.rs` (lines 574–584)
- Why fragile: Conflict detection works by comparing the cached eTag with the server eTag via a separate `get_item` call, then uploading. Between the check and the upload, another client could modify the file (TOCTOU race). The `If-Match` header provides server-side protection for normal uploads, but the conflict copy upload uses `None` for `if_match`, meaning a conflict-of-a-conflict could silently overwrite.
- Safe modification: The `If-Match` header on the primary upload path catches most races. The conflict copy upload is best-effort — the worst case is two conflict copies rather than data loss.
- Test coverage: `crates/carminedesktop-vfs/tests/sync_processor_tests.rs` and `crates/carminedesktop-app/tests/integration_tests.rs` cover conflict detection scenarios.

**Delta sync snapshot holds two locks simultaneously:**
- Files: `crates/carminedesktop-app/src/main.rs` (lines 1519–1520)
- Why fragile: `start_delta_sync` acquires `mount_caches` then `effective_config` in the same scope to build the snapshot. If any other code path acquires them in the opposite order, deadlock occurs.
- Safe modification: Extract data from each lock separately, never nest. The current code does hold both but releases quickly. Audit all other lock sites for inverse ordering.
- Test coverage: None for deadlock scenarios.

## Scaling Limits

**Memory cache capped at 10,000 entries:**
- Current capacity: `MAX_ENTRIES = 10_000` items in `MemoryCache` (configurable TTL, default 60s).
- Files: `crates/carminedesktop-cache/src/memory.rs` (lines 8–9)
- Limit: Drives with >10,000 items will experience frequent cache eviction and SQLite/Graph fallback lookups, degrading `readdir` and `lookup` performance.
- Scaling path: Make `MAX_ENTRIES` configurable. Consider per-drive memory cache limits rather than a global cap.

**Streaming buffer capped at 256 MB:**
- Current capacity: `MAX_STREAMING_BUFFER_SIZE = 256 * 1024 * 1024` bytes.
- Files: `crates/carminedesktop-vfs/src/core_ops.rs` (line 50)
- Limit: Files larger than 256 MB cannot use the streaming buffer and fall through to `read_content` which loads the entire file into memory.
- Scaling path: For very large files (>256 MB), use the `read_range_direct` approach or chunked disk-based caching instead of in-memory buffers.

**Single SQLite connection behind Mutex:**
- Current capacity: One `rusqlite::Connection` per `SqliteStore`, serialized by `Mutex`.
- Files: `crates/carminedesktop-cache/src/sqlite.rs` (line 8)
- Limit: All cache metadata operations are serialized. Under heavy concurrent FUSE/VFS access, the SQLite mutex becomes a bottleneck.
- Scaling path: WAL mode helps with read concurrency but writes are still serialized. Consider a connection pool or moving hot-path lookups to the in-memory tier.

## Dependencies at Risk

**`fuser` v0.17 — FUSE library:**
- Risk: `fuser` is the only maintained Rust FUSE library. If it becomes unmaintained, migrating is non-trivial since the entire VFS layer depends on its `Filesystem` trait.
- Impact: Cannot compile FUSE backend without it.
- Migration plan: No alternative Rust FUSE library exists with comparable maturity. Would need to use raw `libfuse` FFI.

**`winfsp` v0.12 + `winfsp-sys` v0.12 — WinFsp Rust bindings:**
- Risk: Relatively niche crate with a small maintenance team. The WinFsp user-mode driver itself is well-maintained, but the Rust bindings may lag.
- Impact: Windows VFS backend depends entirely on this.
- Migration plan: Fall back to `winfsp-sys` raw FFI if the safe wrapper becomes stale.

**`keyring` v3.6 — OS keychain access:**
- Risk: Cross-platform keychain access is notoriously fragile across Linux desktop environments. The `keyring` crate depends on `secret-service` D-Bus API on Linux, which requires a running keychain daemon.
- Impact: When keychain is unavailable (headless servers, minimal Linux installs, Flatpak sandboxes), falls back to encrypted file. The fallback is already implemented and well-tested.
- Migration plan: Fallback already exists. No action needed.

## Missing Critical Features

**No cache size display or management UI:**
- Problem: Users cannot see how much disk space the cache is consuming or manually manage individual cached items.
- Blocks: Users cannot diagnose disk space issues caused by the cache.

**No offline/online status indicator:**
- Problem: The VFS silently enters offline mode when network fails (`set_offline` in `core_ops.rs`). There is no UI indicator showing the user that they are working from cache.
- Blocks: Users may not realize their changes are queued locally and not yet synced.

## Test Coverage Gaps

**No tests for desktop Tauri commands:**
- What's not tested: All 25+ `#[tauri::command]` functions in `crates/carminedesktop-app/src/commands.rs` (sign_in, sign_out, add_mount, save_settings, etc.) have no unit tests. Testing requires a Tauri `AppHandle` which is hard to construct in tests.
- Files: `crates/carminedesktop-app/src/commands.rs`
- Risk: Regressions in user-facing command handlers go undetected. Config persistence bugs could corrupt user state.
- Priority: Medium — integration tests in `crates/carminedesktop-app/tests/integration_tests.rs` cover some flows end-to-end, but not individual command edge cases.

**No tests for shell integration:**
- What's not tested: `crates/carminedesktop-app/src/shell_integration.rs` (1347 lines) — Windows registry manipulation, macOS file association via `duti`, Linux context menu stubs, file handler discovery/override logic.
- Files: `crates/carminedesktop-app/src/shell_integration.rs`
- Risk: Registry/file-association changes are particularly risky — bugs can make Office files unopenable or create infinite handler loops.
- Priority: High — shell integration bugs directly impact user's system state outside the app.

**No tests for WinFsp backend:**
- What's not tested: `crates/carminedesktop-vfs/src/winfsp_fs.rs` (1168 lines) has no dedicated test file. FUSE has `fuse_integration.rs` but WinFsp has nothing equivalent.
- Files: `crates/carminedesktop-vfs/src/winfsp_fs.rs`
- Risk: Windows-specific VFS bugs go undetected until manual testing.
- Priority: Medium — `core_ops.rs` (shared logic) is well-tested, which mitigates platform-specific risk somewhat.

**No tests for tray/notification modules:**
- What's not tested: `crates/carminedesktop-app/src/tray.rs` (306 lines), `crates/carminedesktop-app/src/notify.rs`, `crates/carminedesktop-app/src/update.rs`.
- Files: `crates/carminedesktop-app/src/tray.rs`, `crates/carminedesktop-app/src/notify.rs`, `crates/carminedesktop-app/src/update.rs`
- Risk: Low — these are mostly presentation/notification wrappers. Bugs are visible and non-destructive.
- Priority: Low

**No tests for encrypted token storage round-trip (outside `#[cfg(test)]`):**
- What's not tested: The encrypted file fallback path in `storage.rs` has a single inline `#[cfg(test)]` unit test for `sanitize_account_id`. No integration test exercises the full encrypt → store → load → decrypt cycle on disk.
- Files: `crates/carminedesktop-auth/src/storage.rs`
- Risk: Argon2 parameter changes or serialization changes could silently break token loading, locking users out.
- Priority: Medium

**Graph client tests don't cover upload session resumption:**
- What's not tested: `upload_large` (chunked upload via upload session) has no test for partial failure/resumption. Only success paths are tested in `crates/carminedesktop-graph/tests/graph_tests.rs`.
- Files: `crates/carminedesktop-graph/src/client.rs` (lines 363–408), `crates/carminedesktop-graph/tests/graph_tests.rs`
- Risk: Interrupted large file uploads may fail without proper retry, and the user gets no clear feedback.
- Priority: Medium

---

*Concerns audit: 2026-03-18*
