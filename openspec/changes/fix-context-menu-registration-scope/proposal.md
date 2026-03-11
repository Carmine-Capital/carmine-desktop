## Why

Windows Explorer context-menu registration for "Open in SharePoint" is currently tied to a single mount lifecycle event. With multiple CloudMount sync roots, unmounting any one mount removes the global registry entry, breaking the feature for remaining mounted drives.

This causes inconsistent behavior for users and undermines confidence in the integration. We need deterministic registration/cleanup behavior that matches the actual number of active CloudMount Windows mounts.

## What Changes

- Track Windows context-menu registration state against active CfApi mounts instead of per-mount create/remove events.
- Register the Explorer context-menu entry when the first eligible CloudMount Windows mount becomes active.
- Keep the context-menu entry registered while at least one eligible mount remains active.
- Remove the context-menu entry only when the last eligible mount is unmounted.
- Make registration and cleanup idempotent and resilient to partial failures or stale pre-existing registry keys.
- Add lifecycle coverage tests for multi-mount scenarios to prevent regressions.

## Capabilities

### New Capabilities
- `windows-context-menu-lifecycle`: Defines reference-counted lifecycle rules for registering and removing the Windows "Open in SharePoint" Explorer context-menu entry across multiple active CloudMount mounts.

### Modified Capabilities
- None.

## Impact

- Affected code: `crates/cloudmount-vfs/src/cfapi.rs` (mount/unmount lifecycle and registry integration).
- Potential new helper state: process-wide tracking for active Windows mounts (e.g., atomic counter or guarded singleton).
- Windows-only behavior change; Linux/macOS code paths remain unchanged.
- No API or config breaking changes expected.
