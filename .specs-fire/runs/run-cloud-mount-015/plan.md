# Plan: fix-ci-build-quality

**Run**: run-cloud-mount-015
**Mode**: autopilot
**Work Item**: CI clippy all platforms, workspace deps, test portability

## Approach

Four targeted fixes to improve CI coverage and build quality:

1. **CI clippy all platforms**: Remove the `if: runner.os == 'Linux'` condition from the `Clippy (desktop)` step so desktop clippy runs on Linux, macOS, AND Windows.

2. **libc workspace dep**: Add `libc = "0.2"` to root `[workspace.dependencies]`, change `crates/cloudmount-vfs/Cargo.toml` to use `libc = { workspace = true }`.

3. **parse_cache_size cfg gate**: Remove the mixed platform+feature `#[cfg]` gate. The function is pure utility with no platform-specific code. Since callers are themselves gated, the function is dead_code on some platform×feature combos — suppress with `#[allow(dead_code)]`.

4. **Unix-specific test gating**: Add `#[cfg(unix)]` to `test_expand_mount_point_home` which hardcodes `/home/` paths and asserts Unix behavior.

## Files to Modify

| File | Change |
|------|--------|
| `.github/workflows/ci.yml` | Remove `if: runner.os == 'Linux'` from Clippy (desktop) step |
| `Cargo.toml` (workspace root) | Add `libc = "0.2"` to `[workspace.dependencies]` |
| `crates/cloudmount-vfs/Cargo.toml` | Change `libc = "0.2"` → `libc = { workspace = true }` |
| `crates/cloudmount-app/src/main.rs` | Remove cfg gate from `parse_cache_size`, add `#[allow(dead_code)]` |
| `crates/cloudmount-core/tests/config_tests.rs` | Add `#[cfg(unix)]` to `test_expand_mount_point_home` |

## Tests

No new tests. Existing tests validated via `cargo test --all-targets`.
