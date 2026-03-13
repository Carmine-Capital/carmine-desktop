## Context

The `collaborative-file-opening` change introduced a full dialog-based workflow for opening Office files: CollabGate intercepts VFS opens, sends a request to Tauri, which either auto-resolves via stored preferences or shows a native dialog. This works but is over-engineered for a pre-launch product where online-first is the correct default. The dialog, per-extension preferences, config UI, and context menu entries all serve a "let the user choose" philosophy that adds friction without clear benefit.

The core CollabGate mechanism (VFS interception, process filtering, async channel to Tauri, Office URI resolution) is sound and stays. Everything built on top to support user choice is removed.

## Goals / Non-Goals

**Goals:**
- Make online opening the hardcoded default for collaborative files — no user interaction required
- Remove all user-facing choice mechanisms: dialog, settings UI, context menu entries
- Eliminate dead code paths created by removing choice: `Cancel` response, `Ask` action, `OperationCancelled` error, `has_local_changes` detection
- Simplify `CollaborativeOpenConfig` to power-user-only TOML settings (timeout, extra shell processes)

**Non-Goals:**
- Changing how non-collaborative files open (stays local, unchanged)
- Modifying the process filtering logic (stays as-is)
- Implementing the future "download folder locally for a time" feature (separate proposal)
- Adding migration logic for existing context menu installations (pre-production, no users to migrate)

## Decisions

### 1. Delete integration modules rather than stub them

**Decision**: Delete `linux_integrations.rs`, `windows_integrations.rs`, and `macos_integrations.rs` entirely rather than keeping empty stubs or cleanup-only code.

**Rationale**: No production users exist, so no migration is needed. Keeping cleanup code for entries that were never deployed outside development adds maintenance cost for no benefit.

### 2. Remove `Ask` variant rather than change default

**Decision**: Remove `CollabDefaultAction` enum entirely instead of changing the default from `Ask` to `Online`.

**Rationale**: With no settings UI and no dialog, the concept of a "default action" is meaningless — the action is always online. Keeping the enum with unused variants creates dead code. If a future feature needs user choice again, it will be a different mechanism.

### 3. Keep `CollabOpenResponse::OpenLocally` as error fallback

**Decision**: Keep the `OpenLocally` variant even though the intended path is always `OpenOnline`. Remove `Cancel`.

**Rationale**: `OpenLocally` is still needed as a fallback when the online open fails (Office URI resolution failure, Graph API unreachable). The `Cancel` variant was only reachable from the dialog and becomes dead code.

### 4. Remove `has_local_changes` from `CollabOpenRequest`

**Decision**: Stop computing and transmitting whether the file has pending local writes.

**Rationale**: This field only existed to force the dialog when local modifications were present. With no dialog, the field is unused. The writeback system handles uploading local changes independently — CollabGate doesn't need to know about them.

### 5. Simplify `spawn_collab_handler` to unconditional online open

**Decision**: The handler receives a request, immediately calls `handle_collab_open_online()`, and replies `OpenOnline` on success or `OpenLocally` on failure. No preference resolution, no dialog branch.

**Rationale**: This is the minimal logic needed. The handler becomes a straightforward try-online-then-fallback pipeline.

## Risks / Trade-offs

**[Risk] User has no way to open a collaborative file locally** → The VFS still serves the file locally for non-interactive processes (CLI tools, scripts, IDEs). Users who need local access can open via terminal (`xdg-open`, `open`, `start`) or any non-shell process. A dedicated "download folder locally" feature is planned as a separate proposal.

**[Trade-off] Power users lose per-extension control** → Removing per-extension overrides means a user who wants Excel files local but Word files online cannot configure this. Acceptable pre-launch; the future offline feature will address this with a more coherent mechanism.

**[Trade-off] No graceful degradation to dialog** → If the online open fails (no internet, broken URI), the fallback is local open with a notification — not a dialog asking what to do. This is intentional: the system is opinionated, and error recovery should be automatic.
