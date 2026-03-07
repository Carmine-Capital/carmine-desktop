## ADDED Requirements

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

## MODIFIED Requirements

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
