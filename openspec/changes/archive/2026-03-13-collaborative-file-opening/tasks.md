## 1. Core Types and Config

- [x] 1.1 Add `CollabOpenRequest` and `CollabOpenResponse` types to `carminedesktop-core/src/types.rs`
- [x] 1.2 Add `is_collaborative(extension: &str) -> bool` function to `carminedesktop-core/src/open_online.rs`
- [x] 1.3 Add `CollaborativeOpenConfig` struct to `carminedesktop-core` config system (`enabled`, `default_action`, `timeout_seconds`, `shell_processes`, per-extension overrides)
- [x] 1.4 Add `VfsEvent::CollabGateTimeout { path }` variant to the VfsEvent enum
- [x] 1.5 Write tests for `is_collaborative()` — Office, ODF, non-collaborative, case insensitivity

## 2. Process Filtering

- [x] 2.1 Implement `is_interactive_shell(pid: u32) -> bool` for Linux (read `/proc/<pid>/exe`, extract process name, match against known shells)
- [x] 2.2 Implement `is_interactive_shell(pid: u32) -> bool` for Windows (use `windows-rs` to query process name by PID, match against `explorer.exe`)
- [x] 2.3 Implement `is_interactive_shell(pid: u32) -> bool` for macOS (`libproc::pid_path`, match against `Finder`)
- [x] 2.4 Support configurable extra shell process names from `CollaborativeOpenConfig.shell_processes`
- [x] 2.5 Write tests for process filtering (mock/stub approach for PID resolution)

## 3. VFS CollabGate Integration

- [x] 3.1 Add `Option<mpsc::Sender<(CollabOpenRequest, oneshot::Sender<CollabOpenResponse>)>>` parameter to `CoreOps::new()`
- [x] 3.2 Implement CollabGate check in `CoreOps::open_file()` — before content download, check `is_collaborative` + `is_interactive_shell`, send request, await response with timeout
- [x] 3.3 Implement local modification detection (`has_local_changes`) — check open file table for dirty handles and writeback pending entries for the inode
- [x] 3.4 Handle `OpenOnline` response — return appropriate error code without downloading content
- [x] 3.5 Handle `OpenLocally` response — proceed with normal open flow
- [x] 3.6 Handle `Cancel` response — return error code (file not opened)
- [x] 3.7 Handle timeout — fallback to local, emit `VfsEvent::CollabGateTimeout`
- [x] 3.8 Extract caller PID in `winfsp_fs.rs` `open()` and pass to CoreOps
- [x] 3.9 Extract caller PID in `fuse_fs.rs` `open()` via `fuse_context` and pass to CoreOps

## 4. Tauri App — Dialog and Preferences

- [x] 4.1 Add CollabGate event listener in `main.rs` — spawn task to receive `CollabOpenRequest` from the mpsc channel
- [x] 4.2 Implement preference resolution — check `CollaborativeOpenConfig` for per-extension override, then `default_action`, auto-respond if preference set and no local changes
- [x] 4.3 Implement native dialog using `tauri::api::dialog` — title, message, local changes warning, "Open Locally" / "Open Online" buttons
- [x] 4.4 Implement "Remember my choice" — save per-extension preference to config on checkbox selection
- [x] 4.5 Implement `OpenOnline` action — call existing `resolve_web_url()` + `office_uri()` + `open_with_clean_env()` from the dialog handler
- [x] 4.6 Handle dialog errors and webUrl resolution failures — fallback to local open with notification
- [x] 4.7 Surface `VfsEvent::CollabGateTimeout` as a user notification

## 5. Context Menu Enhancements

- [x] 5.1 Update Windows context menu registration to create two entries: `carminedesktop.OpenOnline` and `carminedesktop.OpenLocally` with appropriate labels and commands
- [x] 5.2 Update Windows context menu cleanup to remove both entries
- [x] 5.3 Update Linux Nautilus scripts in `linux_integrations.rs` — add "Open Online (SharePoint)" and "Open Locally" scripts
- [x] 5.4 Update KDE Dolphin service menus in `linux_integrations.rs` — add both entries
- [x] 5.5 Add macOS Finder integration (Quick Action or equivalent) for "Open Online" and "Open Locally"

## 6. Settings UI

- [x] 6.1 Add collaborative open section to settings UI (`settings.html` / `settings.js`) — master switch, default action dropdown, per-extension preference management
- [x] 6.2 Add Tauri command to read/write collaborative open preferences
- [x] 6.3 Wire settings UI to Tauri commands with `showStatus()` feedback

## 7. Testing

- [x] 7.1 Integration test: CollabGate sends request for collaborative file opened by shell process
- [x] 7.2 Integration test: CollabGate skips non-collaborative files
- [x] 7.3 Integration test: CollabGate skips non-interactive processes
- [x] 7.4 Integration test: CollabGate timeout falls back to local open
- [x] 7.5 Integration test: preference resolution auto-responds when preference set
- [x] 7.6 Integration test: `has_local_changes` is true when dirty handles exist
