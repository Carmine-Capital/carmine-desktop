## 1. Atomic inode allocation (D1)

- [x] 1.1 In `inode.rs`, define a private `struct InodeMaps { inode_to_item: HashMap<u64, String>, item_to_inode: HashMap<String, u64> }` and replace the two `RwLock<HashMap>` fields with a single `RwLock<InodeMaps>`
- [x] 1.2 Rewrite `allocate()` to take a single write lock, check `maps.item_to_inode.get(item_id)`, return existing or insert into both maps atomically, then release the lock
- [x] 1.3 Rewrite `get_item_id()` and `get_inode()` to take a single read lock on the unified `RwLock<InodeMaps>`
- [x] 1.4 Rewrite `remove_by_item_id()` to take a single write lock, remove from both maps atomically
- [x] 1.5 Rewrite `reassign()` to take a single write lock (simplifies existing double-lock pattern at lines 87-94)
- [x] 1.6 Rewrite `set_root()` to take a single write lock (fixes the existing split-lock at lines 105-112)

## 2. CfApi closed() mtime guard (D2)

- [x] 2.1 In `cfapi.rs::closed()`, after resolving the item and before `std::fs::metadata`, retrieve `item.last_modified` as an `Option<DateTime<Utc>>`
- [x] 2.2 After the `std::fs::metadata` call, convert the file's `metadata.modified()` `SystemTime` to `DateTime<Utc>` via `chrono::DateTime::<Utc>::from()`
- [x] 2.3 If both timestamps are available and their difference is less than 1 second (`abs(file_mtime - server_mtime) < Duration::seconds(1)`), log at `debug` level "cfapi: closed skipping unmodified file" and return early
- [x] 2.4 If either timestamp is unavailable (`None` for `last_modified`, or `Err` for `metadata.modified()`), fall through to the existing writeback path (conservative — assume modified)

## 3. CfMountHandle `_connection` rename (D3)

- [x] 3.1 In `cfapi.rs`, rename the struct field `_connection` to `connection` in `CfMountHandle`
- [x] 3.2 Add a doc comment on the `connection` field: `/// Must be dropped before `sync_root_id` is unregistered. See `unmount()`.`
- [x] 3.3 Update `unmount()` to reference `self.connection` instead of `self._connection`
- [x] 3.4 Update the constructor (inside `mount()`) to assign `connection` instead of `_connection`

## 4. `run_headless` Windows dead-code elimination (D4)

- [x] 4.1 In `main.rs::run_headless()`, wrap the entire body after the Windows early-exit block (from `let rt = tokio::runtime::Builder...` through end of function) in `#[cfg(not(target_os = "windows"))]`
- [x] 4.2 Remove the split `let mut` / `let` for `mount_entries` — use a single unconditional `let mut mount_entries` inside the gated block
- [x] 4.3 Remove the `#[cfg(target_os = "windows")]` on the `mount_entries` binding (it's now inside a `not(windows)` block)
- [x] 4.4 Verify that `mounts_config`, `mount_handles`, `rt_handle`, `effective_cache_dir`, `max_cache_bytes`, `metadata_ttl` no longer need their own `#[cfg]` gates (they're inside the structural `not(windows)` block)

## 5. Path separator fix in `commands.rs` (D5)

- [x] 5.1 In `commands.rs::get_default_mount_root()`, replace `format!("~/{}/", config.root_dir)` with a `PathBuf`-based construction: use `expand_mount_point` on `format!("~/{}", config.root_dir)` then ensure the result is normalized via `PathBuf::from()` and `to_string_lossy()`

## 6. Minor robustness (D6)

- [x] 6.1 In `notify.rs::send()`, remove `let _ = e;` and change the `tracing::warn!` to include `{e}`: `tracing::warn!("failed to send notification '{title}': {e}")`
- [x] 6.2 In `tray.rs::setup()`, replace `.unwrap()` on `app.default_window_icon()` with `.ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?` (or equivalent error propagation via `?`)

## 7. Verification

- [x] 7.1 `make clippy` passes with zero warnings
- [x] 7.2 `make test` passes — existing tests still green
- [x] 7.3 Verify `inode.rs` changes compile and that `allocate`, `get_inode`, `get_item_id`, `reassign`, `set_root`, `remove_by_item_id` all use the single unified lock
- [x] 7.4 Verify the `closed()` mtime guard compiles on the CfApi-gated block (manual read of generated code, since local build is Linux-only)
