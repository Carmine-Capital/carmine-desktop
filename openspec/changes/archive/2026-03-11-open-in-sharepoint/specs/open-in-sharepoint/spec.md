## ADDED Requirements

### Requirement: Resolve local path to SharePoint URL
The system SHALL resolve any absolute local file path within a mounted drive to its corresponding SharePoint `webUrl`. The resolution SHALL use the existing inode table and cache tiers (memory, SQLite, Graph API fallback) without additional API calls when the item is cached.

#### Scenario: Resolve cached file path
- **WHEN** a user requests the SharePoint URL for a file at `<mount_point>/Reports/report.docx` and the file's `DriveItem` is in cache with a populated `webUrl`
- **THEN** the system returns the `webUrl` (e.g., `https://contoso.sharepoint.com/sites/eng/Shared%20Documents/Reports/report.docx`) without any Graph API call

#### Scenario: Resolve uncached file path
- **WHEN** a user requests the SharePoint URL for a file whose `DriveItem` is not in the memory or SQLite cache
- **THEN** the system fetches the item via the Graph API, caches it, and returns the `webUrl`

#### Scenario: Resolve path with missing webUrl on cached item
- **WHEN** a user requests the SharePoint URL for a file whose cached `DriveItem` has `webUrl` as `None` (e.g., cached before the field was added)
- **THEN** the system fetches a fresh `DriveItem` via `get_item()`, updates the cache, and returns the `webUrl`

#### Scenario: Path outside any mount
- **WHEN** a user requests the SharePoint URL for a path that is not inside any active carminedesktop mount point
- **THEN** the system returns an error indicating the path is not managed by carminedesktop

#### Scenario: Path for non-existent file
- **WHEN** a user requests the SharePoint URL for a path that does not resolve to any known inode
- **THEN** the system returns a "file not found" error

### Requirement: Open file in SharePoint via Tauri command
The system SHALL provide a Tauri command (`open_online`) that accepts a local file path, resolves it to a SharePoint URL, and opens it in the appropriate application.

#### Scenario: Open Office document on Windows
- **WHEN** a user invokes `open_online` for a `.docx` file on Windows with Microsoft Office installed
- **THEN** the system constructs a `ms-word:ofe|u|<webUrl>` URI and opens it via the OS, launching Word with a direct SharePoint connection (co-authoring enabled)

#### Scenario: Open Office document on macOS
- **WHEN** a user invokes `open_online` for a `.xlsx` file on macOS with Microsoft Office installed
- **THEN** the system constructs a `ms-excel:ofe|u|<webUrl>` URI and opens it via the OS, launching Excel with a direct SharePoint connection

#### Scenario: Open Office document on Linux
- **WHEN** a user invokes `open_online` for any Office document on Linux
- **THEN** the system opens the plain `webUrl` in the default browser via `xdg-open`, where it opens in Office Online with co-authoring support

#### Scenario: Open non-Office file
- **WHEN** a user invokes `open_online` for a non-Office file (e.g., `.pdf`, `.png`, `.txt`) on any platform
- **THEN** the system opens the plain `webUrl` in the default browser

#### Scenario: Office URI scheme fails
- **WHEN** a user invokes `open_online` for an Office file and the Office URI scheme fails to open (e.g., Office not installed)
- **THEN** the system falls back to opening the plain `webUrl` in the default browser

### Requirement: Office URI scheme mapping
The system SHALL map Office file extensions to their corresponding URI schemes for desktop co-authoring.

#### Scenario: Word document mapping
- **WHEN** a file has extension `.doc`, `.docx`, or `.docm`
- **THEN** the system maps it to the `ms-word:ofe|u|<webUrl>` URI scheme

#### Scenario: Excel document mapping
- **WHEN** a file has extension `.xls`, `.xlsx`, or `.xlsm`
- **THEN** the system maps it to the `ms-excel:ofe|u|<webUrl>` URI scheme

#### Scenario: PowerPoint document mapping
- **WHEN** a file has extension `.ppt`, `.pptx`, or `.pptm`
- **THEN** the system maps it to the `ms-powerpoint:ofe|u|<webUrl>` URI scheme

#### Scenario: Unknown extension
- **WHEN** a file has an extension not in the Office mapping table
- **THEN** the system uses the plain `webUrl` (no URI scheme prefix)

### Requirement: Deep-link protocol handler
The system SHALL register a `carminedesktop://` URL protocol handler that allows external tools to trigger the "Open in SharePoint" action.

#### Scenario: Handle open-online deep link
- **WHEN** the OS dispatches `carminedesktop://open-online?path=<percent-encoded-path>` to the application
- **THEN** the system decodes the path, resolves it to a SharePoint URL, and opens it as if `open_online` were invoked directly

#### Scenario: Deep link with invalid path
- **WHEN** the OS dispatches a `carminedesktop://open-online` deep link with a path that is not inside any active mount
- **THEN** the system shows a desktop notification indicating the file is not managed by carminedesktop

#### Scenario: Deep link with unrecognized action
- **WHEN** the OS dispatches a `carminedesktop://` deep link with an action other than `open-online`
- **THEN** the system ignores the request and logs a warning

### Requirement: Windows Explorer context menu
On Windows, the system SHALL register a context menu entry in Explorer that allows users to open files in SharePoint directly from the right-click menu.

#### Scenario: Context menu registration on mount
- **WHEN** a CfApi sync root is registered during mount
- **THEN** the system creates registry entries under `HKCU\Software\Classes` that add an "Open in SharePoint" menu item for files, with the command invoking the `carminedesktop://open-online` deep link with the selected file's path

#### Scenario: Context menu cleanup on unmount
- **WHEN** a CfApi sync root is unregistered during unmount
- **THEN** the system removes the registry entries for the context menu

#### Scenario: User clicks "Open in SharePoint"
- **WHEN** a user right-clicks a file in Explorer inside a carminedesktop sync root and selects "Open in SharePoint"
- **THEN** Explorer invokes the registered command, which dispatches the `carminedesktop://open-online` deep link, and the file opens in desktop Office or the browser

### Requirement: Linux file manager integration
On Linux, the system SHALL provide a Nautilus script that allows users to open files in SharePoint from the file manager.

#### Scenario: Nautilus script available
- **WHEN** carminedesktop is installed on a Linux system with Nautilus
- **THEN** a script is available (installed or documented for manual placement) at `~/.local/share/nautilus/scripts/Open in SharePoint` that invokes the `carminedesktop://open-online` deep link or the Tauri command for the selected file

#### Scenario: User triggers script from Nautilus
- **WHEN** a user right-clicks a file in Nautilus inside a carminedesktop mount and selects Scripts > "Open in SharePoint"
- **THEN** the script resolves the file path and opens it in the default browser via `xdg-open`
