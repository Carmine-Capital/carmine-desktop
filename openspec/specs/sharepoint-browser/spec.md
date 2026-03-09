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

### Requirement: Wizard unified sources step (step-sources)
The system SHALL implement a `step-sources` wizard screen that replaces the former `step-source` and `step-sharepoint` screens. After sign-in completes, the wizard calls `GET /me/drive` and `GET /me/followedSites` in parallel and renders a single screen where the user assembles the set of sources they want to mount before finishing setup.

#### Scenario: OneDrive auto-detected
- **WHEN** `GET /me/drive` returns successfully after sign-in
- **THEN** the wizard renders an OneDrive card pre-checked with the proposed mount point (`~/Cloud/OneDrive`); the user may uncheck it to skip mounting OneDrive

#### Scenario: OneDrive not available
- **WHEN** `GET /me/drive` returns an error or the account has no OneDrive
- **THEN** the OneDrive section is absent from step-sources; the wizard proceeds with SharePoint-only sources

#### Scenario: SharePoint browser shown for org accounts
- **WHEN** `GET /me/followedSites` returns successfully (M365 org account with SharePoint access)
- **THEN** the wizard renders a SharePoint section with the followed sites listed as clickable rows, a search input above the list, and an "Add a SharePoint library" affordance

#### Scenario: SharePoint section hidden for personal accounts
- **WHEN** `GET /me/followedSites` returns an error or HTTP 403 (personal MSA account or account without SharePoint license)
- **THEN** the SharePoint section is absent from step-sources; the user can still proceed with OneDrive only

#### Scenario: Search for SharePoint sites
- **WHEN** the user types a query in the search input and submits (Enter or button click)
- **THEN** the wizard calls `invoke('search_sites', { query })`, shows a loading indicator, clears previous results, and renders matching sites as clickable rows

#### Scenario: No search results
- **WHEN** `search_sites` returns an empty array
- **THEN** the wizard displays "No sites found — try a different search term" and keeps the search input focused

#### Scenario: Site selected — library list
- **WHEN** the user clicks a site row
- **THEN** the wizard calls `invoke('list_drives', { siteId })` and renders the site's document libraries as clickable rows with a "Back" affordance to return to the site list

#### Scenario: Library added
- **WHEN** the user clicks a library row
- **THEN** the wizard calls `invoke('add_mount', { mount_type: 'sharepoint', drive_id, site_id, site_name, library_name, mount_point })` where `mount_point` is auto-derived as `~/Cloud/<site_name> - <library_name>/`; on success the library appears in the added-sources list below the SharePoint browser, the browser resets to the site search/recent view, and the "Get started" button becomes active

#### Scenario: Library add error
- **WHEN** `add_mount` rejects with an error (e.g., mount point conflict)
- **THEN** the wizard displays the error inline and the user may select a different library or adjust the mount point

#### Scenario: Added source removed before finishing
- **WHEN** the user clicks "Remove" on an entry in the added-sources list
- **THEN** the wizard calls `invoke('remove_mount', { id })` and removes the entry from the list; if it was the only source and OneDrive is unchecked, the "Get started" button becomes inactive

#### Scenario: Get started — at least one source
- **WHEN** at least one source is selected (OneDrive checked or ≥ 1 SharePoint library added)
- **THEN** the "Get started" button is active; clicking it calls `invoke('complete_wizard')`, starts the selected mounts, and transitions to step-success

#### Scenario: Get started — no sources
- **WHEN** OneDrive is unchecked and no SharePoint libraries have been added
- **THEN** the "Get started" button is disabled; a hint explains that at least one source is required

### Requirement: Wizard auth-aware routing
The wizard window SHALL detect the current authentication state on load and route the user to the appropriate starting step. If the user is already authenticated when the wizard opens, the wizard SHALL bypass the sign-in step and navigate directly to `step-sources`.

#### Scenario: Wizard opened when already authenticated
- **WHEN** the wizard window is created or navigated while `is_authenticated` returns true
- **THEN** the wizard SHALL skip `step-welcome` and transition to `step-sources`, calling `loadSources()` to populate OneDrive and SharePoint options

#### Scenario: Wizard opened when not authenticated
- **WHEN** the wizard window is created and `is_authenticated` returns false
- **THEN** the wizard SHALL display `step-welcome` with the "Sign in with Microsoft" button, following the normal sign-in flow

#### Scenario: Re-focused existing wizard navigated to step-sources for add-mount
- **WHEN** the wizard window already exists and "Add Mount" is triggered (from the settings Mounts tab or the tray menu)
- **THEN** the system SHALL call `goToAddMount()` on the existing wizard window via `win.eval()`, navigating it to `step-sources` regardless of which step it was previously displaying

