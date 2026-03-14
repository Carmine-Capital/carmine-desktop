## Why

When the app starts with 5-10 drives configured, users receive 5-10 separate "Mount Ready" notifications—a noisy and annoying experience. A single summary notification is more appropriate for batch operations like startup mounting.

## What Changes

- **Startup mount batch**: `start_all_mounts` collects results and emits one summary notification instead of one per mount
- **Individual mount actions**: Per-mount notifications remain for user-initiated actions (add mount, toggle mount) since the user explicitly triggered that single operation
- **Failure handling**: Summary notification includes both successes and failures (e.g., "5 drives mounted, 1 failed")

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-lifecycle`: The "Mount Ready" notification behavior changes from per-mount to batched on startup, while preserving per-mount notifications for individual user actions

## Impact

- `crates/carminedesktop-app/src/main.rs`: `start_all_mounts` function, notification logic
- `crates/carminedesktop-app/src/notify.rs`: New `mounts_summary` notification function
