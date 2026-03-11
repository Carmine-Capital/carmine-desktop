## 1. WinFsp flush callback

- [x] 1.1 Add `flush` method to `FileSystemContext` impl in `winfsp_fs.rs`: extract `fh` from context, call `self.ops.flush_handle(fh)`, update `file_info` with fresh item metadata on success, map errors via `vfs_err_to_ntstatus`
- [x] 1.2 Add error logging in `flush` failure path: look up file name from `self.ops.lookup_item(context.ino)`, emit `VfsEvent::UploadFailed` (same pattern as `cleanup`)

## 2. Fix stale item in WinFsp open

- [x] 2.1 In `winfsp_fs::open`, after calling `self.ops.open_file(ino)`, re-fetch the item via `self.ops.lookup_item(ino)` and use the fresh item for `item_to_file_info` instead of the pre-refresh `item` variable

## 3. Transient file upload filter

- [x] 3.1 Add `fn is_transient_file(name: &str) -> bool` in `core_ops.rs`: match `~$` prefix, `~*.tmp` pattern, and case-insensitive exact matches for `Thumbs.db`, `desktop.ini`, `.DS_Store`
- [x] 3.2 Add early return in `flush_inode` after the writeback read: if `is_transient_file(&item.name)`, remove the writeback entry, log at debug level, and return `Ok(())`
- [x] 3.3 Add unit tests for `is_transient_file`: positive cases (`~$Book1.xlsx`, `~WRS0001.tmp`, `Thumbs.db`, `THUMBS.DB`, `desktop.ini`, `Desktop.ini`, `.DS_Store`), negative cases (`Budget Report.xlsx`, `~notes.txt`, `file.tmp`, `thumbs.db.bak`)

## 4. Verification

- [x] 4.1 Run `make check` (fmt, clippy, build, test) — all must pass with zero warnings
