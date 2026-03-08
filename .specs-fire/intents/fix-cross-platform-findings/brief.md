---
id: fix-cross-platform-findings
title: Fix Cross-Platform Findings
status: completed
created: 2026-03-08T00:00:00Z
completed_at: 2026-03-08T15:05:38.745Z
---

# Intent: Fix Cross-Platform Findings

## Goal

Fix all 9 cross-platform portability issues identified by the cross-platform reviewer across `crates/cloudmount-core/src/config.rs` and `crates/cloudmount-app/src/main.rs`.

## Users

CloudMount users on Linux, macOS, and Windows — all affected by platform-specific bugs, silent failures, or degraded runtime behavior.

## Problem

The cross-platform reviewer found issues spanning three severity levels:

- **High**: macOS FUSE detection always returns `false` because it probes `fusermount` (a Linux binary) instead of the macFUSE install path.
- **Medium**: Mount point paths are assembled with hardcoded `/` separators, producing malformed mixed-separator paths on Windows. Windows headless mode never populates `mount_entries`, silently skipping crash recovery and delta sync.
- **Low / Info**: Forbidden-path validation mixes Unix and Windows paths without `#[cfg]` gates; redundant cfg branches; inaccurate comment; `cache_dir` stored as `String`; systemd `.service` file written before confirming systemd exists.

## Success Criteria

- macOS `fuse_available()` returns `true` when macFUSE is installed
- Mount point derivation and expansion use `Path::join()` with OS-native separators on all platforms
- Windows headless mode populates mount tracking or emits a clear diagnostic (no silent no-op)
- Forbidden-path list is split into platform-gated sets with `#[cfg]`
- `drive_id` redundant cfg branches collapsed to a single unconditional expression
- Inaccurate comment at `main.rs:326` corrected
- `cache_dir` string-vs-PathBuf inconsistency documented or resolved
- Systemd autostart probes for systemd availability before writing `.service` file
- Zero new Clippy warnings (`RUSTFLAGS=-Dwarnings`, `--all-targets --all-features`)

## Constraints

- Rust 2024 / MSRV 1.85
- No new dependencies unless strictly necessary
- All `#[cfg]` gates must be correct — no dead code on any supported target
- Tests must pass: `cargo test --all-targets`

## Notes

Issues reference the cross-platform review report (2026-03-08):
- #1 Medium: path separator in `derive_mount_point`/`expand_mount_point` (config.rs:323-346)
- #2 Low: mixed platform forbidden-path list (config.rs:267-285)
- #3 High: macOS `fusermount` probe (main.rs:148-154)
- #4 Info: inaccurate comment (main.rs:326)
- #5 Medium: `mount_entries` empty on Windows headless (main.rs:1153+)
- #6 Low: redundant identical cfg branches for `drive_id` (main.rs:822-830)
- #7 Low: mixed-separator path to `CfMountHandle::mount` (main.rs:784)
- #8 Info: `cache_dir` as String (config.rs:157,208)
- #9 Low: systemd write before availability check (config.rs:466-481)
