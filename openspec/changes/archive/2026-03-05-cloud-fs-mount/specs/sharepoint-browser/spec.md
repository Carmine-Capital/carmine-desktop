## ADDED Requirements

### Requirement: List available SharePoint sites
The system SHALL allow the user to discover and browse SharePoint sites available to their account.

#### Scenario: Search for sites
- **WHEN** the user types a search query in the SharePoint site selection UI
- **THEN** the system calls `GET /sites?search={query}` and displays a list of matching sites with their display names and URLs

#### Scenario: List followed sites
- **WHEN** the user opens the SharePoint site browser without entering a search query
- **THEN** the system calls `GET /me/followedSites` and displays the user's followed sites as a default starting list

#### Scenario: No sites found
- **WHEN** a search query returns no matching sites
- **THEN** the system displays "No sites found" and suggests the user check the site name or their access permissions

### Requirement: List document libraries for a site
The system SHALL list all document libraries within a selected SharePoint site.

#### Scenario: Browse site libraries
- **WHEN** the user selects a SharePoint site
- **THEN** the system calls `GET /sites/{siteId}/drives` and displays the list of document libraries with their names and sizes

#### Scenario: Site with single library
- **WHEN** the selected site has only one document library
- **THEN** the system auto-selects that library and proceeds to mount configuration

#### Scenario: Distinguish libraries from lists
- **WHEN** the system retrieves drives for a site
- **THEN** the system only displays items with `driveType` equal to `documentLibrary`, filtering out any other drive types

### Requirement: Configure SharePoint mount
The system SHALL allow the user to configure which SharePoint document library to mount and where.

#### Scenario: Select library and mount point
- **WHEN** the user selects a document library and specifies a mount point path
- **THEN** the system validates the mount point (directory exists or can be created, not already in use) and saves the mount configuration

#### Scenario: Mount a subfolder of a library
- **WHEN** the user wants to mount only a specific subfolder within a document library
- **THEN** the system allows browsing the library's folder structure and selecting a subfolder as the mount root

#### Scenario: Invalid mount point
- **WHEN** the user specifies a mount point that already has an active mount or is a system directory
- **THEN** the system displays an error and asks for a different mount point

### Requirement: Persist SharePoint mount configuration
The system SHALL persist the selected SharePoint sites and libraries across application restarts.

#### Scenario: Save mount configuration
- **WHEN** the user completes the SharePoint mount setup
- **THEN** the system saves the site ID, drive ID, site display name, library name, mount point, and enabled state to the configuration file

#### Scenario: Restore mounts on startup
- **WHEN** the application starts and has saved SharePoint mount configurations
- **THEN** the system automatically mounts all enabled SharePoint drives using the persisted configuration

#### Scenario: Site access revoked
- **WHEN** the application attempts to mount a previously configured SharePoint site and receives HTTP 403 Forbidden
- **THEN** the system marks the mount as errored, displays a notification "Access denied to {siteName} — your permissions may have changed", and skips mounting that site

### Requirement: Multiple SharePoint site support
The system SHALL support mounting multiple SharePoint document libraries simultaneously.

#### Scenario: Add additional SharePoint mount
- **WHEN** the user already has one or more SharePoint mounts configured and selects "Add Mount"
- **THEN** the system presents the site browser again, allowing selection of a different site or library, and each mount operates independently

#### Scenario: Independent mount lifecycles
- **WHEN** multiple SharePoint libraries are mounted
- **THEN** each mount has its own cache, sync state, and can be individually mounted, unmounted, or removed without affecting other mounts
