## 1. Workspace Dependencies & Module Structure

- [x] 1.1 Add `winfsp` and `winfsp-sys` as Windows-only workspace dependencies in root `Cargo.toml`
- [x] 1.2 Add `winfsp` dependency to `carminedesktop-vfs/Cargo.toml` (`workspace = true`, `cfg(target_os = "windows")`)
- [x] 1.3 Create `crates/carminedesktop-vfs/src/winfsp_fs.rs` with `carminedesktopWinFsp`, `WinFspFileContext`, `WinFspMountHandle`, and `WinFspDeltaObserver` struct skeletons
- [x] 1.4 Add `#[cfg(target_os = "windows")] mod winfsp_fs;` declaration in `carminedesktop-vfs/src/lib.rs` and update public exports

## 2. Remove CfApi Backend

- [x] 2.1 Remove `cloud-filter` from workspace dependencies in root `Cargo.toml` and from `carminedesktop-vfs/Cargo.toml`
- [x] 2.2 Delete `crates/carminedesktop-vfs/src/cfapi.rs`
- [x] 2.3 Remove `cfapi` module declaration and CfApi-specific public exports (`apply_delta_placeholder_updates`, `CfMountHandle`) from `carminedesktop-vfs/src/lib.rs`
- [x] 2.4 Remove CfApi-specific integration tests from `crates/carminedesktop-vfs/tests/`

## 3. WinFsp Core Types & Helpers

- [x] 3.1 Define `WinFspFileContext` struct with `ino: u64`, `fh: Option<u64>`, `is_dir: bool`
- [x] 3.2 Define `carminedesktopWinFsp` struct holding `Arc<CoreOps>`, `tokio::runtime::Handle`, `Arc<OpenFileTable>`, and `VfsEventSender`
- [x] 3.3 Implement `VfsError` to `NTSTATUS` error mapping helper (`NotFound` -> `STATUS_OBJECT_NAME_NOT_FOUND`, `NotADirectory` -> `STATUS_NOT_A_DIRECTORY`, `DirectoryNotEmpty` -> `STATUS_DIRECTORY_NOT_EMPTY`, `PermissionDenied` -> `STATUS_ACCESS_DENIED`, `TimedOut` -> `STATUS_IO_TIMEOUT`, `QuotaExceeded` -> `STATUS_DISK_FULL`, `IoError` -> `STATUS_IO_DEVICE_ERROR`)
- [x] 3.4 Implement `DriveItem` to WinFsp `FileInfo` mapping helper: file_size, allocation_size (4KB-aligned), file_attributes (`FILE_ATTRIBUTE_DIRECTORY`/`FILE_ATTRIBUTE_NORMAL`), timestamps as Windows FILETIME (100ns since 1601-01-01), default to Windows epoch for `None` timestamps, open handle size override
- [x] 3.5 Implement `U16CStr` path to `OsString` component splitting helper (backslash separator, root path `\` special case, lossless Unicode)

## 4. WinFsp FileSystemContext — Read Path

- [x] 4.1 Implement `get_security_by_name`: split path, resolve via `CoreOps::resolve_path()`, return `FileSecurity` with file attributes; root `\` resolves to `ROOT_INODE` without calling `resolve_path`; return `STATUS_OBJECT_NAME_NOT_FOUND` on miss
- [x] 4.2 Implement `open`: resolve path to inode, call `CoreOps::open_file(ino)` for files (store fh), set `fh: None` for directories, return `WinFspFileContext`
- [x] 4.3 Implement `get_file_info`: map inode metadata to `FileInfo` via the attribute mapping helper; prefer open handle content size over cached DriveItem size
- [x] 4.4 Implement `read`: delegate to `CoreOps::read_handle(fh, offset, size)`, copy bytes into WinFsp buffer, return byte count; 0 bytes for read-beyond-EOF
- [x] 4.5 Implement `read_directory`: call `CoreOps::list_children(ino)`, emit `.` and `..` entries first, encode each child as `DirInfo` with full attributes; stop and return when buffer is full (WinFsp re-requests with marker)
- [x] 4.6 Implement `get_volume_info`: call `CoreOps::get_quota()`, return total/free sizes with volume label "carminedesktop"; fall back to 1 TB total / 1 TB free when quota is unavailable

## 5. WinFsp FileSystemContext — Write Path

- [x] 5.1 Implement `create`: for files (`FILE_DIRECTORY_FILE` not set) call `CoreOps::create_file(parent_ino, name)` and return `WinFspFileContext` with fh; for directories call `CoreOps::mkdir(parent_ino, name)` and return context with `fh: None`
- [x] 5.2 Implement `write`: delegate to `CoreOps::write_handle(fh, offset, data)`; when `write_to_eof` is true, determine current EOF from handle content size and set offset accordingly; update `FileInfo` output with new size
- [x] 5.3 Implement `overwrite`: truncate existing file to zero via `CoreOps::truncate(ino, 0)`, return updated `FileInfo` with size 0
- [x] 5.4 Implement `cleanup`: flush dirty file handles via `CoreOps::flush_handle(fh)`; emit `VfsEvent::UploadFailed` on flush error; execute delete-on-close via `CoreOps::unlink` or `CoreOps::rmdir`; no-op for directories without delete flag
- [x] 5.5 Implement `close`: release CoreOps file handle via `CoreOps::release_file(fh)`; no-op for directories (`fh: None`)
- [x] 5.6 Implement `set_file_size`: delegate to `CoreOps::truncate(ino, new_size)`, return updated `FileInfo`
- [x] 5.7 Implement `set_basic_info`: no-op, return current `FileInfo` from cache (server timestamps authoritative)
- [x] 5.8 Implement `set_delete`: for files record deletion intent (deferred to `cleanup`); for non-empty directories return `STATUS_DIRECTORY_NOT_EMPTY`
- [x] 5.9 Implement `rename`: resolve source and destination parent inodes from paths, delegate to `CoreOps::rename(src_parent, src_name, dst_parent, dst_name)`; return `STATUS_OBJECT_NAME_COLLISION` when `replace_if_exists` is false and destination exists

## 6. WinFsp Mount Lifecycle

- [x] 6.1 Define `WinFspMountHandle` struct with `FileSystemHost<carminedesktopWinFsp>`, `Arc<CacheManager>`, `Arc<GraphClient>`, `drive_id: String`, `rt: Handle`, `mountpoint: String`, `delta_observer: Arc<WinFspDeltaObserver>`
- [x] 6.2 Implement `WinFspMountHandle::mount()`: fetch drive root from Graph API, seed into caches as ROOT_INODE, create `carminedesktopWinFsp`, create `WinFspDeltaObserver` sharing `OpenFileTable`, configure `VolumeParams` (filesystem name "carminedesktop", `FileInfoTimeout` 5000ms), create and start `FileSystemHost`
- [x] 6.3 Implement `WinFspMountHandle::unmount()`: call shared `flush_pending()`, stop `FileSystemHost`, unmount
- [x] 6.4 Implement accessor methods: `drive_id() -> &str`, `mountpoint() -> &str`, `delta_observer() -> Arc<dyn DeltaSyncObserver>`

## 7. WinFsp Delta Sync Observer

- [x] 7.1 Define `WinFspDeltaObserver` struct holding `Arc<OpenFileTable>`
- [x] 7.2 Implement `DeltaSyncObserver` trait: `on_inode_content_changed(ino)` calls `OpenFileTable::mark_stale_by_ino(ino)`

## 8. Application Integration (main.rs)

- [x] 8.1 Update `start_mount()` Windows path to create `WinFspMountHandle::mount()` instead of `CfMountHandle`, remove CfApi-specific params (account_name, display_name, icon), store `Some(observer)` in `mount_caches`
- [x] 8.2 Update `stop_mount()` Windows path to call `WinFspMountHandle::unmount()`
- [x] 8.3 Remove the `#[cfg(target_os = "windows")]` block in `start_delta_sync()` that calls `apply_delta_placeholder_updates`; delta sync loop becomes platform-uniform
- [x] 8.4 Replace CfApi version check in `preflight_checks()` with WinFsp driver detection: query registry `HKLM\SOFTWARE\WinFsp\InstallDir`, verify `winfsp-x64.dll` exists in `bin/`; error message directs user to https://winfsp.dev/
- [x] 8.5 Remove the early-exit block in `run_headless()` that rejects Windows; WinFsp mounts work without a desktop session

## 9. CfApi Migration & Packaging

- [x] 9.1 Add `cfapi_migrated: bool` field to user config schema in `carminedesktop-core` (default `false`)
- [x] 9.2 Implement CfApi sync root cleanup in `setup_after_launch()`: if `cfapi_migrated` is false, enumerate carminedesktop sync roots, unregister each, set flag to true; log warning and continue on failure
- [x] 9.3 Add WinFsp FLOSS attribution text to UI About dialog: "WinFsp - Windows File System Proxy, Copyright (C) Bill Zissimopoulos"
- [x] 9.4 Update Windows installer/packaging notes to bundle or require WinFsp driver installation
