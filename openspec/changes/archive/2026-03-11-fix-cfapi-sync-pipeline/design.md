## Context

carminedesktop uses the Windows Cloud Files API (CfApi) via the `cloud-filter` crate (v0.0.6) to present OneDrive/SharePoint as a sync root in Explorer. Remote-to-local sync works (delta sync updates placeholders), but local-to-remote sync is completely broken. Five independent bugs in the CfApi writeback pipeline prevent any local file mutation from reaching OneDrive.

The `cloud-filter` crate provides a `SyncFilter` trait with callbacks (`closed`, `rename`, `state_changed`, `delete`) and a `spawn_root_watcher` that uses `ReadDirectoryChangesW`. The crate is third-party (v0.0.6), evolving, and we do not control its watcher implementation.

Current flow for local changes:
1. CfApi callback fires (e.g., `closed()`, `rename()`, `state_changed()`)
2. `ingest_local_change()` resolves the file, looks up or creates a DriveItem
3. `stage_writeback_from_disk()` compares local file against cached DriveItem
4. If modified, queues content for `flush_inode()` upload via the 15-second retry loop in `main.rs`

This flow fails at multiple points as described in the proposal.

## Goals / Non-Goals

**Goals:**
- All local file operations (create, modify, copy-in, internal copy, rename, delete) in the sync root trigger upload to OneDrive
- Deferred operations (safe-save reconciliation, failed ingests) are processed reliably on a timer, not dependent on further CfApi callbacks
- Successfully uploaded local files become CfApi placeholders, integrating into the standard callback pipeline
- Rename acknowledgment is always sent to the OS regardless of Graph API outcome

**Non-Goals:**
- Modifying or forking the `cloud-filter` crate (v0.0.6 is still evolving; changes belong upstream)
- Adding the `notify` crate as a dependency (it wraps the same `ReadDirectoryChangesW` API we need and is only useful on Windows)
- Changing the FUSE (Linux/macOS) code path in any way
- Replacing the 15-second retry loop in `main.rs` (it serves as a crash-recovery safety net)
- Real-time conflict resolution for concurrent local+remote edits (handled by existing eTag-based conflict detection in `flush_inode`)

## Decisions

### Decision 1: Skip unmodified guard for `local:*` items

**Choice**: When `stage_writeback_from_disk()` encounters an item whose ID starts with `local:`, skip the mtime/size comparison entirely.

**Rationale**: The unmodified guard compares the local file's mtime and size against the cached DriveItem. For `local:*` items, `register_local_file()` creates the DriveItem with the exact same mtime and size as the local file, so the comparison always concludes "unmodified." There is no server copy to compare against -- the guard is only meaningful for items that already exist on OneDrive.

**Alternatives considered**:
- *Store a sentinel mtime in `register_local_file()`*: Would require changing the DriveItem creation logic and documenting a magic value. Less clear intent.
- *Remove the unmodified guard entirely*: Would cause redundant uploads for genuine re-triggers of already-synced files. The guard is valuable for server-backed items.

### Decision 2: Own `ReadDirectoryChangesW` watcher using `windows-sys`

**Choice**: Add a dedicated watcher thread in `cfapi.rs` that calls `ReadDirectoryChangesW` with `FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_DIR_NAME | FILE_NOTIFY_CHANGE_SIZE | FILE_NOTIFY_CHANGE_LAST_WRITE`. Events are debounced (500ms per path) and routed to `ingest_local_change()`.

**Rationale**: The `cloud-filter` crate's `spawn_root_watcher` only passes `FILE_NOTIFY_CHANGE_ATTRIBUTES` to `ReadDirectoryChangesW`, meaning it only fires for attribute changes (pin/unpin, hidden flag). File creation, content modification, renaming, and deletion never trigger `state_changed()`. We need a broader watcher, but cannot modify the crate.

**Alternatives considered**:
- *`notify` crate*: Adds a dependency only needed on Windows, uses the same `ReadDirectoryChangesW` underneath, and brings transitive dependencies. Since we already have `windows-sys` transitively, a direct ~100-150 line implementation is more appropriate.
- *Fork `cloud-filter`*: Maintenance burden for a crate that is actively evolving (v0.0.6). The fix is a single flag change, but forking creates ongoing merge overhead.
- *Patch `cloud-filter` upstream*: Good long-term, but we need a fix now and cannot control the upstream release schedule.

### Decision 3: Periodic 500ms timer thread for deferred operations

**Choice**: Spawn a background thread that wakes every 500ms and runs:
1. `process_safe_save_timeouts()` -- commits expired deferred renames
2. `process_deferred_timeouts()` -- cleans up expired deferred ingest entries
3. `retry_deferred_ingest()` -- retries failed ingests from the in-memory HashMap

**Rationale**: Currently these functions are only called inside CfApi callbacks (`closed()`, `rename()`, `state_changed()`). If no further callback fires within the timeout window (common for single-file operations), deferred items stay in the queue forever. Decoupling timeout processing from callbacks ensures reliable processing regardless of callback activity. 500ms is cheap -- it only checks timestamps and retries from in-memory data structures.

**Alternatives considered**:
- *Tokio timer task*: The CfApi filter methods are sync (trait constraint from `cloud-filter`). Using `block_in_place` to drive a Tokio timer would work but adds complexity. A simple `std::thread` with `thread::sleep(500ms)` is more straightforward for purely synchronous timer processing.
- *Longer interval (e.g., 5s)*: Would increase latency for safe-save operations. 500ms matches the debounce window and keeps user-perceived sync time low.

### Decision 4: Post-upload placeholder conversion

**Choice**: After `flush_inode()` successfully uploads a `local:*` file, call `Placeholder::convert_to_placeholder()` with the item blob (server item ID bytes) and `mark_in_sync()`.

**Rationale**: Without conversion, `local:*` files remain regular NTFS files. All future operations go through the filesystem watcher (slower, debounced) instead of the CfApi callback pipeline (immediate). Conversion makes the file a first-class CfApi citizen. The `cloud-filter` crate already exposes `CfConvertToPlaceholder` via `Placeholder::convert_to_placeholder()`.

**Alternatives considered**:
- *Leave as regular files*: Works but means the watcher is the permanent pathway for these files. Slower sync, higher watcher load, and no Explorer sync status overlay.
- *Delete and re-create as placeholder*: Destructive, risks data loss if the operation is interrupted, and triggers unnecessary watcher events.

### Decision 5: Always call `ticket.pass()` on rename

**Choice**: In the `rename()` callback, call `ticket.pass()` in both the success and error branches of `core.rename()`.

**Rationale**: By the time the `rename()` callback fires, the OS has already performed the rename on disk. Failing to acknowledge via `ticket.pass()` may cause the OS to interpret this as the provider denying the rename, leading to local/remote name divergence. The Graph API rename failure is a server-side issue that should be retried, not a reason to reject the local rename.

## Risks / Trade-offs

- **[Watcher event storms]** A bulk copy of thousands of files into the sync root will generate a burst of watcher events. **Mitigation**: 500ms debounce per path collapses rapid successive events. The existing ingest pipeline already handles high concurrency via the writeback queue and 15-second retry loop.

- **[Timer thread overhead]** A 500ms wake cycle adds a thread that runs continuously while mounted. **Mitigation**: The work per wake is trivial -- lock a Mutex, check timestamps on a small HashMap, unlock. No I/O, no allocations in the common (empty queue) case.

- **[Placeholder conversion race]** If a user modifies a file between upload completion and placeholder conversion, the conversion would mark a stale version as in-sync. **Mitigation**: The watcher will detect the subsequent modification and trigger a new ingest cycle. The file content on disk is never modified by the conversion call -- only NTFS reparse point metadata is updated.

- **[cloud-filter crate evolution]** Future versions of `cloud-filter` may fix their watcher flags, leading to duplicate events (both their watcher and ours). **Mitigation**: The ingest pipeline is already idempotent -- duplicate events for the same path result in a single upload. If/when the crate fixes the watcher, we can remove our custom watcher with no functional change.

- **[Watcher vs. CfApi callback overlap]** For placeholder files, both the CfApi callback and our watcher may fire for the same operation. **Mitigation**: `stage_writeback_from_disk()` uses mtime/size comparison to skip unmodified files, so the second trigger for an already-synced file is a no-op. The `local:*` skip applies only to the initial upload path.
