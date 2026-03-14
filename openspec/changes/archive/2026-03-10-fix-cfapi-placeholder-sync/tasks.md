## 1. Delta sync result type (`carminedesktop-cache`)

- [x] 1.1 Define `DeltaSyncResult` struct in `crates/carminedesktop-cache/src/sync.rs` with fields: `changed_items: Vec<DriveItem>` (items whose eTag changed), `deleted_items: Vec<DeletedItemInfo>` (deleted item ID + name + parent path captured before cache removal). Define `DeletedItemInfo` struct with `id: String`, `name: String`, `parent_path: Option<String>`.
- [x] 1.2 Change `run_delta_sync` return type from `Result<()>` to `Result<DeltaSyncResult>`. Populate `changed_items` with `DriveItem` clones for file items where `etag_changed` is true. For deleted items, look up the prior entry in SQLite (`get_item_by_id`) before cache removal to capture `name` and `parent_reference.path`, and add to `deleted_items`.
- [x] 1.3 Export `DeltaSyncResult` and `DeletedItemInfo` from `carminedesktop-cache` crate root (`lib.rs`).

## 2. Path resolution helper (`carminedesktop-cache` or `carminedesktop-core`)

- [x] 2.1 Add a helper function `resolve_relative_path(item: &DriveItem) -> Option<PathBuf>` that extracts the parent path from `item.parent_reference.path` (stripping the `/drive/root:` or `/drives/{id}/root:` prefix), joins it with `item.name`, and returns the relative path. Place in `carminedesktop-cache/src/sync.rs` (private) or as a utility in `carminedesktop-core` if reusable.
- [x] 2.2 Add a similar helper `resolve_deleted_path(info: &DeletedItemInfo) -> Option<PathBuf>` that resolves the path from the captured parent path and name for deleted items.

## 3. Placeholder update function (`carminedesktop-vfs`)

- [x] 3.1 Add a new public function `apply_delta_placeholder_updates` in `crates/carminedesktop-vfs/src/cfapi.rs`, gated with `#[cfg(target_os = "windows")]`. Signature: `pub fn apply_delta_placeholder_updates(mount_path: &Path, changed: &[(PathBuf, DriveItem)], deleted: &[PathBuf], writeback: &WriteBackBuffer, drive_id: &str)`.
- [x] 3.2 Implement the changed-items loop: for each `(relative_path, item)`, join with `mount_path` to get the absolute path. If the file does not exist on disk, skip. Check `writeback.list_pending()` result (or add `has_pending` check) — if the item has pending writeback, log a warning and skip. Open `Placeholder::open(abs_path)`, build `UpdateOptions::default().metadata(item_to_metadata(&item)).dehydrate().mark_in_sync().blob(item.id.as_bytes())` (skip `.dehydrate()` for folders), call `ph.update(update, None)`. Log warnings on failure and continue.
- [x] 3.3 Implement the deleted-items loop: for each `relative_path`, join with `mount_path` to get the absolute path. If the path does not exist, skip (no-op). Attempt `std::fs::remove_file` for files, `std::fs::remove_dir` for directories. Log warnings on failure (sharing violation, non-empty dir) and continue.
- [x] 3.4 Make `item_to_metadata` a module-level function (currently `carminedesktopCfFilter::item_to_metadata`) or extract it as a standalone `pub(crate)` function so the new `apply_delta_placeholder_updates` can use it without access to the filter struct.
- [x] 3.5 Export `apply_delta_placeholder_updates` from `carminedesktop-vfs` crate root with appropriate `#[cfg]` gate.

## 4. Wire notification bridge (`carminedesktop-app`)

- [x] 4.1 Update the `start_delta_sync` loop in `crates/carminedesktop-app/src/main.rs` to capture the `DeltaSyncResult` from `run_delta_sync`. On `Ok(result)`, if there are changed or deleted items, resolve paths and call `apply_delta_placeholder_updates`.
- [x] 4.2 Resolve paths for changed items: for each `DriveItem` in `result.changed_items`, call `resolve_relative_path` to get the relative path, pair it with the item, and collect into `Vec<(PathBuf, DriveItem)>`.
- [x] 4.3 Resolve paths for deleted items: for each `DeletedItemInfo` in `result.deleted_items`, call `resolve_deleted_path` to get the relative path, and collect into `Vec<PathBuf>`.
- [x] 4.4 Look up the mount path for the current drive from `state.mounts` (the `CfMountHandle` exposes `mount_path()`). Gate the entire call with `#[cfg(target_os = "windows")]`.
- [x] 4.5 Update the `DeltaSyncTimer::start` call site (if still used) to handle the new `DeltaSyncResult` return type — either ignore the result or apply the same logic. Verify no other callers of `run_delta_sync` are broken.

## 5. Writeback safety check

- [x] 5.1 Add a method `has_pending(&self, drive_id: &str, item_id: &str) -> bool` to `WriteBackBuffer` in `crates/carminedesktop-cache/src/writeback.rs` that checks if a pending entry exists for the given drive+item without loading content. Use `tokio::fs::try_exists` on the expected path.
- [x] 5.2 Use `has_pending` in `apply_delta_placeholder_updates` to skip dehydration for items with pending writes.

## 6. Testing

- [x] 6.1 Add unit tests for `resolve_relative_path` covering: standard nested path, root-level item, missing `parentReference`, path with `/drives/{id}/root:` prefix format.
- [x] 6.2 Add unit tests for `DeltaSyncResult` population in `run_delta_sync`: verify `changed_items` contains items with eTag changes, verify `deleted_items` contains items flagged for deletion with name/path captured, verify items with unchanged eTag are NOT in `changed_items`. Use `wiremock` to mock the Graph delta endpoint.
- [x] 6.3 Add a cross-platform unit test for the path resolution and writeback-pending logic (these parts don't require CfApi). Ensure the test compiles on all platforms.
- [x] 6.4 Document manual Windows testing procedure: mount a drive, modify a file on the server (web UI), wait for delta sync, verify Explorer shows updated file size and the file can be opened with correct content. Similarly test server-side deletion.

## 7. CI compliance

- [x] 7.1 Ensure all new code passes `cargo clippy --all-targets --all-features` with zero warnings (RUSTFLAGS=-Dwarnings). Pay attention to: unused variables in `#[cfg]` gated code, `allow(dead_code)` for cross-platform stubs if needed.
- [x] 7.2 Ensure `cargo build --target x86_64-pc-windows-msvc` compiles the Windows-gated code (if cross-compilation target is available in CI). Otherwise, verify via conditional compilation that non-Windows builds exclude the new code cleanly.
- [x] 7.3 Run `make check` (or equivalent `cargo fmt --check && cargo clippy && cargo build && cargo test`) to verify no regressions.
