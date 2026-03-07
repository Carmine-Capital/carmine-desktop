## Why

`flush_pending` — ~60 lines of write-back upload logic — is duplicated verbatim between `MountHandle` (FUSE, `mount.rs`) and `CfMountHandle` (CfApi, `cfapi.rs`). The logic is platform-agnostic (it operates on `CacheManager` and `GraphClient`, not FUSE or CfApi primitives), so any bug fix or behavioural change must be applied in two places. This is the one genuine DRY violation in the VFS crate identified during architecture review.

## What Changes

- Extract `flush_pending` into a single free async function in a new non-platform-gated module (`crates/cloudmount-vfs/src/pending.rs`).
- Remove the duplicated `flush_pending` implementations from `MountHandle` and `CfMountHandle`; both call the shared function instead.
- No public API changes — `flush_pending` is private to both structs today and remains so.

## Capabilities

### New Capabilities

_(none — this is a pure internal refactor with no new user-facing capability)_

### Modified Capabilities

_(none — no spec-level behaviour changes. The write-back flush semantics are unchanged: same timeout, same upload loop, same error handling.)_

## Impact

- **Files changed**: `crates/cloudmount-vfs/src/mount.rs`, `crates/cloudmount-vfs/src/cfapi.rs`, `crates/cloudmount-vfs/src/lib.rs` (new module), new file `crates/cloudmount-vfs/src/pending.rs`.
- **No API changes**: `flush_pending` is private; callers (`unmount`) are within the same crate.
- **No dependency changes**: `pending.rs` uses only types already in scope (`CacheManager`, `GraphClient`, `tokio`).
- **Compile targets**: all three platforms (Linux, macOS, Windows) compile the new module; it is not `#[cfg]`-gated.
