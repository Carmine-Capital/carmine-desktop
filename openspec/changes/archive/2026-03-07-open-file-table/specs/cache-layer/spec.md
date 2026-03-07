## MODIFIED Requirements

### Requirement: Write-back buffer
The system SHALL buffer file writes locally and upload them asynchronously. The writeback buffer SHALL serve as the persistence/crash-safety layer — it is written to on `flush`/`release`, not on every individual `write()` call.

#### Scenario: Write buffered locally
- **WHEN** a file with pending writes is flushed or released
- **THEN** the system writes the complete content from the `OpenFile` buffer to the writeback buffer and returns success to the caller without waiting for upload

#### Scenario: Buffer flushed on close
- **WHEN** a file with buffered writes is closed
- **THEN** the system initiates an asynchronous upload of the complete file to the Graph API

#### Scenario: Buffer flushed on sync
- **WHEN** the application receives an `fsync` call for a file with buffered writes
- **THEN** the system writes the `OpenFile` buffer to the writeback buffer, uploads the buffered content to the Graph API, and blocks until the upload completes

#### Scenario: Unflushed writes on crash
- **WHEN** the application terminates unexpectedly with writes in the buffer
- **THEN** on next start, the system detects pending uploads in the buffer directory and resumes uploading them
