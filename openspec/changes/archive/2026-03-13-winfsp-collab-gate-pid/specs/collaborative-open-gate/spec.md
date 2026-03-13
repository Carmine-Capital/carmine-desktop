## MODIFIED Requirements

### Requirement: Process filtering for interactive shell detection
The system SHALL identify whether a file open request originates from an interactive shell process by checking the caller's PID against a platform-specific list of known shell process names.

Known interactive shells:
- Windows: `explorer.exe`
- Linux: `nautilus`, `dolphin`, `thunar`, `nemo`, `pcmanfm`, `caja`
- macOS: `Finder`

The list of shell process names SHALL be configurable in the CloudMount config.

On Windows, the system SHALL also check the **parent process** of the caller against `KNOWN_SHELLS`. This is necessary because Windows Explorer does not call `CreateFile` directly when a user double-clicks a file — it launches the associated application (e.g., Excel), which then calls `CreateFile`. The caller PID resolves to the application, but its parent PID resolves to `explorer.exe`.

The parent-PID check SHALL use `CreateToolhelp32Snapshot` with `TH32CS_SNAPPROCESS` to enumerate process entries and find the parent via `th32ParentProcessID`. If snapshot creation or parent resolution fails, the system SHALL fall back to checking only the caller PID (fail-safe: non-interactive).

The parent-PID check SHALL only be performed on Windows. On Linux and macOS, the file manager process calls `open(2)` directly, so the caller PID is sufficient.

#### Scenario: Explorer opens a file on Windows
- **WHEN** a file open request arrives with a caller PID
- **AND** the PID resolves to `explorer.exe`
- **THEN** the system identifies the request as interactive

#### Scenario: Excel launched by Explorer opens a file on Windows
- **WHEN** a file open request arrives with a caller PID
- **AND** the caller PID resolves to `EXCEL.EXE`
- **AND** the caller's parent PID resolves to `explorer.exe`
- **THEN** the system identifies the request as interactive

#### Scenario: Excel launched by a script opens a file on Windows
- **WHEN** a file open request arrives with a caller PID
- **AND** the caller PID resolves to `EXCEL.EXE`
- **AND** the caller's parent PID resolves to `cmd.exe`
- **THEN** the system identifies the request as non-interactive

#### Scenario: Antivirus scans a file on Windows
- **WHEN** a file open request arrives with a caller PID
- **AND** the PID resolves to a process not in the interactive shell list
- **AND** the parent PID resolves to a process not in the interactive shell list
- **THEN** the system identifies the request as non-interactive

#### Scenario: Nautilus opens a file on Linux
- **WHEN** a file open request arrives with a caller PID
- **AND** the PID resolves to `nautilus` via `/proc/<pid>/exe`
- **THEN** the system identifies the request as interactive

#### Scenario: PID resolution fails
- **WHEN** a file open request arrives with a caller PID
- **AND** the process name cannot be resolved (e.g., permission denied, process exited)
- **THEN** the system identifies the request as non-interactive (fail-safe to local open)

#### Scenario: Parent PID resolution fails on Windows
- **WHEN** a file open request arrives with a caller PID on Windows
- **AND** the caller PID resolves to `EXCEL.EXE` (not in KNOWN_SHELLS)
- **AND** the parent PID cannot be resolved (snapshot fails)
- **THEN** the system identifies the request as non-interactive (fail-safe to local open)

### Requirement: CollabGate intercepts open for collaborative files from interactive shells
When a collaborative file is opened by an interactive shell process, the VFS SHALL send a `CollabOpenRequest` to the Tauri app via an async channel and block on a oneshot reply before serving the file.

The `CollabOpenRequest` SHALL include: local path, file extension, DriveItem id, and cached `web_url` (if available).

The `CollabOpenResponse` SHALL be one of: `OpenOnline` or `OpenLocally`.

The CollabGate guard SHALL skip files matching transient patterns (as defined by `is_transient_file()`). Office lock files like `~$Report.xlsx` have collaborative extensions but are local-only artifacts that do not exist on OneDrive and SHALL NOT trigger online-open attempts.

#### Scenario: Interactive shell opens a collaborative file
- **WHEN** `explorer.exe` (or an app launched by Explorer) opens a `.docx` file on a CloudMount mount
- **AND** CollabGate is enabled (channel is `Some`)
- **THEN** the VFS sends a `CollabOpenRequest` via the channel
- **AND** blocks on the oneshot reply

#### Scenario: Non-interactive process opens a collaborative file
- **WHEN** a non-shell process opens a `.docx` file on a CloudMount mount
- **THEN** the VFS serves the file locally without sending a `CollabOpenRequest`

#### Scenario: Interactive shell opens a non-collaborative file
- **WHEN** `explorer.exe` opens a `.pdf` file on a CloudMount mount
- **THEN** the VFS serves the file locally without sending a `CollabOpenRequest`

#### Scenario: CollabGate is disabled (headless mode)
- **WHEN** CoreOps was constructed with `collab_sender: None`
- **AND** any file is opened
- **THEN** the VFS serves all files locally without CollabGate checks

#### Scenario: Office lock file opened
- **WHEN** an interactive process opens `~$Report.xlsx` on a CloudMount mount
- **AND** `is_transient_file("~$Report.xlsx")` returns `true`
- **THEN** the VFS serves the file locally without sending a `CollabOpenRequest`

#### Scenario: Office temp file opened
- **WHEN** any process opens `~WRS0001.tmp` on a CloudMount mount
- **AND** `is_transient_file("~WRS0001.tmp")` returns `true`
- **THEN** the VFS serves the file locally without sending a `CollabOpenRequest`

### Requirement: Tauri app always opens collaborative files online
The Tauri app SHALL listen for `CollabOpenRequest` messages and unconditionally attempt to open the file online via Office URI scheme or browser. No dialog SHALL be shown. No user preference SHALL be consulted.

On Windows, the handler SHALL respond `OpenOnline` to the VFS **before** launching the Office URI. After responding, the handler SHALL wait approximately 200 milliseconds, then launch the Office URI scheme (`ms-excel:ofe|u|...`). This deferred launch ensures the VFS returns an error to the calling application before the Office URI arrives at the same application, preventing duplicate-workbook name collisions.

On Linux and macOS, the handler SHALL launch the browser/URI first, then respond (current behavior unchanged).

#### Scenario: Collaborative file opened on Windows with deferred URI
- **WHEN** a `CollabOpenRequest` arrives for a `.xlsx` file on Windows
- **THEN** the handler responds `OpenOnline` immediately
- **AND** waits ~200ms
- **AND** launches `ms-excel:ofe|u|<webUrl>`
- **AND** the VFS returns an error to the caller before the Office URI is processed

#### Scenario: Collaborative file opened on Linux
- **WHEN** a `CollabOpenRequest` arrives for a `.docx` file on Linux
- **THEN** the handler opens the `web_url` in the default browser via `xdg-open`
- **AND** responds `OpenOnline`

#### Scenario: Online open fails on Windows
- **WHEN** a `CollabOpenRequest` arrives on Windows
- **AND** the handler cannot resolve a SharePoint URL or the URI launch fails
- **THEN** the handler responds `OpenLocally` as fallback
- **AND** shows a notification about the failure
- **AND** does NOT attempt the deferred URI launch
