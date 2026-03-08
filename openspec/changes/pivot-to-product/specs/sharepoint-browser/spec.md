## ADDED Requirements

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

## REMOVED Requirements

### Requirement: Wizard SharePoint source step
**Reason**: Replaced by the unified `step-sources` screen which combines OneDrive and SharePoint selection in one step.
**Migration**: Remove `step-sharepoint` HTML/JS routing; implement `step-sources` per the requirement above.

### Requirement: Wizard OneDrive source step
**Reason**: Replaced by the unified `step-sources` screen. OneDrive is now auto-detected and pre-checked rather than requiring a separate step.
**Migration**: Remove `step-source` OneDrive button logic; OneDrive is handled in `step-sources` via auto-detection.
