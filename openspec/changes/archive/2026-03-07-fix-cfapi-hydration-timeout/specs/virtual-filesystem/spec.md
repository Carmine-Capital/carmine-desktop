## ADDED Requirements

### Requirement: CfApi fetch_data immediate failure signaling
On Windows, the `fetch_data` Cloud Files API callback SHALL signal failure to the operating system immediately on any error, rather than returning without issuing any CfExecute operation. Returning without a CfExecute call leaves Windows waiting until its 60-second internal timeout expires, resulting in error 426 for the requesting process. The callback SHALL resolve the item ID from the placeholder blob set at creation time (`request.file_blob()`), without making any Graph API network call for item resolution.

#### Scenario: fetch_data — item ID decoded from placeholder blob
- **WHEN** the OS dispatches a `fetch_data` callback for a dehydrated file
- **THEN** the system decodes the item ID from `request.file_blob()` (UTF-8 bytes written at placeholder creation), looks up the corresponding inode in the inode table, and proceeds to hydrate using that inode
- **AND** no Graph API `list_children` or `get_item` call is made to resolve the file path

#### Scenario: fetch_data — blob decode or inode lookup failure
- **WHEN** the placeholder blob is missing, invalid UTF-8, or the decoded item ID has no matching inode in the inode table
- **THEN** the system returns a failure status to the OS immediately (equivalent to `CfExecute` with a non-success `CompletionStatus`)
- **AND** the OS surfaces an error to the requesting process without waiting for any timeout

#### Scenario: fetch_data — download failure
- **WHEN** the Graph API download for the required byte range fails (network error, auth error, HTTP error)
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — empty content returned
- **WHEN** the Graph API returns an empty response body for a non-zero-length file
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — path outside sync root
- **WHEN** the OS dispatches a `fetch_data` callback for a path that is not under the registered sync root
- **THEN** the system returns a failure status to the OS immediately
- **AND** the OS surfaces an error to the requesting process without waiting 60 seconds

#### Scenario: fetch_data — write_at failure mid-transfer
- **WHEN** a `write_at` call fails during the chunk transfer loop (e.g., connection closed)
- **THEN** the system aborts the transfer and returns a failure status to the OS immediately
- **AND** Windows discards the partial transfer and leaves the file in dehydrated state
