## Context

The `cloud-filter` crate (v0.0.6) registers `extern "system"` callbacks with the Windows CfApi. When any callback returns `Err`, the crate's `proxy.rs` attempts to report the failure back to the OS via `CfExecute`. It does so with `.unwrap()`. If `CfExecute` itself fails — which it does when the operation context is invalid (HRESULT `0x8007017C`, `ERROR_CLOUD_FILE_INVALID_REQUEST`) — the `.unwrap()` panics across the FFI boundary, producing `STATUS_STACK_BUFFER_OVERRUN` and an unrecoverable process crash.

The previous `fix-cfapi-toctou-crash` change addressed this for `fetch_placeholders` by catching `0x8007017C` per-item. The same proxy unwrap exists for every other callback: `fetch_data` (proxy.rs:97), and the ticket-passing callbacks `delete`, `rename`, and `dehydrate`. These are still exposed.

A secondary trigger identified in CI: the integration test helper `create_root_placeholders` creates file placeholders with `mark_in_sync()` but no on-disk bytes. `mark_in_sync()` tells the OS the file is fully synchronized locally. When the OS then fires `fetch_data` for that file (because no local bytes exist despite declared size), `CfExecute(TRANSFER_DATA)` is rejected with `0x8007017C` because the OS believes the file is already complete. `fetch_data` then returns `Err`, landing in the proxy crash.

Key constraint: `cloud-filter 0.0.6` is external and its `proxy.rs` cannot be modified.

## Goals / Non-Goals

**Goals:**
- Ensure no CfApi callback can trigger the proxy `.unwrap()` crash, regardless of what goes wrong inside the callback.
- Preserve full observability: all errors that would have crashed the process are logged at `warn` level.
- Fix the integration test fixture so that file placeholders are created in a state the OS actually accepts data transfers for.
- Keep changes isolated to `cfapi.rs` and the integration test file.

**Non-Goals:**
- Modifying `cloud-filter` or patching `proxy.rs`.
- Fixing non-CfApi (FUSE) error handling.
- Eliminating the underlying causes of individual callback errors (network failures, cache misses) — only the crash is being fixed.

## Decisions

### D1 — fetch_data: convert all Err paths to log-and-Ok

**Decision**: Replace every `return Err(...)` and `?` propagation in `fetch_data` with `tracing::warn!` + `return Ok(())`.

**Current paths that return Err:**
1. `resolve_path` → `None` → `ok_or(CloudErrorKind::NotInSync)?`
2. `read_range_direct` failure → `map_err(|_| CloudErrorKind::Unsuccessful)?`
3. `ticket.write_at` failure → `map_err(|_| CloudErrorKind::Unsuccessful)?`

**New behaviour for each:**
1. `resolve_path` → `None` → log warn, return `Ok(())`
2. `read_range_direct` failure → log warn (include error), return `Ok(())`
3. `ticket.write_at` failure → log warn (include error), break out of loop, return `Ok(())`

**Rationale**: Returning `Err` from `fetch_data` causes `proxy.rs:97` to call `command::Write::fail(...).unwrap()`. If `CfExecute` fails there, the process crashes. Returning `Ok(())` without writing data causes the OS to surface an I/O error to the application that requested the read — a far better outcome than a process crash.

**Alternative considered**: Match on `write_at` error code and only swallow `0x8007017C`, propagating others. Rejected because any `write_at` error causes the proxy crash regardless of code; consistent behaviour is safer and simpler.

### D2 — delete, rename, dehydrate: convert ticket.pass() Err to log-and-Ok

**Decision**: Replace `ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)?` with:
```rust
if let Err(e) = ticket.pass() {
    tracing::warn!("cfapi: ticket.pass() failed in <callback>: {e:?}");
}
```

**Rationale**: By the time `ticket.pass()` is called in these three callbacks, all side effects (Graph API calls, cache invalidation, writeback removal) have already been performed. The sole function of `ticket.pass()` is to acknowledge the OS operation. If the acknowledgement fails and we return `Err`, the proxy crashes. If we return `Ok(())`, the OS retries or surfaces the failure independently. The side effects are idempotent, so a retry is safe.

**Scope**: This applies to `dehydrate`, `delete`, and `rename`. The `closed` and `state_changed` callbacks are `fn ... -> ()` and cannot trigger the crash.

### D3 — Test fixture: dehydrate file placeholders

**Decision**: In `create_root_placeholders()`, remove `.mark_in_sync()` from the `hello.txt` file placeholder. Directory placeholders (`docs`) may keep it.

**Rationale**: `mark_in_sync()` signals to the OS that the file is fully synchronized and its local bytes are complete. When the OS later fires `fetch_data` for a file it believes is in-sync, `CfExecute(TRANSFER_DATA)` is rejected with `ERROR_CLOUD_FILE_INVALID_REQUEST`. Creating the file placeholder without `mark_in_sync()` (dehydrated) is the correct representation of a cloud-only file: the OS knows to hydrate it on access and accepts `TRANSFER_DATA` writes.

**Scope**: Integration test only (`cfapi_integration.rs`). No production code uses this helper.

### D4 — Retain fetch_placeholders TOCTOU fix unchanged

**Decision**: The per-item loop and `0x8007017C` check in `fetch_placeholders` from `fix-cfapi-toctou-crash` is correct and stays exactly as implemented.

## Risks / Trade-offs

- **fetch_data returning Ok() without writing** → the application's read will fail at the OS level (I/O error or stall). This is acceptable: a failed read is recoverable at the application level; a process crash is not.
- **ticket.pass() failures silenced for delete/rename/dehydrate** → if the OS does not record the acknowledgement, it may retry the callback. Our side effects (cache invalidation, Graph API call) have already run; idempotency means a retry is safe. Logged at `warn` for observability.
- **Error code information lost** → by converting to `Ok(())` after logging, the specific error codes are no longer visible to the OS caller. Mitigation: structured `tracing::warn!` fields preserve the error for log aggregation.
