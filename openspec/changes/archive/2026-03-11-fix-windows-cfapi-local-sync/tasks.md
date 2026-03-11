## 1. CfApi local-change ingest foundation

- [x] 1.1 Add a Windows local-change ingest dispatcher in the CfApi backend and wire it from `state_changed` plus relevant close/rename paths.
- [x] 1.2 Implement non-placeholder path resolution and parent association so externally copied/created files can be represented in VFS state.
- [x] 1.3 Stage ingested file bytes into writeback storage and trigger `CoreOps` flush using existing conflict-safe upload semantics.

## 2. Safe-save transaction handling

- [x] 2.1 Implement safe-save transaction detection for rename-to-temp/backup and replacement flows.
- [x] 2.2 Defer remote rename commit during reconciliation and resolve final action as either content update or true rename.
- [x] 2.3 Add timeout/cleanup logic for unresolved safe-save transactions with deterministic logs.

## 3. Runtime retry and sync-root policy

- [x] 3.1 Add an in-session retry loop that processes pending writeback entries while mounts are active.
- [x] 3.2 Reuse pending recovery primitives for retry attempts and preserve pending content across repeated failures.
- [x] 3.3 Register Windows sync roots with explicit supported in-sync attributes (including file and directory last-write-time).

## 4. Observability hardening

- [x] 4.1 Add structured reason logs for every early-return guard in CfApi `closed()`.
- [x] 4.2 Add structured logs for local-change ingest outcomes (`enqueued`, `deferred`, `skipped`, `retried`) with path and reason.
- [x] 4.3 Ensure unresolved-path and non-placeholder cases surface diagnostics instead of silent returns.

## 5. Windows CfApi validation coverage

- [x] 5.1 Add/extend integration coverage for external copy-in followed by upload completion.
- [x] 5.2 Add integration coverage for safe-save rename/replace flow verifying correct final remote state.
- [x] 5.3 Add retry-path coverage where initial upload fails transiently and succeeds during mounted-session retry.
- [ ] 5.4 Run CfApi integration checks for edit/rename/delete regressions and record results.
