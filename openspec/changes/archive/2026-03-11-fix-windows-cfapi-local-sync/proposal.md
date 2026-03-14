## Why

On Windows, local file operations in mounted drives (copy-in from outside the mount, safe-save rewrites, and some edits/renames) can stay permanently in a pending sync state and never upload to Microsoft Graph. This creates data-loss risk and breaks the core promise that local changes eventually synchronize.

## What Changes

- Add a Windows-local change ingestion path that does not depend only on placeholder `closed()` callbacks, so copy/create/safe-save outputs are reliably staged for upload.
- Ensure local changes detected through CfApi `state_changed` are converted into actionable sync work (or explicitly logged as skipped with reason), instead of cache invalidation only.
- Add in-session retry behavior for failed writeback uploads so transient failures recover without requiring app restart or unmount.
- Improve CfApi observability by logging all early-return guard paths in `closed()` and local-change handling decisions with path + reason.
- Register Windows sync roots with explicit supported in-sync attributes to keep Explorer sync-state transitions deterministic.
- Add Windows CfApi integration coverage for copy-in, safe-save style replace, and retry-on-failure flows.

## Capabilities

### New Capabilities
- `cfapi-local-change-sync`: Reliable Windows upload pipeline for local non-placeholder changes (create/copy/safe-save) with eventual sync guarantees.

### Modified Capabilities
- `virtual-filesystem`: Extend Windows CfApi write semantics to include non-placeholder ingestion, runtime retry guarantees, and stricter diagnostics for skipped uploads.

## Impact

- Affected code: `crates/carminedesktop-vfs/src/cfapi.rs`, `crates/carminedesktop-vfs/src/core_ops.rs`, `crates/carminedesktop-vfs/src/pending.rs`, `crates/carminedesktop-app/src/main.rs`, Windows CfApi integration tests.
- Affected behavior: Windows upload reliability for local edits/copies/moves and sync-state transitions shown in Explorer.
- Dependency/system impact: may require extending current `cloud-filter` watcher usage or adding complementary filesystem event handling for non-placeholder change detection.
