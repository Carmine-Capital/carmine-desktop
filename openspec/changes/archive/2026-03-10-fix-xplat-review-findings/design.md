## Context

After the `fix-cfapi-safety-parity` (49/54 tasks done) and `fix-vfs-residual-parity-gaps` (0/8 tasks done) changes are applied, six cross-platform defects remain unaddressed. These were found during a full review of `carminedesktop-vfs/` and `carminedesktop-app/`, verified against the current working tree.

Current state of each:

1. **InodeTable TOCTOU** (`inode.rs:34-53`) — `allocate()` drops the read lock before acquiring write locks, allowing two concurrent calls to allocate different inodes for the same `item_id`. `reassign()` already demonstrates the correct pattern (holding both write locks simultaneously at lines 87-94).

2. **CfApi `closed()` on every open** (`cfapi.rs:267`) — `cloud-filter-0.0.6`'s `info::Closed` only exposes `deleted()`. The Windows SDK defines `CF_CALLBACK_CLOSE_COMPLETION_FLAG_MODIFIED = 0x2` but the crate hasn't wrapped it. Every file close — including read-only opens — triggers `std::fs::read()` + `writeback.write()` + `flush_inode()` (Graph API GET for eTag check + PUT upload).

3. **`_connection` naming** (`cfapi.rs:617-618`) — the `_` prefix conventionally means "intentionally unused" but the field's drop order is safety-critical. `unmount()` explicitly calls `drop(self._connection)` before `sync_root_id.unregister()`.

4. **`run_headless` dead code on Windows** (`main.rs:1232-1500`) — after the `#[cfg(target_os = "windows")]` early exit at line 1157, the rest of the function body still compiles on Windows. `mounts_config` is built unconditionally (line 1241), `mount_config` is an unused loop variable (line 1262), and `mount_entries.clone()` at line 1384 is redundant. All trigger clippy warnings under `RUSTFLAGS=-Dwarnings` on Windows CI.

5. **Path separator in `commands.rs`** (line 626) — `format!("~/{}/", config.root_dir)` hard-codes `/`. After `expand_mount_point`, the result on Windows is `C:\Users\...\Cloud/` with a trailing forward-slash.

6. **Minor robustness** — `notify.rs:137` discards the error object (`let _ = e;`) before the `tracing::warn!` line; `tray.rs:30` panics via `.unwrap()` when no default icon is configured.

## Goals / Non-Goals

**Goals:**
- Eliminate the InodeTable race so `item_id → inode` is always 1:1
- Prevent unnecessary Graph API uploads when a file is opened read-only on Windows
- Fix all issues that would cause clippy warnings on Windows CI under `RUSTFLAGS=-Dwarnings`
- Improve debuggability and robustness of notification/tray code

**Non-Goals:**
- Forking or patching `cloud-filter` upstream (we work within the 0.0.6 API)
- Changing the `Closed` callback API contract (we add a guard, not a new API)
- Refactoring `run_headless` beyond the structural cfg gate needed for Windows CI
- Addressing the `check_fuse_available` naming or the `Ctrl+C` dead handler on `windows_subsystem = "windows"` (cosmetic, no CI impact)

## Decisions

### D1: InodeTable — single `RwLock` over a struct containing both maps

Merge `inode_to_item` and `item_to_inode` into one `RwLock<InodeMaps>` where `InodeMaps` holds both `HashMap`s. `allocate()` takes a write lock once, checks `item_to_inode`, and either returns the existing inode or inserts into both maps atomically. Read-only callers (`get_item_id`, `get_inode`) take a single read lock.

This follows the pattern already used by `reassign()` (lines 87-94), which takes both write locks — we're just making it structural.

**Alternative considered:** Keeping two `RwLock`s but using a `Mutex` for the write path only. Rejected — it adds a third lock and makes the locking protocol harder to reason about.

**Alternative considered:** Using `DashMap` for lock-free concurrent access. Rejected — two `DashMap`s still have the split-insert problem, and a custom `DashMap` entry API would be more complex than a simple `RwLock<struct>`.

### D2: CfApi `closed()` — mtime comparison guard

Before reading the file content, compare the file's Last Write Time (from `std::fs::metadata`) against `item.last_modified` (from the cached `DriveItem`). If they match (within 1-second tolerance for FAT32/NTFS clock resolution), skip the writeback entirely.

This works because:
- `mark_placeholder_synced()` calls `ph.update()` with `meta.written(ft)` set to `item.last_modified` from the server (lines 79-83, 93-106 in `cfapi.rs`). This sets the file's Last Write Time to the server's `lastModifiedDateTime`.
- When a user modifies the file, Windows updates the Last Write Time to the current wall clock.
- A read-only open does NOT change the Last Write Time.
- After `fetch_data` hydrates the file, `CfExecute(TRANSFER_DATA, ...)` does NOT reset the Last Write Time — it preserves the metadata set during placeholder creation.

The 1-second tolerance handles the case where `chrono::DateTime<Utc>` has sub-second precision but NTFS stores times in 100-nanosecond intervals. We round both to seconds for comparison.

**Alternative considered:** Patching `cloud-filter` to expose `info.modified()`. This is the correct long-term fix but requires a fork or upstream PR, adding a dependency management burden. We can adopt it later if/when upstream merges the 3-line addition.

**Alternative considered:** Comparing file size instead of mtime. Rejected — an edit that replaces content with same-length content would be missed.

**Testability:** This heuristic can be validated on Windows by: (1) hydrating a file, (2) closing it without edits, (3) verifying no Graph API call is made. The mtime comparison is deterministic and doesn't require mock clocks.

### D3: `_connection` rename to `connection`

Rename `_connection` to `connection` in `CfMountHandle`. Add a doc comment explaining that `connection` must be dropped before `sync_root_id` is unregistered. Update `unmount()` to reference `self.connection`.

Rust drops struct fields in declaration order (not reverse). Since `connection` is declared before `sync_root_id`, if the struct is dropped without calling `unmount()` (e.g., panic during shutdown), the implicit drop order is correct: `connection` drops first, then `sync_root_id`. The explicit `drop(self.connection)` in `unmount()` is still needed because `unmount()` takes `self` by value and needs to control the sequence before calling `unregister()`.

### D4: `run_headless` — wrap post-exit body in `#[cfg(not(target_os = "windows"))]`

The Windows early-exit block at line 1157 calls `process::exit(1)`, but since it's inside a `#[cfg(target_os = "windows")]` block (not a structural `return`), the compiler still processes the rest of the function on Windows. Wrap lines 1165-1502 (from `let rt = tokio::runtime::Builder...` to end of the `rt.block_on` closure) in `#[cfg(not(target_os = "windows"))]`.

This eliminates: the split `let mut` / `let` for `mount_entries`, the unconditional `mounts_config`, the unused `mount_config` loop variable, the `mount_entries.clone()`, and the dead `#[cfg(not(unix))]` ctrl_c handler — all in one structural change.

**Alternative considered:** Restructuring `run_headless` into two functions (`run_headless_unix` + `run_headless_windows`). Rejected — the Windows path is 4 lines (print error + exit); a full function split is overkill.

### D5: `get_default_mount_root` — use `Path::join`

Replace `format!("~/{}/", config.root_dir)` with `expand_mount_point` called on a `PathBuf`-constructed path. The `~` expansion is already handled by `expand_mount_point`, so construct `~/` + `root_dir` via string, let `expand_mount_point` resolve it, then normalize the result through `PathBuf` to ensure consistent separators.

### D6: Minor robustness

- `notify.rs`: Remove `let _ = e;` and include `{e}` in the `tracing::warn!` format string.
- `tray.rs`: Replace `.unwrap()` with `.ok_or_else(|| ...)` and propagate via `?`. The `setup()` function returns `tauri::Result<()>`, so error propagation is already supported.

## Risks / Trade-offs

- **[Risk: mtime comparison false-positive on rapid edits]** If a user saves a file at the exact same second as the server's `lastModifiedDateTime`, the guard would skip the upload. Mitigation: this requires the server mtime to be within the same second as the local wall clock at save time, which is astronomically unlikely given network latency. The eTag conflict check in `flush_inode` provides a second safety net.

- **[Risk: InodeTable lock contention under heavy FUSE load]** Merging to a single `RwLock` means `allocate()` write-locks both maps simultaneously, blocking all `get_inode`/`get_item_id` readers. Mitigation: `allocate()` is called once per unique item (typically during `lookup` or `readdir`), and the critical section is two `HashMap::insert` calls — sub-microsecond. FUSE serialises `lookup` per directory entry, so contention is bounded.

- **[Risk: `set_root` also has split-lock pattern]** `set_root()` takes two separate write locks (lines 105-112). After the D1 refactor, all methods use the unified lock, so this is automatically fixed. Verify in implementation.
