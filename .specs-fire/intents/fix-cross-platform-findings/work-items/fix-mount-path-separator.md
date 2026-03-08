---
id: fix-mount-path-separator
title: Fix mount point path construction to use OS-native separators
intent: fix-cross-platform-findings
complexity: medium
mode: confirm
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-008
completed_at: 2026-03-08T15:05:18.677Z
---

# Work Item: Fix mount point path construction to use OS-native separators

## Description

Addresses Issues #1 and #7 from the cross-platform review.

`derive_mount_point` and `expand_mount_point` in `config.rs` build paths using
`format!("{home}/{root_dir}/...")` with a hardcoded `/` separator. On Windows,
`dirs::home_dir()` returns `C:\Users\Alice`, producing a mixed-separator path
(`C:\Users\Alice/Cloud/OneDrive`). CfApi sync-root registration and path
comparisons in `validate_mount_point` expect consistent separators.

Additionally, `main.rs:784` passes the resulting `String` to
`std::path::Path::new(&mountpoint)` without normalising, compounding the issue.

Fix: use `std::path::Path::new(&home).join(root_dir).join(...)` and call
`.to_string_lossy().into_owned()` at the boundary. Normalise the path through
`PathBuf` before passing to `CfMountHandle::mount`.

## Acceptance Criteria

- [ ] `derive_mount_point` uses `Path::join()` for all path assembly — no literal `/` in format strings
- [ ] `expand_mount_point` uses `Path::join()` for the `~/...` expansion
- [ ] `main.rs`: mountpoint string passed to `CfMountHandle::mount` is normalised via `PathBuf::from(&mountpoint).to_string_lossy()`
- [ ] `validate_mount_point` still works correctly (compares normalised paths)
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings
- [ ] `cargo test -p cloudmount-core` passes

## Technical Notes

Key locations:
- `crates/cloudmount-core/src/config.rs` — `derive_mount_point()` (line ~323), `expand_mount_point()` (line ~345)
- `crates/cloudmount-app/src/main.rs` — `start_mount()` Windows branch (line ~784)

Pattern to use:
```rust
let path = std::path::Path::new(&home)
    .join(&root_dir)
    .join("OneDrive")
    .to_string_lossy()
    .into_owned();
```

The `to_string_lossy().into_owned()` boundary converts `PathBuf` back to `String`
for storage in config (which uses `String` fields). This is the minimal-change
approach: fix the separator without changing field types throughout.

## Dependencies

(none)
