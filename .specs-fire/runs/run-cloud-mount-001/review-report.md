# Code Review Report: per-mount-cache-isolation

**Run**: run-cloud-mount-001
**Date**: 2026-03-08

---

## Summary

| Category | Auto-Fixed | Suggested | Skipped |
|----------|-----------|-----------|---------|
| Code Quality | 1 | 0 | 0 |
| Correctness (Bug) | 1 | 0 | 0 |
| Security | 0 | 0 | 0 |
| Architecture | 0 | 0 | 0 |

**Auto-fixed: 2 issues. No suggestions requiring approval.**

---

## Auto-Fixed Issues

### 1. Formatting: Stale rustfmt diffs in commands.rs
- **File**: `crates/cloudmount-app/src/commands.rs`
- **Action**: Ran `cargo fmt --all` — fixed pre-existing formatting divergence in `get_drive_info` and `get_followed_sites` functions.
- **Tests**: Still passing after fix.

### 2. Bug: clear_cache collected caches after stop_all_mounts removed them
- **File**: `crates/cloudmount-app/src/commands.rs:clear_cache`
- **Issue**: Cache references were collected from `mount_caches` AFTER `stop_all_mounts()`, which removes entries from `mount_caches` via `stop_mount`. Result: the clear loop iterated over an empty Vec and cleared nothing.
- **Fix**: Moved cache collection to BEFORE `stop_all_mounts()`. Arc references keep the CacheManager alive even after removal from `mount_caches`.
- **Tests**: Still passing after fix.

---

## No Suggestions

The implementation is clean and follows project conventions:
- All error types use `cloudmount_core::Error` / `Result<T>` via `?` or `.map_err`
- Logging uses `tracing::info!`/`warn!`/`error!` with structured fields
- No `.unwrap()` in library paths; only in infallible contexts (Mutex locks that can't be poisoned here)
- `Arc` cloning is appropriate — no unnecessary copies
- `mount_caches` HashMap key (drive_id string) is the correct isolation boundary
- Per-mount SQLite DB naming (`drive-{safe_id}.db`) is safe on all platforms
