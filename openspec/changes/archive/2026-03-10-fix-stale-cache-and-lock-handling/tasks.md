## 1. Error type and Graph client changes

- [x] 1.1 Add `Error::Locked` variant to `carminedesktop-core::Error` (analogous to `PreconditionFailed`)
- [x] 1.2 Map HTTP 423 to `Error::Locked` in `GraphClient::handle_error`

## 2. Fix stale disk cache in `open_file`

- [x] 2.1 Move the `get_item()` server metadata refresh in `CoreOps::open_file` to BEFORE the disk cache validation block
- [x] 2.2 On `get_item` failure, fall back to existing disk cache validation with cached metadata (preserve offline behavior)
- [x] 2.3 Ensure disk cache validation uses fresh metadata when available (etag/size comparison against server response)

## 3. File lock detection on open

- [x] 3.1 After the `get_item` call in `open_file`, check the response for lock indicators and emit `VfsEvent::FileLocked` if locked
- [x] 3.2 Add `VfsEvent::FileLocked { file_name: String }` variant to `VfsEvent` enum

## 4. Handle 423 Locked in `flush_inode`

- [x] 4.1 Add `Error::Locked` match arm in `flush_inode` upload result handling
- [x] 4.2 On 423: upload conflict copy using `conflict_name()` to the same parent folder
- [x] 4.3 On successful conflict copy upload, remove writeback buffer entry and emit `VfsEvent::FileLocked`
- [x] 4.4 On failed conflict copy upload, preserve writeback buffer and return error

## 5. FUSE upload failure notification (parity with CfApi)

- [x] 5.1 Add `VfsEvent::UploadFailed { file_name: String, reason: String }` variant to `VfsEvent` enum
- [x] 5.2 In FUSE `flush` callback, emit `VfsEvent::UploadFailed` when `flush_handle` returns an error (need inode→name resolution)

## 6. App layer event handling

- [x] 6.1 Handle `VfsEvent::UploadFailed` in `spawn_event_forwarder` — call new notification function
- [x] 6.2 Handle `VfsEvent::FileLocked` in `spawn_event_forwarder` — call new notification function
- [x] 6.3 Add `notify::upload_failed()` and `notify::file_locked()` notification functions

## 7. Tests

- [x] 7.1 Test `open_file` serves fresh content when server eTag differs from cached eTag (stale cache scenario)
- [x] 7.2 Test `open_file` falls back to disk cache when `get_item` fails (offline scenario)
- [x] 7.3 Test `flush_inode` creates conflict copy on `Error::Locked` and clears writeback buffer
- [x] 7.4 Test `flush_inode` preserves writeback buffer when conflict copy upload also fails
- [x] 7.5 Test `handle_error` maps 423 to `Error::Locked`
