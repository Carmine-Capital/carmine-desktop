## ADDED Requirements

### Requirement: TOCTOU-safe placeholder population on Windows
The system SHALL handle `ERROR_CLOUD_FILE_INVALID_REQUEST` returned by `CfCreatePlaceholders` as a per-item recoverable condition during the `FetchPlaceholders` callback. When a TOCTOU collision is detected, the system SHALL log a `warn!`-level message identifying the item and continue processing remaining items. The system SHALL NOT propagate `ERROR_CLOUD_FILE_INVALID_REQUEST` as a callback error, and SHALL NOT allow such a collision to crash the process. The `FetchPlaceholders` callback SHALL iterate over candidate placeholder items individually so that each item's result can be inspected independently.

#### Scenario: TOCTOU race during placeholder creation
- **WHEN** `CfCreatePlaceholders` returns `ERROR_CLOUD_FILE_INVALID_REQUEST` for an item during the `FetchPlaceholders` callback (because the placeholder was created by another process or thread between the existence check and the API call)
- **THEN** the system logs a `warn!` message identifying the item name and the collision, skips that item, and continues creating placeholders for remaining items without returning an error from the callback

#### Scenario: No TOCTOU race — normal placeholder creation
- **WHEN** `CfCreatePlaceholders` succeeds for an item during the `FetchPlaceholders` callback
- **THEN** the system registers the placeholder and continues to the next item

#### Scenario: Genuine API failure during placeholder creation
- **WHEN** `CfCreatePlaceholders` returns an error other than `ERROR_CLOUD_FILE_INVALID_REQUEST` during the `FetchPlaceholders` callback
- **THEN** the system returns that error from `fetch_placeholders` so the Cloud Files API infrastructure can signal failure to the OS

#### Scenario: Pre-filter removes already-existing items before API call
- **WHEN** the `FetchPlaceholders` callback is invoked for a directory that already has some placeholder files on disk
- **THEN** the system filters out those items before calling `CfCreatePlaceholders`, reducing unnecessary API calls; the per-item error handling acts as a safety net for items that appear between the filter check and the API call
