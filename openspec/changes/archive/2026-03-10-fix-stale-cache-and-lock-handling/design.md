## Context

carminedesktop mounts OneDrive/SharePoint as local filesystems via FUSE (Linux/macOS) and CfApi (Windows). Three related issues exist:

1. **Stale disk cache corruption**: `open_file` validates the disk cache against memory-cached metadata (which may also be stale). The server metadata refresh added in commit `27c53be` is positioned *after* the disk cache check, so it never fires when both caches are stale-but-consistent (before the 60s delta sync cycle).

2. **Silent upload failure on FUSE**: When `flush_inode` fails (e.g. 423 Locked), the FUSE `flush` callback returns `EIO` with no `VfsEvent` emitted. CfApi already emits `WritebackFailed` in the equivalent path.

3. **No lock awareness**: OneDrive locks files during online co-authoring. carminedesktop has no awareness of this — users can edit locally, then the upload silently fails. The Graph API exposes lock status that we don't check.

## Goals / Non-Goals

**Goals:**
- Eliminate stale-cache corruption by always validating against server metadata before serving cached content
- Notify users on all upload failures, not just conflicts (FUSE parity with CfApi)
- Detect 423 Locked specifically: save a conflict copy and notify, rather than silently failing
- Warn users at open time when a file is locked online
- Keep all changes cross-platform where applicable (lock detection in `CoreOps`, notification in platform backends)

**Non-Goals:**
- True co-authoring (WOPI/MS-FSSHTTP protocol support)
- Automatic retry of failed uploads (dangerous — server content may have diverged)
- Blocking open of locked files (user should still be able to open and edit locally)

## Decisions

### 1. Move server refresh before disk cache check in `open_file`

The `get_item()` call moves from after the disk cache validation (line ~1098) to before it (before line ~1072). The disk cache validation then compares against the fresh server metadata. If the server responds with a different eTag, the stale disk cache entry is evicted and a fresh download proceeds.

**Alternative considered**: Add a TTL to disk cache entries. Rejected because it adds complexity without solving the root cause — the disk cache *should* be validated against the source of truth (server), not against another cache layer.

**Fallback**: If `get_item()` fails (network error), fall through to the existing disk cache validation with cached metadata. This preserves offline-capable reads.

### 2. New `Error::Locked` variant for 423

Add `Error::Locked` to `carminedesktop-core::Error`, analogous to the existing `PreconditionFailed` for 412. `handle_error` in `GraphClient` maps 423 to this variant. `flush_inode` matches on `Error::Locked` and handles it distinctly from generic errors.

**Alternative considered**: Reuse `PreconditionFailed`. Rejected because the semantics and handling differ — 412 triggers a conflict upload of the same content, while 423 means the file is actively locked and we need a copy.

### 3. Conflict copy on 423 Locked (not retry)

When `flush_inode` gets `Error::Locked`, it uploads the local content as a conflict copy using the existing `conflict_name()` function (e.g. `report.conflict.1741612345.xlsx`). The writeback buffer is cleared after the copy upload succeeds.

This reuses the same conflict copy pattern as the eTag conflict path. The conflict copy is uploaded to the same parent folder.

**Alternative considered**: Retry with backoff when lock clears. Rejected because by the time the lock clears, the server version may have diverged — retry would silently overwrite the co-author's changes.

### 4. New `VfsEvent::UploadFailed` variant

Add `VfsEvent::UploadFailed { file_name: String, reason: String }` for generic upload failures. The FUSE `flush` callback emits this when `flush_handle` returns an error. The existing `WritebackFailed` on CfApi covers the equivalent path already.

A separate `VfsEvent::FileLocked { file_name: String }` is emitted:
- From `flush_inode` when 423 is detected (alongside the conflict copy upload)
- From `open_file` when the file's lock status indicates it's locked online

### 5. Lock check on open via Graph API

In `open_file`, after the server metadata refresh (which already calls `get_item`), inspect the response for lock indicators. The Graph API's `publication` facet or the `@microsoft.graph.conflicts` annotation can indicate a locked file. If locked, emit `VfsEvent::FileLocked` as a warning — do not block the open.

This piggybacks on the `get_item` call we're already making (from decision #1), so no additional API call is needed.

### 6. Lock check scope

The lock check applies to both FUSE and CfApi backends since it's implemented in `CoreOps::open_file`. The `VfsEvent::FileLocked` is consumed by the platform-specific event forwarder.

## Risks / Trade-offs

- **Extra API call on every open**: The `get_item()` call that was previously only reached on disk cache miss now runs on every `open_file`. This adds ~50-200ms latency per open. Mitigation: this is acceptable for correctness — serving corrupted data is worse than a small latency hit. The call is cheap (single metadata fetch, no content download).

- **Conflict copy accumulation**: If a user repeatedly saves while a file is locked online, multiple conflict copies are created. Mitigation: the timestamp in the name makes each unique; same behavior as existing eTag conflict path. Users can clean up copies manually.

- **Lock detection reliability**: The Graph API lock indicators may not cover all lock types (e.g. SharePoint checkout locks vs. co-authoring locks). Mitigation: the 423 handling in `flush_inode` is the safety net — even if the open-time check misses a lock, the save-time check catches it.

- **Offline degradation**: Moving the `get_item()` before disk cache means offline opens fail if there's no cached metadata. Mitigation: on `get_item` failure, fall back to existing disk cache validation path (stale-but-available is better than nothing when offline).
