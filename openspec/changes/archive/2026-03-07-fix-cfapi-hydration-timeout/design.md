## Context

The Windows Cloud Files API requires every `fetch_data` callback to complete in one of two ways:
1. Call `CfExecute(TRANSFER_DATA)` covering the required byte range (success — hydration complete).
2. Call `CfExecute` with a non-success `CompletionStatus` (failure — Windows cancels and surfaces an error immediately).

If neither happens, Windows waits 60 seconds before returning error 426 ("The cloud operation was not completed before the time-out period expired") to the calling process.

The `cloud-filter` Rust crate's `fetch_data` proxy (`proxy.rs`) implements this protocol:
- On `Ok(())`: proxy returns immediately — **no CfExecute is issued**.
- On `Err(e)`: proxy calls `command::Write::fail(...).unwrap()` — this issues CfExecute with a non-success status, immediately cancelling the operation.

Commit `e0c410b` changed all error paths in `fetch_data` to return `Ok(())` to prevent a STATUS_STACK_BUFFER_OVERRUN crash. This traded a crash for a guaranteed 60-second hang on every hydration failure, breaking the `cfapi_hydrate_file_on_read` and `cfapi_edit_and_sync_file` CI tests.

A secondary problem: `fetch_data` resolves the item by calling `resolve_path`, which on a cache miss traverses from root via `list_children` — a network round-trip in the hot hydration path. The item ID is already available via `request.file_blob()` (set at placeholder creation time).

## Goals / Non-Goals

**Goals:**
- Error paths in `fetch_data` signal failure to Windows immediately (no 60-second timeout).
- Hydration resolves item ID from the placeholder blob, not from a Graph API call.
- Both `cfapi_hydrate_file_on_read` and `cfapi_edit_and_sync_file` tests pass.

**Non-Goals:**
- Changing FUSE error handling.
- Changing Graph API retry behavior.
- Forking or patching the `cloud-filter` crate.
- Fixing unrelated CfApi callbacks (`fetch_placeholders`, `closed`, `rename`, `delete`).

## Decisions

### D1: Return `Err(CloudErrorKind::Unsuccessful)` from all error paths in `fetch_data`

The only way to trigger proxy.rs's `Write::fail` path (and thus issue CfExecute with an error status) is to return `Err` from our `fetch_data` implementation. There is no way to call `Write::fail` directly — `connection_key` and `transfer_key` are private fields in the `FetchData` ticket type.

**Why `Unsuccessful` and not another error kind?** `CloudErrorKind::Unsuccessful` is the appropriate catch-all for "the provider could not fulfill this request" — it maps to `STATUS_CLOUD_FILE_UNSUCCESSFUL`. None of the other error kinds (`NotSupported`, `InvalidRequest`, etc.) are more accurate for network or resolution failures.

**Concern — STATUS_STACK_BUFFER_OVERRUN risk**: Commit `e0c410b` introduced `Ok(())` returns to avoid this crash. The crash occurs when `Write::fail`'s internal `CfExecute` call fails and proxy.rs's `.unwrap()` panics, propagating a Rust panic across the FFI boundary.

**Mitigation**: `CfExecute` inside `Write::fail` fails only if the transfer key is invalid or the connection is closed. In a live `fetch_data` callback — invoked by Windows because a process tried to read a dehydrated file — the transfer key is always valid; Windows would not dispatch the callback if it had already cancelled the operation. The error paths we are fixing (path not in sync root, item ID decode failure, download failure, empty content) all occur early in the callback, before any partial write, so the transfer key is guaranteed fresh and valid. `Write::fail` will succeed.

**Alternative considered**: Leave `Ok(())` returns (current behavior) — this is the 60-second hang; not acceptable.

### D2: Resolve item ID from `request.file_blob()` instead of `resolve_path`

When a placeholder is created via `PlaceholderFile::blob(item.id.as_bytes())`, the item ID is embedded in the placeholder and returned verbatim as `request.file_blob()` in every subsequent `fetch_data` callback for that file.

Using `file_blob()`:
- Eliminates the `resolve_path` call (which on cache miss traverses from root via `list_children` — one or more Graph API network round-trips).
- Is infallible for well-formed placeholders (only fails if the blob is invalid UTF-8, which cannot happen since item IDs are ASCII).
- Makes `fetch_data` require only: blob decode → item lookup by inode (already cached after first mount) → download.

The inode is still needed for `read_range_direct`. After extracting the item ID from blob, we look it up in the inode table to get the inode. If the inode is not found (item was never registered), we return `Err(CloudErrorKind::Unsuccessful)`.

**Alternative considered**: Keep `resolve_path` — works correctly but is slow (network) and fragile (cache miss ≠ real miss, and any transient failure causes a 60-second timeout rather than a quick error).

## Risks / Trade-offs

- **`Write::fail` in partially-written state**: If `write_at` fails mid-loop after some chunks were already written, returning `Err` causes Windows to issue `Write::fail`. Windows will discard the partial transfer and put the file back to dehydrated state. This is correct — the file is not hydrated. The risk of triggering `CfExecute` with a partially-open transfer is the same as in the no-write case; the transfer key remains valid.

- **`file_blob()` returns wrong data**: If a placeholder was created without a blob, or with wrong bytes, `from_utf8` may fail or the item ID lookup may miss. Both cases return `Err` immediately — correct behavior (Windows signals failure; user retries access).

## Migration Plan

1. Change `fetch_data` in `crates/carminedesktop-vfs/src/cfapi.rs`:
   - Replace the `resolve_path` lookup with `file_blob()` decode + inode table lookup.
   - Change all `return Ok(())` error returns to `return Err(CloudErrorKind::Unsuccessful)`.
   - Keep the `write_at` loop; on loop error, return `Err(CloudErrorKind::Unsuccessful)`.
2. Run Windows CI — `cfapi_hydrate_file_on_read` and `cfapi_edit_and_sync_file` must pass.
3. Verify `cfapi_browse_populates_placeholders`, `cfapi_rename_file`, `cfapi_delete_file`, `cfapi_mount_and_unmount_lifecycle` continue to pass (they don't call `fetch_data`).

## Open Questions

None — root cause is confirmed; approach is deterministic.
