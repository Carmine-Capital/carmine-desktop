# Codebase Concerns

**Analysis Date:** 2026-03-10

## Large/Complex Modules

**`cloudmount-vfs/src/core_ops.rs` (1893 lines):**
- Issue: Monolithic VFS business logic module handling cache lookups, Graph API calls, inode management, file reads/writes, conflict detection, and streaming buffers in a single file
- Files: `crates/cloudmount-vfs/src/core_ops.rs`
- Impact: Makes code difficult to test in isolation, harder to reason about mutation points, increases cognitive load for debugging platform-specific issues (FUSE vs CfApi both delegate here)
- Fix approach: Extract distinct concerns into focused modules (e.g., `conflict_resolution.rs`, `streaming.rs`, `file_operations.rs`) with clear responsibility boundaries

**`cloudmount-app/src/main.rs` (1674 lines):**
- Issue: Entry point, configuration loading, mount orchestration, delta sync coordination, auto-start setup, signal handling, and Tauri desktop setup all in one file
- Files: `crates/cloudmount-app/src/main.rs`
- Impact: Difficult to test mount logic independently, hard to understand initialization order dependencies, platform-specific code (#[cfg]) interspersed throughout
- Fix approach: Extract mount lifecycle management into `mount_orchestration.rs`, signal handling into `shutdown.rs`, Tauri setup into `desktop_integration.rs`

## Mutex Lock Unwraps (Panic Risk)

**SQLite connection access patterns:**
- Issue: All `SqliteStore` methods call `.lock().unwrap()` on the connection mutex (line examples: `crates/cloudmount-cache/src/sqlite.rs`)
- Files: `crates/cloudmount-cache/src/sqlite.rs`, `crates/cloudmount-auth/src/oauth.rs`, `crates/cloudmount-vfs/src/mount.rs`, `crates/cloudmount-app/src/main.rs`
- Impact: If a panic occurs while holding the lock, subsequent access will permanently deadlock (panics in Drop, IO, or signal handlers could hold locks). Rust's abort-on-panic-in-drop means process death.
- Fix approach: Implement lock poisoning detection with `Result` from `lock()` rather than `unwrap()`. Provide recovery path that resets lock on poison detection. Consider `parking_lot::Mutex` which doesn't panic-poison.

**AppState Mutex guards:**
- Issue: Widespread use of `.lock().unwrap()` on AppState fields (`user_config`, `effective_config`, `mount_caches`, `mounts`, `sync_cancel`, `active_sign_in`, `account_id`)
- Files: `crates/cloudmount-app/src/main.rs`, `crates/cloudmount-app/src/commands.rs`
- Impact: Any panic while holding AppState lock prevents further Tauri commands from executing, silently breaking UI responsiveness
- Fix approach: Return `Result` from Tauri commands when lock fails, propagate to frontend as error state. Alternatively use `parking_lot::Mutex` without panic poisoning.

## Unsafe Code Usage

**`libc::getuid()` / `libc::getgid()` calls:**
- Issue: Unsafe FFI calls to Unix libc functions without verification of contract safety
- Files: `crates/cloudmount-vfs/src/fuse_fs.rs` (lines 94–95)
- Impact: Low risk (getuid/getgid are simple syscalls), but lack of safety comments makes future reviewers uncertain
- Fix approach: Add `// SAFETY: getuid() and getgid() are always safe to call` comments explaining why these unsafe blocks are sound

**Windows version check via RtlGetVersion:**
- Issue: Unsafe transmutation of function pointer (`mem::transmute(proc)`) with minimal validation
- Files: `crates/cloudmount-app/src/main.rs` (lines 281–289, `cfapi_version_meets` function)
- Impact: If the Windows API contract changes (unlikely), transmutation could create unsound function pointer. No validation that `proc` points to a callable function.
- Fix approach: Use `windows` crate's safe wrappers (e.g., `GetVersionEx` or check Win32 API guards). Add SAFETY comments explaining why transmutation is valid.

## File Locking and Crash Recovery Gaps

**Writeback buffer orphans on ungraceful shutdown:**
- Issue: If the app crashes mid-flush (after writing to writeback buffer but before Graph API upload succeeds), files remain in `.pending/` directory. Recovery requires manual cleanup or detecting stale .pending files on restart.
- Files: `crates/cloudmount-cache/src/writeback.rs`, `crates/cloudmount-vfs/src/core_ops.rs` (flush_inode)
- Impact: Lost user edits if a .pending file is accidentally deleted, or users see orphaned files consuming disk space; app doesn't actively validate .pending consistency on launch
- Fix approach: On app startup, scan .pending directory and retry uploads for files with recent timestamps; add journaling (write intent log before writeback buffer write) to track which operations were in-progress

**No write-intent log for complex operations:**
- Issue: Conflict detection + upload sequence in `flush_inode` is not atomic: check server eTag, upload conflict copy if mismatch, then upload final file. If app crashes between steps, conflict copy might be uploaded but final upload skipped (or vice versa).
- Files: `crates/cloudmount-vfs/src/core_ops.rs` (lines 920–972, `flush_inode`)
- Impact: Incomplete uploads or partial conflict detection; user may lose understanding of which version is on the server
- Fix approach: Write operation intent (e.g., "flush inode {id}, check for conflict, expected eTag {tag}") to disk before starting, clear on success; on recovery, replay or retry operations

## Race Condition in Stale Cache Validation

**Open file metadata refresh vs concurrent delta sync:**
- Issue: `open_file()` calls `graph.get_item()` to refresh metadata before checking disk cache (line 1121, core_ops.rs). Meanwhile, `run_delta_sync` updates the same item metadata in memory cache. No synchronization between these two reads.
- Files: `crates/cloudmount-vfs/src/core_ops.rs`, `crates/cloudmount-cache/src/sync.rs`
- Impact: If delta sync updates an item's eTag while `open_file` is reading stale metadata, the freshness check may compare two different eTag values and think the file is stale when it's not (or vice versa). Memory cache entry is updated after the Graph call, creating a window where different threads see different eTag values.
- Fix approach: Acquire read lock on inode before Graph API call, hold until memory cache insert (or use RwLock on individual items in memory cache). Ensure delta sync and open_file serialize their updates to the same inode's cache entry.

## Error Propagation and Silent Failures

**Debug eprintln in production code:**
- Issue: Line 1059 in `core_ops.rs` has `eprintln!("DEBUG conflict copy upload failed: {e:?}")` left in code
- Files: `crates/cloudmount-vfs/src/core_ops.rs` (line 1059)
- Impact: Prints to console in production, confusing users; should be removed or converted to `tracing::debug!`
- Fix approach: Remove or replace with `tracing::error!` to ensure it goes through configured logging system

**Conflict copy upload failures silently preserve writeback buffer:**
- Issue: If `flush_inode` creates a conflict copy but the conflict copy upload fails, the code intentionally preserves the writeback buffer (line 1060: `// Preserve writeback buffer for crash recovery`), but returns an error. Caller may retry upload, potentially uploading the same conflict copy again.
- Files: `crates/cloudmount-vfs/src/core_ops.rs` (lines 1054–1061)
- Impact: Duplicate conflict copies on retry, confusion about which version is authoritative, unbounded writeback buffer growth on repeated failures
- Fix approach: Add idempotency key or track attempted conflict copies; limit retries with exponential backoff; after N failures, quarantine the file and notify user

**Cache layer errors not surfaced to VFS layer:**
- Issue: `find_child`, `list_children` log SQLite lookup failures as warnings but continue with Graph API fallback (lines 652–655, 715–717, core_ops.rs). If SQLite is corrupted, this silently falls back to remote data without alerting operators.
- Files: `crates/cloudmount-vfs/src/core_ops.rs`
- Impact: Cache corruption goes unnoticed; app silently bypasses corrupted cache, potentially losing freshness guarantees
- Fix approach: Expose cache error metric via tracing; if cache errors exceed threshold, emit warning to user and suggest clearing cache; add integrity check on SQLite open

## Performance Bottlenecks

**Sequential iteration for case-insensitive child lookup on Windows:**
- Issue: On Windows, `find_child` iterates all children in memory cache to find case-insensitive match (lines 630–633, core_ops.rs), then falls back to SQLite and Graph API with the same iteration. O(n) per lookup where n = child count.
- Files: `crates/cloudmount-vfs/src/core_ops.rs`
- Impact: Large directories (1000+ children) will see slow lookups; repeated child lookups in a loop become O(n²)
- Fix approach: Pre-compute case-insensitive index in memory cache (HashMap<lowercase_name, inode>); maintain in parallel with case-sensitive map. On insert/delete, update both.

**Full file download before serving reads:**
- Issue: `open_file()` for files < 4 MB (SMALL_FILE_LIMIT) eagerly downloads entire file into memory before returning (lines 1119–1149, core_ops.rs). For 4 MB files opened for small sequential reads, this wastes memory and time.
- Files: `crates/cloudmount-vfs/src/core_ops.rs`, `crates/cloudmount-graph/src/client.rs`
- Impact: User opens 4 MB file to read first 1 KB, app downloads full 4 MB, blocks for 100+ ms on slow connections
- Fix approach: For eager-load files, return immediately with streaming buffer; start download in background and block only on actual read if necessary data isn't available yet (convert to on-demand range requests for large uncached files)

**SQLite busy timeout and concurrent writes:**
- Issue: SQLite in NORMAL journaling mode (pragmas in `SqliteStore`) will serialize concurrent writes via locks. No explicit busy timeout; if two Graph operations update the same inode's SQLite record, the second will either wait or fail immediately.
- Files: `crates/cloudmount-cache/src/sqlite.rs`
- Impact: Delta sync and user operations (flush) can block each other; on high concurrency, SQLite BUSY errors may cascade
- Fix approach: Set explicit `PRAGMA busy_timeout = 5000` (5 second retry); monitor busy lock contention via tracing; consider splitting inode cache into per-drive shards to reduce lock contention

## Platform-Specific Assumptions and Gaps

**NTFS vs ext4 filename semantics not fully handled:**
- Issue: Code acknowledges Windows case-insensitivity but assumes Graph API returns all valid names (lines 618–621, core_ops.rs). NTFS allows colons in alternate data streams (`:` in filenames) which Graph API may store but which cannot be represented as filesystem paths on Windows.
- Files: `crates/cloudmount-vfs/src/core_ops.rs`, `crates/cloudmount-cache/src/writeback.rs` (sanitize_filename)
- Impact: If OneDrive contains files with colons (allowed in cloud), Windows mount will fail to list/access them; writeback sanitization is inconsistent (only sanitizes in pending directory, not in memory representation)
- Fix approach: When loading from Graph API, filter or transform unsupported names (e.g., replace `:` with `-` or block download); document to user which characters are unsupported on each platform

**CfApi version check only on app startup:**
- Issue: Windows CfApi compatibility is checked in `cfapi_version_meets()` once at startup (lines 261–297, main.rs). If user downgrades Windows after app is running, the app continues to try CfApi calls without version checks.
- Files: `crates/cloudmount-app/src/main.rs`
- Impact: Runtime panics or unexpected errors if Windows version changes (e.g., downgrade via rollback); no graceful degradation
- Fix approach: Cache version check result with TTL and re-verify periodically; wrap CfApi calls in version-safe guards that fallback to error if version is no longer sufficient

## Testing and Coverage

**Limited integration test coverage for crash recovery:**
- Issue: `cloudmount-cache/tests/cache_tests.rs` and `cloudmount-vfs/tests/` test normal paths but don't simulate crashes (SIGKILL) during operations like flush or delta sync
- Files: `crates/cloudmount-cache/tests/cache_tests.rs`, `crates/cloudmount-vfs/tests/`
- Impact: Crash recovery bugs (orphaned .pending files, incomplete uploads, lost inodes) are not caught by test suite; discovered only in production
- Fix approach: Add crash simulation tests using `libc::_exit()` or SIGKILL to kill mid-operation; verify writeback recovery and inode consistency on restart

**No tests for case-insensitive child lookups on Unix (mocked platform):**
- Issue: `core_ops.rs` has platform-gated case-sensitivity logic (Linux/macOS use exact match, Windows uses case-insensitive), but tests run on Linux and only test one code path
- Files: `crates/cloudmount-vfs/tests/`, `crates/cloudmount-vfs/src/core_ops.rs` (lines 625–633)
- Impact: Case-insensitive logic bugs on Windows are not caught until product deployment
- Fix approach: Add platform-generic unit tests that instantiate the functions with explicit names_match behavior; create Windows-specific integration tests (VM or CI runner)

**Mutex/Lock poisoning not explicitly tested:**
- Issue: No tests verify behavior when Mutex::lock() returns Err (lock poisoned by panic). Code assumes always succeeds via unwrap().
- Files: All crates using `.lock().unwrap()`
- Impact: Lock poisoning bugs are silent until a panic actually occurs in production
- Fix approach: Add unit tests that intentionally panic while holding locks, verify that subsequent lock attempts return Err, add recovery path tests

## Dependencies and Security

**Edition 2024 not yet stable (uses nightly Rust):**
- Issue: Workspace edition is "2024" (lines 14, Cargo.toml), which is not an official Rust edition
- Files: `Cargo.toml`
- Impact: Code will not compile on stable Rust without edition downgrade; users must use nightly toolchain, which is not a stable platform. Breaks reproducibility for security audits or binary distribution.
- Fix approach: Downgrade to edition "2021" (the current stable edition) and verify no 2024-only features are in use

**Keyring backend fallback lacks explicit user notification:**
- Issue: If keyring is unavailable, code silently falls back to AES-256-GCM encrypted files (lines 18–48, storage.rs). User is not informed that their tokens are now on disk instead of in OS keychain.
- Files: `crates/cloudmount-auth/src/storage.rs`
- Impact: User may believe tokens are secure in OS keyring, but they're actually encrypted on disk; compromise of encryption key (if machine password is weak) compromises all tokens
- Fix approach: Log a persistent warning; optionally show UI toast on startup warning that OS keyring is unavailable; provide diagnostic command to check keyring status

**No validation of Graph API response field presence:**
- Issue: Code accesses `.parent_reference.id` and `.name` fields without null checks (e.g., lines 915, 943, core_ops.rs). If Graph API response is missing these fields, code will panic (accessing Option::None).
- Files: `crates/cloudmount-vfs/src/core_ops.rs`, `crates/cloudmount-graph/src/client.rs`
- Impact: Unexpected Graph API schema changes (e.g., API version difference) cause panics; no graceful error recovery
- Fix approach: Use `?` operator to propagate Result from serde deserialization; catch and log deserialization errors; define minimum required fields and validate presence on API response

## Configuration and Defaults

**Cache TTL defaults to 60 seconds (too short for large directories):**
- Issue: Default `metadata_ttl_secs = 60` (line 118, main.rs), but listing large directories (1000+ items) can take > 60 seconds. By the time list completes, entries are already stale.
- Files: `crates/cloudmount-app/src/main.rs`
- Impact: Cache efficiency is poor for large directories; frequent redundant Graph API calls
- Fix approach: Increase default TTL to 300 seconds (5 minutes) or make adaptive based on directory size; allow user to tune via config

**No validation that mount points are distinct or exist:**
- Issue: User can configure two mounts to the same path in config file; no validation on load
- Files: `crates/cloudmount-core/src/config.rs`, `crates/cloudmount-app/src/commands.rs`
- Impact: Second mount will fail silently or clobber first mount's contents; errors are not clear
- Fix approach: In `EffectiveConfig::build()`, check for duplicate mount points; return error with clear message; ensure mount points exist before mounting (or create them if missing)

## Data Consistency Gaps

**Inode table reassignment for new files not idempotent:**
- Issue: When a new file (with temporary "local:" ID) is uploaded successfully, `reassign()` is called (line 1008, core_ops.rs). If the reassign call somehow fails or the file is re-flushed, reassignment might be called twice with different server IDs, corrupting the inode→ID mapping.
- Files: `crates/cloudmount-vfs/src/core_ops.rs`, `crates/cloudmount-vfs/src/inode.rs`
- Impact: Inode table corruption; same inode maps to multiple item IDs; subsequent operations reference wrong items on server
- Fix approach: Make `reassign()` idempotent (check current mapping before changing); log old→new mapping for debugging; add consistency check that detects and logs inode table corruption on startup

**Memory cache and SQLite can diverge after errors:**
- Issue: If `upsert_item()` succeeds in memory cache but fails in SQLite (DB corruption, disk full), the two caches are inconsistent. Subsequent reads use stale SQLite data.
- Files: `crates/cloudmount-vfs/src/core_ops.rs`, `crates/cloudmount-cache/src/manager.rs`
- Impact: User sees outdated file metadata; modifications appear to be lost; cache corruption invisible to user until explicit cache clear
- Fix approach: Make memory and SQLite updates transactional (insert to memory only after SQLite succeeds); if SQLite fails, invalidate memory cache entry to force re-fetch

---

*Concerns audit: 2026-03-10*
