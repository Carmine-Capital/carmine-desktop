---
run: run-cloud-mount-013
work_item: fix-vfs-cross-platform
intent: fix-comprehensive-review
generated: 2026-03-09T19:18:09Z
mode: confirm
---

# Implementation Walkthrough: Fix VFS cross-platform bugs

## Summary

Fixed six cross-platform issues in the `cloudmount-vfs` crate: replaced hardcoded Linux errno values with portable `libc` constants, added case-insensitive file name lookup for Windows NTFS/CfApi, made CfApi `state_changed` invalidate the memory cache, added chunked file reading for large files in CfApi `closed`, and replaced bare `.unwrap()` calls on RwLock guards with descriptive `.expect()` messages.

## Structure Overview

All changes are within the VFS crate. The errno fix in `mount.rs` ensures stale FUSE mount detection works on macOS (where ENOTCONN has a different numeric value). The `names_match` helper in `core_ops.rs` is a module-level function that compiles to exact match on Linux/macOS and ASCII case-insensitive match on Windows, applied at all three name comparison points in the cache lookup chain (memory, SQLite, Graph API). The CfApi changes in `cfapi.rs` are Windows-only code behind `#[cfg]` gates. The inode table changes are purely diagnostic improvements with no behavioral change.

## Files Changed

### Created

(none)

### Modified

| File | Changes |
|------|---------|
| `crates/cloudmount-vfs/src/mount.rs` | Replaced `Some(107)` / `Some(5)` with `Some(libc::ENOTCONN)` / `Some(libc::EIO)` |
| `crates/cloudmount-vfs/src/core_ops.rs` | Added `names_match()` helper with `#[cfg]` gates; updated 3 comparison sites in `find_child` |
| `crates/cloudmount-vfs/src/cfapi.rs` | `state_changed`: resolve path → invalidate memory cache; `closed`: size-based branching with BufReader for files > 4MB; added `SMALL_FILE_LIMIT` import |
| `crates/cloudmount-vfs/src/inode.rs` | All 10 `.unwrap()` → `.expect("inode table lock poisoned")` |

## Key Implementation Details

### 1. Portable errno constants

On Linux, `libc::ENOTCONN` is 107 and `libc::EIO` is 5. On macOS, `ENOTCONN` is 57. The `libc` crate provides the correct value per-platform. Clippy confirmed no `as i32` cast is needed since `libc` types are already `i32` on both platforms.

### 2. Case-insensitive file lookup on Windows

NTFS uses OrdinalIgnoreCase for name comparisons. When Windows sends a `find_child` request with user-typed casing (e.g., "README.md" vs "readme.md"), the lookup must match. The `names_match` helper uses `eq_ignore_ascii_case` on Windows (matching NTFS's ASCII-based comparison) and exact equality elsewhere. The memory cache HashMap uses String keys, so on Windows we iterate rather than using `.get()`.

### 3. CfApi state_changed cache invalidation

Previously a no-op (debug log only). Now resolves each changed path to an inode and invalidates the memory cache entry. This ensures that pin/unpin state changes are reflected on the next file access.

### 4. CfApi chunked read for large files

Below `SMALL_FILE_LIMIT` (4MB), keeps the existing `std::fs::read()`. Above, uses `BufReader::with_capacity(4MB)` for buffered I/O. Note: the writeback API takes `&[u8]`, so content still accumulates in memory — the improvement is buffered syscalls, not streaming.

## Decisions Made

| Decision | Choice | Rationale |
|----------|--------|-----------|
| `names_match` vs inline `#[cfg]` | Module-level helper | Single definition, used at 3 sites — avoids duplication |
| Memory cache lookup on Windows | Iterate HashMap | HashMap::get requires exact key; no clean way to override hasher for just this |
| Chunked read threshold | Reuse `SMALL_FILE_LIMIT` (4MB) | Same constant used for simple vs session upload — consistent semantics |
| libc cast | No `as i32` | Clippy correctly identified it as unnecessary on this target |
| libc workspace dep | Already done | Found in root Cargo.toml from prior work item |

## Deviations from Plan

- **libc workspace dependency** was already completed in a prior change (`fix-cross-platform-findings`). Skipped.
- **Chunked CfApi closed** still accumulates in memory due to writeback API contract (`&[u8]`). Noted as known limitation — streaming would require API changes.

## Dependencies Added

(none — `libc` was already in workspace dependencies)

## How to Verify

1. **Build and lint**
   ```bash
   toolbox run -c cloudmount-build cargo clippy -p cloudmount-vfs --lib -- -D warnings
   ```
   Expected: clean (0 warnings)

2. **Run tests**
   ```bash
   toolbox run -c cloudmount-build cargo test -p cloudmount-vfs
   ```
   Expected: 31 passed, 0 failed, 13 ignored

3. **Verify errno constants (code review)**
   Check `mount.rs:22` — should reference `libc::ENOTCONN` and `libc::EIO`, not numeric literals

4. **Verify names_match (code review)**
   Check `core_ops.rs` — `names_match` with `#[cfg(target_os = "windows")]` and `#[cfg(not(...))]` variants

## Test Coverage

- Tests passed: 31
- Tests ignored: 13 (FUSE integration, requires mount)
- Status: all passing

## Developer Notes

- The `#[cfg(target_os = "windows")]` branch in `find_child` iterates the HashMap on every memory cache hit. For directories with thousands of children this could be slower than a case-folding HashMap. If perf becomes an issue, consider a separate `CaseInsensitiveMap` wrapper.
- `libc` types on Linux are `i32` for errno values, but on some exotic platforms they might differ. The current code works for Linux + macOS (the only FUSE targets).
- The CfApi chunked read is an incremental improvement. True streaming would require `writeback.write_stream()` accepting `impl Read` — a bigger refactor.

---
*Generated by FIRE Builder Agent — Run run-cloud-mount-013*
