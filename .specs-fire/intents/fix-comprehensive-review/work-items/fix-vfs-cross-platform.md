---
id: fix-vfs-cross-platform
title: Fix VFS cross-platform bugs (errno, case-insensitive, CfApi)
intent: fix-comprehensive-review
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-013
completed_at: 2026-03-09T19:18:09.376Z
---

# Work Item: Fix VFS cross-platform bugs (errno, case-insensitive, CfApi)

## Description

Fix cross-platform issues in the VFS crate:

1. **Hardcoded errno** (`mount.rs:22`): `ENOTCONN` is 107 on Linux but 57 on macOS. Replace `raw == Some(107)` with `raw == Some(libc::ENOTCONN as i32)` and same for `EIO`. The `libc` crate is already a dependency.

2. **Case-insensitive lookup on Windows** (`core_ops.rs:365`): `find_child` uses `item.name == name` (case-sensitive). Windows (NTFS/CfApi) sends paths with user's casing. Add `#[cfg(target_os = "windows")]` branch using `eq_ignore_ascii_case`.

3. **CfApi state_changed no-op** (`cfapi.rs:408-412`): Only logs debug, ignores pin/unpin. At minimum, invalidate the memory cache entry so next access re-fetches from server.

4. **CfApi closed reads entire file** (`cfapi.rs:279`): `std::fs::read(&abs_path)` loads entire file into memory. For large files, use chunked reading or streaming upload.

5. **libc dep inline** (`vfs/Cargo.toml:21`): Move `libc = "0.2"` to workspace root `[workspace.dependencies]`, reference as `{ workspace = true }`.

6. **RwLock cascading panic** (`inode.rs`): All `read().unwrap()` / `write().unwrap()` will cascade-panic on poisoned lock. Add `.expect("inode lock poisoned — fatal")` with clear message, or handle gracefully by returning an error.

## Acceptance Criteria

- [ ] `cleanup_stale_mount` uses `libc::ENOTCONN` and `libc::EIO` instead of magic numbers
- [ ] `find_child` uses case-insensitive comparison on Windows via `#[cfg]`
- [ ] CfApi `state_changed` invalidates memory cache for changed paths
- [ ] CfApi `closed` callback uses chunked/streaming read for files above a threshold
- [ ] `libc` dependency declared in workspace root, referenced as `{ workspace = true }`
- [ ] RwLock unwrap calls have descriptive panic messages
- [ ] Existing tests pass on Linux; code compiles on all platforms

## Technical Notes

For case-insensitive lookup, use `eq_ignore_ascii_case` (not Unicode case folding — NTFS uses OrdinalIgnoreCase which is ASCII-based for non-Unicode paths).

For CfApi closed, consider a size threshold (e.g., 4MB) below which full read is acceptable, above which use chunked upload via `upload_large`.

## Dependencies

(none)
