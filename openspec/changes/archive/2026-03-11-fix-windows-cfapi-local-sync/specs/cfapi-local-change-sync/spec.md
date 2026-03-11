## ADDED Requirements

### Requirement: Windows local non-placeholder changes are ingested for upload
On Windows, the system SHALL detect local file creation and copy-in operations inside a mounted sync root even when no placeholder `closed()` callback is raised. The system SHALL stage file content through the writeback pipeline and trigger upload using the same conflict-safe `CoreOps` flow used by placeholder-backed writes.

#### Scenario: External file copied into sync root
- **WHEN** a user copies a file from outside the mounted drive into the mounted Windows sync root
- **THEN** the system detects the new local file, associates it with its parent folder in the VFS model, stages its bytes in writeback storage, and uploads it to Graph without requiring restart or unmount

#### Scenario: Local file created by application save
- **WHEN** an application creates a new file directly in the mounted sync root using standard filesystem APIs
- **THEN** the system ingests the file as pending local content and uploads it through the normal flush pipeline

#### Scenario: Path cannot be ingested immediately
- **WHEN** a local file change is detected but parent/item resolution is temporarily unavailable
- **THEN** the system records a retryable pending ingest state and logs the reason, rather than silently dropping the change

### Requirement: Safe-save transactions preserve final user intent
The system SHALL handle safe-save sequences (temporary file write + replacement/rename of target) as a transactional local update. The system SHALL avoid committing intermediate temporary renames to Graph when a replacement transaction is still in progress, and SHALL synchronize the final user-visible file state.

#### Scenario: Office-style safe-save replace
- **WHEN** an editor performs safe-save by writing a temporary file and replacing `report.xlsx`
- **THEN** the system synchronizes the final `report.xlsx` content and SHALL NOT leave the remote item renamed to an intermediate backup/temp name

#### Scenario: Genuine user rename
- **WHEN** a user explicitly renames `report.xlsx` to `report-final.xlsx` and no replacement transaction follows
- **THEN** the system propagates the rename to Graph as a normal rename operation

### Requirement: Pending writeback is retried while mount is active
The system SHALL retry failed pending writeback uploads during normal mounted runtime, without requiring restart, re-auth trigger, or unmount. Retry processing SHALL reuse existing pending-write persistence so no pending content is lost between attempts.

#### Scenario: Transient network failure recovers in-session
- **WHEN** an upload fails due to a transient network error and pending writeback remains queued
- **THEN** the mounted retry loop retries later and uploads successfully once connectivity returns

#### Scenario: Persistent failure remains visible
- **WHEN** uploads continue to fail across retries (for example due to auth or permission errors)
- **THEN** the system keeps pending content persisted, emits failure events for user feedback, and continues retry scheduling until the failure condition is resolved or mount stops

### Requirement: Local-change ingest decisions are observable
The system SHALL emit structured diagnostics for all non-success local-ingest decisions, including explicit reason codes for skip, defer, and retry paths.

#### Scenario: Ingest skipped due to unsupported path state
- **WHEN** a local change is skipped because the file is outside the sync root, already deleted, or not upload-eligible
- **THEN** the system logs the file path and a machine-readable skip reason

#### Scenario: Ingest deferred for reconciliation
- **WHEN** local changes are deferred for safe-save reconciliation
- **THEN** the system logs defer start and resolution outcome (committed as update, committed as rename, or cancelled)
