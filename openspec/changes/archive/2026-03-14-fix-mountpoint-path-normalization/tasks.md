## 1. Path normalization in `expand_mount_point`

- [x] 1.1 Add trailing separator stripping and OS-native separator normalization to `expand_mount_point()` in `crates/carminedesktop-core/src/config.rs`. On Windows, replace all `/` with `\`. On all platforms, strip trailing `/` or `\` (preserving bare drive roots like `C:\`). Apply normalization to all return paths in the function.
- [x] 1.2 Add or update tests in `crates/carminedesktop-core/tests/config_tests.rs` for `expand_mount_point`: trailing slash stripped, forward slashes normalized on Windows (cfg-gated test), bare drive root preserved.

## 2. Defensive strip in `start_mount_common`

- [x] 2.1 In `start_mount_common()` in `crates/carminedesktop-app/src/main.rs`, strip trailing separators from the expanded mountpoint immediately after calling `expand_mount_point()`, before the path is used for directory creation or passed to the VFS backend.

## 3. Config creation cleanup

- [x] 3.1 In `add_onedrive_mount()` and `add_sharepoint_mount()` in `crates/carminedesktop-core/src/config.rs`, strip trailing separators from the constructed mount point template before inserting into the config.

## 4. Verification

- [x] 4.1 Run `make check` (fmt, clippy, build, test) to verify no regressions.
