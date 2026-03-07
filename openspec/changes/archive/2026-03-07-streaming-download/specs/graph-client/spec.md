## ADDED Requirements

### Requirement: Streaming file download
The system SHALL support downloading file content as a byte stream, delivering chunks progressively as they arrive from the network rather than buffering the entire response in memory before returning.

#### Scenario: Stream large file download
- **WHEN** the VFS requests a streaming download of a file
- **THEN** the system initiates a `GET /drives/{driveId}/items/{itemId}/content` request and returns a byte stream that yields chunks as they arrive from the server, without waiting for the complete response body

#### Scenario: Streaming download with retry on failure
- **WHEN** a streaming download connection drops mid-transfer due to a network error
- **THEN** the system reports the error to the caller via the stream; the caller is responsible for retry decisions (e.g., restarting the download or falling back to range requests)

#### Scenario: Streaming download authentication
- **WHEN** a streaming download is initiated
- **THEN** the system obtains a fresh access token before starting the HTTP request, using the same token acquisition mechanism as other Graph API calls

## MODIFIED Requirements

### Requirement: Download file content
The system SHALL download file content from OneDrive/SharePoint via the Graph API.

#### Scenario: Download small file (< 4MB)
- **WHEN** the VFS requests the content of a file smaller than 4 MB
- **THEN** the system calls `GET /drives/{driveId}/items/{itemId}/content` and returns the file bytes

#### Scenario: Download with byte range
- **WHEN** the VFS requests a specific byte range of a file (offset + length)
- **THEN** the system includes a `Range: bytes={offset}-{offset+length-1}` header in the request and returns only the requested bytes

#### Scenario: Download large file via streaming
- **WHEN** the VFS requests content of a file larger than 4 MB that is not in the disk cache
- **THEN** the system downloads the file using a streaming connection, delivering chunks to the caller as they arrive from the network, and the caller writes each chunk to a buffer as it is received

#### Scenario: Random-access download via range request
- **WHEN** the VFS requests a specific byte range of a large file during a streaming download (e.g., due to a seek operation)
- **THEN** the system issues a separate `GET` request with a `Range` header for the requested bytes and returns them independently of the ongoing streaming download
