## Context

Currently, each successful mount triggers `notify::mount_success()` at the end of `start_mount()`. For `start_all_mounts()`, this means N notifications for N drives. The user explicitly requested batching startup notifications while preserving individual notifications for user-initiated mount actions.

## Goals / Non-Goals

**Goals:**
- Batch notifications for startup mount sequence into a single summary
- Include both successes and failures in the summary
- Preserve per-mount notifications for `add_mount` and `toggle_mount` commands

**Non-Goals:**
- Changing any notification content other than mount success/failure
- Adding new configuration options for notification behavior
- Modifying the tray menu or UI

## Decisions

### D1: Move notification dispatch from `start_mount` to caller

**Decision**: `start_mount` returns a `Result<MountHandle, MountError>` without sending notifications. The caller (`start_all_mounts`, `add_mount`, `toggle_mount`) decides whether and how to notify.

**Rationale**: Gives callers full control over notification strategy without needing to pass flags or context into `start_mount`. Simpler than adding a `silent: bool` parameter.

**Alternative considered**: Add `silent: bool` parameter to `start_mount`. Rejected because it propagates through the call chain and doesn't scale if we need more nuanced notification modes.

### D2: Summary notification format

**Decision**: 
- All successful: `"N drives mounted"`
- Some failures: `"N drives mounted, M failed"` (failures already logged with details)
- All failed: `"Failed to mount N drives"` (details in log)

**Rationale**: Concise and actionable. Failures are already logged with specific reasons; notification just signals the user to check the app/logs.

### D3: Collect results in `start_all_mounts`

**Decision**: Iterate through mounts, collect `(name, result)` pairs, then send summary at the end.

**Rationale**: Simple and doesn't require async primitives. Mount startup is sequential anyway.

## Risks / Trade-offs

| Risk | Mitigation |
|------|------------|
| User misses individual mount failures in summary | Failures are logged with mount name and reason; user can check logs |
| Summary feels impersonal for single mount | Edge case—single mount startup still gets one notification, just phrased as summary |
