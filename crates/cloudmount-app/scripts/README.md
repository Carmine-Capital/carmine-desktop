# CloudMount Linux File Manager Integration

This directory contains integration scripts for Linux file managers, enabling the "Open in SharePoint" feature from the right-click context menu.

## Available Integrations

### GNOME / Nautilus

- **Script**: `open-in-sharepoint.sh`
- **Location**: `~/.local/share/nautilus/scripts/Open in SharePoint`
- **Trigger**: Right-click → Scripts → "Open in SharePoint"

### KDE / Dolphin

- **Service Menu**: `cloudmount-open-in-sharepoint.desktop`
- **Helper Script**: `cloudmount-kde-helper`
- **Trigger**: Right-click → "Open in SharePoint"

## Installation

### Nautilus (GNOME)

1. Copy the script:
   ```bash
   cp open-in-sharepoint.sh ~/.local/share/nautilus/scripts/Open\ in\ SharePoint
   ```
2. Make it executable:
   ```bash
   chmod +x ~/.local/share/nautilus/scripts/Open\ in\ SharePoint
   ```
3. Restart Nautilus:
   ```bash
   nautilus -q
   ```

### Dolphin (KDE)

#### Plasma 5

1. Create the ServiceMenus directory:
   ```bash
   mkdir -p ~/.local/share/kservices5/ServiceMenus
   ```
2. Copy the Service Menu file:
   ```bash
   cp cloudmount-open-in-sharepoint.desktop ~/.local/share/kservices5/ServiceMenus/
   ```
3. Copy the helper script to a directory in your PATH:
   ```bash
   cp cloudmount-kde-helper ~/.local/bin/
   chmod +x ~/.local/bin/cloudmount-kde-helper
   ```
4. Rebuild the service cache:
   ```bash
   kbuildsycoca5
   ```

#### Plasma 6

1. Create the servicemenus directory:
   ```bash
   mkdir -p ~/.local/share/kio/servicemenus
   ```
2. Copy the Service Menu file:
   ```bash
   cp cloudmount-open-in-sharepoint.desktop ~/.local/share/kio/servicemenus/
   ```
3. Copy the helper script to a directory in your PATH:
   ```bash
   cp cloudmount-kde-helper ~/.local/bin/
   chmod +x ~/.local/bin/cloudmount-kde-helper
   ```
4. Rebuild the service cache:
   ```bash
   kbuildsycoca6
   ```

## Prerequisites

- **CloudMount must be running** — The deep-link protocol handler must be active to receive `cloudmount://` URLs
- **xdg-utils** — For `xdg-open` to dispatch deep-links (typically pre-installed)
- **python3** — For URL percent-encoding

## Coexistence with Nautilus

The KDE and Nautilus integrations are independent and can coexist on the same system. Each uses its own native extension mechanism:

- **Nautilus**: Uses the scripts directory mechanism
- **Dolphin**: Uses KDE Service Menus

Both invoke the same `cloudmount://open-online?path=<encoded>` deep-link, so behavior is consistent regardless of which file manager you use.

## Multi-Selection Behavior

When multiple files are selected:

1. Each file triggers a **separate** deep-link dispatch
2. This provides **per-file failure isolation** — one invalid path doesn't block others
3. Expect **multiple browser tabs/windows** to open (one per file)

**Note**: This is the current behavior for v1. Future versions may introduce batching if user feedback indicates a need.

## Troubleshooting

### Menu item doesn't appear in Dolphin

1. Verify the `.desktop` file is in the correct location for your Plasma version
2. Run `kbuildsycoca5` (Plasma 5) or `kbuildsycoca6` (Plasma 6) to rebuild the cache
3. Restart Dolphin
4. Check that the helper script is executable and in your PATH

### Nothing happens when clicking the menu item

1. Ensure CloudMount is running
2. Test the deep-link manually:
   ```bash
   xdg-open "cloudmount://open-online?path=%2Fhome%2Fuser%2FCloudMount%2Fdrive%2Ftest.docx"
   ```
3. Check that `cloudmount-kde-helper` is in your PATH:
   ```bash
   which cloudmount-kde-helper
   ```

### "Open in SharePoint" appears for non-CloudMount files

This is expected behavior for v1. The menu appears for all files. CloudMount validates the path when the deep-link is received and shows an error notification if the file is not in a CloudMount mount.
