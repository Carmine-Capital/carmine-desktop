# Tasks: Make Available Offline

## Summary

Add a folder pinning system with TTL-based expiry that allows users to mark VFS folders for persistent offline sync. Core pin management and download orchestration live in `carminedesktop-cache`, OS-specific shell integration (Windows context menu, IPC) lives in `carminedesktop-app`, and configuration extends `carminedesktop-core`.

## Phase 1: Configuration & Data Layer [M]

**3 tasks** — Establishes the foundation: config fields, SQLite table, and eviction protection. No dependencies on later phases.

### T-01: Add offline settings to configuration [S] — DONE
- **Files**: `crates/carminedesktop-core/src/config.rs`
- **Implements**: CAP-07
- Constants, `UserGeneralSettings` fields, `EffectiveConfig` computed fields with clamping, `ConfigChangeEvent` variants, `diff_configs()`, `reset_setting()`.

### T-02: Create PinStore with pinned_folders table [M] — DONE
- **Files**: `crates/carminedesktop-cache/src/pin_store.rs` (NEW), `crates/carminedesktop-cache/src/sqlite.rs`, `crates/carminedesktop-cache/src/lib.rs`
- **Implements**: CAP-01
- New module with `PinnedFolder` struct, `PinStore` struct, CRUD methods, `is_protected()` parent-chain walk. DDL added to `SqliteStore::create_tables()`.

### T-03: Add eviction filter to DiskCache [S] — DONE
- **Files**: `crates/carminedesktop-cache/src/disk.rs`
- **Implements**: CAP-02
- `RwLock<Option<Arc<dyn Fn>>>` field, `set_eviction_filter()`, skip protected entries in `evict_if_needed()`, warning log.

## Phase 2: Offline Manager [M]

**2 tasks** — Builds the orchestration layer on top of Phase 1's data primitives.

### T-04: Create OfflineManager facade [M] — DONE
- **Files**: `crates/carminedesktop-cache/src/offline.rs` (NEW), `crates/carminedesktop-cache/src/lib.rs`
- **Implements**: CAP-03, CAP-10
- `pin_folder()` (validate + insert + background download), `unpin_folder()`, `process_expired()`, `redownload_changed_items()`, `PinResult` enum, private `recursive_download()`.

### T-05: Wire PinStore and eviction filter into CacheManager [S] — DONE
- **Files**: `crates/carminedesktop-cache/src/manager.rs`
- **Implements**: CAP-01, CAP-02
- Add `pin_store: Arc<PinStore>` field, create alongside `SqliteStore`, wire eviction filter callback.

## Phase 3: App Integration — Notifications, CLI, Delta Sync [M]

**3 tasks** — Connects the cache-layer offline system to the Tauri app: notifications, CLI entry points, delta sync hooks, and mount wiring.

### T-06: Add offline notification functions [S] — DONE
- **Files**: `crates/carminedesktop-app/src/notify.rs`
- **Implements**: CAP-09
- Four functions: `offline_pin_complete`, `offline_pin_rejected`, `offline_pin_failed`, `offline_unpin_complete`.

### T-07: Add CLI args and single-instance handler for offline pin/unpin [M] — DONE
- **Files**: `crates/carminedesktop-app/src/main.rs`
- **Implements**: CAP-05, CAP-08
- `--offline-pin` / `--offline-unpin` CLI args, single-instance dispatch, `handle_offline_pin/unpin()` handlers, extend `MountCacheEntry`/`SyncSnapshotRow` with `OfflineManager`, delta sync hooks for `redownload_changed_items()` and `process_expired()`.

### T-08: Wire OfflineManager into mount startup [M] — DONE
- **Files**: `crates/carminedesktop-app/src/main.rs`
- **Implements**: CAP-03, CAP-08
- Create `OfflineManager` in `start_mount()`, store in `MountCacheEntry`, update all destructuring sites, propagate config changes.

## Phase 4: Windows Shell Integration [M]

**2 tasks** — Registry-based context menu and lifecycle management.

### T-09: Register context menu verbs for offline pin/unpin [M] — DONE
- **Files**: `crates/carminedesktop-app/src/shell_integration.rs`
- **Implements**: CAP-06
- `register_context_menu()`, `unregister_context_menu()`, `update_context_menu_paths()` with `AppliesTo` AQS filter, `SHChangeNotify`. Linux/macOS no-op stubs.

### T-10: Integrate context menu registration into app lifecycle [S] — DONE
- **Files**: `crates/carminedesktop-app/src/main.rs`
- **Implements**: CAP-06, CAP-11
- Register in `setup_after_launch()`, unregister in `graceful_shutdown()` and `sign_out()`.

## Phase 5: IPC Server (Windows) [M]

### T-11: Create Windows named pipe IPC server [M] — DONE
- **Files**: `crates/carminedesktop-app/src/ipc_server.rs` (NEW), `crates/carminedesktop-app/src/main.rs`
- **Implements**: CAP-04
- `IpcServer` with named pipe at `\\.\pipe\CarmineDesktop`, JSON protocol, `CancellationToken`, 64KB/5s limits. Start in `setup_after_launch()`, stop in `graceful_shutdown()`. `AppState` gains `ipc_server` field behind `#[cfg(target_os = "windows")]`.

## Phase 6: Settings UI & Commands [S]

### T-12: Expose offline settings in Tauri commands and frontend [S] — DONE
- **Files**: `crates/carminedesktop-app/src/commands.rs`
- **Implements**: CAP-07
- Add fields to `SettingsInfo`, populate in `get_settings()`, accept in `save_settings()`, make `resolve_item_for_path()` `pub(crate)`.

## Phase 7: Tests [M]

### T-13: PinStore unit tests [S] — DONE
- **Files**: `crates/carminedesktop-cache/tests/cache_tests.rs`
- **Implements**: CAP-01
- 6 test cases: pin/is_pinned, unpin, unpin nonexistent, upsert refresh, list_expired, list_all.

### T-14: DiskCache eviction filter tests [S] — DONE
- **Files**: `crates/carminedesktop-cache/tests/cache_tests.rs`
- **Implements**: CAP-02
- 2 test cases: protected entries survive eviction, no-filter behavior unchanged.

### T-15: OfflineManager integration tests [M] — DONE
- **Files**: `crates/carminedesktop-cache/tests/test_offline.rs` (NEW)
- **Implements**: CAP-03, CAP-10
- 5 test cases with `wiremock`: pin success, too large, not a folder, unpin, process expired.

---

**Totals**: 7 phases, 15 tasks (8×[S] + 7×[M] + 0×[L])
