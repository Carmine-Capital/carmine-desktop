# File Open Simplification Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove CollabGate from all platforms, remove Linux file associations, make macOS Office files open in browser only.

**Architecture:** Strip the VFS-level file interception (CollabGate) entirely. On Linux, remove file associations too — no interception at all. On macOS, change `office_uri_scheme()` to return `None` so Office files open in browser via `web_url`. On Windows, no functional change (file associations + Office URI schemes remain).

**Tech Stack:** Rust 2024, Tauri, FUSE (fuser), WinFsp, tokio

---

## Chunk 1: Core types and config cleanup

### Task 1: Remove CollabGate types from carminedesktop-core

**Files:**
- Modify: `crates/carminedesktop-core/src/types.rs:165-180`
- Modify: `crates/carminedesktop-core/src/config.rs:71,175,217-236,263,304-306,343`
- Modify: `crates/carminedesktop-core/src/open_online.rs:12-14,44-62`

- [ ] **Step 1: Remove `CollabOpenRequest` and `CollabOpenResponse` from types.rs**

Delete lines 165-180 in `types.rs`:

```rust
// DELETE: lines 165-180
/// Request sent from VFS to Tauri app when a collaborative file is opened
/// by an interactive shell process.
#[derive(Debug, Clone)]
pub struct CollabOpenRequest {
    pub path: String,
    pub extension: String,
    pub item_id: String,
    pub web_url: Option<String>,
}

/// Response from Tauri app indicating how to handle the file open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollabOpenResponse {
    OpenLocally,
    OpenOnline,
}
```

- [ ] **Step 2: Remove `CollaborativeOpenConfig` from config.rs**

Delete `default_collab_timeout()` (lines 217-219), `CollaborativeOpenConfig` struct and impl (lines 221-236):

```rust
// DELETE: lines 217-236
fn default_collab_timeout() -> u64 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborativeOpenConfig {
    #[serde(default = "default_collab_timeout")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub shell_processes: Vec<String>,
}

impl Default for CollaborativeOpenConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: default_collab_timeout(),
            shell_processes: Vec::new(),
        }
    }
}
```

Remove `collaborative_open` field from `UserGeneralSettings` (line 175):

```rust
// DELETE: line 174-175
    #[serde(default)]
    pub collaborative_open: Option<CollaborativeOpenConfig>,
```

Remove `"collaborative_open"` arm from `reset_setting()` (line 71):

```rust
// DELETE: line 71
                "collaborative_open" => g.collaborative_open = None,
```

Remove `collaborative_open` field from `EffectiveConfig` (line 263):

```rust
// DELETE: line 263
    pub collaborative_open: CollaborativeOpenConfig,
```

Remove its construction in `EffectiveConfig::build()` (lines 304-306):

```rust
// DELETE: lines 304-306
        let collaborative_open = user_general
            .and_then(|g| g.collaborative_open.clone())
            .unwrap_or_default();
```

Remove `collaborative_open,` from the `Self { ... }` block (line 343):

```rust
// DELETE: line 343
            collaborative_open,
```

- [ ] **Step 3: Remove `is_collaborative()` from open_online.rs**

Delete lines 44-62 in `open_online.rs`:

```rust
// DELETE: lines 44-62
/// Returns `true` if the file extension is editable collaboratively via Microsoft 365 Online.
pub fn is_collaborative(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        ".doc"
            | ".docx"
            | ".docm"
            | ".xls"
            | ".xlsx"
            | ".xlsm"
            | ".ppt"
            | ".pptx"
            | ".pptm"
            | ".odt"
            | ".ods"
            | ".odp"
            | ".vsdx"
    )
}
```

- [ ] **Step 4: Change `office_uri_scheme()` to return `None` on non-Windows**

In `open_online.rs` line 12, change:

```rust
// BEFORE:
    if cfg!(target_os = "linux") {
        return None;
    }
// AFTER:
    if cfg!(not(target_os = "windows")) {
        return None;
    }
```

- [ ] **Step 5: Change `register_file_associations` default to `true` on macOS**

In `config.rs` lines 308-312, change:

```rust
// BEFORE:
        #[cfg(target_os = "windows")]
        let default_file_assoc = true;
        #[cfg(not(target_os = "windows"))]
        let default_file_assoc = false;
// AFTER:
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let default_file_assoc = true;
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let default_file_assoc = false;
```

- [ ] **Step 6: Verify core crate compiles**

Run: `make check 2>&1 | head -50` (or `cargo check -p carminedesktop-core`)

Expected: May show errors in downstream crates (vfs, app) that still reference removed types — that's expected, we'll fix those in Tasks 2-4.

- [ ] **Step 7: Commit**

```bash
git add crates/carminedesktop-core/src/types.rs crates/carminedesktop-core/src/config.rs crates/carminedesktop-core/src/open_online.rs
git commit -m "refactor: remove CollabGate types, config, and is_collaborative from core"
```

---

## Chunk 2: VFS CollabGate removal

### Task 2: Remove CollabGate from core_ops.rs

**Files:**
- Modify: `crates/carminedesktop-vfs/src/core_ops.rs`

- [ ] **Step 1: Remove CollabGate imports**

In `core_ops.rs` lines 21-24, change:

```rust
// BEFORE:
use carminedesktop_core::config::CollaborativeOpenConfig;
use carminedesktop_core::types::{
    CollabOpenRequest, CollabOpenResponse, DriveItem, DriveQuota, FileFacet, ParentReference,
};
// AFTER:
use carminedesktop_core::types::{DriveItem, DriveQuota, FileFacet, ParentReference};
```

- [ ] **Step 2: Remove `VfsEvent::CollabGateTimeout` and `CollabOpenOnlineBackground`**

In `core_ops.rs` lines 361-368, delete:

```rust
// DELETE: lines 361-368
    /// CollabGate dialog timed out; file opened locally.
    CollabGateTimeout { path: String },
    /// CollabGate requests the app to open a file online in the background.
    ///
    /// Unlike the blocking CollabGate flow, this event fires when file associations
    /// are NOT registered. The VFS proceeds with a normal local open while the app
    /// asynchronously launches the Office URI scheme. No error dialog is shown.
    CollabOpenOnlineBackground { path: String },
```

- [ ] **Step 3: Remove `VfsError::CollabRedirect`**

In `core_ops.rs` lines 424-425, delete:

```rust
// DELETE: lines 424-425
    /// CollabGate redirected the open to the browser (FUSE: EACCES, Windows: STATUS_ACCESS_DENIED)
    CollabRedirect,
```

- [ ] **Step 4: Remove `CollabSender` type alias**

In `core_ops.rs` lines 481-485, delete:

```rust
// DELETE: lines 481-485
/// Channel type for sending CollabGate requests with a oneshot reply channel.
pub type CollabSender = tokio::sync::mpsc::Sender<(
    CollabOpenRequest,
    tokio::sync::oneshot::Sender<CollabOpenResponse>,
)>;
```

- [ ] **Step 5: Remove CollabGate fields from `CoreOps` struct**

In `core_ops.rs`, remove these fields from the struct (lines 498-509):

```rust
// DELETE from struct:
    collab_tx: Option<CollabSender>,
    collab_config: CollaborativeOpenConfig,
    mountpoint: Option<String>,
    collab_cooldown: dashmap::DashMap<u64, (Instant, bool)>,
    file_associations_registered: bool,
```

And their initialization in `CoreOps::new()` (lines 531-535):

```rust
// DELETE from new():
            collab_tx: None,
            collab_config: CollaborativeOpenConfig::default(),
            mountpoint: None,
            collab_cooldown: dashmap::DashMap::new(),
            file_associations_registered: false,
```

- [ ] **Step 6: Remove CollabGate builder methods**

Delete `with_collab_sender()` (lines 554-557), `with_collab_config()` (lines 559-562), `with_file_associations_registered()` (lines 564-571), and `with_mountpoint()` (lines 573-576).

- [ ] **Step 7: Remove `handle_collab_gate_fallback()` (Windows)**

Delete the entire `#[cfg(target_os = "windows")]` function at lines 584-663.

- [ ] **Step 8: Simplify `open_file()` — remove CollabGate logic and unused parameters**

Change the signature and remove CollabGate block (lines 1121-1293):

```rust
// BEFORE:
    pub fn open_file(
        &self,
        ino: u64,
        caller_pid: Option<u32>,
        file_path: Option<&str>,
    ) -> VfsResult<u64> {
        // CollabGate: intercept collaborative file opens...
        // [~170 lines of CollabGate logic]

        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;
// AFTER:
    pub fn open_file(&self, ino: u64) -> VfsResult<u64> {
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;
```

Delete everything between the function opening brace and `let item_id = ...` (lines 1127-1293).

- [ ] **Step 9: Verify core_ops compiles in isolation**

Run: `cargo check -p carminedesktop-vfs 2>&1 | head -50`

Expected: Errors in fuse_fs.rs, winfsp_fs.rs, mount.rs — they still pass removed parameters. Fixed in Task 3.

- [ ] **Step 10: Commit**

```bash
git add crates/carminedesktop-vfs/src/core_ops.rs
git commit -m "refactor: remove CollabGate logic from core_ops.rs"
```

### Task 3: Remove process_filter.rs and update VFS lib.rs

**Files:**
- Delete: `crates/carminedesktop-vfs/src/process_filter.rs`
- Modify: `crates/carminedesktop-vfs/src/lib.rs:4`

- [ ] **Step 1: Remove `process_filter` module from lib.rs**

In `lib.rs` line 4, delete:

```rust
// DELETE: line 4
pub mod process_filter;
```

- [ ] **Step 2: Delete process_filter.rs**

```bash
rm crates/carminedesktop-vfs/src/process_filter.rs
```

- [ ] **Step 3: Commit**

```bash
git add -u crates/carminedesktop-vfs/src/process_filter.rs crates/carminedesktop-vfs/src/lib.rs
git commit -m "refactor: remove process_filter module (CollabGate only)"
```

### Task 4: Update FUSE backend (fuse_fs.rs)

**Files:**
- Modify: `crates/carminedesktop-vfs/src/fuse_fs.rs`

- [ ] **Step 1: Remove collab parameters from `CarmineDesktopFs::new()`**

In `fuse_fs.rs` lines 87-138, change the constructor:

```rust
// BEFORE (lines 87-98):
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        mountpoint: &str,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
        sync_handle: Option<crate::sync_processor::SyncHandle>,
        collab_tx: Option<crate::core_ops::CollabSender>,
        collab_config: Option<carminedesktop_core::config::CollaborativeOpenConfig>,
        file_associations_registered: bool,
    ) -> Self {
// AFTER:
    pub fn new(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        mountpoint: &str,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
        sync_handle: Option<crate::sync_processor::SyncHandle>,
    ) -> Self {
```

Remove the collab and mountpoint wiring inside the body (lines 117-131):

```rust
// DELETE:
        ops = ops.with_mountpoint(mountpoint.to_string());
        // ...
        ops = ops.with_file_associations_registered(file_associations_registered);
        // ...
        if let Some(tx) = collab_tx {
            ops = ops.with_collab_sender(tx);
        }
        if let Some(cfg) = collab_config {
            ops = ops.with_collab_config(cfg);
        }
```

- [ ] **Step 2: Remove `CollabRedirect` from error mapping**

In `fuse_fs.rs` line 234, change:

```rust
// BEFORE:
            VfsError::PermissionDenied | VfsError::CollabRedirect => Errno::EACCES,
// AFTER:
            VfsError::PermissionDenied => Errno::EACCES,
```

- [ ] **Step 3: Simplify `open()` — remove caller_pid and file_path**

In `fuse_fs.rs` lines 416-423, change:

```rust
// BEFORE:
    fn open(&self, req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        let caller_pid = Some(req.pid());
        let file_path = self.ops.lookup_item(ino.0).map(|item| item.name.clone());
        match self.ops.open_file(ino.0, caller_pid, file_path.as_deref()) {
// AFTER:
    fn open(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        match self.ops.open_file(ino.0) {
```

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-vfs/src/fuse_fs.rs
git commit -m "refactor: remove CollabGate plumbing from FUSE backend"
```

### Task 5: Update WinFsp backend (winfsp_fs.rs)

**Files:**
- Modify: `crates/carminedesktop-vfs/src/winfsp_fs.rs`

- [ ] **Step 1: Remove collab parameters from WinFsp constructor**

In `winfsp_fs.rs`, find the constructor (around lines 244-265) and remove `collab_tx`, `collab_config`, `file_associations_registered` parameters. Remove the wiring lines:

```rust
// DELETE:
        if let Some(tx) = collab_tx {
            ops = ops.with_collab_sender(tx);
        }
        if let Some(cfg) = collab_config {
            ops = ops.with_collab_config(cfg);
        }
        ops = ops.with_file_associations_registered(file_associations_registered);
```

Also remove these from the `mount()` function signature (around lines 1020-1025).

- [ ] **Step 2: Remove `CollabRedirect` from error mapping**

In `winfsp_fs.rs` line 121, change:

```rust
// BEFORE:
        VfsError::CollabRedirect => STATUS_CANCELLED,
// AFTER: (delete this line entirely)
```

- [ ] **Step 3: Simplify `open_file()` call site**

Find the `open_file(ino, caller_pid, Some(&path_str))` call (around line 387) and change to:

```rust
// BEFORE:
                .open_file(ino, caller_pid, Some(&path_str))
// AFTER:
                .open_file(ino)
```

Remove the `caller_pid` extraction (lines 371-372), `path_str` construction (lines 360-365), the `tracing::debug!` statement referencing both (lines 373-379), and the `ops = ops.with_mountpoint(...)` call (around line 252). Also remove the same collab parameters from the `mount()` function (around lines 1020-1025).

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-vfs/src/winfsp_fs.rs
git commit -m "refactor: remove CollabGate plumbing from WinFsp backend"
```

### Task 6: Update mount.rs

**Files:**
- Modify: `crates/carminedesktop-vfs/src/mount.rs`

- [ ] **Step 1: Remove collab parameters from `MountHandle::mount()`**

In `mount.rs` lines 93-106, change the signature:

```rust
// BEFORE:
    pub fn mount(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        mountpoint: &str,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
        sync_handle: Option<crate::sync_processor::SyncHandle>,
        collab_tx: Option<crate::core_ops::CollabSender>,
        collab_config: Option<carminedesktop_core::config::CollaborativeOpenConfig>,
        file_associations_registered: bool,
    ) -> carminedesktop_core::Result<Self> {
// AFTER:
    pub fn mount(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        mountpoint: &str,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
        sync_handle: Option<crate::sync_processor::SyncHandle>,
    ) -> carminedesktop_core::Result<Self> {
```

- [ ] **Step 2: Update `try_mount` closure**

Remove collab parameters from the `try_mount` closure (lines 154-178) and the `CarmineDesktopFs::new()` calls inside:

```rust
// BEFORE:
        let try_mount = |auto_unmount: bool,
                         event_tx: ...,
                         sync_handle: ...,
                         collab_tx: Option<crate::core_ops::CollabSender>,
                         collab_config: Option<carminedesktop_core::config::CollaborativeOpenConfig>| {
            let fs = CarmineDesktopFs::new(
                ..., event_tx, sync_handle, collab_tx, collab_config, file_associations_registered,
            );
// AFTER:
        let try_mount = |auto_unmount: bool,
                         event_tx: ...,
                         sync_handle: ...| {
            let fs = CarmineDesktopFs::new(
                ..., event_tx, sync_handle,
            );
```

Update both `try_mount(...)` call sites (lines 183-194) to remove collab args.

- [ ] **Step 3: Verify VFS crate compiles**

Run: `cargo check -p carminedesktop-vfs 2>&1 | head -50`

Expected: Clean compile for VFS crate. App crate will still have errors (fixed in Task 7).

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-vfs/src/mount.rs
git commit -m "refactor: remove CollabGate plumbing from mount lifecycle"
```

---

## Chunk 3: App-level cleanup

### Task 7: Remove CollabGate from main.rs

**Files:**
- Modify: `crates/carminedesktop-app/src/main.rs`

- [ ] **Step 1: Remove `handle_collab_open_online()` function**

Delete the `handle_collab_open_online()` function (around lines 1108-1115).

- [ ] **Step 2: Remove `spawn_collab_handler()` function**

Delete the entire `spawn_collab_handler()` function (around lines 1117-1178).

- [ ] **Step 3: Remove CollabGate VfsEvent handling in `spawn_event_forwarder()`**

In the event forwarder match arms, delete the `CollabGateTimeout` and `CollabOpenOnlineBackground` arms (around lines 1081-1104).

- [ ] **Step 4: Remove collab channel creation and passing in `start_mount()`**

In `start_mount()` (around lines 1210-1240), remove:
- `let collab_config = { ... }` block (lines 1213-1217)
- `let (collab_tx, collab_rx) = tokio::sync::mpsc::channel(8);` (line 1218)
- `Some(collab_tx), Some(collab_config), file_associations_registered` args from `MountHandle::mount()` call (lines 1231-1233)
- `spawn_collab_handler(&ctx.rt, app, collab_rx);` call (line 1240)

Do the same for the second `start_mount` variant (around lines 1285-1315) — same pattern.

- [ ] **Step 5: Update headless mount call**

In the headless mount (around line 1743-1748), remove:
- `None, // no collab channel in headless mode`
- `None,` (collab_config)
- `false, // no file associations in headless mode`

Updated call should only pass: `graph, cache, inodes, drive_id, mountpoint, rt, None, None`

- [ ] **Step 6: Remove unused imports**

Remove any now-unused imports in main.rs (e.g., `CollabOpenRequest`, `CollabOpenResponse`, `CollaborativeOpenConfig`).

- [ ] **Step 7: Commit**

```bash
git add crates/carminedesktop-app/src/main.rs
git commit -m "refactor: remove CollabGate handler and channel from app layer"
```

### Task 8: Remove collab notifications from notify.rs

**Files:**
- Modify: `crates/carminedesktop-app/src/notify.rs`

- [ ] **Step 1: Remove collab notification functions**

Delete `collab_gate_timeout()` (around line 160-165) and `collab_open_failed()` (around line 168-174).

- [ ] **Step 2: Commit**

```bash
git add crates/carminedesktop-app/src/notify.rs
git commit -m "refactor: remove CollabGate notification functions"
```

### Task 9: Remove Linux file association code from commands.rs

**Files:**
- Modify: `crates/carminedesktop-app/src/commands.rs`

- [ ] **Step 1: Remove Linux fallback in `open_file()` command**

In `commands.rs`, find the `#[cfg(target_os = "linux")]` block inside `open_file()` (around lines 1197-1357) and delete it entirely.

- [ ] **Step 2: Remove `launch_desktop_exec()` function**

Delete the `#[cfg(target_os = "linux")]` function `launch_desktop_exec()` (around lines 1520-1565).

- [ ] **Step 3: Remove `shell_tokenize()` function**

Delete the `#[cfg(target_os = "linux")]` function `shell_tokenize()` (around lines 1570-1592).

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-app/src/commands.rs
git commit -m "refactor: remove Linux file association fallback code"
```

### Task 10: Remove Linux module from shell_integration.rs

**Files:**
- Modify: `crates/carminedesktop-app/src/shell_integration.rs`

- [ ] **Step 1: Remove entire `linux` module**

Delete the `#[cfg(target_os = "linux")]` `mod linux { ... }` block (around lines 622-1059).

- [ ] **Step 2: Update public API delegates for Linux**

Ensure the public functions (`register_file_associations()`, `unregister_file_associations()`, etc.) still compile. The Linux cfg-gated delegates should now be no-ops or removed. Check that the `#[cfg(target_os = "linux")]` delegates in the public API section either:
- Return sensible defaults (e.g., `false` for `are_file_associations_registered()`, `Ok(())` for register/unregister)
- Or are removed if they point to the deleted linux module

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/src/shell_integration.rs
git commit -m "refactor: remove Linux shell integration module"
```

---

## Chunk 4: Test cleanup and frontend

### Task 11: Remove CollabGate test files and fix core tests

**Files:**
- Delete: `crates/carminedesktop-vfs/tests/test_collab_gate.rs`
- Delete: `crates/carminedesktop-vfs/tests/test_process_filter.rs`
- Delete: `crates/carminedesktop-core/tests/test_open_online.rs` (tests only `is_collaborative()` which was removed)
- Modify: `crates/carminedesktop-core/tests/config_tests.rs` (remove `collaborative_open: None` field)

- [ ] **Step 1: Delete test files**

```bash
rm crates/carminedesktop-vfs/tests/test_collab_gate.rs
rm crates/carminedesktop-vfs/tests/test_process_filter.rs
rm crates/carminedesktop-core/tests/test_open_online.rs
```

- [ ] **Step 2: Fix config_tests.rs**

In `crates/carminedesktop-core/tests/config_tests.rs`, find and remove the line `collaborative_open: None,` (around line 164) from the `UserGeneralSettings` construction.

- [ ] **Step 3: Remove any Linux-specific test code in shell_integration tests**

Check for and remove tests that reference the deleted Linux module (`.desktop` files, `xdg-mime`, `launch_desktop_exec`, `shell_tokenize`).

Run: `grep -r "launch_desktop_exec\|shell_tokenize\|desktop_file_content\|xdg-mime" crates/carminedesktop-app/tests/` to find them.

- [ ] **Step 4: Update `open_file()` test call sites in VFS tests**

Search for `open_file(` in `crates/carminedesktop-vfs/tests/` and update calls from `open_file(ino, caller_pid, file_path)` to `open_file(ino)`:

```bash
grep -rn "open_file(" crates/carminedesktop-vfs/tests/
```

Update each call site accordingly.

- [ ] **Step 5: Update `office_uri()` doc comment in open_online.rs**

In `open_online.rs` lines 1-4, update the doc comment:

```rust
// BEFORE:
/// Returns `None` on Linux or for non-Office file types.
// AFTER:
/// Returns `None` on non-Windows platforms or for non-Office file types.
```

- [ ] **Step 6: Commit**

```bash
git add -u crates/carminedesktop-vfs/tests/ crates/carminedesktop-core/tests/ crates/carminedesktop-core/src/open_online.rs
git commit -m "test: remove CollabGate and process_filter tests, update open_file call sites"
```

### Task 12: Frontend CSS cleanup

**Files:**
- Modify: `crates/carminedesktop-app/dist/styles.css:613`

- [ ] **Step 1: Update CSS comment**

In `styles.css` line 613, the comment `/* Extension preference list (collab settings) */` references CollabGate. Update it:

```css
/* BEFORE: */
/* Extension preference list (collab settings) */
/* AFTER: */
/* Extension preference list */
```

- [ ] **Step 2: Commit**

```bash
git add crates/carminedesktop-app/dist/styles.css
git commit -m "chore: remove collab reference from CSS comment"
```

### Task 13: Full build verification

- [ ] **Step 1: Run clippy with all targets and features**

Run: `make clippy` (which runs `RUSTFLAGS=-Dwarnings cargo clippy --all-targets --all-features`)

Expected: Zero warnings, zero errors.

- [ ] **Step 2: Run tests**

Run: `make test`

Expected: All tests pass. CollabGate and process_filter tests no longer exist.

- [ ] **Step 3: Fix any remaining issues**

If clippy or tests reveal unused imports, dead code warnings, or failing tests, fix them and re-run.

- [ ] **Step 4: Final commit if needed**

```bash
git add -A
git commit -m "chore: fix remaining warnings from CollabGate removal"
```
