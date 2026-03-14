## Context

carminedesktop mounts OneDrive/SharePoint document libraries as local filesystems. When users double-click Office files, they open locally — losing access to real-time co-authoring. The existing `open_online` Tauri command can resolve local paths to SharePoint webUrls and launch Office URI schemes (`ms-word:ofe|u|<url>`), but it requires explicit invocation. There is no mechanism to intercept a standard file open and redirect it to collaborative mode.

The VFS layer (CoreOps) handles all `open()` calls from both FUSE and WinFsp backends. Both backends have access to the caller's process ID. The Tauri app already has an event/command system for communicating with the VFS.

## Goals / Non-Goals

**Goals:**
- Make collaborative editing the default experience for Office files opened from carminedesktop mounts
- Intercept file opens at the VFS level, filtered by caller process (interactive shells only)
- Present a native dialog for open mode selection with per-extension preference memory
- Warn users when local modifications conflict with online opening
- Provide explicit right-click context menu entries for both "Open Online" and "Open Locally"
- Work cross-platform: Office URI on Windows/macOS, browser on Linux

**Non-Goals:**
- Shell namespace extension or COM DLL on Windows (Approach C — rejected in favor of VFS interception)
- Intercepting opens from non-shell processes (IDEs, CLI tools, scripts)
- Collaborative editing for non-Office files that don't support browser editing
- Real-time sync between local and online editing sessions
- Replacing the existing `open_online` Tauri command (it remains as the backend)

## Decisions

### 1. VFS-level interception via async channel (CollabGate)

**Decision**: Insert a gate in `CoreOps::open_file()` that, for collaborative file types opened by interactive shell processes, sends a request over a `tokio::sync::mpsc` channel to the Tauri app and blocks on a `tokio::sync::oneshot` reply.

**Rationale**: This is the only approach that intercepts the open *before* the file is served, without requiring platform-specific shell extensions. The VFS already uses `rt.block_on()` for async operations, so blocking on a oneshot is consistent with the existing pattern.

**Alternatives considered**:
- *Shell extension (COM DLL)*: Best UX scoping but high complexity, risk of crashing Explorer, Windows-only
- *Registry verb override*: Simpler but system-wide scope for all Office files, not just carminedesktop mounts
- *Passive notification*: Simplest but poor UX — file opens locally first, user must manually switch

### 2. Process filtering by PID

**Decision**: Check the caller's PID against a known list of interactive shell process names per platform. On WinFsp, use the request's process info. On FUSE, use `fuse_context::pid()` and read `/proc/<pid>/exe` (Linux) or `libproc` (macOS).

**Known interactive shells**:
- Windows: `explorer.exe`
- Linux: `nautilus`, `dolphin`, `thunar`, `nemo`, `pcmanfm`, `caja`
- macOS: `Finder`

**Rationale**: Without process filtering, every file access (antivirus scan, thumbnail generation, indexing, search) would trigger a dialog. PID-based filtering is the least invasive approach that reliably distinguishes interactive opens.

**Trade-off**: Unknown or new file managers won't trigger CollabGate. This is acceptable — the context menu provides a manual fallback, and the shell list is configurable.

### 3. Timeout with local fallback

**Decision**: 15-second timeout on the oneshot channel. If the dialog isn't answered, fall back to local open and show a toast notification.

**Rationale**: VFS operations cannot block indefinitely — WinFsp and FUSE both have implicit timeout expectations. 15 seconds is generous enough for user interaction but prevents hung mounts if the Tauri app is unresponsive.

### 4. Per-extension preference storage in config

**Decision**: Store collaborative open preferences in the existing `carminedesktopConfig` structure under a new `[collaborative_open]` section. Per-extension overrides are a flat map (`extensions.docx = "online"`).

**Rationale**: Reuses the existing config system (TOML file + in-memory). No new storage mechanism needed. Per-extension granularity matches user expectations (always open Word online, but Excel locally).

### 5. CollabGate channel injection via CoreOps constructor

**Decision**: Pass an `Option<mpsc::Sender<CollabOpenRequest>>` to `CoreOps::new()`. When `None`, CollabGate is disabled (headless mode, tests). When `Some`, the gate activates for collaborative file types.

**Rationale**: Keeps CoreOps testable without a running Tauri app. The existing `VfsEventSender` pattern already uses optional channels for event delivery.

### 6. File type classification as a pure function

**Decision**: `is_collaborative(extension: &str) -> bool` as a pure function in `carminedesktop-core`, not configurable per-mount. Returns true for Office formats (.docx, .xlsx, .pptx and legacy/macro variants) plus ODF formats (.odt, .ods, .odp) that SharePoint Online can edit.

**Rationale**: The set of collaborative file types is determined by Microsoft 365 capabilities, not user preference. Keeping it as a pure function simplifies testing and avoids config bloat.

## Risks / Trade-offs

**[Risk] VFS thread blocked during dialog** → The open operation blocks a VFS thread while waiting for user input. Mitigated by the 15-second timeout and by the fact that VFS backends (WinFsp/FUSE) run on thread pools — one blocked thread doesn't stall the filesystem. If many collaborative files are opened simultaneously, multiple threads block; this is acceptable for interactive use.

**[Risk] Process name heuristic is fragile** → New file managers or renamed executables won't be recognized. Mitigated by making the shell process list configurable in config, and by the context menu fallback for manual triggering.

**[Risk] Race condition between dialog and file access** → While the dialog is shown, the calling process (Explorer) is waiting for the VFS to respond. If the user takes too long, Explorer may show a "not responding" indicator. Mitigated by the timeout, and by the fact that Office file opens are inherently slow (download) so users expect a brief delay.

**[Risk] Stale webUrl in cached DriveItem** → The cached `web_url` may be stale if the item was moved on the server. Mitigated by the existing `resolve_web_url()` fallback to Graph API when the cached URL fails.

**[Trade-off] Dialog fatigue before preferences are set** → First-time users see a dialog on every Office file open. Mitigated by the "Remember my choice" checkbox and by showing the dialog only for interactive shell opens, not programmatic access.
