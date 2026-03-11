## REMOVED Requirements

### Requirement: Convert uploaded local files to CfApi placeholders
**Reason**: WinFsp has no placeholder concept. Files exist only as cached metadata and on-demand content served through callbacks. After a `local:*` file is uploaded via `flush_inode()`, the inode is reassigned from the temporary `local:` ID to the server-assigned ID (via `InodeTable::reassign()`), and the cache is updated. No NTFS reparse point conversion is needed because WinFsp files don't have placeholder state.
**Migration**: The `flush_inode()` path already handles inode reassignment and cache updates. Remove the `#[cfg(target_os = "windows")]` post-upload `convert_to_placeholder` call from `flush_inode()` (if present) or from the CfApi `closed()` callback. The inode reassignment logic in CoreOps is platform-agnostic and remains unchanged.

### Requirement: Placeholder conversion is Windows-only
**Reason**: The `#[cfg(target_os = "windows")]` gating for placeholder conversion is removed along with the conversion logic itself. There is no WinFsp equivalent because WinFsp does not use NTFS reparse points for file tracking.
**Migration**: No migration needed. Remove the cfg-gated conversion code.
