---
phase: 01-winfsp-offline-pin-fix
verified: 2026-03-18T10:01:31Z
status: passed
score: 6/6 must-haves verified
---

# Phase 1: WinFsp Offline Pin Fix — Verification Report

**Phase Goal:** Users can navigate offline-pinned mounts in File Explorer without crashes or hangs
**Verified:** 2026-03-18T10:01:31Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Graph API calls from VFS callbacks time out within 5 seconds instead of 30-120s OS TCP defaults | ✓ VERIFIED | `VFS_GRAPH_TIMEOUT: Duration = Duration::from_secs(5)` at core_ops.rs:459; `graph_with_timeout` helper at core_ops.rs:553-572 wraps with `tokio::time::timeout`; test `test_core_ops_find_child_returns_within_timeout_on_slow_server` validates <8s return on 10s delay |
| 2 | First VFS-path timeout sets the offline flag, protecting all subsequent calls from blocking | ✓ VERIFIED | `graph_with_timeout` calls `self.set_offline()` on both `Err(_elapsed)` (timeout, line 569) and `Error::Network` (line 564); test `test_core_ops_timeout_sets_offline_flag` validates flag is set; test `test_core_ops_offline_skips_graph_api` validates subsequent calls skip Graph API |
| 3 | Log files are automatically rotated with a maximum of 31 files | ✓ VERIFIED | main.rs:444-450 uses `tracing_appender::rolling::Builder::new()` with `.max_log_files(31)`; old `rolling::daily` call is absent from codebase |
| 4 | Pinned items are never evicted from memory cache by TTL expiry or LRU eviction | ✓ VERIFIED | memory.rs:12 defines `MemoryEvictionFilter = Arc<dyn Fn(&DriveItem) -> bool + Send + Sync>`; `get()` (line 48-58) and `get_children()` (line 71-80) bypass TTL for protected entries; `maybe_evict()` (line 191-196) skips protected entries; 5 tests in test_memory_eviction_protection.rs cover all paths |
| 5 | Offline directory listings are complete for all paths inside a pinned folder | ✓ VERIFIED | offline.rs:232-245 calls `cache.sqlite.upsert_item()` for every child during `recursive_download`; root folder persisted at lines 102-108 before spawning download; temporary inode counter starts at 1,000,000 (line 98) to avoid VFS inode collisions |
| 6 | recursive_download populates SQLite with folder and file metadata during pin | ✓ VERIFIED | offline.rs:219-226 signature includes `parent_temp_inode: u64` and `next_inode: &AtomicU64`; line 230 assigns child inodes; line 233-236 calls `upsert_item` for each child; recursive call at lines 248-256 passes child_inode as parent for subdirectories |

**Score:** 6/6 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/carminedesktop-vfs/src/core_ops.rs` | VFS-path Graph calls wrapped with `tokio::time::timeout` | ✓ VERIFIED | Contains `VFS_GRAPH_TIMEOUT` (line 459), `graph_with_timeout` (line 553), 5 call sites converted + 1 inline for `get_quota` |
| `crates/carminedesktop-cache/src/memory.rs` | Eviction protection via filter callback for pinned items | ✓ VERIFIED | Contains `eviction_filter` field (line 17), `set_eviction_filter` method (line 40), TTL bypass in `get`/`get_children`, skip in `maybe_evict` |
| `crates/carminedesktop-cache/src/manager.rs` | Memory cache eviction filter wired to `PinStore::is_protected` | ✓ VERIFIED | Contains `memory.set_eviction_filter` (line 50) with closure calling `ps2.is_protected(&drive_id_owned, &item.id)` (line 51); `drive_id: String` parameter added to `CacheManager::new` (line 29) |
| `crates/carminedesktop-cache/src/offline.rs` | `recursive_download` populates SQLite metadata | ✓ VERIFIED | Contains `cache.sqlite.upsert_item` at lines 105 (root) and 236 (children); `AtomicU64` temp inode counter; continues on metadata persistence failure (warn + continue pattern) |
| `crates/carminedesktop-app/src/main.rs` | Log rotation with `max_log_files(31)` | ✓ VERIFIED | Contains `Builder::new()` with `.max_log_files(31)` at line 448; `rolling::daily` fully replaced |
| `crates/carminedesktop-vfs/tests/core_ops_tests.rs` | Integration tests for timeout behavior | ✓ VERIFIED | 4 tests: timeout speed (10s delay → <8s return), offline flag set, offline skips Graph API, normal success within timeout |
| `crates/carminedesktop-cache/tests/test_memory_eviction_protection.rs` | Integration tests for eviction protection | ✓ VERIFIED | 5 tests: LRU protection, TTL bypass get, TTL bypass get_children, normal TTL without filter, eviction target with protected entries |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `core_ops.rs` | `GraphClient` | timeout-wrapped calls in `find_child`, `list_children`, `read_content`, `open_file`, `has_server_conflict` | ✓ WIRED | 5 call sites use `self.graph_with_timeout(self.graph.xxx(...))` (lines 624, 770, 841, 916, 1109); `get_quota` uses inline timeout (line 593) |
| `core_ops.rs` | `CoreOps::set_offline()` | timeout triggers offline flag | ✓ WIRED | `graph_with_timeout` calls `self.set_offline()` on `Err(_elapsed)` (line 569) and `Error::Network` (line 564); `get_quota` also calls `set_offline` on both paths (lines 606, 612) |
| `manager.rs` | `memory.rs` | `set_eviction_filter` wiring at `CacheManager::new()` | ✓ WIRED | Line 50: `memory.set_eviction_filter(Arc::new(move \|item: &DriveItem\| { ps2.is_protected(...) }))` |
| `manager.rs` | `pin_store.rs` | `PinStore::is_protected` used as eviction filter predicate | ✓ WIRED | Line 51: `ps2.is_protected(&drive_id_owned, &item.id)` — bridges inode-keyed memory cache to item_id-keyed PinStore |
| `offline.rs` | `sqlite.rs` | `upsert_item` called during `recursive_download` | ✓ WIRED | Line 105 (root folder) and line 236 (all children) — both call `cache.sqlite.upsert_item()` with temp inodes and parent references |
| Non-VFS callers | Graph API | unaffected by VFS timeout | ✓ VERIFIED | Only 2 raw `rt.block_on(self.graph.*)` remain in core_ops.rs (lines 1691, 1857) — both are write-path operations (conflict copy upload, copy status poll), not read-path VFS callbacks |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| BUG-01 | 01-01, 01-02 | WinFsp offline pin crash resolved — File Explorer no longer hangs when navigating a mounted drive with pinned folders after losing network connectivity | ✓ SATISFIED | Three root causes addressed: (1) VFS-path 5s timeout prevents indefinite blocking, (2) memory cache eviction protection prevents pinned item fallthrough to Graph API, (3) SQLite metadata population ensures offline directory listings are complete. REQUIREMENTS.md marks BUG-01 as `[x]` Complete. |

No orphaned requirements found — BUG-01 is the only requirement mapped to Phase 1 in REQUIREMENTS.md, and it is claimed by both plans.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| — | — | — | — | No anti-patterns found |

All 5 modified source files scanned for TODO/FIXME/HACK/PLACEHOLDER/stub patterns. Zero matches.

### Commit Verification

All 6 commits documented in SUMMARYs exist in git history:

| Commit | Message | Plan |
|--------|---------|------|
| `f1c45e7` | test(01-01): add failing tests for VFS-path Graph API timeout wrapping | 01-01 |
| `f032df6` | feat(01-01): add VFS-path timeout wrapping to CoreOps Graph API calls | 01-01 |
| `d6c0997` | fix(01-01): add log rotation with 31-day maximum via Builder API | 01-01 |
| `90b1565` | test(01-02): add failing tests for memory cache eviction protection | 01-02 |
| `7a0ca2b` | feat(01-02): add memory cache eviction protection for pinned items | 01-02 |
| `a4a3255` | feat(01-02): populate SQLite metadata during recursive_download for pin | 01-02 |

### Human Verification Required

### 1. File Explorer Navigation Under Offline Pin

**Test:** On Windows, mount a OneDrive drive, pin a folder for offline, disconnect network, navigate the pinned folder tree in File Explorer.
**Expected:** Directory listings appear immediately (no hang). Files in disk cache open normally. Files NOT in disk cache show a standard Windows error dialog immediately (no 30-second stall).
**Why human:** Requires actual Windows + WinFsp + network disconnect. Programmatic verification confirms the code paths are correct, but real-world Explorer behavior depends on OS VFS callback timing, NTSTATUS code interpretation, and Explorer retry logic.

### 2. Reconnection Resumes Normal Sync

**Test:** After the offline navigation test, reconnect network and verify sync resumes without remount.
**Expected:** Delta sync detects connectivity and clears the offline flag. New changes from server appear. Local changes (if any) are uploaded.
**Why human:** Requires actual network state transitions and observation of sync behavior over time.

### 3. Memory Cache Eviction Under Real Load

**Test:** With a large drive (10,000+ items), pin a folder, browse extensively to fill memory cache, then go offline and navigate pinned paths.
**Expected:** Pinned items are always served from memory cache without falling through to Graph API (verified via log output showing no Graph API calls for pinned items while offline).
**Why human:** Requires real-scale cache pressure that can't be fully simulated in unit tests.

### Build Verification

`cargo check --all-targets` passes cleanly (0.21s, all targets). All 27 `CacheManager::new` call sites across the workspace include the new `drive_id` parameter.

## Summary

Phase 1 goal is **achieved**. All three root causes of the File Explorer hang have been addressed:

1. **VFS-path timeout** (Plan 01): All 6 VFS-callback Graph API call sites are wrapped with a 5-second `tokio::time::timeout`. Timeout or network error triggers offline mode via `set_offline()`, protecting all subsequent calls. Non-VFS callers (uploads, delta sync, copy operations) are unaffected.

2. **Memory cache eviction protection** (Plan 02): Pinned items survive both TTL expiry and LRU eviction in the memory cache. The eviction filter is wired through `CacheManager::new()` to `PinStore::is_protected()`, mirroring the existing disk cache protection pattern.

3. **SQLite metadata population during pin** (Plan 02): `recursive_download` now persists every folder and file's metadata to SQLite during pin_folder, ensuring that offline `find_child` and `list_children` have complete directory tree data without needing Graph API calls.

9 new integration tests validate the key behaviors. All 6 commits exist. Zero anti-patterns. BUG-01 requirement is satisfied.

---

_Verified: 2026-03-18T10:01:31Z_
_Verifier: Claude (gsd-verifier)_
