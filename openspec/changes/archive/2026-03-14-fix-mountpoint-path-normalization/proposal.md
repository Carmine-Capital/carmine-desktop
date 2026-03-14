## Why

Mountpoint paths with a trailing separator (`/` or `\`) cause WinFsp to crash with `STATUS_ACCESS_VIOLATION` (0xc0000005) when mounting the drive. Additionally, `expand_mount_point` produces paths with forward slashes on Windows (e.g. `C:\Users\nyxa\Cloud/OneDrive`) because it preserves the `/` from `~/` templates rather than normalizing to OS-native separators. While WinFsp tolerates mixed separators in most cases, a trailing `/` is fatal, and mixed separators are incorrect on Windows regardless.

## What Changes

- `expand_mount_point()` will normalize all path separators to the OS-native separator and strip any trailing separator before returning.
- `start_mount_common()` will defensively strip trailing separators from the expanded mountpoint as a safety net, ensuring WinFsp never receives a path that could crash.
- Mount config creation functions (`add_onedrive_mount`, `add_sharepoint_mount`) will strip trailing separators from user-provided input at write time to prevent bad values from being persisted.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `config-persistence`: Mount point expansion must normalize path separators to OS-native format and strip trailing separators. Mount point creation must strip trailing separators from input.
