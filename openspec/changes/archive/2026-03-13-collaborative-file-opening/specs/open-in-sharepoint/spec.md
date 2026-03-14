## MODIFIED Requirements

### Requirement: Windows Explorer context menu
The system SHALL register two context menu entries on first active WinFsp mount: `carminedesktop.OpenOnline` ("Open Online") and `carminedesktop.OpenLocally` ("Open Locally"), under `HKCU\Software\Classes\*\shell\`. Both entries SHALL be removed on last active WinFsp mount unmount.

`carminedesktop.OpenOnline` SHALL invoke `carminedesktop://open-online?path=%1`.
`carminedesktop.OpenLocally` SHALL invoke the system default handler for the file type.

#### Scenario: Context menu registration on mount
- **WHEN** the first WinFsp mount starts
- **THEN** the system registers `carminedesktop.OpenOnline` with command `carminedesktop://open-online?path=%1` and label "Open Online (SharePoint)"
- **AND** registers `carminedesktop.OpenLocally` with command that opens the file via the default system handler and label "Open Locally"

#### Scenario: Context menu cleanup on unmount
- **WHEN** the last active WinFsp mount is unmounted
- **THEN** both `carminedesktop.OpenOnline` and `carminedesktop.OpenLocally` registry entries are removed

#### Scenario: User clicks "Open in SharePoint"
- **WHEN** the user right-clicks a file on a carminedesktop mount and selects "Open Online (SharePoint)"
- **THEN** the system invokes the `carminedesktop://open-online?path=<file>` deep link
- **AND** the file opens collaboratively via Office URI scheme or browser

### Requirement: Linux file manager integration
The system SHALL install both "Open Online" and "Open Locally" entries for supported Linux file managers (Nautilus, KDE Dolphin).

Nautilus: two scripts in `~/.local/share/nautilus/scripts/`.
KDE Dolphin: two service menu entries.

"Open Online" dispatches `carminedesktop://open-online?path=<percent-encoded>`.
"Open Locally" opens the file via `xdg-open` directly (default local behavior).

#### Scenario: Nautilus script available
- **WHEN** carminedesktop integrations are installed
- **THEN** two Nautilus scripts "Open Online (SharePoint)" and "Open Locally" are available in the right-click Scripts menu

#### Scenario: User triggers "Open Online" from Nautilus
- **WHEN** the user right-clicks a file on a carminedesktop mount in Nautilus and selects "Open Online (SharePoint)"
- **THEN** the system dispatches `carminedesktop://open-online?path=<percent-encoded-path>` for the selected file

## ADDED Requirements

### Requirement: macOS Finder integration for collaborative open
The system SHALL provide a mechanism for macOS users to choose between "Open Online" and "Open Locally" from Finder. This MAY be implemented as Finder Quick Actions, Automator services, or a Finder Sync extension.

"Open Online" SHALL invoke the `open_online` Tauri command or `carminedesktop://open-online` deep link.

#### Scenario: User triggers "Open Online" from Finder
- **WHEN** the user right-clicks a file on a carminedesktop mount in Finder and selects "Open Online"
- **THEN** the system resolves the file's webUrl and opens it via Office URI scheme or browser
