### Requirement: List drive contents
The system SHALL list the children of any folder in a OneDrive or SharePoint drive via the Microsoft Graph API.

#### Scenario: List root folder
- **WHEN** the VFS requests the contents of the root directory of a mounted drive
- **THEN** the system calls `GET /drives/{driveId}/root/children` and returns a list of DriveItem objects containing name, size, lastModifiedDateTime, folder/file type, id, and eTag

#### Scenario: List subfolder
- **WHEN** the VFS requests the contents of a subfolder identified by item ID
- **THEN** the system calls `GET /drives/{driveId}/items/{itemId}/children` and returns the list of child items

#### Scenario: Paginated results
- **WHEN** a folder contains more items than the default page size (200)
- **THEN** the system follows `@odata.nextLink` URLs to retrieve all pages and returns the complete list

### Requirement: Download file content
The system SHALL download file content from OneDrive/SharePoint via the Graph API.

#### Scenario: Download small file (< 4MB)
- **WHEN** the VFS requests the content of a file smaller than 4 MB
- **THEN** the system calls `GET /drives/{driveId}/items/{itemId}/content` and returns the file bytes

#### Scenario: Download with byte range
- **WHEN** the VFS requests a specific byte range of a file (offset + length)
- **THEN** the system includes a `Range: bytes={offset}-{offset+length-1}` header in the request and returns only the requested bytes

#### Scenario: Download large file
- **WHEN** the VFS requests content of a file larger than 4 MB
- **THEN** the system downloads the file in chunks using range requests, writing each chunk to the disk cache as it arrives

### Requirement: Upload file content
The system SHALL upload file content to OneDrive/SharePoint via the Graph API.

#### Scenario: Upload small file (< 4MB)
- **WHEN** a modified file smaller than 4 MB is flushed
- **THEN** the system calls `PUT /drives/{driveId}/items/{itemId}/content` with the file content and updates the local metadata with the returned eTag

#### Scenario: Upload large file via session
- **WHEN** a modified file of 4 MB or larger is flushed
- **THEN** the system creates an upload session via `POST /drives/{driveId}/items/{itemId}/createUploadSession`, uploads the file in sequential 10 MB chunks with `PUT` requests to the upload URL including `Content-Range` headers, and commits on completion

#### Scenario: Upload session interrupted
- **WHEN** a chunk upload fails due to network error
- **THEN** the system retries the failed chunk up to 3 times with exponential backoff, and if all retries fail, marks the file as "upload pending" and retries on next sync cycle

### Requirement: Create folder
The system SHALL create folders in OneDrive/SharePoint via the Graph API.

#### Scenario: Create new folder
- **WHEN** the VFS receives a `mkdir` operation
- **THEN** the system calls `POST /drives/{driveId}/items/{parentId}/children` with `{"name": "<folderName>", "folder": {}}` and returns the created DriveItem metadata

### Requirement: Delete item
The system SHALL delete files and folders in OneDrive/SharePoint via the Graph API.

#### Scenario: Delete file or empty folder
- **WHEN** the VFS receives an `unlink` or `rmdir` operation
- **THEN** the system calls `DELETE /drives/{driveId}/items/{itemId}` and removes the item from local caches upon HTTP 204

### Requirement: Move and rename items
The system SHALL support moving and renaming files and folders via the Graph API.

#### Scenario: Rename item
- **WHEN** the VFS receives a rename operation within the same parent folder
- **THEN** the system calls `PATCH /drives/{driveId}/items/{itemId}` with `{"name": "<newName>"}` and updates the local metadata

#### Scenario: Move item to different folder
- **WHEN** the VFS receives a rename operation where the parent changes
- **THEN** the system calls `PATCH /drives/{driveId}/items/{itemId}` with `{"parentReference": {"id": "<newParentId>"}, "name": "<name>"}` and updates local caches for both old and new parent

### Requirement: Delta query for change tracking
The system SHALL use Microsoft Graph delta queries to efficiently detect changes since the last sync.

#### Scenario: Initial delta sync
- **WHEN** a drive is mounted for the first time (no stored delta token)
- **THEN** the system calls `GET /drives/{driveId}/root/delta` to retrieve all items, stores them in the metadata cache, and persists the returned `@odata.deltaLink` token

#### Scenario: Incremental delta sync
- **WHEN** the sync interval elapses (default: 60 seconds)
- **THEN** the system calls the stored delta link URL, processes only the changed/deleted items, updates the metadata cache accordingly, and stores the new delta token

#### Scenario: Delta token expired
- **WHEN** a delta query returns HTTP 410 Gone
- **THEN** the system discards the expired token and performs a full initial delta sync

### Requirement: Rate limiting and retry
The system SHALL respect Microsoft Graph API rate limits and implement retry logic.

#### Scenario: Throttled request
- **WHEN** an API call returns HTTP 429 Too Many Requests with a `Retry-After` header
- **THEN** the system waits for the duration specified in `Retry-After` before retrying the request

#### Scenario: Transient server error
- **WHEN** an API call returns HTTP 500, 502, 503, or 504
- **THEN** the system retries up to 3 times with exponential backoff (1s, 2s, 4s) with jitter

#### Scenario: Request batching
- **WHEN** multiple metadata requests are pending within a 50ms window
- **THEN** the system MAY batch them into a single `POST /$batch` request containing up to 20 individual requests
