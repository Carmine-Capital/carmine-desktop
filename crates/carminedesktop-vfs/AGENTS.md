# carminedesktop-vfs

Virtual filesystem exposing OneDrive/SharePoint as local mount. FUSE on Linux/macOS, WinFsp on Windows. All platform-gated via `#[cfg]`.

## CONFLICT DETECTION

In `flush_inode`, before uploading existing files:
1. Compare cached eTag with server eTag (via `graph.get_item`)
2. On mismatch → upload local copy as `{name}.conflict.{timestamp}` to same parent
3. Proceed with normal upload regardless

## CONVENTIONS

- All `Filesystem`/`FileSystemContext` trait methods are sync. Bridge to async via `rt.block_on()`.
- Reply `Errno::ENOENT` for missing items, `Errno::EIO` for server/upload errors.
- After child mutations (create, delete, rename): update parent's children map via `add_child`/`remove_child`. Do NOT call `invalidate(parent_ino)` — that destroys the entire cache entry (item + children map) and forces a full re-fetch.

## ANTI-PATTERNS

- Do NOT make Filesystem trait methods async — `fuser` requires sync.
- Do NOT hold cache locks across `block_on` calls — deadlock risk.
- Do NOT skip conflict detection in flush — data loss risk.
- Do NOT call `invalidate(parent_ino)` after child mutations — use `add_child`/`remove_child` instead. `invalidate` destroys the children map and triggers unnecessary Graph API re-fetches.
- Do NOT remove writeback entry before successful upload confirmation.
