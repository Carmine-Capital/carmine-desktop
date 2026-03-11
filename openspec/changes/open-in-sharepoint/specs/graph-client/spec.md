## MODIFIED Requirements

### Requirement: List drive contents
The system SHALL list the children of any folder in a OneDrive or SharePoint drive via the Microsoft Graph API.

#### Scenario: List root folder
- **WHEN** the VFS requests the contents of the root directory of a mounted drive
- **THEN** the system calls `GET /drives/{driveId}/root/children` and returns a list of DriveItem objects containing name, size, lastModifiedDateTime, folder/file type, id, eTag, and webUrl

#### Scenario: List subfolder
- **WHEN** the VFS requests the contents of a subfolder identified by item ID
- **THEN** the system calls `GET /drives/{driveId}/items/{itemId}/children` and returns the list of child items including webUrl for each item

#### Scenario: Paginated results
- **WHEN** a folder contains more items than the default page size (200)
- **THEN** the system follows `@odata.nextLink` URLs to retrieve all pages and returns the complete list
