## ADDED Requirements

### Requirement: CollabGate hook in file open path
The VFS open path SHALL check whether the file being opened is collaborative-capable and the caller is an interactive shell process. If both conditions are met and CollabGate is enabled, the VFS SHALL delegate the open decision to the Tauri app before serving the file.

If CollabGate determines `OpenOnline`, the VFS SHALL NOT serve the file content and SHALL return an appropriate error code to the caller (so the local application does not open).

If CollabGate determines `OpenLocally` or times out, the VFS SHALL proceed with the normal open flow (download content, return file handle).

#### Scenario: CollabGate triggers for interactive Office file open
- **WHEN** an interactive shell process opens a `.docx` file
- **AND** CollabGate is enabled
- **THEN** the VFS sends a `CollabOpenRequest` before downloading content
- **AND** blocks until a response or timeout

#### Scenario: CollabGate response is OpenOnline
- **WHEN** the Tauri app responds with `OpenOnline`
- **THEN** the VFS does NOT allocate a file handle or download content
- **AND** returns an error to the caller (preventing local open)
- **AND** the Tauri app launches the Office URI or browser

#### Scenario: CollabGate response is OpenLocally
- **WHEN** the Tauri app responds with `OpenLocally`
- **THEN** the VFS proceeds with the normal open flow
- **AND** downloads content and returns a file handle

#### Scenario: CollabGate response is Cancel
- **WHEN** the Tauri app responds with `Cancel`
- **THEN** the VFS returns an error to the caller (file not opened)
- **AND** no content is downloaded

### Requirement: VfsEvent for CollabGate timeout
The VFS SHALL emit a `VfsEvent::CollabGateTimeout` when the CollabGate oneshot channel times out, including the file path for user notification.

#### Scenario: CollabGate times out
- **WHEN** the CollabGate oneshot does not receive a response within the configured timeout
- **THEN** a `VfsEvent::CollabGateTimeout { path }` is emitted
- **AND** the VFS proceeds with local open

### Requirement: Local modification detection for CollabOpenRequest
When constructing a `CollabOpenRequest`, the VFS SHALL check whether the file has pending local modifications (dirty handle in the open file table, or pending writeback entry) and set `has_local_changes` accordingly.

#### Scenario: File with dirty handle
- **WHEN** a collaborative file is opened
- **AND** another handle to the same inode has unsynchronized writes
- **THEN** `has_local_changes` is `true` in the `CollabOpenRequest`

#### Scenario: File with no pending changes
- **WHEN** a collaborative file is opened
- **AND** no open handles have unsynchronized writes for that inode
- **AND** no writeback entry exists for that inode
- **THEN** `has_local_changes` is `false` in the `CollabOpenRequest`
