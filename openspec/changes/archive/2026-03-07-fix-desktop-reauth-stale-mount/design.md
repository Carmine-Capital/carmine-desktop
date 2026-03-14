## Context

`setup_after_launch` in `main.rs` has two branches after token restore:
1. `restored == true` → start mounts, sync, etc.
2. `first_run == true` → open wizard

After sign-out, `first_run` is false (config file persists) and `restored` is false (accounts cleared). No branch fires. The app is stuck.

Separately, FUSE mounts can become stale when a process exits without proper unmount (killed, crashed, or `auto_unmount` unsupported). The mountpoint directory becomes inaccessible — `stat` returns ENOTCONN — causing `create_dir_all` to fail with EEXIST. There is currently no stale mount detection anywhere in the codebase.

## Goals / Non-Goals

**Goals:**
- Desktop app always provides a path to authentication after restart (including post-sign-out)
- Stale FUSE mounts are detected and cleaned up automatically before mounting
- Both headless and desktop paths benefit from stale mount recovery

**Non-Goals:**
- Changing sign-out behavior (clearing mounts, deleting config file)
- Supporting non-FUSE stale mounts (CfApi on Windows — different mechanism)
- Adding a retry loop — one cleanup attempt is sufficient

## Decisions

### D1: Fix the desktop re-auth gap by extending `setup_after_launch`

**Choice:** Add an `else` branch: when `!restored && !first_run`, check if accounts are empty and open the wizard.

**Why not always call `sign_in` like headless does?** The desktop app uses the wizard webview for onboarding, not a raw browser redirect. Reopening the wizard is consistent with the existing UX (sign-out already opens the wizard at line 131 in `commands.rs`). The wizard triggers `commands::sign_in` when the user proceeds.

**Why not just check `!restored`?** If `!restored && !first_run` but accounts exist, it means token restore failed for an existing account (expired, keyring cleared, etc.). Opening the wizard is still appropriate — it lets the user re-authenticate. So the condition simplifies to: `!restored && !first_run` → open wizard.

### D2: Stale mount detection via `stat` + `/proc/mounts`

**Choice:** Before `create_dir_all`, attempt to `stat` the mountpoint. If stat returns `ENOTCONN` (errno 107, "Transport endpoint is not connected"), the mount is stale. As a secondary check (for edge cases where stat returns other errors), parse `/proc/mounts` for the path.

**Why not just check `/proc/mounts`?** Stat is faster and handles the common case. `/proc/mounts` is Linux-specific and requires path normalization (symlinks like `/home` → `/var/home`). Using stat as primary and `/proc/mounts` as fallback covers both.

**macOS:** Stale mounts on macOS present differently (stat may return EIO). The check will handle both ENOTCONN and EIO. macOS uses `umount` instead of `fusermount`.

### D3: Cleanup via `fusermount -u` with `fusermount3` fallback

**Choice:** Run `fusermount3 -u <path>` first (Fedora 43+ ships fusermount3 by default). If that fails, try `fusermount -u <path>`. On macOS, use `umount <path>`.

**Why `fusermount` over `umount`?** Regular `umount` requires root on most Linux setups. `fusermount -u` works for user-owned FUSE mounts without privilege escalation.

**Failure mode:** If cleanup fails, log a warning with actionable guidance ("run `fusermount -u <path>` manually") and skip the mount — same as current behavior, but with a better error message.

### D4: Place stale mount logic in `carminedesktop-vfs::mount`

**Choice:** Add a `pub fn cleanup_stale_mount(path: &str) -> bool` function in `mount.rs`. Both desktop `start_mount` and headless mount loop call it before `create_dir_all`.

**Why in VFS, not in the app crate?** Stale mount handling is a FUSE concern. Keeping it in the VFS crate means any future consumer of the mount API gets cleanup for free. The function is platform-gated (`#[cfg(any(target_os = "linux", target_os = "macos"))]`).

## Risks / Trade-offs

- **`fusermount` not in PATH** → Unlikely (required for FUSE to work at all), but the function handles this gracefully by logging and returning false.
- **Race condition: mount cleaned up between check and re-mount** → Extremely unlikely in practice (user's own filesystem). Not worth adding locking for.
- **Symlink resolution for `/proc/mounts` check** → `/home` → `/var/home` on Fedora Silverblue. Use `std::fs::canonicalize` on both the mountpoint and `/proc/mounts` entries when comparing. If canonicalize fails (stale mount), fall back to string comparison.
