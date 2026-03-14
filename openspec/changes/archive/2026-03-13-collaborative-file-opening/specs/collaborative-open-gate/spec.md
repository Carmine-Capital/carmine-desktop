## ADDED Requirements

### Requirement: File type classification for collaborative editing
The system SHALL provide a pure function `is_collaborative(extension)` that returns `true` for file types editable collaboratively via Microsoft 365 Online, and `false` otherwise.

Collaborative extensions: `.doc`, `.docx`, `.docm`, `.xls`, `.xlsx`, `.xlsm`, `.ppt`, `.pptx`, `.pptm`, `.odt`, `.ods`, `.odp`, `.vsdx`.

Non-collaborative extensions (open locally): `.pdf`, `.txt`, `.csv`, `.jpg`, `.png`, and all others.

#### Scenario: Office document is collaborative
- **WHEN** `is_collaborative(".docx")` is called
- **THEN** it returns `true`

#### Scenario: PDF is not collaborative
- **WHEN** `is_collaborative(".pdf")` is called
- **THEN** it returns `false`

#### Scenario: ODF format is collaborative
- **WHEN** `is_collaborative(".odt")` is called
- **THEN** it returns `true`

#### Scenario: Unknown extension is not collaborative
- **WHEN** `is_collaborative(".xyz")` is called
- **THEN** it returns `false`

#### Scenario: Case insensitive matching
- **WHEN** `is_collaborative(".DOCX")` is called
- **THEN** it returns `true`

### Requirement: Process filtering for interactive shell detection
The system SHALL identify whether a file open request originates from an interactive shell process by checking the caller's PID against a platform-specific list of known shell process names.

Known interactive shells:
- Windows: `explorer.exe`
- Linux: `nautilus`, `dolphin`, `thunar`, `nemo`, `pcmanfm`, `caja`
- macOS: `Finder`

The list of shell process names SHALL be configurable in the carminedesktop config.

#### Scenario: Explorer opens a file on Windows
- **WHEN** a file open request arrives with a caller PID
- **AND** the PID resolves to `explorer.exe`
- **THEN** the system identifies the request as interactive

#### Scenario: Antivirus scans a file on Windows
- **WHEN** a file open request arrives with a caller PID
- **AND** the PID resolves to a process not in the interactive shell list
- **THEN** the system identifies the request as non-interactive

#### Scenario: Nautilus opens a file on Linux
- **WHEN** a file open request arrives with a caller PID
- **AND** the PID resolves to `nautilus` via `/proc/<pid>/exe`
- **THEN** the system identifies the request as interactive

#### Scenario: PID resolution fails
- **WHEN** a file open request arrives with a caller PID
- **AND** the process name cannot be resolved (e.g., permission denied, process exited)
- **THEN** the system identifies the request as non-interactive (fail-safe to local open)

### Requirement: CollabGate intercepts open for collaborative files from interactive shells
When a collaborative file is opened by an interactive shell process, the VFS SHALL send a `CollabOpenRequest` to the Tauri app via an async channel and block on a oneshot reply before serving the file.

The `CollabOpenRequest` SHALL include: local path, file extension, DriveItem id, cached `web_url` (if available), and whether the file has unsynchronized local modifications.

The `CollabOpenResponse` SHALL be one of: `OpenLocally`, `OpenOnline`, or `Cancel`.

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
The VFS SHALL wait at most 15 seconds (configurable) for a `CollabOpenResponse`. If the timeout expires, the VFS SHALL serve the file locally and emit a `VfsEvent` to notify the user.

#### Scenario: Dialog answered within timeout
- **WHEN** a `CollabOpenRequest` is sent
- **AND** the Tauri app responds with `OpenLocally` within 15 seconds
- **THEN** the VFS serves the file locally

#### Scenario: Dialog times out
- **WHEN** a `CollabOpenRequest` is sent
- **AND** no response is received within 15 seconds
- **THEN** the VFS serves the file locally
- **AND** a `VfsEvent::CollabGateTimeout` is emitted

#### Scenario: Channel communication error
- **WHEN** a `CollabOpenRequest` is sent
- **AND** the oneshot receiver returns an error (sender dropped)
- **THEN** the VFS serves the file locally

### Requirement: Tauri app handles CollabOpenRequest with preference resolution
The Tauri app SHALL listen for `CollabOpenRequest` messages and resolve them using stored user preferences. If a preference exists and no local modifications conflict, the response is sent automatically. Otherwise, a native dialog is shown.

#### Scenario: User preference is "always online" with no local changes
- **WHEN** a `CollabOpenRequest` arrives for a `.docx` file
- **AND** user preference for `.docx` is `online`
- **AND** `has_local_changes` is `false`
- **THEN** the app responds `OpenOnline` without showing a dialog

#### Scenario: User preference is "always online" but local changes exist
- **WHEN** a `CollabOpenRequest` arrives for a `.docx` file
- **AND** user preference for `.docx` is `online`
- **AND** `has_local_changes` is `true`
- **THEN** the app shows a dialog warning about unsynchronized modifications

#### Scenario: No preference set
- **WHEN** a `CollabOpenRequest` arrives for a `.xlsx` file
- **AND** no user preference exists for `.xlsx`
- **THEN** the app shows the open mode dialog

#### Scenario: User preference is "always local"
- **WHEN** a `CollabOpenRequest` arrives for a `.pptx` file
- **AND** user preference for `.pptx` is `local`
- **THEN** the app responds `OpenLocally` without showing a dialog

### Requirement: Native dialog for collaborative open mode selection
The Tauri app SHALL display a native dialog with the file name, a message about collaborative editing availability, open mode buttons, and a "Remember my choice" option.

When `has_local_changes` is true, the dialog SHALL display an additional warning about unsynchronized local modifications.

#### Scenario: Dialog shown for file without local changes
- **WHEN** a dialog is shown for `report.docx`
- **AND** `has_local_changes` is `false`
- **THEN** the dialog displays "This file can be edited collaboratively."
- **AND** shows buttons "Open Locally" and "Open Online"
- **AND** shows a checkbox "Remember my choice for .docx files"

#### Scenario: Dialog shown for file with local changes
- **WHEN** a dialog is shown for `budget.xlsx`
- **AND** `has_local_changes` is `true`
- **THEN** the dialog displays an additional warning: "This file has unsynchronized local modifications. Opening online may cause conflicts."
- **AND** shows buttons "Open Locally" and "Open Online"

#### Scenario: User checks "Remember my choice"
- **WHEN** the user selects "Open Online"
- **AND** checks "Remember my choice for .docx files"
- **THEN** the preference `extensions.docx = "online"` is saved to config
- **AND** future `.docx` opens skip the dialog (unless local changes exist)

#### Scenario: User clicks Cancel or closes the dialog
- **WHEN** the user closes the dialog without selecting an option
- **THEN** the response is `Cancel`
- **AND** the VFS returns an error (file not opened)

### Requirement: Online open launches Office URI or browser
When `CollabOpenResponse` is `OpenOnline`, the system SHALL resolve the file's `web_url`, apply the Office URI scheme mapping on Windows/macOS, and open via the system shell. On Linux, it SHALL open the plain `web_url` in the default browser.

#### Scenario: Open Word document online on Windows
- **WHEN** response is `OpenOnline` for a `.docx` file
- **THEN** the system resolves `web_url` and opens `ms-word:ofe|u|<webUrl>`
- **AND** the VFS returns an error to prevent local open

#### Scenario: Open ODF document online (browser fallback)
- **WHEN** response is `OpenOnline` for an `.odt` file
- **THEN** the system resolves `web_url` and opens it in the default browser
- **AND** the VFS returns an error to prevent local open

#### Scenario: Open collaborative file online on Linux
- **WHEN** response is `OpenOnline` for a `.docx` file on Linux
- **THEN** the system resolves `web_url` and opens it in the default browser via `xdg-open`

#### Scenario: web_url resolution fails
- **WHEN** response is `OpenOnline`
- **AND** `web_url` cannot be resolved (not cached and Graph API fails)
- **THEN** the system falls back to local open
- **AND** shows a notification about the failure

### Requirement: Collaborative open preferences in config
The carminedesktop config SHALL support a `[collaborative_open]` section with:
- `enabled`: bool (master switch, default `true`)
- `default_action`: `"ask"` | `"online"` | `"local"` (default `"ask"`)
- `timeout_seconds`: u64 (default `15`)
- `shell_processes`: list of additional interactive shell process names
- `extensions.<ext>`: per-extension override (`"online"` | `"local"`)

#### Scenario: CollabGate disabled via config
- **WHEN** `collaborative_open.enabled` is `false`
- **THEN** the VFS never sends `CollabOpenRequest` messages
- **AND** all files open locally

#### Scenario: Global default action is "online"
- **WHEN** `collaborative_open.default_action` is `"online"`
- **AND** a collaborative file is opened by an interactive shell
- **AND** no per-extension override exists
- **AND** no local changes exist
- **THEN** the system responds `OpenOnline` without dialog

#### Scenario: Per-extension override takes precedence
- **WHEN** `collaborative_open.default_action` is `"online"`
- **AND** `collaborative_open.extensions.xlsx` is `"local"`
- **AND** an `.xlsx` file is opened by an interactive shell
- **THEN** the system responds `OpenLocally` without dialog

#### Scenario: Custom shell process added
- **WHEN** `collaborative_open.shell_processes` includes `"my-file-manager"`
- **AND** a file open request arrives from a process named `my-file-manager`
- **THEN** the system identifies the request as interactive
