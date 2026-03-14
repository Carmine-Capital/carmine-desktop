## Why

Two bugs prevent normal app usage after sign-out or unclean shutdown:

1. **Desktop mode dead end after sign-out**: When a user signs out and restarts the desktop app, the setup wizard never opens. The config file exists (so `first_run=false`) but accounts are empty (so `restored=false`). Neither branch in `setup_after_launch` fires — the app sits silently in the tray with no way to re-authenticate.

2. **"File exists" error on mount**: If a previous run exits without properly unmounting FUSE (crash, kill, or `auto_unmount` unsupported), the mountpoint becomes a stale FUSE entry. `create_dir_all` fails with EEXIST because `mkdir` succeeds but `stat` (for `is_dir()` check) returns ENOTCONN on the stale mount. The app logs the error and skips the mount entirely, with no recovery.

Both bugs were discovered after using sign-out, making the app effectively unusable without manual intervention (`fusermount -u` and config file deletion).

## What Changes

- **Desktop re-auth flow**: When `setup_after_launch` detects `!restored` and the app is not a first run, check whether accounts are empty. If so, open the setup wizard to prompt re-authentication. This covers the sign-out → restart path.
- **Stale FUSE mount recovery**: Before calling `create_dir_all` for a mountpoint, detect stale FUSE mounts (stat returns ENOTCONN or path appears in `/proc/mounts` as a FUSE entry). Attempt `fusermount -u` (or `fusermount3 -u`) to clean up, then retry directory creation.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `app-lifecycle`: Add re-auth recovery when desktop app starts with no authenticated accounts (post-sign-out restart). Add stale FUSE mount detection and cleanup before mount creation.
- `virtual-filesystem`: Add stale mount detection and `fusermount` cleanup as a pre-mount step.

## Impact

- `crates/carminedesktop-app/src/main.rs` — `setup_after_launch` logic, `start_mount` (desktop), and headless mount loop.
- `crates/carminedesktop-vfs/src/mount.rs` — Pre-mount stale detection utility (shared by both desktop and headless paths).
- No new dependencies. `fusermount`/`fusermount3` is already required for FUSE operation.
- No API or config changes.
