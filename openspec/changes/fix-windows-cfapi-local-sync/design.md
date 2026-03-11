## Context

Windows users report that local edits, copy-in operations, and Office safe-save flows remain indefinitely in a pending sync state. Current CfApi behavior relies heavily on placeholder-oriented callbacks (`closed`) and attribute-change notifications (`state_changed`) that are not sufficient for non-placeholder local file lifecycle events.

The current implementation has two structural gaps:
- local non-placeholder changes may never enter the writeback/flush pipeline,
- failed uploads are only recovered on restart/unmount, not during normal mounted runtime.

This change must preserve existing cross-platform `CoreOps` semantics while making Windows local-change ingestion reliable and observable.

## Goals / Non-Goals

**Goals:**
- Ensure Windows local create/copy/replace/safe-save flows reliably stage and upload without requiring restart/unmount.
- Prevent safe-save rename sequences from causing incorrect remote rename side effects.
- Add runtime retry for pending writeback uploads while mounts remain active.
- Make skip/failure decisions in CfApi callbacks diagnosable through structured logs.
- Keep FUSE/Linux/macOS behavior unchanged.

**Non-Goals:**
- Redesigning `CoreOps` upload/conflict model.
- Changing Graph API contract, token model, or cache schema format.
- Introducing user-visible configuration for retry policy in this change.

## Decisions

### 1) Add a Windows local-change ingress path independent of placeholder `closed()`
- **Decision:** Introduce a dedicated ingest pipeline for filesystem events under CfApi mounts that can stage and flush content even when `closed()` is not triggered.
- **Rationale:** Placeholder callbacks alone do not cover external copy-in and some safe-save outputs.
- **Alternatives considered:**
  - Expand `closed()` only: rejected because missing callbacks cannot be fixed from inside `closed()`.
  - Keep cache invalidation-only `state_changed`: rejected because it detects change but never initiates upload.

### 2) Detect safe-save as a transaction, not a plain rename
- **Decision:** Treat rename-to-temp/backup plus replacement of original path as a safe-save transaction. Defer server-side rename commit briefly and reconcile final file state before deciding between rename vs content update.
- **Rationale:** Office/modern editors frequently perform safe-save; immediate rename propagation can misrepresent user intent remotely.
- **Alternatives considered:**
  - Always execute remote rename immediately: rejected (causes remote drift in safe-save flows).
  - Disable rename propagation completely: rejected (breaks legitimate user renames).

### 3) Add mounted-session retry loop for pending writeback
- **Decision:** Add a periodic retry task while mounts are active, reusing existing pending-write recovery primitives.
- **Rationale:** Current restart/unmount-only recovery is insufficient for normal desktop use.
- **Alternatives considered:**
  - Retry only on next file event: rejected as nondeterministic and sparse.
  - Retry only in delta-sync loop: rejected due to coupling and weaker isolation from remote metadata sync.

### 4) Strengthen CfApi observability and sync-root policy registration
- **Decision:** Log each early-return path in `closed()` and local ingest decisions with explicit reason codes; register sync root supported in-sync attributes (including last-write-time attributes).
- **Rationale:** Current silent guard exits make field diagnosis difficult; explicit in-sync attribute policy avoids ambiguous Explorer sync-state behavior.
- **Alternatives considered:**
  - Keep current sparse debug logs: rejected (insufficient for production triage).
  - Add only ad-hoc logs without reason taxonomy: rejected (hard to aggregate and compare runs).

## Risks / Trade-offs

- **[Risk] Event amplification and duplicate processing** -> **Mitigation:** path-level debounce/coalescing and idempotent staging keyed by inode/item identity.
- **[Risk] False safe-save classification for unusual rename patterns** -> **Mitigation:** bounded decision window, conservative pattern checks, and fallback to normal rename when confidence is low.
- **[Risk] Retry loop increases Graph traffic under prolonged failures** -> **Mitigation:** bounded interval, jitter/backoff, and preserving existing error-specific handling.
- **[Risk] Additional Windows-specific complexity** -> **Mitigation:** isolate logic behind CfApi-only components; keep `CoreOps` interface minimal and reusable.

## Migration Plan

1. Introduce ingest/retry logic behind the Windows CfApi path only.
2. Add targeted integration tests for copy-in, safe-save replace, and retry recovery.
3. Roll out with enhanced logging to validate event coverage in real user flows.
4. If regressions occur, disable the new ingest trigger path while retaining existing `closed()` flow as fallback.

## Open Questions

- Should safe-save reconciliation window be fixed or adaptive per file extension/app behavior?
- Should retry cadence be shared with `sync_interval_secs` or remain independent?
- Do we need cloud-filter crate changes for broader event flags, or is a CloudMount-side watcher layer sufficient long-term?
