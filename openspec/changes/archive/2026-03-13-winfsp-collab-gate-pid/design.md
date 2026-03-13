## Context

The `online-first-file-open` change made CollabGate unconditionally open collaborative files online when intercepted from an interactive shell. Testing on Windows revealed two bugs:

1. **Duplicate workbook**: Excel says "cannot open two workbooks with the same name" because CollabGate fires for Excel's own `CreateFile` call (not just Explorer's), then `ms-excel:ofe|u|...` targets the same running Excel instance, creating a name collision.
2. **Lock file noise**: Office lock files (`~$Abo.xlsx`) have collaborative extensions (`.xlsx`), pass the CollabGate guard, and trigger spurious online-open attempts that fail with "item has no SharePoint URL."

Root cause: on Windows, `caller_pid` is hardcoded to `None` and `is_interactive` to `true` because the WinFsp Rust bindings were assumed to not expose caller PID. Investigation revealed the PID **is** available via `winfsp_sys::FspFileSystemOperationProcessIdF()` during Create/Open callbacks.

## Goals / Non-Goals

**Goals:**
- Fix both Windows-only CollabGate bugs (duplicate workbook + lock file noise)
- Use WinFsp's caller PID to properly filter interactive vs. non-interactive opens
- Ensure the Office URI scheme opens the file in desktop Excel without browser involvement
- Maintain existing Linux/macOS behavior unchanged

**Non-Goals:**
- Shell extension for intercepting the open before Explorer launches the app (future work)
- Changes to the Linux or macOS CollabGate flow
- Changes to the `is_collaborative()` extension list or cooldown mechanism
- Supporting non-Office applications for online open (e.g., LibreOffice with SharePoint)

## Decisions

### 1. Extract caller PID via `FspFileSystemOperationProcessIdF()`

**Decision**: Use the direct FFI call rather than `with_operation_request()` + manual `AccessToken` bit extraction.

**Rationale**: `FspFileSystemOperationProcessIdF()` is a single `unsafe` function call that returns `u32` directly. The alternative (`with_operation_request`) requires accessing the raw `FSP_FSCTL_TRANSACT_REQ`, checking `req.Kind == FspFsctlTransactCreateKind`, then extracting `(req.Req.Create.AccessToken >> 32) as u32`. The FFI function does exactly this internally and is the canonical WinFsp API for this purpose. Less unsafe code, same result.

**Alternative considered**: `with_operation_request()` — rejected because it's more code, more unsafe surface, and the FFI function encapsulates the same logic.

### 2. Parent-PID resolution via `CreateToolhelp32Snapshot`

**Decision**: On Windows, when the caller PID is not in `KNOWN_SHELLS`, resolve the parent process name and check that against `KNOWN_SHELLS` too.

**Rationale**: On Windows, Explorer never calls `CreateFile` directly when a user double-clicks a file. Explorer launches the associated app (e.g., `excel.exe "M:\Abo.xlsx"`), and Excel calls `CreateFile`. The caller PID resolves to `EXCEL.EXE`, which is not in `KNOWN_SHELLS`. But its parent is `explorer.exe`. Checking the parent catches this indirection.

**Implementation**: Add `resolve_parent_pid(pid: u32) -> Option<u32>` to `process_filter.rs` using `CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)` + `Process32FirstW`/`Process32NextW` to walk the process list and find the entry whose `th32ProcessID` matches, then return `th32ParentProcessID`. Then call `resolve_process_name(parent_pid)` and check against `KNOWN_SHELLS`.

**Alternative considered**: `NtQueryInformationProcess(ProcessBasicInformation)` — rejected because it requires `ntdll.dll` linkage and `PROCESS_BASIC_INFORMATION` struct layout, which is less stable than the Toolhelp API. The Toolhelp API is public, documented, and doesn't require opening the process handle.

**Feature gates**: The parent-PID check is `#[cfg(target_os = "windows")]` only. On Linux/macOS the file manager calls `open(2)` directly, so the caller PID is the file manager — no parent check needed.

### 3. Transient file filter in CollabGate guard

**Decision**: Add `is_transient_file(filename)` as an early-exit check in the CollabGate block, before the `is_collaborative()` + `is_interactive` check.

**Rationale**: The function already exists and is tested (9 positive + 5 negative cases). It catches `~$*.xlsx` lock files that would otherwise pass `is_collaborative()`. Placing it before the extension/interactive checks is both an optimization (no PID resolution for lock files) and a correctness fix.

**Call site**: In `core_ops.rs::open_file()`, after extracting the filename from the path but before the `is_collaborative(&ext) && is_interactive` check. The filename is available from the `file_path` parameter.

### 4. `STATUS_CANCELLED` for `CollabRedirect` on WinFsp

**Decision**: Map `VfsError::CollabRedirect` to `STATUS_CANCELLED` (0xC0000120) instead of `STATUS_ACCESS_DENIED`.

**Rationale**: `STATUS_ACCESS_DENIED` maps to Win32 `ERROR_ACCESS_DENIED`, which causes Excel to show a prominent "file locked" dialog with Read-Only/Notify options. `STATUS_CANCELLED` maps to `ERROR_OPERATION_ABORTED` (995), which semantically means "the operation was intentionally cancelled." Applications generally don't show dialogs or retry for cancelled operations — they treat it as "someone decided not to do this." This gives the best chance of Excel silently abandoning the local open.

**Alternative considered**:
- `STATUS_OBJECT_NAME_NOT_FOUND` ("file not found") — minimal dialog but semantically wrong; Explorer just showed the file in its listing.
- `STATUS_DELETE_PENDING` — apps may retry expecting the delete to complete.
- `STATUS_SHARING_VIOLATION` — Excel retries aggressively with exponential backoff, then shows "File In Use" dialog.

**FUSE unchanged**: On Linux, `CollabRedirect` still maps to `EACCES`. The file manager (Nautilus) handles EACCES gracefully and doesn't launch the application. No change needed.

### 5. Deferred Office URI launch on Windows

**Decision**: On Windows, the collab handler responds `OpenOnline` to the VFS **immediately** (before launching the Office URI), then waits ~200ms, then launches `ms-excel:ofe|u|...`.

**Rationale**: The current flow launches the Office URI first, then responds. This means the VFS is blocked (holding `CreateFile`) while the URI is dispatched. The `ms-excel:ofe|u|...` URI uses DDE/COM to message the same running Excel instance, potentially registering the filename before Excel processes the `STATUS_CANCELLED` from the VFS. By responding first, we ensure:
1. `CreateFile` returns `STATUS_CANCELLED` immediately
2. Excel processes the cancellation and abandons the filename
3. 200ms later, the Office URI arrives with no name collision

**Implementation**: Platform-conditional logic in `spawn_collab_handler()`:
```
#[cfg(target_os = "windows")]  → respond first, sleep, then launch URI
#[cfg(not(target_os = "windows"))] → launch URI first, then respond (current behavior)
```

The 200ms delay is a heuristic. If testing shows it's insufficient, it can be tuned via `collaborative_open.uri_delay_ms` in TOML (not exposed in this change — only if needed).

### 6. `is_interactive_shell` signature change

**Decision**: Modify `is_interactive_shell()` to internally handle the parent-PID check on Windows, keeping the existing `(pid, extra_shells)` signature.

**Rationale**: The caller (`core_ops.rs`) shouldn't need to know about platform-specific parent-PID logic. The function already has platform-specific `resolve_process_name()` implementations. Adding platform-specific parent resolution inside the function keeps the abstraction clean.

**Implementation**: On Windows, if the direct PID check fails (not in `KNOWN_SHELLS`), call `resolve_parent_pid(pid)` and check the parent's name. On Linux/macOS, return `false` immediately if the direct check fails (no parent check).

## Risks / Trade-offs

**[Risk: 200ms delay insufficient]** → The deferred URI launch relies on Excel processing `STATUS_CANCELLED` within 200ms. If Excel is slow to start or under heavy load, the race condition could reappear. **Mitigation**: The delay is conservative (most IPC processing happens in <50ms). Can be increased if testing shows issues. Worst case is the existing "two workbooks" error, which is no worse than current behavior.

**[Risk: `STATUS_CANCELLED` behavior varies across Office versions]** → The analysis is based on general Windows API contracts, not exhaustive testing across Excel 2016/2019/2021/365. **Mitigation**: Test on the target Office version (365). If `STATUS_CANCELLED` causes unexpected behavior, `STATUS_OBJECT_NAME_NOT_FOUND` is the fallback candidate.

**[Risk: Parent PID not always Explorer]** → If the user opens a file from a non-Explorer shell (PowerShell `Start-Process`, Total Commander, etc.), the parent PID won't be `explorer.exe`. **Mitigation**: The `shell_processes` TOML config allows adding custom file managers. PowerShell/cmd-launched opens are intentionally non-interactive (programmatic access should not trigger CollabGate).

**[Risk: Process tree walking overhead]** → `CreateToolhelp32Snapshot` enumerates all processes on the system. **Mitigation**: Only called when the caller PID is not directly in `KNOWN_SHELLS` (most opens from non-interactive processes will be caught by the first check). The snapshot is lightweight (<1ms on modern systems) and only happens once per CollabGate fire (cooldown prevents repeated calls).

**[Trade-off: Windows-only complexity]** → This change adds Windows-specific code paths (parent PID, deferred URI, `STATUS_CANCELLED`) that don't exist on Linux/macOS. This is inherent to the platform differences (WinFsp vs FUSE, Shell behavior, Office URI schemes) and cannot be unified.
