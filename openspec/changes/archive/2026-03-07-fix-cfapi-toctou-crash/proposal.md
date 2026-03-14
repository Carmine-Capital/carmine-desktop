## Why

`CfCreatePlaceholders` returns `ERROR_CLOUD_FILE_INVALID_REQUEST` when a placeholder already exists on disk. The current code pre-filters with `.exists()` but a TOCTOU window between that check and the `CfCreatePlaceholders` call allows races; when the race is lost, the error propagates to `proxy.rs` in the `cloud-filter` crate, where `command::CreatePlaceholders::fail(...).unwrap()` panics across the FFI boundary, producing an unrecoverable `STATUS_STACK_BUFFER_OVERRUN` process crash.

## What Changes

- **`cfapi.rs` — placeholder creation loop**: Replace the single batch `pass_with_placeholder` call with a per-item loop. For each item, call `ticket.pass_with_placeholder` with a one-element slice; catch `ERROR_CLOUD_FILE_INVALID_REQUEST` (Win32 `0x8007017C`) and log `warn!` then continue — turning a TOCTOU collision from a fatal crash into a logged non-event. Genuine API errors (any other error code) are still returned as `Err` so the OS callback infrastructure is correctly notified of real failures.
- **Pre-filter retained as optimisation hint**: The existing `.filter(|..| !exists())` check stays in place as a best-effort early-out; the per-item error handling is the safety net that makes the TOCTOU gap non-fatal.

## Capabilities

### New Capabilities

_None — this is a targeted bug fix with no new user-visible capability._

### Modified Capabilities

- `virtual-filesystem`: The placeholder-population scenario on Windows gains a requirement that `ERROR_CLOUD_FILE_INVALID_REQUEST` from `CfCreatePlaceholders` is treated as a per-item recoverable skip, not a fatal error. The VFS MUST NOT crash on TOCTOU collisions during `FetchPlaceholders` callbacks.

## Impact

- **Files changed**: `crates/carminedesktop-vfs/src/cfapi.rs` only.
- **Platform scope**: Windows only (`#[cfg(target_os = "windows")]`).
- **External crate `cloud-filter 0.0.6`**: The panic site (`proxy.rs:153`, `command::CreatePlaceholders::fail(...).unwrap()`) is in an external dependency and cannot be modified. The fix in `cfapi.rs` prevents the Err path from being reached for TOCTOU collisions, making the external unwrap a dead code path for this class of error.
- **No API or ABI changes**: The `SyncFilter` trait signature is unchanged; only the internal implementation of `fetch_placeholders` changes.
- **No other crates affected**.
