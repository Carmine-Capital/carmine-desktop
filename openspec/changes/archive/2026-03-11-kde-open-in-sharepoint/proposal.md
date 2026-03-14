## Why

carminedesktop already supports "Open in SharePoint" on Windows and via a Nautilus script on Linux, but KDE users currently have no native Dolphin right-click integration. This creates an inconsistent Linux UX and leaves a visible gap for a major desktop environment.

## What Changes

- Add KDE Dolphin integration for "Open in SharePoint" using a Service Menu entry.
- Add a Linux helper entrypoint that accepts file paths from Dolphin, percent-encodes them, and invokes `carminedesktop://open-online?path=<encoded>`.
- Explicitly depend on the existing `carminedesktop://open-online` action contract from the Open in SharePoint flow (no protocol extension in this change).
- Handle single and multi-selection behavior consistently with current Linux deep-link handling.
- Document installation and troubleshooting for Plasma 5/6 locations and required tooling.

## Capabilities

### New Capabilities
- `kde-open-in-sharepoint`: Provide a native Dolphin context-menu action that triggers carminedesktop's existing deep-link flow for opening mounted files in SharePoint.

### Modified Capabilities
- None.

## Impact

- **carminedesktop-app**: New KDE-facing integration assets (Service Menu and helper script) plus packaging/documentation updates.
- **Linux UX**: Parity improvement between GNOME/Nautilus and KDE/Dolphin workflows.
- **Dependencies**: No new Rust dependencies expected; relies on existing `xdg-open` deep-link handling.
