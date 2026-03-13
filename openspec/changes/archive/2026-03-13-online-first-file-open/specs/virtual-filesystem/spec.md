## REMOVED Requirements

### Requirement: VfsError::OperationCancelled for CollabGate cancel
**Reason**: The `Cancel` response variant is removed — CollabGate always resolves to `OpenOnline` or `OpenLocally` (fallback). The `OperationCancelled` error and its platform mappings (ECANCELED on FUSE, STATUS_CANCELLED on WinFsp) are dead code.
**Migration**: None. Pre-production change.
