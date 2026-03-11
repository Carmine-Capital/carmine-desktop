## ADDED Requirements

### Requirement: Register context menu on first active Windows mount
The system SHALL register the Windows Explorer "Open in SharePoint" context-menu command when CloudMount transitions from zero active CfApi mounts to one active CfApi mount.

#### Scenario: First mount starts
- **WHEN** a Windows CfApi mount completes successfully and no other CfApi mounts are currently active
- **THEN** the system creates (or updates) the required registry keys under `HKCU\\Software\\Classes\\*\\shell\\CloudMount.OpenInSharePoint` and sets the command to launch `cloudmount://open-online?path=%1`

#### Scenario: Additional mount starts
- **WHEN** a Windows CfApi mount completes successfully and at least one other CfApi mount is already active
- **THEN** the system does not remove or reinitialize lifecycle state and keeps the context-menu command available

### Requirement: Remove context menu only after last active Windows mount
The system SHALL remove the Windows Explorer "Open in SharePoint" context-menu command only when CloudMount transitions from one active CfApi mount to zero active CfApi mounts.

#### Scenario: Non-final unmount
- **WHEN** one Windows CfApi mount is unmounted while at least one other CfApi mount remains active
- **THEN** the system keeps the context-menu registry keys in place

#### Scenario: Final unmount
- **WHEN** the final active Windows CfApi mount is unmounted
- **THEN** the system removes the context-menu registry subtree for `CloudMount.OpenInSharePoint`

### Requirement: Idempotent registry lifecycle operations
The system SHALL treat pre-existing registry keys during registration and missing keys during cleanup as successful no-op states.

#### Scenario: Registration with pre-existing keys
- **WHEN** CloudMount attempts to register the context-menu and the target keys already exist
- **THEN** the system updates the expected default values and continues without failing the mount

#### Scenario: Cleanup with missing keys
- **WHEN** CloudMount attempts to remove the context-menu keys and they are already absent
- **THEN** the system treats cleanup as successful and continues unmount flow
