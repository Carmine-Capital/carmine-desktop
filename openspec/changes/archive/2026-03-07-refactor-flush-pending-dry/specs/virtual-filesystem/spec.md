## ADDED Requirements

### Requirement: Pending writes flushed on unmount via shared implementation
On unmount, both the FUSE and CfApi backends SHALL flush any pending write-back
uploads for the unmounting drive using a single shared implementation. The flush
logic SHALL NOT be duplicated per platform.

The flush procedure SHALL:
- List all pending write-back entries for the drive being unmounted.
- Upload each pending entry to the Graph API.
- Remove each entry from the write-back buffer upon successful upload.
- Enforce a maximum flush duration of 30 seconds; if exceeded, log a warning and
  proceed with unmount (data remains in the write-back buffer for crash recovery).

#### Scenario: Pending writes present on unmount
- **WHEN** a mount is stopped with one or more entries in the write-back buffer for that drive
- **THEN** the system SHALL attempt to upload all pending entries before completing the unmount
- **THEN** successfully uploaded entries SHALL be removed from the write-back buffer
- **THEN** the unmount SHALL complete within 30 seconds regardless of upload outcome

#### Scenario: No pending writes on unmount
- **WHEN** a mount is stopped with no entries in the write-back buffer for that drive
- **THEN** the system SHALL skip the flush step and unmount immediately

#### Scenario: Flush timeout exceeded
- **WHEN** uploading pending writes takes longer than 30 seconds
- **THEN** the system SHALL log a warning indicating how many writes remain pending
- **THEN** the unmount SHALL proceed (remaining writes are preserved in the write-back buffer for crash recovery on next launch)
