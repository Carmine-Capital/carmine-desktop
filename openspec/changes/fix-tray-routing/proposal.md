## Why

Three tray UX defects — wrong left-click destination when unauthenticated, no recovery action when auth is degraded, and fragile mount-count logic based on substring matching — make the application confusing or broken for users who have not yet signed in or whose session has expired. All three stem from the same `tray.rs` file and share a single fix window, so they are addressed together.

## What Changes

- **Left-click routing**: When the user has not authenticated, left-clicking the tray icon will open the setup wizard instead of the settings window. When authenticated, the existing behavior (open settings) is preserved.
- **Re-authenticate menu item**: When `auth_degraded == true`, the tray menu will show a prominent "Re-authenticate…" item (which opens the wizard) in addition to "Sign Out", giving the user a single-click recovery path.
- **Bool-based mount status**: The `mount_entries` Vec is changed from `Vec<(String, String, String)>` to `Vec<(String, String, bool)>` where the third field is `is_mounted`. The tooltip mount count is derived from this bool rather than by substring-matching the label string.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `tray-app`: Three requirement changes — (1) left-click routing depends on auth state, (2) degraded-auth menu includes a "Re-authenticate…" recovery item, (3) mount count in tooltip is derived from a boolean status field, not string matching.

## Impact

- `crates/cloudmount-app/src/tray.rs`: All three fixes land here. No other crates are affected.
- `openspec/specs/tray-app/spec.md`: Delta spec adds/updates requirements for left-click routing, auth-degraded menu, and mount status tracking.
