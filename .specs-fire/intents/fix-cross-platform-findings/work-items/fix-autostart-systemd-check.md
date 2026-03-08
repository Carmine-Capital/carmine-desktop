---
id: fix-autostart-systemd-check
title: Check systemd availability before writing .service file
intent: fix-cross-platform-findings
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-08T00:00:00Z
run_id: run-cloud-mount-007
completed_at: 2026-03-08T14:51:46.504Z
---

# Work Item: Check systemd availability before writing .service file

## Description

Addresses Issue #9 (Low severity) from the cross-platform review.

`autostart::enable()` in `config.rs` (line ~466) writes
`~/.config/systemd/user/cloudmount.service` and then calls
`systemctl --user enable cloudmount`. On non-systemd Linux distributions
(Alpine/OpenRC, Void/runit, Artix, etc.), `systemctl` is absent or returns an
error. The current code propagates the `systemctl` error as `Err`, which is
correct, but the `.service` file has already been written. This leaves a stale
artifact in the user's config directory.

Fix: probe for systemd availability (e.g., check `systemctl --version` or
verify `$XDG_RUNTIME_DIR/systemd/` exists) before writing the file. If systemd
is unavailable, return a clear error without touching the filesystem.

## Acceptance Criteria

- [ ] `autostart::enable()` probes systemd availability before writing the `.service` file
- [ ] If systemd is not available, returns `Err(...)` without writing any file
- [ ] If systemd is available, existing write-then-enable behavior is preserved
- [ ] `autostart::disable()` is reviewed for the same ordering issue (remove file after `systemctl disable`, not before)
- [ ] `cargo clippy --all-targets --all-features` passes with zero warnings
- [ ] `cargo test -p cloudmount-core` passes

## Technical Notes

Key location:
- `crates/cloudmount-core/src/config.rs` — `autostart::enable()` at line ~466

Lightweight systemd probe (no extra dep):
```rust
let systemd_available = std::process::Command::new("systemctl")
    .arg("--version")
    .output()
    .map(|o| o.status.success())
    .unwrap_or(false);

if !systemd_available {
    return Err(Error::Other(anyhow::anyhow!(
        "systemd is not available on this system"
    )));
}
// ... then write file and enable
```

## Dependencies

(none)
