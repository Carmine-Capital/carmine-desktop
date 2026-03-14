## ADDED Requirements

### Requirement: Dolphin context menu action
The system SHALL provide a KDE Dolphin context menu action named `Open in SharePoint` that is available through the Dolphin Service Menu mechanism.

#### Scenario: Service menu is installed
- **WHEN** carminedesktop Linux integration assets are installed on a KDE system
- **THEN** Dolphin discovers a Service Menu entry labeled `Open in SharePoint` for file selections

#### Scenario: User triggers action for one file
- **WHEN** a user right-clicks a file in Dolphin and selects `Open in SharePoint`
- **THEN** Dolphin invokes the configured helper command with the selected absolute file path

### Requirement: Deep-link invocation from Dolphin selection
The Dolphin integration SHALL transform each selected file path into a percent-encoded `carminedesktop://open-online?path=<encoded>` URL and dispatch it through the system opener.

#### Scenario: Path contains spaces or unicode
- **WHEN** a selected path contains spaces or non-ASCII characters
- **THEN** the integration percent-encodes the full absolute path before building the deep-link URL

#### Scenario: Multi-selection in Dolphin
- **WHEN** the user triggers `Open in SharePoint` with multiple files selected
- **THEN** the integration dispatches one deep-link invocation per selected file
- **AND** a failure for one selected file does not prevent dispatch attempts for remaining selected files

#### Scenario: Linux app-instance behavior under multi-selection
- **WHEN** multiple deep links are dispatched on Linux desktop environments
- **THEN** the integration treats app instance behavior as best-effort and documents that multiple launches/windows may occur depending on environment

### Requirement: Reuse existing Open in SharePoint behavior
The KDE integration SHALL reuse the existing `carminedesktop://open-online` flow so that path validation, SharePoint URL resolution, and Office/browser fallback behavior remain consistent with other entry points.

#### Scenario: Selected file is inside a carminedesktop mount
- **WHEN** a deep-link generated from Dolphin references a path under an active carminedesktop mount
- **THEN** the application resolves the file and opens the corresponding SharePoint URL using platform-specific `open_online` behavior

#### Scenario: Selected file is outside carminedesktop mounts
- **WHEN** a deep-link generated from Dolphin references a path not managed by carminedesktop
- **THEN** the application rejects the request and shows an error notification to the user

### Requirement: KDE installation guidance
The project SHALL document KDE setup steps for the Dolphin Service Menu integration, including expected install location and prerequisites.

#### Scenario: User follows KDE setup documentation
- **WHEN** a Linux KDE user follows the documented setup steps
- **THEN** they can install/enable the `Open in SharePoint` Dolphin action without requiring source code changes
