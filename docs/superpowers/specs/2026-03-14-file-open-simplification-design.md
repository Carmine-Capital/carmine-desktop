# File Open Simplification — Design Spec

## Summary

Simplify the file opening architecture by removing the CollabGate interception from all platforms, removing all Linux file association/integration code, and ensuring macOS opens Office files in the browser (Office Online) instead of via URI schemes.

## Context

The current architecture has two parallel interception mechanisms:
1. **File associations** (shell_integration.rs) — register Carmine Desktop as handler for Office file types at the OS level
2. **CollabGate** (core_ops.rs) — intercept file opens at the VFS/FUSE/WinFsp layer, show dialog or fire background event

This dual mechanism is overly complex. File associations alone are sufficient for interception. CollabGate adds complexity (channels, cooldowns, process detection, timeouts) for marginal benefit.

Additionally, Linux file association code (.desktop files, xdg-mime, launch_desktop_exec) is unnecessary — the sole Linux user doesn't need it.

## Target Behavior by Platform

| Platform | Office files on mount | Non-Office files on mount | Files off mount |
|----------|----------------------|--------------------------|-----------------|
| **Linux** | Open locally (OS default) | Open locally (OS default) | N/A (no interception) |
| **macOS** | Open in browser (Office Online) | Open locally (OS default) | Pass to previous handler |
| **Windows** | Office URI scheme (ms-word:ofe\|u\|url) | Open locally | Pass to previous handler |

## Changes

### 1. CollabGate — Remove from ALL platforms

**Files affected:**

- `crates/carminedesktop-vfs/src/core_ops.rs`:
  - Remove `handle_collab_gate_fallback()` (Windows background event, ~L595-663)
  - Remove CollabGate blocking dialog (non-Windows, ~L1147-1292)
  - Remove `collab_tx` channel field from CoreOps
  - Remove cooldown tracking (`last_collab_open` or similar)
  - Remove `VfsEvent::CollabGateTimeout` and `VfsEvent::CollabOpenOnlineBackground` variants
  - Remove `VfsError::CollabRedirect` variant (~L424-425)
  - Remove `file_associations_registered` field and `with_file_associations_registered()` builder (~L509, ~L568) — only used by CollabGate
  - Simplify `open_file()` signature: `caller_pid` and `file_path` parameters become unused after CollabGate removal — remove them to avoid CI warnings (`RUSTFLAGS=-Dwarnings`)

- `crates/carminedesktop-vfs/src/fuse_fs.rs`:
  - Remove `collab_tx` and `collab_config` passing into CoreOps (~L96-130)
  - Remove `VfsError::CollabRedirect => Errno::EACCES` mapping (~L234)
  - Remove `file_associations_registered` passing

- `crates/carminedesktop-vfs/src/winfsp_fs.rs`:
  - Remove `collab_tx` and `collab_config` passing into CoreOps (~L247-265)
  - Remove `VfsError::CollabRedirect => STATUS_CANCELLED` mapping (~L121)
  - Remove `file_associations_registered` passing (~L1023-1063)

- `crates/carminedesktop-vfs/src/mount.rs`:
  - Remove `collab_tx`, `collab_config`, `file_associations_registered` threading through mount lifecycle (~L103-157)

- `crates/carminedesktop-vfs/src/process_filter.rs`:
  - Remove entire file — `is_interactive_shell()` and process detection logic are only used by CollabGate

- `crates/carminedesktop-core/src/types.rs`:
  - Remove `CollabOpenRequest` struct
  - Remove `CollabOpenResponse` enum

- `crates/carminedesktop-core/src/config.rs`:
  - Remove `CollaborativeOpenConfig` struct
  - Remove `default_collab_timeout()` function
  - Remove `collaborative_open` field from parent config

- `crates/carminedesktop-core/src/open_online.rs`:
  - Remove `is_collaborative()` function (~L45-62) — only used by CollabGate in core_ops.rs. Dead code after removal.

- `crates/carminedesktop-app/src/main.rs`:
  - Remove CollabGate channel creation (`collab_tx`/`collab_rx`)
  - Remove CollabGate config passing to mount initialization
  - Remove CollabGate event handling loop

- `crates/carminedesktop-app/src/notify.rs`:
  - Remove `collab_gate_timeout()` function (~L160)
  - Remove `collab_open_failed()` function (~L168)

**Result:** `open_file()` in core_ops.rs becomes a simple file-open that loads content into OpenFileTable with no interception logic.

### 2. Linux — Remove file association code

**Files affected:**

- `crates/carminedesktop-app/src/shell_integration.rs`:
  - Remove entire `linux` module (~L622-1059): .desktop file creation, xdg-mime registration, previous handler saving/restoring, `get_desktop_exec()`, `discover()`

- `crates/carminedesktop-app/src/commands.rs`:
  - Remove Linux fallback in `open_file()` (~L1197-1357)
  - Remove `launch_desktop_exec()` function (~L1522-1565)
  - Remove shell tokenizer function (~L1572-1592)

- `crates/carminedesktop-core/src/config.rs`:
  - Ensure `register_file_associations` defaults to `false` on Linux

**Result:** On Linux, no file association registration, no interception. Files on the mount open with the OS default handler like any normal filesystem.

### 3. macOS — Browser-only for Office files

**Files affected:**

- `crates/carminedesktop-core/src/open_online.rs`:
  - Change `office_uri_scheme()`: `cfg!(target_os = "linux")` → `cfg!(not(target_os = "windows"))`
  - This makes it return `None` on both Linux and macOS (and any future non-Windows platform), so `open_online()` always falls back to opening `web_url` in the browser

- `crates/carminedesktop-core/src/config.rs`:
  - Change `register_file_associations` default to `true` on macOS (currently true only on Windows)

- `crates/carminedesktop-app/src/shell_integration.rs`:
  - Verify/complete macOS module (~L1110-1449, public API delegates ~L1452-1494): `register()`, `unregister()`, `is_registered()`, `get_previous()`, `resolve_app_path()`, `discover()` via duti

**macOS flow:**
1. duti registers Carmine Desktop as handler for Office file types
2. User opens .docx in Finder → macOS launches `carminedesktop --open path`
3. `open_file()` detects path is on mount → calls `open_online()`
4. `office_uri_scheme()` returns `None` (macOS) → fallback to browser
5. Opens `item.web_url` in default browser (Office Online)

### 4. Windows — No functional changes

Windows behavior is unchanged. The only impact is removing `handle_collab_gate_fallback()` which fired a background `CollabOpenOnlineBackground` event when a user opened an Office file through the VFS while file associations were not registered. This is acceptable because file associations should always be registered on Windows (default: true) — the fallback was for an edge case that shouldn't occur in practice.

The existing flow remains:
1. Registry file associations → `carminedesktop --open path`
2. `open_file()` → on mount → `open_online()`
3. `office_uri_scheme()` returns scheme → tries Office URI (`ms-word:ofe|u|url`)
4. Fallback to browser if URI fails

### 5. Test impact

**Test files to remove entirely:**
- `crates/carminedesktop-vfs/tests/test_collab_gate.rs` (~572 lines)
- `crates/carminedesktop-vfs/tests/test_process_filter.rs` (~40 lines)

**Tests to remove (within existing files):**
- Linux shell_integration tests (.desktop file, xdg-mime, previous_handlers)
- `launch_desktop_exec()` and shell tokenizer tests

**Tests to adapt:**
- `open_file()` in core_ops tests: remove assertions on CollabRedirect, update call sites to match simplified signature (no caller_pid/file_path)
- `office_uri_scheme()`: verify returns `None` on macOS if platform-gated tests exist

**Tests to add:** None — simplified behavior (no VFS interception) is the default FUSE/WinFsp open behavior.

**Tests unchanged:**
- OpenFileTable, read/write/flush/release, streaming, writeback
- Shell integration Windows (registry, ProgID)
- Shell integration macOS (duti)

## Implementation order

To avoid intermediate compilation failures:

1. Core types/config removal (CollabOpenRequest, CollabOpenResponse, CollaborativeOpenConfig, is_collaborative)
2. VFS changes (core_ops.rs — CollabGate logic, VfsError::CollabRedirect, process_filter.rs)
3. Backend adapter changes (fuse_fs.rs, winfsp_fs.rs, mount.rs — remove collab plumbing)
4. App-level changes (main.rs, commands.rs, notify.rs, shell_integration.rs)
5. Config changes (office_uri_scheme, register_file_associations defaults)
6. Test cleanup (remove test files, adapt remaining tests)

## Non-goals

- No changes to the cache, graph, or auth crates
- No new UI for file opening
- No runtime configuration for open strategy (compile-time platform gates only)

## Frontend cleanup

Check for any collaborative-open references in `dist/` (CSS, JS, HTML). Remove settings UI elements related to CollabGate configuration (timeout, shell processes) if they exist.

## Risk

- **Low:** Windows is functionally unchanged (CollabGate background fallback removed but file associations cover the same use case)
- **Low:** Linux simplification removes code, doesn't add it
- **Medium:** macOS shell integration (duti) needs verification — existing code may need completion. Mitigated by the fact that `open_online()` browser fallback already works.
- **Low:** CI enforces zero warnings — implementation order above ensures no intermediate dead code warnings.
