## Context

The `CloudMountCfFilter::fetch_placeholders` callback in `cfapi.rs` populates a directory with placeholder files by calling `ticket.pass_with_placeholder(&mut placeholders)`. The current implementation filters out already-existing items using `.filter(|..| !dir_path.join(&item.name).exists())` before building the batch, then passes the whole batch in a single call.

`pass_with_placeholder` wraps `CfCreatePlaceholders` (via `CF_OPERATION_TYPE_TRANSFER_PLACEHOLDERS`). If a placeholder already exists on disk at call time — due to a race between the `.exists()` check and the actual Windows API call — `CfCreatePlaceholders` returns `ERROR_CLOUD_FILE_INVALID_REQUEST` (HRESULT `0x8007017C`).

When `pass_with_placeholder` returns `Err`, our `fetch_placeholders` propagates it with `?`. The `cloud-filter` crate's `proxy.rs` catches the `Err` and calls `command::CreatePlaceholders::fail(...).unwrap()`. This invokes `CfExecute` with a failure status; if `CfExecute` itself fails (which it does in certain broken-connection states arising from the TOCTOU confusion), the `.unwrap()` panics across the `extern "system"` FFI boundary, producing `STATUS_STACK_BUFFER_OVERRUN` — an unrecoverable process crash.

Key constraint: `cloud-filter 0.0.6` is an external crate. Its `proxy.rs` cannot be modified without forking or patching.

## Goals / Non-Goals

**Goals:**
- Make TOCTOU collisions during `FetchPlaceholders` non-fatal: log a warning and continue to the next item.
- Preserve correct error propagation for genuine API failures (non-TOCTOU errors from `CfCreatePlaceholders`).
- Keep the change entirely within `crates/cloudmount-vfs/src/cfapi.rs`.

**Non-Goals:**
- Eliminate the TOCTOU window itself (impossible without OS-level atomic create-or-skip semantics from `CfCreatePlaceholders`).
- Modify `cloud-filter` or patch its `proxy.rs` unwrap.
- Change behaviour on Linux/macOS.

## Decisions

### D1 — Per-item placeholder creation instead of batch

**Decision**: Replace the single `pass_with_placeholder(&mut placeholders)` call with a `for` loop that calls `pass_with_placeholder(&mut [placeholder])` (one-element slice) per item.

**Rationale**: The batch API gives us a single success/failure for the entire set. With per-item calls, we can inspect each result independently and distinguish TOCTOU collisions (skip) from genuine errors (propagate or log). The batch variant has no partial-success reporting.

**Alternative considered**: Keep the batch call and catch `ERROR_CLOUD_FILE_INVALID_REQUEST` at the batch level by retrying failed items one-by-one. Rejected because it is more complex, performs at best the same number of API calls in the race case, and harder to reason about.

### D2 — Treat ERROR_CLOUD_FILE_INVALID_REQUEST as a per-item skip

**Decision**: After a per-item `pass_with_placeholder` failure, check if the Windows error code is `ERROR_CLOUD_FILE_INVALID_REQUEST` (`0x17C` facility-cloud, or raw Win32 `error_code & 0xFFFF == 0x17C`). If yes, log `warn!` and `continue`; otherwise return `Err` as today.

**Rationale**: `ERROR_CLOUD_FILE_INVALID_REQUEST` is the documented error for "placeholder already exists". It is the only error code that can arise from a TOCTOU race on this API. All other error codes represent genuine failures (invalid handle, permission denied, etc.) that deserve error propagation.

**Error code matching**: `windows::core::Error` carries a Win32 HRESULT. The HRESULT for `ERROR_CLOUD_FILE_INVALID_REQUEST` is `0x8007017C`. Match via `e.code().0 == 0x8007017cu32 as i32`.

### D3 — Retain the `.exists()` pre-filter as an optimisation hint

**Decision**: Keep the existing `filter(|..| !dir_path.join(&item.name).exists())` in place.

**Rationale**: The pre-filter eliminates the common steady-state case where a placeholder already exists (e.g., on re-entry into a directory). Removing it would increase the number of unnecessary per-item API calls. The per-item error handling is the _safety net_, not the _primary_ deduplication strategy.

## Risks / Trade-offs

- **N API calls instead of 1 batch** → For typical directory sizes (tens to hundreds of items) this is negligible. Windows CfApi callbacks are invoked on demand (not at mount time for every directory), so burst frequency is low.
- **Genuine errors no longer crash but still propagate as Err** → The `proxy.rs` `.unwrap()` in `cloud-filter` is still present for genuine errors; those remain a theoretical crash site if `CfExecute` itself fails during the failure-notification path. However, genuine `CfExecute` failures are rare and outside the TOCTOU scenario, so this is accepted.
- **Error code `0x8007017C` is a Win32 constant** → Stable since Windows 8.1 (Cloud Files API minimum requirement). Not a moving target.
