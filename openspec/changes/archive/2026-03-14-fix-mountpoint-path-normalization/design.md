## Context

`expand_mount_point()` in `carminedesktop-core/src/config.rs` expands `~/` and `{home}` templates into absolute paths using `Path::join`. On Windows, `Path::join("Cloud/OneDrive")` preserves the forward slash from the template, producing mixed-separator paths like `C:\Users\nyxa\Cloud/OneDrive`. While WinFsp tolerates mixed separators, a trailing `/` or `\` on the mountpoint causes `FspFileSystemSetMountPoint` (inside `host.mount()`) to crash with `STATUS_ACCESS_VIOLATION`.

The trailing separator originates from user config or from mount creation functions that don't strip it.

Affected code:
- `carminedesktop-core/src/config.rs` — `expand_mount_point()`, `add_onedrive_mount()`, `add_sharepoint_mount()`
- `carminedesktop-app/src/main.rs` — `start_mount_common()` (consumer of expanded paths)

## Goals / Non-Goals

**Goals:**
- Eliminate the WinFsp crash caused by trailing separators in mountpoint paths
- Produce OS-native separators on all platforms (backslash on Windows, forward slash elsewhere)
- Defend at multiple layers: config creation, path expansion, mount-time

**Non-Goals:**
- Migrating or rewriting existing user config files — the fix handles bad values at runtime
- Changing FUSE mount behavior — FUSE tolerates trailing slashes, but the normalization applies uniformly for correctness

## Decisions

### 1. Normalize in `expand_mount_point()` as the primary fix

All mountpoint paths flow through `expand_mount_point()`. Adding separator normalization and trailing-separator stripping here is the single most impactful change and catches all existing configs.

On Windows, after path expansion, replace all `/` with `\`. On all platforms, strip trailing separators. Use `std::path::MAIN_SEPARATOR` for platform-correct behavior.

**Alternative considered:** Normalize only at mount time in `start_mount_common()`. Rejected because it would leave `expand_mount_point()` returning incorrect paths for other callers (logging, config display, directory creation).

### 2. Defensive strip in `start_mount_common()` as a safety net

Even though `expand_mount_point()` handles normalization, `start_mount_common()` should strip trailing separators from the result before any filesystem operations. This defends against paths that bypass `expand_mount_point()` (e.g., hardcoded paths, future callers) and follows defense-in-depth.

A single `path.trim_end_matches(['/', '\\'])` after `expand_mount_point()` returns, before the path is used for directory creation or passed to `WinFspMountHandle::mount()`.

### 3. Strip trailing separators at config creation time

`add_onedrive_mount()` and `add_sharepoint_mount()` construct mount point templates. These should strip trailing separators from the site/library names and the final template string before persisting to config. This prevents bad data from being written in the first place.

## Risks / Trade-offs

- **[Risk] Over-stripping edge case: bare drive letter** — `C:\` stripped to `C:` changes meaning on Windows. Mitigation: only strip if the path is longer than 3 characters (e.g., `C:\`), or use `Path::components()` + rebuild which naturally handles this. In practice, mountpoints are always subdirectories (`~/Cloud/...`), never bare drive roots.
- **[Risk] Behavioral change for paths used elsewhere** — `expand_mount_point()` is also called in headless mode and for display purposes. Normalization changes the string representation. This is a correctness improvement, not a regression — displayed paths will now match the OS convention.
