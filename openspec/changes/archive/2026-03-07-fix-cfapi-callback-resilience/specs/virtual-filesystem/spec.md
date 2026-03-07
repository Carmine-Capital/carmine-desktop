## ADDED Requirements

### Requirement: Resilient CfApi callback error handling
On Windows, CfApi sync filter callbacks (`fetch_data`, `delete`, `rename`, `dehydrate`) SHALL NOT propagate errors to the cloud-filter proxy. When a callback encounters any error, it SHALL log the error at `warn` level with sufficient context (callback name, file path, error details) and return success to the proxy. The failure SHALL be surfaced through OS-level mechanisms (e.g., an I/O error returned to the reading application, an OS-level retry of the operation) rather than through process termination. This requirement exists because the `cloud-filter` crate's proxy unconditionally calls `.unwrap()` on its `CfExecute` failure-reporting path; a panicking `.unwrap()` across the `extern "system"` FFI boundary produces `STATUS_STACK_BUFFER_OVERRUN` and terminates the process.

#### Scenario: fetch_data cannot resolve the file path
- **WHEN** the `fetch_data` callback is invoked for a file whose path cannot be resolved in the cache or via the Graph API
- **THEN** the system logs a warning including the relative path and returns success to the proxy without writing any data to the transfer ticket
- **AND** the OS surfaces an I/O error to the application that requested the file read

#### Scenario: fetch_data download fails
- **WHEN** the `fetch_data` callback resolves the file successfully but the content download (`read_range_direct`) fails due to a network error or API error
- **THEN** the system logs a warning including the file path and error details and returns success to the proxy without writing any data to the transfer ticket
- **AND** the OS surfaces an I/O error to the application that requested the file read

#### Scenario: fetch_data write_at fails mid-transfer
- **WHEN** the `fetch_data` callback begins writing hydration data via `ticket.write_at` but a write chunk fails
- **THEN** the system logs a warning including the file path and error details, stops writing further chunks, and returns success to the proxy
- **AND** the OS surfaces an I/O error to the application that requested the file read

#### Scenario: delete ticket acknowledgement fails
- **WHEN** the `delete` callback has completed its Graph API and cache cleanup but `ticket.pass()` fails
- **THEN** the system logs a warning and returns success to the proxy
- **AND** the OS may retry the delete callback; the cache and Graph API side effects are idempotent

#### Scenario: rename ticket acknowledgement fails
- **WHEN** the `rename` callback has completed its Graph API and cache update but `ticket.pass()` fails
- **THEN** the system logs a warning and returns success to the proxy
- **AND** the OS may retry the rename callback; the cache and Graph API side effects are idempotent

#### Scenario: dehydrate ticket acknowledgement fails
- **WHEN** the `dehydrate` callback has completed its disk cache removal but `ticket.pass()` fails
- **THEN** the system logs a warning and returns success to the proxy
- **AND** the OS may retry the dehydrate callback; the disk cache removal is idempotent
