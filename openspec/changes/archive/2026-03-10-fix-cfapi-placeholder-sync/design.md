## Context

On Windows, CloudMount uses the Cloud Files API (CfApi) to present OneDrive/SharePoint files as NTFS placeholders. Placeholder metadata (size, timestamps) is set once during `fetch_placeholders()` and updated only after writeback in `closed()` via `mark_placeholder_synced()`. Delta sync (`run_delta_sync` in `cloudmount-cache`) detects remote changes and updates internal caches, but has no mechanism to propagate those changes to the NTFS placeholder layer.

The architecture creates a gap: `cloudmount-cache` owns the sync loop and knows what changed, but `cloudmount-vfs` owns the CfApi placeholder operations. There is currently no cross-crate notification path between them. The app orchestration layer (`cloudmount-app`) has access to both, making it the natural bridge point.

Key existing infrastructure:
- `Placeholder::open(path)` + `UpdateOptions::metadata().dehydrate().mark_in_sync().blob()` — the `cloud_filter` crate already supports atomic metadata update + dehydration in a single `CfUpdatePlaceholder` call.
- `mark_placeholder_synced()` in `cfapi.rs` already demonstrates the pattern of opening a placeholder by path and calling `ph.update()`.
- `start_delta_sync` in `main.rs` runs the sync loop with access to both `mount_caches` and `mounts` (which contains `CfMountHandle` with `mount_path`).

## Goals / Non-Goals

**Goals:**
- After delta sync detects a remote file content change (eTag mismatch), update the NTFS placeholder metadata to reflect the new size and timestamps, and dehydrate the placeholder so the next access fetches fresh content.
- After delta sync detects a remote item deletion, remove the placeholder from the NTFS filesystem.
- Return structured results from `run_delta_sync` so callers can act on what changed.
- Keep the implementation platform-gated (`#[cfg(target_os = "windows")]`) — no impact on FUSE.

**Non-Goals:**
- FUSE-side cache invalidation (FUSE kernel cache is managed via TTLs, not explicit invalidation — separate concern).
- Real-time push notifications from Microsoft Graph (webhooks/subscriptions) — delta sync polling is sufficient for v1.
- Handling placeholder updates for items that were renamed/moved without content change — metadata-only changes don't cause stale content (the existing inode-based lookup handles this).
- Handling conflicts where the user has local unsaved changes when a remote update arrives — the writeback buffer is checked before dehydration.

## Decisions

### Decision 1: Return `DeltaSyncResult` from `run_delta_sync` instead of using channels

**Choice**: Change `run_delta_sync` to return `Result<DeltaSyncResult>` where `DeltaSyncResult` contains vectors of changed items and deleted item IDs.

**Alternatives considered**:
- *Async channel (mpsc)*: Would require `run_delta_sync` to accept a sender, adding complexity and a new dependency pattern. Overkill since the caller already awaits the result synchronously.
- *Callback trait*: Would create a trait in `cloudmount-cache` that `cloudmount-vfs` implements, but this inverts the dependency direction unnecessarily. The cache crate should not know about VFS concepts.
- *Event bus*: Too complex for a single consumer.

**Rationale**: The sync loop in `start_delta_sync` already calls `run_delta_sync` and handles the result. Returning structured data is the simplest approach — no new async primitives, no trait objects, no cross-crate dependency inversion. The app layer applies the results to the appropriate platform layer.

### Decision 2: Atomic update + dehydrate in a single `CfUpdatePlaceholder` call

**Choice**: Use `UpdateOptions::default().metadata(new_meta).dehydrate().mark_in_sync().blob(item_id)` to update metadata and dehydrate in one OS call.

**Alternatives considered**:
- *Two-step: update metadata, then separate dehydrate*: Two OS calls, risk of inconsistent intermediate state.
- *Delete and recreate placeholder*: Destructive — loses pin state, user-set attributes, and Explorer column data.

**Rationale**: The `cloud_filter` crate's `UpdateOptions` supports combining `.metadata()` and `.dehydrate()` flags, which maps directly to `CfUpdatePlaceholder` with `CF_UPDATE_FLAG_DEHYDRATE`. This is the canonical Windows API pattern for server-initiated content updates. The `.mark_in_sync()` flag ensures the Explorer overlay shows the correct sync state.

### Decision 3: Skip placeholder dehydration for items with pending writeback

**Choice**: Before dehydrating a placeholder, check `cache.writeback.has_pending(drive_id, &item_id)`. If the item has pending local writes, skip the dehydration and log a warning.

**Rationale**: If the user modified a file locally and the writeback hasn't completed yet, dehydrating would discard their local changes. The writeback will eventually upload and reconcile via conflict detection. This is a safety guard, not a common path.

### Decision 4: Place the placeholder update logic as a public function in `cloudmount-vfs`

**Choice**: Add `pub fn apply_delta_updates(mount_path: &Path, items: &[(PathBuf, DriveItem)], deleted_paths: &[PathBuf])` to `cloudmount-vfs` (gated with `#[cfg(target_os = "windows")]`).

**Alternatives considered**:
- *Method on `CfMountHandle`*: Would work, but `CfMountHandle` is behind a `Mutex` in the app state, and we'd need to hold the lock for the duration of placeholder updates. A free function taking `mount_path` avoids lock contention.
- *Inline in `cloudmount-app`*: Would put CfApi-specific code in the app crate, violating the separation of concerns.

**Rationale**: The VFS crate already owns all CfApi placeholder operations. A free function keeps the implementation contained while allowing the app layer to call it with the resolved paths and items.

### Decision 5: Resolve item paths via parent chain from SQLite

**Choice**: `run_delta_sync` returns `DriveItem` objects that contain `parent_reference`. The app layer resolves the full filesystem path by walking the parent chain from the item's `parent_reference.path` (which Graph delta responses include) and prepending the mount path.

**Alternatives considered**:
- *InodeTable path resolution*: The inode table maps item_id → inode, not item_id → path. Would need reverse resolution through the memory cache, which may not have the full tree populated.
- *Storing paths in the delta result*: Graph delta responses include `parentReference.path` which gives the server-side path. Combined with `item.name`, this gives us the relative path from the drive root.

**Rationale**: Microsoft Graph delta responses include `parentReference.path` in the format `/drive/root:/path/to/parent`. Stripping the `/drive/root:` prefix and joining with `item.name` gives the relative path. This is reliable and avoids cache lookups.

### Decision 6: Delete placeholders via `std::fs::remove_file` / `remove_dir`

**Choice**: For deleted items, use standard filesystem removal. The CfApi filter's `delete` callback will NOT fire for programmatic deletions by the sync provider itself (the provider is the owner).

**Rationale**: Windows expects the sync provider to manage its own placeholders. Removing via filesystem APIs is the documented approach. The `state_changed` callback only fires for user-initiated changes, not provider-initiated ones.

## Risks / Trade-offs

- **[Risk] Placeholder in use during update** → The `CfUpdatePlaceholder` call may fail with a sharing violation if the file is currently open/hydrating. Mitigation: Log a warning and skip — the next delta sync cycle will retry. The item is already marked dirty in the cache, so `open_file` will download fresh content regardless.

- **[Risk] Path resolution relies on Graph `parentReference.path`** → If the delta response doesn't include `parentReference.path` (rare, but possible for root-level items), path resolution fails. Mitigation: Fall back to using just `item.name` relative to mount root for items whose parent is the drive root. Log and skip items where path cannot be resolved.

- **[Risk] Race between delta sync placeholder update and user file access** → A user could open a file between the metadata update and dehydration. Mitigation: `CfUpdatePlaceholder` with `CF_UPDATE_FLAG_DEHYDRATE` is atomic at the OS level — metadata and dehydration happen in one call. The user will either see the old state or the new state, never an inconsistent mix.

- **[Trade-off] `run_delta_sync` return type change is technically breaking** → All callers of `run_delta_sync` must handle the new return type. Mitigation: There are only two call sites (`start_delta_sync` in main.rs and the old `DeltaSyncTimer::start`). Both are internal. `DeltaSyncTimer` is unused in production (replaced by the inline loop in `start_delta_sync`).

- **[Trade-off] Deleted item path resolution** → For deleted items, the delta response provides the item ID but the item may already be removed from SQLite by the time we try to resolve its path. Mitigation: `run_delta_sync` captures the filesystem-relative path of each deleted item BEFORE removing it from the caches, and includes these paths in the result.
