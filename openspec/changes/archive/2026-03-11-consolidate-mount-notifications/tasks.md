## 1. Notification Infrastructure

- [x] 1.1 Add `mounts_summary(app, succeeded: usize, failed: usize)` function to `notify.rs` with format logic per design.md D2

## 2. Core Mount Logic

- [x] 2.1 Remove `notify::mount_success()` calls from both `start_mount` functions (FUSE and CfApi variants in main.rs)
- [x] 2.2 Modify `start_all_mounts` to collect `(mount_name, result)` for each mount attempt
- [x] 2.3 Add summary notification dispatch at end of `start_all_mounts` using counts

## 3. User-Initiated Mount Actions

- [x] 3.1 Add `notify::mount_success()` call in `add_mount` command after successful mount (commands.rs)
- [x] 3.2 Add `notify::mount_success()` call in `toggle_mount` command after successful enable (commands.rs)
