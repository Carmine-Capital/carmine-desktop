## Why

The CollabGate on Windows has two critical bugs discovered during testing of the `online-first-file-open` change. First, CollabGate fires for ALL callers (including Excel itself) because WinFsp was assumed to not expose caller PID — but it does, via `FspFileSystemOperationProcessIdF()`. This causes a race condition where `ms-excel:ofe|u|...` targets the same Excel instance that triggered the open, producing "Cannot open two workbooks with the same name." Second, Office lock files (`~$Abo.xlsx`) pass the `is_collaborative()` check and trigger spurious online-open attempts that fail with "item has no SharePoint URL."

## What Changes

- **Extract caller PID from WinFsp**: Use `winfsp_sys::FspFileSystemOperationProcessIdF()` (available in winfsp-sys 0.12.1) to get the calling process PID during Create/Open callbacks. Pass it to `CoreOps::open_file()` instead of `None`.
- **Add parent-PID resolution on Windows**: On Windows, Explorer never calls `CreateFile` directly for a double-click — it launches the associated app. Add a `resolve_parent_process_name()` function so `is_interactive_shell` can check the caller's parent against `KNOWN_SHELLS` (e.g., Excel started by Explorer = interactive).
- **Filter transient files in CollabGate**: Add an `is_transient_file()` check to the CollabGate guard in `CoreOps::open_file()`, blocking lock files like `~$Abo.xlsx` from triggering online open.
- **Change WinFsp CollabRedirect error code**: Replace `STATUS_ACCESS_DENIED` with `STATUS_CANCELLED` for `VfsError::CollabRedirect` on WinFsp. `STATUS_CANCELLED` maps to `ERROR_OPERATION_ABORTED` — apps generally don't show dialogs or retry for cancelled operations, unlike `ACCESS_DENIED` which triggers Excel's "file locked" dialog.
- **Deferred Office URI launch on Windows**: Restructure the collab handler so on Windows it responds `OpenOnline` immediately (unblocking the VFS fast), then waits ~200ms before launching the Office URI. This ensures Excel processes the `STATUS_CANCELLED` and abandons the local filename before the `ms-excel:ofe|u|...` URI arrives, avoiding the duplicate-workbook name collision.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `collaborative-open-gate`: Add parent-PID resolution for Windows interactive-shell detection. Add transient-file filtering in CollabGate guard. Add deferred Office URI launch on Windows.
- `winfsp-filesystem`: Change `CollabRedirect` error mapping from `STATUS_ACCESS_DENIED` to `STATUS_CANCELLED`. Extract caller PID via `FspFileSystemOperationProcessIdF()` in the `open` callback.
- `temp-file-upload-filter`: Extend `is_transient_file()` usage from upload-only to also cover CollabGate filtering (no changes to the function itself, just a new call site).

## Impact

- **carminedesktop-vfs** (`process_filter.rs`): New `resolve_parent_process_name()` function, updated `is_interactive_shell()` to check parent PID on Windows.
- **carminedesktop-vfs** (`core_ops.rs`): Add `is_transient_file()` guard in CollabGate block.
- **carminedesktop-vfs** (`winfsp_fs.rs`): Extract caller PID via FFI call, change `CollabRedirect` NTSTATUS mapping.
- **carminedesktop-app** (`main.rs`): Platform-conditional handler logic — on Windows, respond before launching URI with a delay.
- **carminedesktop-vfs/tests/**: Update CollabGate tests for transient-file filtering and new Windows PID behavior.
- **Dependencies**: No new crates. Uses existing `winfsp-sys` FFI (`FspFileSystemOperationProcessIdF`), existing `windows-sys` (`STATUS_CANCELLED`, process query APIs).
