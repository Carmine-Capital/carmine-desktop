## MODIFIED Requirements

### Requirement: Register context menu on first active Windows mount
The system SHALL register two context menu entries under `HKCU\Software\Classes\*\shell\` on the first active WinFsp mount:

1. `CloudMount.OpenOnline` with label "Open Online (SharePoint)" and command `cloudmount://open-online?path=%1`
2. `CloudMount.OpenLocally` with label "Open Locally" and command that opens the file via the default system handler

Both entries SHALL use the same reference-counted lifecycle: registered when mount count goes from 0 to 1, removed when it goes from 1 to 0.

#### Scenario: First mount starts
- **WHEN** the first WinFsp mount starts (active mount count transitions from 0 to 1)
- **THEN** the system registers both `CloudMount.OpenOnline` and `CloudMount.OpenLocally` under `HKCU\Software\Classes\*\shell\`

#### Scenario: Additional mount starts
- **WHEN** a subsequent WinFsp mount starts (active mount count is already >= 1)
- **THEN** the system does NOT re-register context menu entries

### Requirement: Remove context menu only after last active Windows mount
The system SHALL remove both `CloudMount.OpenOnline` and `CloudMount.OpenLocally` registry entries only when the active mount count transitions from 1 to 0.

#### Scenario: Non-final unmount
- **WHEN** a WinFsp mount is unmounted but other active mounts remain
- **THEN** the context menu entries are NOT removed

#### Scenario: Final unmount
- **WHEN** the last active WinFsp mount is unmounted (count transitions from 1 to 0)
- **THEN** both `CloudMount.OpenOnline` and `CloudMount.OpenLocally` registry entries are removed

### Requirement: Idempotent registry lifecycle operations
Registration and cleanup operations SHALL be idempotent.

#### Scenario: Registration with pre-existing keys
- **WHEN** context menu registration is triggered
- **AND** one or both registry keys already exist
- **THEN** the operation succeeds (overwrite existing entries)

#### Scenario: Cleanup with missing keys
- **WHEN** context menu cleanup is triggered
- **AND** one or both registry keys do not exist
- **THEN** the operation succeeds (no-op for missing entries)
