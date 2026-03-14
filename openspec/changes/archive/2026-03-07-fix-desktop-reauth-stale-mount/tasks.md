## 1. Stale FUSE mount detection and cleanup

- [x] 1.1 Add `cleanup_stale_mount(path: &str) -> bool` function in `crates/carminedesktop-vfs/src/mount.rs`, platform-gated for Linux/macOS. Stat the path; if ENOTCONN (107) or EIO (5), attempt `fusermount3 -u`, then `fusermount -u` (Linux), or `umount` (macOS). Return true if cleanup succeeded or path was not stale.
- [x] 1.2 Call `cleanup_stale_mount` before `create_dir_all` in the headless mount loop (`main.rs` ~line 930). If it returns false, log an actionable error and skip the mount.
- [x] 1.3 Call `cleanup_stale_mount` before `create_dir_all` in the desktop `start_mount` function (`main.rs` ~line 491). Same error handling.

## 2. Desktop re-auth after sign-out

- [x] 2.1 In `setup_after_launch` (`main.rs` ~line 420-432), add an `else` branch: when `!restored && !first_run`, open the wizard window via `tray::open_or_focus_window(app, "wizard", "Setup", "wizard.html")`.

## 3. Tests

- [x] 3.1 Add a unit test in `crates/carminedesktop-vfs/tests/` for `cleanup_stale_mount` with a non-existent path (should return true — no stale mount).
- [x] 3.2 Add a unit test for `cleanup_stale_mount` with a normal existing directory (should return true — not stale).
