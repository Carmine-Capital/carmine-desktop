//! WinFsp filesystem backend for Windows.
//!
//! Implements the WinFsp `FileSystemContext` trait by delegating all filesystem
//! operations to [`CoreOps`], mirroring the FUSE backend pattern in `fuse_fs.rs`.

use std::ffi::{OsString, c_void};
use std::os::windows::ffi::OsStringExt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::runtime::Handle;
use windows_sys::Win32::Foundation::{
    STATUS_ACCESS_DENIED, STATUS_DIRECTORY_NOT_EMPTY, STATUS_DISK_FULL, STATUS_IO_DEVICE_ERROR,
    STATUS_IO_TIMEOUT, STATUS_NOT_A_DIRECTORY, STATUS_OBJECT_NAME_COLLISION,
    STATUS_OBJECT_NAME_NOT_FOUND,
};
use windows_sys::Win32::Storage::FileSystem::FILE_ACCESS_RIGHTS;
use winfsp::U16CStr;
use winfsp::filesystem::{
    DirInfo, DirMarker, FileInfo, FileSecurity, FileSystemContext, OpenFileInfo, VolumeInfo,
    WideNameInfo,
};
use winfsp::host::{FileSystemHost, VolumeParams};

use crate::core_ops::{CoreOps, OpenFileTable, VfsError, VfsEvent};
use crate::inode::{InodeTable, ROOT_INODE};
use carminedesktop_cache::CacheManager;
use carminedesktop_core::DeltaSyncObserver;
use carminedesktop_core::types::DriveItem;
use carminedesktop_graph::GraphClient;

// ──────────────────────────────────────────────────────────────────────────────
// Group 3: WinFsp Core Types & Helpers
// ──────────────────────────────────────────────────────────────────────────────

/// File attributes for directories and normal files.
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;

/// CreateOptions flag indicating a directory is being created/opened.
const FILE_DIRECTORY_FILE: u32 = 0x00000001;

/// Allocation granularity (4 KiB).
const ALLOC_GRANULARITY: u64 = 4096;

/// Windows FILETIME epoch offset from Unix epoch (in 100-nanosecond intervals).
/// 1601-01-01 to 1970-01-01 = 11644473600 seconds × 10,000,000.
const FILETIME_UNIX_EPOCH_OFFSET: u64 = 116_444_736_000_000_000;

/// File context returned by WinFsp `open`/`create` callbacks.
///
/// Bridges WinFsp's path-based API to CoreOps' inode-based operations.
/// Created during `get_security_by_name`/`open` and used by all subsequent
/// callbacks (read, write, get_file_info, cleanup, close).
pub struct WinFspFileContext {
    /// Resolved inode number from the inode table.
    pub ino: u64,
    /// CoreOps file handle from `open_file()`. `None` for directories.
    pub fh: Option<u64>,
    /// Whether this context represents a directory.
    pub is_dir: bool,
}

/// WinFsp filesystem context implementing `FileSystemContext`.
///
/// Holds shared references to CoreOps (business logic), the Tokio runtime
/// handle (for async bridging via `block_on`), the open file table, and an
/// event sender for upload failure notifications.
pub struct CarmineDesktopWinFsp {
    ops: CoreOps,
    #[allow(dead_code)]
    rt: Handle,
    open_files: Arc<OpenFileTable>,
    #[allow(dead_code)]
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
}

/// Mount handle for a WinFsp filesystem instance.
///
/// Mirrors the FUSE `MountHandle` API: `mount()`, `unmount()`, `drive_id()`,
/// `mountpoint()`, `delta_observer()`.
pub struct WinFspMountHandle {
    host: FileSystemHost<CarmineDesktopWinFsp>,
    cache: Arc<CacheManager>,
    graph: Arc<GraphClient>,
    drive_id: String,
    rt: Handle,
    mountpoint: String,
    delta_observer: Arc<WinFspDeltaObserver>,
    sync_handle: Option<crate::sync_processor::SyncHandle>,
    sync_join: Option<tokio::task::JoinHandle<()>>,
}

/// Delta sync observer for the WinFsp backend.
///
/// Implements [`DeltaSyncObserver`] to mark open file handles as stale when
/// delta sync detects remote content changes. Unlike the FUSE observer, no
/// kernel cache invalidation is needed because WinFsp serves every read
/// through our callbacks.
pub struct WinFspDeltaObserver {
    open_files: Arc<OpenFileTable>,
}

impl DeltaSyncObserver for WinFspDeltaObserver {
    fn on_inode_content_changed(&self, ino: u64) {
        self.open_files.mark_stale_by_ino(ino);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 3.3  VfsError → NTSTATUS mapping
// ──────────────────────────────────────────────────────────────────────────────

/// Map a [`VfsError`] to a WinFsp `FspError::NTSTATUS` error.
fn vfs_err_to_ntstatus(e: VfsError) -> winfsp::FspError {
    let code = match e {
        VfsError::NotFound => STATUS_OBJECT_NAME_NOT_FOUND,
        VfsError::NotADirectory => STATUS_NOT_A_DIRECTORY,
        VfsError::DirectoryNotEmpty => STATUS_DIRECTORY_NOT_EMPTY,
        VfsError::PermissionDenied => STATUS_ACCESS_DENIED,
        VfsError::TimedOut => STATUS_IO_TIMEOUT,
        VfsError::QuotaExceeded => STATUS_DISK_FULL,
        VfsError::IoError(_) => STATUS_IO_DEVICE_ERROR,
    };
    winfsp::FspError::NTSTATUS(code)
}

// ──────────────────────────────────────────────────────────────────────────────
// 3.4  DriveItem → FileInfo mapping
// ──────────────────────────────────────────────────────────────────────────────

/// Convert a `chrono::DateTime<Utc>` to a Windows FILETIME (u64, 100-ns since 1601-01-01).
/// Returns 0 (the Windows epoch) for `None`.
fn datetime_to_filetime(dt: Option<DateTime<Utc>>) -> u64 {
    match dt {
        Some(dt) => {
            let unix_secs = dt.timestamp().max(0) as u64;
            let nanos_100 = dt.timestamp_subsec_nanos() as u64 / 100;
            unix_secs * 10_000_000 + nanos_100 + FILETIME_UNIX_EPOCH_OFFSET
        }
        None => 0,
    }
}

/// Populate a `FileInfo` from a `DriveItem`.
///
/// If `handle_size` is `Some`, overrides `file_size` with the open handle's
/// content size (for consistency while a file is being written).
fn item_to_file_info(item: &DriveItem, handle_size: Option<u64>) -> winfsp::filesystem::FileInfo {
    let is_dir = item.is_folder();

    let file_size = if is_dir {
        0
    } else {
        handle_size.unwrap_or(item.size.max(0) as u64)
    };

    // Round up to nearest ALLOC_GRANULARITY (4096) bytes.
    let allocation_size = file_size.div_ceil(ALLOC_GRANULARITY) * ALLOC_GRANULARITY;

    let file_attributes = if is_dir {
        FILE_ATTRIBUTE_DIRECTORY
    } else {
        FILE_ATTRIBUTE_NORMAL
    };

    let creation_time = datetime_to_filetime(item.created);
    let last_write_time = datetime_to_filetime(item.last_modified);

    winfsp::filesystem::FileInfo {
        file_attributes,
        reparse_tag: 0,
        allocation_size,
        file_size,
        creation_time,
        last_access_time: last_write_time,
        last_write_time,
        change_time: last_write_time,
        index_number: 0,
        hard_links: 0,
        ea_size: 0,
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 3.5  U16CStr path → component splitting
// ──────────────────────────────────────────────────────────────────────────────

/// Split a WinFsp `U16CStr` path (e.g. `\Documents\Reports\file.txt`) into
/// a `Vec<String>` of path components.
///
/// - Root path `\` returns an empty vec (caller should resolve to `ROOT_INODE`).
/// - Splits on `\` (backslash, U+005C), skips empty segments.
fn split_path(path: &U16CStr) -> Vec<String> {
    let wide: &[u16] = path.as_slice();
    // Convert UTF-16 to OsString, then to a Rust String for splitting.
    let os_str = OsString::from_wide(wide);
    let s = os_str.to_string_lossy();
    s.split('\\')
        .filter(|c| !c.is_empty())
        .map(|c| c.to_string())
        .collect()
}

/// Build the correct-case normalized path (UTF-16) for WinFsp.
///
/// Case-insensitive filesystems must report the canonical name via
/// `OpenFileInfo::set_normalized_name` so that Explorer shows the
/// server-side casing rather than whatever the caller typed.
fn build_normalized_path(ops: &CoreOps, components: &[String]) -> Option<Vec<u16>> {
    use crate::inode::ROOT_INODE;

    if components.is_empty() {
        return Some(vec!['\\' as u16]);
    }

    let mut path: Vec<u16> = Vec::new();
    let mut current_ino = ROOT_INODE;

    for comp in components {
        let (child_ino, item) = ops.find_child(current_ino, std::ffi::OsStr::new(comp))?;
        path.push('\\' as u16);
        path.extend(item.name.encode_utf16());
        current_ino = child_ino;
    }

    Some(path)
}

// ──────────────────────────────────────────────────────────────────────────────
// Group 4: FileSystemContext implementation (read path)
// ──────────────────────────────────────────────────────────────────────────────

impl CarmineDesktopWinFsp {
    /// Create a new WinFsp filesystem context.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
        sync_handle: Option<crate::sync_processor::SyncHandle>,
        offline_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        let mut ops = CoreOps::new(graph, cache, inodes, drive_id, rt.clone())
            .with_offline_flag(offline_flag);
        if let Some(tx) = event_tx.clone() {
            ops = ops.with_event_sender(tx);
        }
        if let Some(sh) = sync_handle {
            ops = ops.with_sync_handle(sh);
        }
        let open_files = ops.open_files().clone();
        Self {
            ops,
            rt,
            open_files,
            event_tx,
        }
    }

    /// Creates a delta sync observer that shares the open file table.
    pub fn create_delta_observer(&self) -> Arc<WinFspDeltaObserver> {
        Arc::new(WinFspDeltaObserver {
            open_files: self.open_files.clone(),
        })
    }
}

impl FileSystemContext for CarmineDesktopWinFsp {
    type FileContext = WinFspFileContext;

    // ──────────────────────────────────────────────────────────────────────
    // 4.1  get_security_by_name
    // ──────────────────────────────────────────────────────────────────────

    fn get_security_by_name(
        &self,
        file_name: &U16CStr,
        _security_descriptor: Option<&mut [c_void]>,
        resolve_reparse_points: impl FnOnce(&U16CStr) -> Option<FileSecurity>,
    ) -> winfsp::Result<FileSecurity> {
        // Check for reparse points first (required by WinFsp contract).
        if let Some(security) = resolve_reparse_points(file_name) {
            return Ok(security);
        }

        let components = split_path(file_name);

        // Root path: resolve directly to ROOT_INODE.
        if components.is_empty() {
            let attributes = FILE_ATTRIBUTE_DIRECTORY;
            return Ok(FileSecurity {
                reparse: false,
                sz_security_descriptor: 0,
                attributes,
            });
        }

        // Resolve through CoreOps.
        let (_ino, item) = self
            .ops
            .resolve_path(&components)
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;

        let attributes = if item.is_folder() {
            FILE_ATTRIBUTE_DIRECTORY
        } else {
            FILE_ATTRIBUTE_NORMAL
        };

        Ok(FileSecurity {
            reparse: false,
            sz_security_descriptor: 0,
            attributes,
        })
    }

    // ──────────────────────────────────────────────────────────────────────
    // 4.2  open
    // ──────────────────────────────────────────────────────────────────────

    fn open(
        &self,
        file_name: &U16CStr,
        _create_options: u32,
        _granted_access: FILE_ACCESS_RIGHTS,
        file_info: &mut OpenFileInfo,
    ) -> winfsp::Result<Self::FileContext> {
        let components = split_path(file_name);

        let (ino, item) = if components.is_empty() {
            // Root directory.
            let item = self
                .ops
                .lookup_item(ROOT_INODE)
                .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;
            (ROOT_INODE, item)
        } else {
            self.ops
                .resolve_path(&components)
                .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?
        };

        let is_dir = item.is_folder();

        // Open a file handle for regular files; directories don't need one.
        let fh = if is_dir {
            None
        } else {
            let handle = self.ops.open_file(ino).map_err(vfs_err_to_ntstatus)?;
            Some(handle)
        };

        // Re-fetch item after open_file() which may have refreshed the memory
        // cache with fresh server metadata. Using the stale pre-refresh `item`
        // would produce incorrect timestamps in the returned FileInfo.
        let fresh_item = self
            .ops
            .lookup_item(ino)
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;

        // Determine file size: prefer open handle size for consistency.
        let handle_size = if !is_dir {
            self.open_files.get_content_size_by_ino(ino)
        } else {
            None
        };

        let fi = item_to_file_info(&fresh_item, handle_size);
        *file_info.as_mut() = fi;

        // Report the correct-case path so Explorer shows the server-side name.
        if let Some(normalized) = build_normalized_path(&self.ops, &components) {
            file_info.set_normalized_name(&normalized, None);
        }

        Ok(WinFspFileContext { ino, fh, is_dir })
    }

    // ──────────────────────────────────────────────────────────────────────
    // close (required by trait)
    // ──────────────────────────────────────────────────────────────────────

    fn close(&self, context: Self::FileContext) {
        if let Some(fh) = context.fh {
            let _ = self.ops.release_file(fh);
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // 4.3  get_file_info
    // ──────────────────────────────────────────────────────────────────────

    fn get_file_info(
        &self,
        context: &Self::FileContext,
        file_info: &mut winfsp::filesystem::FileInfo,
    ) -> winfsp::Result<()> {
        let item = self
            .ops
            .lookup_item(context.ino)
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;

        // Prefer open handle content size over cached DriveItem size.
        let handle_size = if !context.is_dir {
            self.open_files.get_content_size_by_ino(context.ino)
        } else {
            None
        };

        *file_info = item_to_file_info(&item, handle_size);
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // 4.4  read
    // ──────────────────────────────────────────────────────────────────────

    fn read(
        &self,
        context: &Self::FileContext,
        buffer: &mut [u8],
        offset: u64,
    ) -> winfsp::Result<u32> {
        let fh = context
            .fh
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_IO_DEVICE_ERROR))?;

        let data = self
            .ops
            .read_handle(fh, offset as usize, buffer.len())
            .map_err(vfs_err_to_ntstatus)?;

        if data.is_empty() {
            return Ok(0);
        }

        let count = data.len().min(buffer.len());
        buffer[..count].copy_from_slice(&data[..count]);
        Ok(count as u32)
    }

    // ──────────────────────────────────────────────────────────────────────
    // 4.5  read_directory
    // ──────────────────────────────────────────────────────────────────────

    fn read_directory(
        &self,
        context: &Self::FileContext,
        _pattern: Option<&U16CStr>,
        marker: DirMarker<'_>,
        buffer: &mut [u8],
    ) -> winfsp::Result<u32> {
        let ino = context.ino;
        let mut cursor = 0u32;

        let children = self.ops.list_children(ino);
        let dir_item = self.ops.lookup_item(ino);

        // Decode the marker name (last-returned entry) if continuation.
        // DirMarker::inner() returns Option<&[u16]> — the UTF-16 name of the
        // last entry returned on the previous call, or None on the first call.
        let marker_name: Option<String> = marker
            .inner()
            .map(|m| OsString::from_wide(m).to_string_lossy().to_string());

        // Helper closure: emit a single DirInfo entry into the buffer.
        // Returns false if the buffer is full.
        let mut emit_entry = |name: &str, item: Option<&DriveItem>, is_dir_entry: bool| -> bool {
            let mut dir_info = DirInfo::<255>::new();
            let os_name: OsString = name.into();
            if dir_info.set_name(&os_name).is_err() {
                return true; // skip entries with names that don't fit
            }
            let fi = dir_info.file_info_mut();
            if let Some(item) = item {
                *fi = item_to_file_info(item, None);
            } else if is_dir_entry {
                fi.file_attributes = FILE_ATTRIBUTE_DIRECTORY;
            }
            dir_info.append_to_buffer(buffer, &mut cursor)
        };

        // Build a flat list of (name, Option<&DriveItem>, is_dot_entry) so we
        // can apply the marker logic uniformly across dot-entries and children.
        //
        // WinFsp enumeration: first call has no marker → emit everything from ".".
        // Subsequent calls carry the last-returned name as marker → skip up to
        // and including that name, then emit the rest.

        // Determine where to start based on the marker.
        // "." < ".." < first child (lexicographic in the flat list).
        let skip_dots = match marker_name.as_deref() {
            None => false,      // no marker → emit "." and ".." first
            Some(".") => true,  // marker is "." → skip ".", emit ".." onwards
            Some("..") => true, // marker is ".." → skip both dots, start at children
            Some(_) => true,    // marker is a child name → skip dots and children before marker
        };

        let skip_dotdot = !matches!(marker_name.as_deref(), None | Some("."));

        // Emit "." if not past it.
        if !skip_dots && !emit_entry(".", dir_item.as_ref(), true) {
            DirInfo::<255>::finalize_buffer(buffer, &mut cursor);
            return Ok(cursor);
        }

        // Emit ".." if not past it.
        if !skip_dotdot && !emit_entry("..", dir_item.as_ref(), true) {
            DirInfo::<255>::finalize_buffer(buffer, &mut cursor);
            return Ok(cursor);
        }

        // For child entries, skip up to and including the marker name.
        let child_marker = match marker_name.as_deref() {
            None | Some(".") | Some("..") => None, // start from first child
            Some(name) => Some(name.to_string()),
        };

        let mut past_marker = child_marker.is_none();

        for (_child_ino, item) in &children {
            if !past_marker {
                if let Some(ref m) = child_marker
                    && item.name.eq_ignore_ascii_case(m)
                {
                    past_marker = true;
                }
                continue;
            }

            if !emit_entry(&item.name, Some(item), false) {
                break; // buffer full
            }
        }

        DirInfo::<255>::finalize_buffer(buffer, &mut cursor);
        Ok(cursor)
    }

    // ──────────────────────────────────────────────────────────────────────
    // 4.6  get_volume_info
    // ──────────────────────────────────────────────────────────────────────

    fn get_volume_info(&self, volume_info: &mut VolumeInfo) -> winfsp::Result<()> {
        let (total_size, free_size) = match self.ops.get_quota() {
            Some(quota) => {
                let total = quota.total.unwrap_or(0).max(0) as u64;
                let remaining = quota.remaining.unwrap_or(total as i64).max(0) as u64;
                (total, remaining)
            }
            None => {
                // Fallback: 1 TB total, 1 TB free
                let one_tb = 1u64 << 40;
                (one_tb, one_tb)
            }
        };

        volume_info.total_size = total_size;
        volume_info.free_size = free_size;
        volume_info.set_volume_label("Carmine Desktop");
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // Group 5: Write-path methods
    // ──────────────────────────────────────────────────────────────────────

    // ──────────────────────────────────────────────────────────────────────
    // 5.1  create
    // ──────────────────────────────────────────────────────────────────────

    fn create(
        &self,
        file_name: &U16CStr,
        create_options: u32,
        _granted_access: FILE_ACCESS_RIGHTS,
        _file_attributes: u32,
        _security_descriptor: Option<&[c_void]>,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        _extra_buffer_is_reparse_point: bool,
        file_info: &mut OpenFileInfo,
    ) -> winfsp::Result<Self::FileContext> {
        let components = split_path(file_name);
        if components.is_empty() {
            return Err(winfsp::FspError::NTSTATUS(STATUS_ACCESS_DENIED));
        }

        // Split into parent path and new entry name.
        let (parent_components, name) = components.split_at(components.len() - 1);
        let name = &name[0];

        // Resolve parent directory inode.
        let (parent_ino, _parent_item) = if parent_components.is_empty() {
            let item = self
                .ops
                .lookup_item(ROOT_INODE)
                .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;
            (ROOT_INODE, item)
        } else {
            self.ops
                .resolve_path(parent_components)
                .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?
        };

        let is_dir = create_options & FILE_DIRECTORY_FILE != 0;

        if is_dir {
            let (ino, item) = self
                .ops
                .mkdir(parent_ino, name)
                .map_err(vfs_err_to_ntstatus)?;

            let fi = item_to_file_info(&item, None);
            *file_info.as_mut() = fi;

            // Set normalized name with correct case from server.
            if let Some(normalized) = build_normalized_path(&self.ops, &components) {
                file_info.set_normalized_name(&normalized, None);
            }

            Ok(WinFspFileContext {
                ino,
                fh: None,
                is_dir: true,
            })
        } else {
            let (fh, ino, item) = self
                .ops
                .create_file(parent_ino, name)
                .map_err(vfs_err_to_ntstatus)?;

            let fi = item_to_file_info(&item, Some(0));
            *file_info.as_mut() = fi;

            // Set normalized name with correct case from server.
            if let Some(normalized) = build_normalized_path(&self.ops, &components) {
                file_info.set_normalized_name(&normalized, None);
            }

            Ok(WinFspFileContext {
                ino,
                fh: Some(fh),
                is_dir: false,
            })
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // 5.2  write
    // ──────────────────────────────────────────────────────────────────────

    fn write(
        &self,
        context: &Self::FileContext,
        buffer: &[u8],
        offset: u64,
        write_to_eof: bool,
        _constrained_io: bool,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<u32> {
        let fh = context
            .fh
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_IO_DEVICE_ERROR))?;

        let actual_offset = if write_to_eof {
            self.open_files
                .get_content_size_by_ino(context.ino)
                .unwrap_or(0)
        } else {
            offset
        };
        let written = self
            .ops
            .write_handle(fh, actual_offset as usize, buffer)
            .map_err(vfs_err_to_ntstatus)?;

        // Update FileInfo with the new size.
        let item = self
            .ops
            .lookup_item(context.ino)
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;

        let handle_size = self.open_files.get_content_size_by_ino(context.ino);
        *file_info = item_to_file_info(&item, handle_size);

        Ok(written)
    }

    // ──────────────────────────────────────────────────────────────────────
    // 5.3  overwrite
    // ──────────────────────────────────────────────────────────────────────

    fn overwrite(
        &self,
        context: &Self::FileContext,
        _file_attributes: u32,
        _replace_file_attributes: bool,
        _allocation_size: u64,
        _extra_buffer: Option<&[u8]>,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<()> {
        // Best-effort: flush pending dirty data before truncating.
        // The user intends to replace the file, so we proceed regardless.
        if self.ops.is_dirty(context.ino)
            && let Some(fh) = context.fh
            && let Err(e) = self.ops.flush_handle(fh, true)
        {
            tracing::warn!(
                ino = context.ino,
                "overwrite: best-effort flush of dirty data failed: {e:?}"
            );
        }

        self.ops
            .truncate(context.ino, 0)
            .map_err(vfs_err_to_ntstatus)?;

        let item = self
            .ops
            .lookup_item(context.ino)
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;

        *file_info = item_to_file_info(&item, Some(0));
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // 5.4  cleanup
    // ──────────────────────────────────────────────────────────────────────

    fn cleanup(&self, context: &Self::FileContext, file_name: Option<&U16CStr>, flags: u32) {
        // FspCleanupDelete = 0x01: file should be deleted on close.
        const FSP_CLEANUP_DELETE: u32 = 0x01;
        // Flush dirty file handles.
        if let Some(fh) = context.fh
            && let Err(e) = self.ops.flush_handle(fh, true)
        {
            let file_name_str = self
                .ops
                .lookup_item(context.ino)
                .map(|i| i.name)
                .unwrap_or_default();
            if !file_name_str.is_empty() {
                self.ops.send_event(VfsEvent::UploadFailed {
                    file_name: file_name_str,
                    reason: format!("{e:?}"),
                });
            }
        }

        // Execute delete-on-close if flagged.
        if flags & FSP_CLEANUP_DELETE != 0
            && let Some(fname) = file_name
        {
            let components = split_path(fname);
            if !components.is_empty() {
                let (parent_components, name_slice) = components.split_at(components.len() - 1);
                let name = &name_slice[0];

                let parent_ino = if parent_components.is_empty() {
                    Some(ROOT_INODE)
                } else {
                    self.ops.resolve_path(parent_components).map(|(ino, _)| ino)
                };

                if let Some(parent_ino) = parent_ino {
                    let result = if context.is_dir {
                        self.ops.rmdir(parent_ino, name)
                    } else {
                        self.ops.unlink(parent_ino, name)
                    };
                    if let Err(e) = result {
                        tracing::warn!(
                            parent_ino,
                            name = %name,
                            is_dir = context.is_dir,
                            "cleanup delete-on-close failed: {e:?}"
                        );
                        self.ops.send_event(VfsEvent::DeleteFailed {
                            file_name: name.to_string(),
                            reason: format!("{e:?}"),
                        });
                    }
                }
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // 5.5  flush
    // ──────────────────────────────────────────────────────────────────────

    fn flush(
        &self,
        context: Option<&Self::FileContext>,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<()> {
        let context = match context {
            Some(ctx) => ctx,
            None => return Ok(()), // no file context — nothing to flush
        };

        let fh = match context.fh {
            Some(fh) => fh,
            None => return Ok(()), // directories — nothing to flush
        };
        match self.ops.flush_handle(fh, true) {
            Ok(()) => {
                // Update FileInfo with fresh metadata after successful flush.
                let item = self
                    .ops
                    .lookup_item(context.ino)
                    .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;
                let handle_size = self.open_files.get_content_size_by_ino(context.ino);
                *file_info = item_to_file_info(&item, handle_size);
                Ok(())
            }
            Err(e) => {
                let file_name = self
                    .ops
                    .lookup_item(context.ino)
                    .map(|i| i.name)
                    .unwrap_or_default();
                if !file_name.is_empty() {
                    self.ops.send_event(VfsEvent::UploadFailed {
                        file_name,
                        reason: format!("{e:?}"),
                    });
                }
                Err(vfs_err_to_ntstatus(e))
            }
        }
    }

    // ──────────────────────────────────────────────────────────────────────
    // 5.6  set_file_size
    // ──────────────────────────────────────────────────────────────────────

    fn set_file_size(
        &self,
        context: &Self::FileContext,
        new_size: u64,
        set_allocation_size: bool,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<()> {
        if set_allocation_size {
            // Allocation size: grow buffer capacity but don't change logical_size.
            // Windows sets this to reserve disk blocks — it does NOT change the
            // actual end-of-file position.
            self.ops
                .ensure_buffer_capacity(context.ino, new_size)
                .map_err(vfs_err_to_ntstatus)?;
        } else {
            // File size: actual truncate/extend — updates logical_size.
            self.ops
                .truncate(context.ino, new_size)
                .map_err(vfs_err_to_ntstatus)?;
        }

        let item = self
            .ops
            .lookup_item(context.ino)
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;

        let handle_size = self.open_files.get_content_size_by_ino(context.ino);
        *file_info = item_to_file_info(&item, handle_size);
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // 5.7  set_basic_info
    // ──────────────────────────────────────────────────────────────────────

    fn set_basic_info(
        &self,
        context: &Self::FileContext,
        _file_attributes: u32,
        _creation_time: u64,
        _last_access_time: u64,
        _last_write_time: u64,
        _change_time: u64,
        file_info: &mut FileInfo,
    ) -> winfsp::Result<()> {
        // Timestamps are server-authoritative — local timestamp changes
        // (creation, last-access, last-write, change) are intentionally
        // ignored. The server sets authoritative timestamps on upload.
        // Return current FileInfo unchanged.
        let item = self
            .ops
            .lookup_item(context.ino)
            .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?;

        let handle_size = if !context.is_dir {
            self.open_files.get_content_size_by_ino(context.ino)
        } else {
            None
        };

        *file_info = item_to_file_info(&item, handle_size);
        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // 5.8  set_delete
    // ──────────────────────────────────────────────────────────────────────

    fn set_delete(
        &self,
        context: &Self::FileContext,
        _file_name: &U16CStr,
        delete_file: bool,
    ) -> winfsp::Result<()> {
        if !delete_file {
            return Ok(());
        }

        // For files, deletion intent is handled in cleanup — just return Ok.
        if !context.is_dir {
            return Ok(());
        }

        // For directories, check if non-empty.
        let children = self.ops.list_children(context.ino);
        if !children.is_empty() {
            return Err(winfsp::FspError::NTSTATUS(STATUS_DIRECTORY_NOT_EMPTY));
        }

        Ok(())
    }

    // ──────────────────────────────────────────────────────────────────────
    // 5.9  rename
    // ──────────────────────────────────────────────────────────────────────

    fn rename(
        &self,
        _context: &Self::FileContext,
        file_name: &U16CStr,
        new_file_name: &U16CStr,
        replace_if_exists: bool,
    ) -> winfsp::Result<()> {
        let src_components = split_path(file_name);
        let dst_components = split_path(new_file_name);
        if src_components.is_empty() || dst_components.is_empty() {
            return Err(winfsp::FspError::NTSTATUS(STATUS_ACCESS_DENIED));
        }

        // Split source into parent + name.
        let (src_parent_comps, src_name_slice) = src_components.split_at(src_components.len() - 1);
        let src_name = &src_name_slice[0];

        let src_parent_ino = if src_parent_comps.is_empty() {
            ROOT_INODE
        } else {
            self.ops
                .resolve_path(src_parent_comps)
                .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?
                .0
        };

        // Split destination into parent + name.
        let (dst_parent_comps, dst_name_slice) = dst_components.split_at(dst_components.len() - 1);
        let dst_name = &dst_name_slice[0];

        let dst_parent_ino = if dst_parent_comps.is_empty() {
            ROOT_INODE
        } else {
            self.ops
                .resolve_path(dst_parent_comps)
                .ok_or(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_NOT_FOUND))?
                .0
        };

        // Check for collision when replace is not allowed.
        if !replace_if_exists
            && self
                .ops
                .find_child(dst_parent_ino, &std::ffi::OsString::from(dst_name.clone()))
                .is_some()
        {
            return Err(winfsp::FspError::NTSTATUS(STATUS_OBJECT_NAME_COLLISION));
        }

        self.ops
            .rename(src_parent_ino, src_name, dst_parent_ino, dst_name)
            .map_err(vfs_err_to_ntstatus)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Group 6: WinFsp Mount Lifecycle
// ──────────────────────────────────────────────────────────────────────────────

impl WinFspMountHandle {
    /// Mount a WinFsp filesystem for the given drive.
    ///
    /// Follows the same pattern as the FUSE `MountHandle::mount()`:
    /// fetch root item, seed caches, create filesystem, configure WinFsp host,
    /// mount and start.
    #[allow(clippy::too_many_arguments)]
    pub fn mount(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        mountpoint: &str,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
        sync_handle: Option<crate::sync_processor::SyncHandle>,
        offline_flag: Arc<std::sync::atomic::AtomicBool>,
    ) -> carminedesktop_core::Result<Self> {
        // 0. Initialize WinFsp DLL (resolves the delay-loaded DLL from PATH or registry).
        winfsp::winfsp_init().map_err(|e| {
            carminedesktop_core::Error::Filesystem(format!(
                "WinFsp initialization failed — is WinFsp installed? ({e:?})"
            ))
        })?;

        // 1. Fetch drive root — cache-first with network fallback for offline support.
        let root_item = match cache.sqlite.get_item_by_inode(ROOT_INODE) {
            Ok(Some(cached_root)) => {
                tracing::debug!("restored root item from SQLite cache for drive {drive_id}");
                match tokio::task::block_in_place(|| rt.block_on(graph.get_item(&drive_id, "root")))
                {
                    Ok(fresh) => fresh,
                    Err(e) => {
                        tracing::warn!("root item refresh failed: {e} — using cached version");
                        offline_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                        cached_root
                    }
                }
            }
            _ => {
                // No cache — must fetch from network (first-time mount)
                tokio::task::block_in_place(|| rt.block_on(graph.get_item(&drive_id, "root")))
                    .map_err(|e| {
                        carminedesktop_core::Error::Filesystem(format!(
                            "failed to fetch root item for drive {drive_id}: {e}"
                        ))
                    })?
            }
        };

        // 2. Seed root into caches.
        inodes.set_root(&root_item.id);
        cache.memory.insert(ROOT_INODE, root_item.clone());
        cache
            .sqlite
            .upsert_item(ROOT_INODE, &drive_id, &root_item, None)?;

        // 2b. Pre-fetch root children to avoid blocking Explorer navigation pane enumeration.
        // Without this, the first `list_children(ROOT_INODE)` hits the Graph API synchronously,
        // causing a ~10s delay when Explorer enumerates the navigation pane on cold cache.
        match tokio::task::block_in_place(|| {
            rt.block_on(graph.list_children(&drive_id, &root_item.id))
        }) {
            Ok(children) => {
                let mut children_map = std::collections::HashMap::new();
                for item in &children {
                    let child_ino = inodes.allocate(&item.id);
                    children_map.insert(item.name.clone(), child_ino);
                    cache.memory.insert(child_ino, item.clone());
                    let _ = cache
                        .sqlite
                        .upsert_item(child_ino, &drive_id, item, Some(ROOT_INODE));
                }
                cache
                    .memory
                    .insert_with_children(ROOT_INODE, root_item.clone(), children_map);
                tracing::info!(
                    "pre-fetched {} root children for {drive_id}",
                    children.len()
                );
            }
            Err(e) => {
                tracing::warn!(
                    "failed to pre-fetch root children: {e} — Explorer may be slow on first access"
                );
            }
        }

        // 3. Create filesystem context.
        let stored_handle = sync_handle.clone();
        let fs = CarmineDesktopWinFsp::new(
            graph.clone(),
            cache.clone(),
            inodes.clone(),
            drive_id.clone(),
            rt.clone(),
            event_tx,
            sync_handle,
            offline_flag,
        );

        // 4. Create delta observer before host takes ownership.
        let delta_observer = fs.create_delta_observer();

        // 5. Configure volume params.
        let mut volume_params = VolumeParams::new();
        volume_params
            .filesystem_name("carminedesktop")
            .file_info_timeout(5000)
            .sector_size(4096)
            .sectors_per_allocation_unit(1)
            .volume_serial_number(0)
            .case_sensitive_search(false)
            .case_preserved_names(true)
            .unicode_on_disk(true)
            .read_only_volume(false);

        // 6. Create host.
        let mut host = FileSystemHost::new(volume_params, fs).map_err(|e| {
            carminedesktop_core::Error::Filesystem(format!("WinFsp host creation failed: {e:?}"))
        })?;

        // 7. Mount at mountpoint.
        host.mount(mountpoint).map_err(|e| {
            carminedesktop_core::Error::Filesystem(format!("WinFsp mount failed: {e:?}"))
        })?;

        // 8. Start.
        host.start().map_err(|e| {
            carminedesktop_core::Error::Filesystem(format!("WinFsp start failed: {e:?}"))
        })?;

        tracing::info!("mounted drive {drive_id} at {mountpoint} (WinFsp)");

        Ok(Self {
            host,
            cache,
            graph,
            drive_id,
            rt,
            mountpoint: mountpoint.to_string(),
            delta_observer,
            sync_handle: stored_handle,
            sync_join: None,
        })
    }

    pub fn set_sync_join(&mut self, join: tokio::task::JoinHandle<()>) {
        self.sync_join = Some(join);
    }

    /// Unmount the WinFsp filesystem.
    ///
    /// Sends shutdown to the sync processor, waits for drain, then flushes
    /// remaining pending items as a safety net before stopping the host.
    pub fn unmount(mut self) -> carminedesktop_core::Result<()> {
        // Send shutdown to sync processor and await drain
        if let Some(ref sh) = self.sync_handle {
            sh.send(crate::sync_processor::SyncRequest::Shutdown);
        }
        if let Some(join) = self.sync_join.take() {
            tokio::task::block_in_place(|| {
                let _ = self.rt.block_on(join);
            });
        }

        // Safety net: flush any remaining pending writes
        tokio::task::block_in_place(|| {
            self.rt.block_on(crate::pending::flush_pending(
                &self.cache,
                &self.graph,
                &self.drive_id,
            ))
        });

        self.host.stop();
        self.host.unmount();

        tracing::info!("unmounted {} (WinFsp)", self.mountpoint);
        Ok(())
    }

    /// Returns the drive ID for this mount.
    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    /// Returns the mountpoint path for this mount.
    pub fn mountpoint(&self) -> &str {
        &self.mountpoint
    }

    /// Returns the delta sync observer for this mount.
    pub fn delta_observer(&self) -> Arc<dyn DeltaSyncObserver> {
        self.delta_observer.clone()
    }
}
