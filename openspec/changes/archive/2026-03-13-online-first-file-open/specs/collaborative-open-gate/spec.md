## MODIFIED Requirements

### Requirement: CollabGate intercepts open for collaborative files from interactive shells
When a collaborative file is opened by an interactive shell process, the VFS SHALL send a `CollabOpenRequest` to the Tauri app via an async channel and block on a oneshot reply before serving the file.

The `CollabOpenRequest` SHALL include: local path, file extension, DriveItem id, and cached `web_url` (if available).

The `CollabOpenResponse` SHALL be one of: `OpenOnline` or `OpenLocally`.

#### Scenario: Interactive shell opens a collaborative file
- **WHEN** `explorer.exe` opens a `.docx` file on a carminedesktop mount
- **AND** CollabGate is enabled (channel is `Some`)
- **THEN** the VFS sends a `CollabOpenRequest` via the channel
- **AND** blocks on the oneshot reply

#### Scenario: Non-interactive process opens a collaborative file
- **WHEN** a non-shell process opens a `.docx` file on a carminedesktop mount
- **THEN** the VFS serves the file locally without sending a `CollabOpenRequest`

#### Scenario: Interactive shell opens a non-collaborative file
- **WHEN** `explorer.exe` opens a `.pdf` file on a carminedesktop mount
- **THEN** the VFS serves the file locally without sending a `CollabOpenRequest`

#### Scenario: CollabGate is disabled (headless mode)
- **WHEN** CoreOps was constructed with `collab_sender: None`
- **AND** any file is opened
- **THEN** the VFS serves all files locally without CollabGate checks

### Requirement: CollabGate timeout with local fallback
The VFS SHALL wait at most 15 seconds (configurable via TOML) for a `CollabOpenResponse`. If the timeout expires, the VFS SHALL serve the file locally and emit a `VfsEvent` to notify the user.

#### Scenario: Online open succeeds within timeout
- **WHEN** a `CollabOpenRequest` is sent
- **AND** the Tauri app responds with `OpenOnline` within 15 seconds
- **THEN** the VFS returns `CollabRedirect` to prevent local file access

#### Scenario: Timeout expires
- **WHEN** a `CollabOpenRequest` is sent
- **AND** no response is received within 15 seconds
- **THEN** the VFS serves the file locally
- **AND** a `VfsEvent::CollabGateTimeout` is emitted

#### Scenario: Channel communication error
- **WHEN** a `CollabOpenRequest` is sent
- **AND** the oneshot receiver returns an error (sender dropped)
- **THEN** the VFS serves the file locally

### Requirement: Tauri app always opens collaborative files online
The Tauri app SHALL listen for `CollabOpenRequest` messages and unconditionally attempt to open the file online via Office URI scheme or browser. No dialog SHALL be shown. No user preference SHALL be consulted.

#### Scenario: Collaborative file opened successfully online
- **WHEN** a `CollabOpenRequest` arrives for a `.docx` file
- **THEN** the app resolves the file's `web_url`
- **AND** opens via Office URI scheme (Windows/macOS) or browser (Linux)
- **AND** responds `OpenOnline`

#### Scenario: Online open fails
- **WHEN** a `CollabOpenRequest` arrives
- **AND** the online open fails (URI resolution error, Graph API unreachable)
- **THEN** the app shows a notification about the failure
- **AND** responds `OpenLocally` as fallback

### Requirement: Collaborative open config (TOML-only)
The carminedesktop config SHALL support a `[collaborative_open]` section with:
- `timeout_seconds`: u64 (default `15`) — CollabGate timeout
- `shell_processes`: list of additional interactive shell process names

This section SHALL NOT be exposed in any settings UI. It is a power-user TOML-only configuration.

#### Scenario: Custom timeout
- **WHEN** `collaborative_open.timeout_seconds` is set to `30` in the TOML config
- **THEN** the VFS waits up to 30 seconds for a CollabGate response

#### Scenario: Custom shell process added
- **WHEN** `collaborative_open.shell_processes` includes `"my-file-manager"`
- **AND** a file open request arrives from a process named `my-file-manager`
- **THEN** the system identifies the request as interactive

## REMOVED Requirements

### Requirement: Tauri app handles CollabOpenRequest with preference resolution
**Reason**: No user preferences exist — behavior is always online. Preference resolution logic is removed entirely.
**Migration**: None. Pre-production change.

### Requirement: Native dialog for collaborative open mode selection
**Reason**: No dialog is shown — online open is unconditional for collaborative files from interactive shells.
**Migration**: None. Pre-production change.

### Requirement: Collaborative open preferences in config
**Reason**: `enabled`, `default_action`, and `extensions` fields are removed. Only `timeout_seconds` and `shell_processes` remain as TOML-only settings. No settings UI.
**Migration**: None. Pre-production change.
