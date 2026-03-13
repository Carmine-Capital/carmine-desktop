## 1. Dependencies and imports

- [x] 1.1 Add `Win32_System_Diagnostics_ToolHelp` feature to `windows-sys` in workspace `Cargo.toml` (line 84) for `CreateToolhelp32Snapshot`, `Process32FirstW`, `Process32NextW`, `PROCESSENTRY32W`, `TH32CS_SNAPPROCESS`
- [x] 1.2 Add `STATUS_CANCELLED` to the `windows_sys::Win32::Foundation` import in `winfsp_fs.rs` (line 12-16)

## 2. WinFsp caller PID extraction

- [x] 2.1 In `winfsp_fs.rs` open callback (~line 365-367): replace `let caller_pid: Option<u32> = None` with an `unsafe` call to `winfsp_sys::FspFileSystemOperationProcessIdF()`, wrapping the result as `Some(pid)` when non-zero, `None` when 0
- [x] 2.2 Update the code comment on line 365-366 to reflect that we now extract the PID (remove the "WinFsp doesn't expose caller PID" claim)

## 3. WinFsp error code change

- [x] 3.1 In `winfsp_fs.rs` `vfs_err_to_ntstatus()` (line 120): split `VfsError::CollabRedirect` from `PermissionDenied`, map it to `STATUS_CANCELLED` instead of `STATUS_ACCESS_DENIED`

## 4. Parent-PID resolution (Windows-only)

- [x] 4.1 In `process_filter.rs`: add a `#[cfg(target_os = "windows")]` function `resolve_parent_pid(pid: u32) -> Option<u32>` using `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)` + `Process32FirstW`/`Process32NextW` to find the entry whose `th32ProcessID == pid` and return `th32ParentProcessID`
- [x] 4.2 In `process_filter.rs` `is_interactive_shell()`: on Windows, if the direct PID name check fails (not in `KNOWN_SHELLS` or `extra_shells`), call `resolve_parent_pid(pid)` then `resolve_process_name(parent_pid)` and check the parent name against the same lists

## 5. Transient file filter in CollabGate guard

- [x] 5.1 In `core_ops.rs` CollabGate block (~line 1026-1031): after extracting the filename from `path`, call `is_transient_file(filename)` and skip the entire CollabGate block if it returns `true` — place this check before the `is_collaborative()` + `is_interactive` check

## 6. Remove Windows `is_interactive = true` hardcode

- [x] 6.1 In `core_ops.rs` (~line 1033-1042): remove the `if cfg!(target_os = "windows") { true }` branch; use the same `caller_pid`-based `is_interactive_shell()` check for all platforms (now that WinFsp provides the PID)

## 7. Deferred Office URI launch on Windows

- [x] 7.1 In `main.rs` `spawn_collab_handler()` (~line 1050-1073): add platform-conditional logic — on Windows (`#[cfg(target_os = "windows")]`), send `OpenOnline` response first, then sleep ~200ms, then call `handle_collab_open_online()`; on non-Windows, keep the current flow (call `handle_collab_open_online()` first, then respond)
- [x] 7.2 On Windows, if `handle_collab_open_online()` fails after the `OpenOnline` response was already sent, log a warning and send a failure notification (the VFS has already unblocked, so no `OpenLocally` fallback is possible)

## 8. Tests

- [x] 8.1 In `tests/test_collab_gate.rs`: add test `collab_gate_skips_transient_lock_file` — open `~$Budget.xlsx` from an interactive PID, verify CollabGate does NOT fire (file opens locally)
- [x] 8.2 In `tests/test_collab_gate.rs`: add test `collab_gate_skips_transient_temp_file` — open `~WRS0001.tmp` from an interactive PID, verify CollabGate does NOT fire
- [x] 8.3 In `tests/test_collab_gate.rs`: add test `collab_gate_fires_for_real_collaborative_file` — open `Budget.xlsx` from an interactive PID, verify CollabGate fires (ensures transient filter doesn't block real files)

## 9. CI validation

- [x] 9.1 Run `make clippy` — verify zero warnings across all targets/features
- [x] 9.2 Run `make test` — verify all tests pass including new ColllabGate tests
- [x] 9.3 Run `make build` — verify clean build
