## MODIFIED Requirements

### Requirement: Mount drive as native filesystem
The system SHALL mount a OneDrive or SharePoint drive as a native filesystem accessible by all applications on the operating system.

#### Scenario: Mount on Linux
- **WHEN** the user enables a mount on Linux
- **THEN** the system creates the mount point directory if it does not exist, mounts the drive using FUSE (libfuse3) at the configured path, and the directory becomes accessible to the user's applications via standard POSIX file operations

#### Scenario: Mount on macOS
- **WHEN** the user enables a mount on macOS
- **THEN** the system mounts the drive using macFUSE or FUSE-T at the configured path, and the volume appears in Finder

#### Scenario: Mount on Windows
- **WHEN** the user enables a mount on Windows
- **THEN** the system registers a Cloud Files API sync root at the configured folder path (default: `%USERPROFILE%\carminedesktop\<mount-name>`), populates the directory with placeholder files showing file names, sizes, and modification times, and the sync root appears as a first-class entry in File Explorer's navigation pane with cloud sync status icons
