## Why

The VFS parity audit verified three residual defects that the in-progress `fix-cfapi-safety-parity` change does not cover: a compile error on Windows (`rel_path` undefined in `fetch_data`), a missing `WritebackFailed` event when `flush_inode` fails after `closed()`, and `state_changed()` not invalidating the parent directory cache. All three are confirmed against current code and remain unaddressed.

## What Changes

- **Fix `rel_path` compile error in `cfapi.rs::fetch_data`** — replace undefined `rel_path` with `abs_path.display()` in the `tracing::warn!` at the `write_at` failure path (line 210). Without this fix the CfApi backend cannot compile on Windows.
- **Emit `VfsEvent::WritebackFailed` on `flush_inode` failure in `closed()`** — the `Err` branch at `cfapi.rs:400-402` only logs; it does not notify the user. Add `send_event(VfsEvent::WritebackFailed { file_name })` so the UI surfaces upload failures.
- **Invalidate parent directory cache in `state_changed()`** — `cfapi.rs::state_changed()` only calls `cache.memory.invalidate(ino)` on the changed item. It must also invalidate the parent inode so `list_children` returns fresh results after a Windows placeholder state change.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `virtual-filesystem`: CfApi `fetch_data` compile error fixed; `closed()` flush failure surfaces a `WritebackFailed` event; `state_changed()` invalidates parent directory cache.

## Impact

- **`crates/cloudmount-vfs/src/cfapi.rs`** — Three targeted edits: `fetch_data` tracing fix, `closed()` flush error handling, `state_changed()` parent invalidation.
- No API changes, no dependency changes, no spec-level behavior changes (these are bug fixes bringing CfApi up to the safety guarantees already documented in the `virtual-filesystem` spec).
