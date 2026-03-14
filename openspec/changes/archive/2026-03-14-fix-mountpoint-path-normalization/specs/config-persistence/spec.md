## ADDED Requirements

### Requirement: Mount point path normalization
The system SHALL normalize expanded mount point paths to use OS-native path separators and SHALL strip any trailing separator before the path is used for filesystem operations or passed to the virtual filesystem backend.

#### Scenario: Forward slashes normalized to backslashes on Windows
- **WHEN** a mount point template `~/Cloud/OneDrive` is expanded on Windows
- **THEN** the expanded path uses backslash separators throughout (e.g. `C:\Users\nyxa\Cloud\OneDrive`), with no forward slashes remaining

#### Scenario: Trailing separator stripped
- **WHEN** a mount point template has a trailing `/` or `\` (e.g. `~/Cloud/MyDrive/`)
- **THEN** the expanded path has no trailing separator (e.g. `~/Cloud/MyDrive` becomes `C:\Users\nyxa\Cloud\MyDrive` on Windows or `/home/nyxa/Cloud/MyDrive` on Linux)

#### Scenario: Bare drive root preserved on Windows
- **WHEN** a mount point expands to a bare drive root like `C:\`
- **THEN** the trailing backslash is preserved because `C:` without the separator changes meaning on Windows

#### Scenario: Defensive normalization at mount time
- **WHEN** a mount point path reaches the mount setup function
- **THEN** the system SHALL strip trailing separators as a safety net regardless of whether `expand_mount_point` was the source of the path

### Requirement: Mount point templates stored without trailing separators
The system SHALL strip trailing path separators from mount point values before persisting them to the configuration file.

#### Scenario: SharePoint mount created with trailing separator in input
- **WHEN** a SharePoint mount is added and the derived mount point template ends with `/` or `\`
- **THEN** the persisted mount point template in the configuration file has no trailing separator

#### Scenario: OneDrive mount created with clean template
- **WHEN** an OneDrive mount is added
- **THEN** the persisted mount point template has no trailing separator
