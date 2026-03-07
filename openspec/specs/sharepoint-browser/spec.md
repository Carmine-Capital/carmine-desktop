## Purpose
Defines how users discover, browse, and configure SharePoint site document libraries as mounts.
## Requirements
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
The system SHALL allow the user to configure which SharePoint document library to mount and where. The mount point SHALL be auto-derived by the wizard as `~/Cloud/<site_name> - <library_name>/`; no manual path entry is required during the wizard flow (path can be changed later in Settings).

#### Scenario: Select library and mount point
- **WHEN** the user selects a document library in the wizard
- **THEN** the wizard auto-derives the mount point as `~/Cloud/<site_name> - <library_name>/`, calls `add_mount`, validates that the mount point is not already in use, and saves the mount configuration; if the derived path is already in use `add_mount` returns an error shown inline

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

### Requirement: Wizard SharePoint source step
The system SHALL implement the `step-sharepoint` wizard step that guides the user through discovering and mounting a SharePoint document library. The step is reached by clicking "SharePoint Site" in `step-source`.

#### Scenario: Navigate to SharePoint step
- **WHEN** the user clicks "SharePoint Site" in `step-source`
- **THEN** the wizard transitions to `step-sharepoint`, which displays a search input, an empty results area, and a "Back" link that returns to `step-source`

#### Scenario: Search for sites
- **WHEN** the user types a query in the search input and submits (Enter or Search button)
- **THEN** the wizard calls `invoke('search_sites', { query })`, shows a loading indicator while waiting, and renders the results as a clickable list showing each site's display name and URL; existing result rows are cleared before each new search

#### Scenario: No sites found
- **WHEN** `search_sites` returns an empty array
- **THEN** the wizard displays "No sites found — try a different search term" and leaves the search input focused

#### Scenario: Search error
- **WHEN** `search_sites` rejects with an error
- **THEN** the wizard displays the error message inline and allows the user to retry

#### Scenario: Site selected — multiple libraries
- **WHEN** the user clicks a site row and `list_drives` returns two or more libraries
- **THEN** the wizard hides the site list, shows a library list with each library's name as a clickable row, and shows a "Back" link that returns to the site list

#### Scenario: Site selected — single library auto-select
- **WHEN** the user clicks a site row and `list_drives` returns exactly one library
- **THEN** the wizard skips the library selection sub-step and immediately calls `invoke('add_mount', ...)` with that library's ID, showing a loading indicator

#### Scenario: Library selected — mount added
- **WHEN** the user clicks a library row (or auto-select fires)
- **THEN** the wizard calls `invoke('add_mount', { mount_type: 'sharepoint', mount_point, drive_id, site_id, site_name, library_name })` where `mount_point` is auto-derived as `~/Cloud/<site_name> - <library_name>/`; on success the wizard transitions to `step-done`

#### Scenario: Mount add error
- **WHEN** `add_mount` rejects with an error
- **THEN** the wizard displays the error message inline in `step-sharepoint` and allows the user to select a different library or go back

#### Scenario: step-done mount list refresh
- **WHEN** the wizard transitions to `step-done` after a successful `add_mount` call
- **THEN** the wizard calls `invoke('list_mounts')` and re-renders the mount list in `step-done` so the newly added mount is visible

### Requirement: Wizard OneDrive source step
The system SHALL handle the OneDrive path in `step-source` by adding an additional OneDrive mount or showing an appropriate state.

#### Scenario: OneDrive button clicked — drive known
- **WHEN** the user clicks "OneDrive" in `step-source` and an existing OneDrive mount is present in `list_mounts`
- **THEN** the wizard derives a unique mount point and calls `invoke('add_mount', { mount_type: 'drive', drive_id, mount_point })`; on success it transitions to `step-done`

#### Scenario: OneDrive button clicked — no drive found
- **WHEN** the user clicks "OneDrive" in `step-source` and `list_mounts` returns no drive-type mounts
- **THEN** the wizard displays an error "OneDrive is not yet available — please wait a moment and try again"

#### Scenario: OneDrive add error
- **WHEN** `add_mount` rejects for an OneDrive mount
- **THEN** the wizard displays the error message inline in `step-source` and the user can retry

