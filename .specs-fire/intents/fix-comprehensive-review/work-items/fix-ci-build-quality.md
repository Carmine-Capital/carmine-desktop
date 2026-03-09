---
id: fix-ci-build-quality
title: CI clippy all platforms, workspace deps, test portability
intent: fix-comprehensive-review
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-015
completed_at: 2026-03-09T19:09:36.620Z
---

# Work Item: CI clippy all platforms, workspace deps, test portability

## Description

Fix CI and build quality issues:

1. **CI clippy Linux-only** (`ci.yml:52-54`): `Clippy (desktop)` step has `if: runner.os == 'Linux'`. Windows and macOS desktop code paths never linted. Fix: remove the `if` condition or add separate clippy-desktop steps for each platform.

2. **libc workspace dep** (`vfs/Cargo.toml:21`): `libc = "0.2"` declared inline instead of via workspace root. Fix: add `libc = "0.2"` to root `[workspace.dependencies]`, change crate to `libc = { workspace = true }`.

3. **parse_cache_size cfg gate** (`main.rs:129`): Gate `#[cfg(any(target_os = "linux", target_os = "macos", feature = "desktop"))]` mixes platform+feature. The function has no platform-specific code. Fix: remove the gate entirely — function always compiles.

4. **Test hardcodes Unix paths** (`config_tests.rs:74`): Test uses `/home/` paths and asserts Unix behavior. Fix: gate with `#[cfg(unix)]` or use platform-appropriate paths.

## Acceptance Criteria

- [ ] CI runs `cargo clippy --all-targets --features desktop` on Linux, macOS, and Windows
- [ ] `libc` dependency in workspace root `[workspace.dependencies]`
- [ ] `parse_cache_size` compiles unconditionally (no cfg gate)
- [ ] Config tests pass on all platforms (Unix-specific tests gated)

## Technical Notes

For CI, the simplest fix is removing the `if: runner.os == 'Linux'` from the Clippy (desktop) step. It already runs on all three matrix entries.

## Dependencies

(none)
