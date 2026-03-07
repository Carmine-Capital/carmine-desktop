## ADDED Requirements

### Requirement: Stale FUSE mount detection and cleanup
The system SHALL detect and attempt to clean up stale FUSE mounts before mounting a drive. A stale mount occurs when a previous FUSE daemon exited without proper unmount (crash, kill signal, or `auto_unmount` not supported).

#### Scenario: Stale mount detected via stat
- **WHEN** the system checks a mountpoint path and `stat` returns ENOTCONN (errno 107, "Transport endpoint is not connected") or EIO (errno 5)
- **THEN** the system identifies the path as a stale FUSE mount and attempts cleanup

#### Scenario: Cleanup via fusermount on Linux
- **WHEN** a stale mount is detected on Linux
- **THEN** the system attempts `fusermount3 -u <path>` first; if `fusermount3` is not available or fails, it attempts `fusermount -u <path>`; the result (success or failure) is logged

#### Scenario: Cleanup via umount on macOS
- **WHEN** a stale mount is detected on macOS
- **THEN** the system attempts `umount <path>` to clean up the stale mount

#### Scenario: Cleanup succeeds
- **WHEN** stale mount cleanup succeeds (fusermount/umount returns exit code 0)
- **THEN** the system logs an info message and the mountpoint path becomes a regular directory accessible for `create_dir_all` and subsequent FUSE mount

#### Scenario: Cleanup fails
- **WHEN** stale mount cleanup fails (fusermount/umount returns non-zero or is not found)
- **THEN** the system logs a warning with the error details and an actionable message suggesting manual cleanup (e.g., "run `fusermount -u <path>` manually"), and returns false to indicate the mountpoint is not usable

#### Scenario: Path is not a stale mount
- **WHEN** the system checks a mountpoint path and `stat` succeeds (returns valid metadata) or the path does not exist
- **THEN** the system takes no cleanup action and proceeds with normal mount setup
