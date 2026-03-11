## Context

The Windows Explorer "Open in SharePoint" context-menu entry is implemented as a global HKCU registry key, but its lifecycle is currently managed from individual CfApi mount/unmount events. In multi-mount sessions this causes premature cleanup: unmounting one drive can remove the menu while other drives remain active.

CloudMount mounts are independent per drive, but shell integration is shared process-wide. The lifecycle policy must therefore be tied to aggregate active-mount state, not to a single mount instance.

## Goals / Non-Goals

**Goals:**
- Keep the Windows context-menu entry available while at least one CloudMount Windows mount is active.
- Remove the entry only after the final active Windows mount is unmounted.
- Make register/unregister operations idempotent and resilient to restarts or stale registry state.
- Preserve existing deep-link behavior and error handling.

**Non-Goals:**
- Dynamic Explorer visibility filtering to CloudMount paths only.
- COM shell extension implementation.
- Changes to Linux/macOS integration behavior.
- Changes to deep-link URL schema or command semantics.

## Decisions

### 1. Process-wide active mount tracking

**Decision:** Introduce a Windows-only process-wide active mount counter (or equivalent guarded state) in `cfapi.rs`. Increment after successful mount setup and decrement during unmount teardown.

**Rationale:** Registry integration is global, so lifecycle decisions must be global. Per-handle logic cannot safely decide when global cleanup is correct in multi-mount scenarios.

**Alternative considered:** Query current sync roots from the OS at each unmount. Rejected for complexity and edge-case handling compared to in-process state.

### 2. First-mount registration and last-mount cleanup policy

**Decision:** Register context menu when transitioning from 0 -> 1 active mounts. Remove it when transitioning from 1 -> 0 active mounts.

**Rationale:** This exactly models desired availability while minimizing redundant registry writes.

**Alternative considered:** Register on every mount and unregister on every unmount. Rejected because it reintroduces the current race/incorrect cleanup behavior.

### 3. Idempotent and fault-tolerant registry helpers

**Decision:** Registry registration helpers tolerate pre-existing keys; cleanup helpers treat missing keys as no-op success.

**Rationale:** Users may restart the app, crash mid-lifecycle, or have stale keys from previous versions. The lifecycle code must converge to correct state without hard failure.

**Alternative considered:** Strict failures on already-exists/not-found. Rejected as brittle and unnecessary.

## Risks / Trade-offs

- **[Risk] Counter desynchronization after unexpected process termination** -> Mitigation: registration on first successful mount converges state quickly; cleanup on normal unmount still applies.
- **[Risk] Multiple CloudMount processes on Windows** -> Mitigation: treat this as unsupported for v1; retain idempotent key handling to reduce impact.
- **[Risk] Unmount error paths might skip decrement** -> Mitigation: ensure decrement happens in teardown paths that run after connection drop/unregister attempts.
- **[Risk] Global key still appears outside CloudMount directories** -> Mitigation: existing deep-link path validation and user notification remain in place.
