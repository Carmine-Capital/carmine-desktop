## ADDED Requirements

### Requirement: Server-side copy via Graph API
The system SHALL support copying items server-side via the Microsoft Graph API without transferring file content through the client.

#### Scenario: Copy item within same drive
- **WHEN** a server-side copy is requested for an item within the same drive
- **THEN** the system calls `POST /drives/{driveId}/items/{itemId}/copy` with body `{ "parentReference": { "driveId": "<driveId>", "id": "<parentId>" }, "name": "<newName>" }`, receives HTTP 202 Accepted, and extracts the monitor URL from the `Location` response header

#### Scenario: Copy request retries on transient error
- **WHEN** the copy POST request returns HTTP 429 or a 5xx status code
- **THEN** the system retries up to 3 times with exponential backoff, following the existing retry pattern

#### Scenario: Copy request fails with client error
- **WHEN** the copy POST request returns HTTP 400, 403, 404, or another non-retryable error
- **THEN** the system returns the error immediately without retrying

### Requirement: Poll copy monitor URL for completion
The system SHALL poll the monitor URL returned by the copy endpoint to track the async copy operation until completion, failure, or timeout.

#### Scenario: Copy in progress
- **WHEN** the monitor URL returns `{ "status": "inProgress", "percentageComplete": <value> }`
- **THEN** the system waits and polls again after an exponential backoff delay (starting at 500ms, doubling up to 5s)

#### Scenario: Copy completed
- **WHEN** the monitor URL returns `{ "status": "completed", "resourceId": "<newItemId>" }`
- **THEN** the system returns the `resourceId` of the newly created item

#### Scenario: Copy failed on server
- **WHEN** the monitor URL returns `{ "status": "failed" }` with an optional error object
- **THEN** the system returns an error containing the failure message from the server response

#### Scenario: Monitor URL unreachable
- **WHEN** a poll request to the monitor URL fails due to a network error
- **THEN** the system retries the poll up to 3 times before treating the copy as failed

#### Scenario: Copy exceeds maximum poll duration
- **WHEN** the total polling duration exceeds 300 seconds (5 minutes)
- **THEN** the system stops polling and returns a timeout error

#### Scenario: Monitor URL does not require authentication
- **WHEN** the system polls the monitor URL
- **THEN** the system SHALL NOT include an `Authorization` header, as the monitor URL is pre-authenticated
