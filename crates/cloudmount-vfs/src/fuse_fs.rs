use std::ffi::OsStr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use fuser::{
    BsdFileFlags, Config, CopyFileRangeFlags, Errno, FileAttr, FileHandle, FileType, Filesystem,
    FopenFlags, Generation, INodeNo, InitFlags, KernelConfig, LockOwner, MountOption, Notifier,
    OpenFlags, RenameFlags, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory, ReplyDirectoryPlus,
    ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request, TimeOrNow, WriteFlags,
};
use tokio::runtime::Handle;

use crate::core_ops::{CoreOps, OpenFileTable, VfsError, VfsEvent};
use crate::inode::InodeTable;
use cloudmount_cache::CacheManager;
use cloudmount_core::DeltaSyncObserver;
use cloudmount_core::types::DriveItem;
use cloudmount_graph::GraphClient;

/// Delta sync observer for the FUSE backend.
///
/// Implements [`DeltaSyncObserver`] to mark open file handles as stale and
/// invalidate the kernel's FUSE page cache when delta sync detects remote
/// content changes. Holds a shared reference to the [`OpenFileTable`] and
/// an optional [`Notifier`] for kernel cache invalidation.
pub struct FuseDeltaObserver {
    open_files: Arc<OpenFileTable>,
    notifier: Arc<std::sync::Mutex<Option<Notifier>>>,
}

impl FuseDeltaObserver {
    /// Set the FUSE notifier for kernel cache invalidation.
    ///
    /// Called after mount when the `BackgroundSession` is available.
    pub fn set_notifier(&self, notifier: Notifier) {
        *self.notifier.lock().unwrap() = Some(notifier);
    }

    /// Clear the notifier (e.g., on unmount).
    pub fn clear_notifier(&self) {
        *self.notifier.lock().unwrap() = None;
    }
}

impl DeltaSyncObserver for FuseDeltaObserver {
    /// Marks open handles stale and invalidates the kernel page cache for `ino`.
    ///
    /// Always calls `inval_inode` regardless of whether the file has open handles.
    /// With `FUSE_WRITEBACK_CACHE`, the kernel refuses to accept size/mtime updates
    /// from `getattr` alone — `inval_inode` is the only way to force it to discard
    /// its cached `i_size` and re-fetch attributes on the next access.
    fn on_inode_content_changed(&self, ino: u64) {
        self.open_files.mark_stale_by_ino(ino);

        let notifier = self.notifier.lock().unwrap();
        if let Some(ref n) = *notifier {
            match n.inval_inode(INodeNo(ino), 0, -1) {
                Ok(()) => {
                    tracing::debug!(ino, "delta observer: invalidated kernel cache");
                }
                Err(e) => {
                    tracing::debug!(ino, "delta observer: kernel cache invalidation failed: {e}");
                }
            }
        } else {
            tracing::debug!(
                ino,
                "delta observer: skipping kernel invalidation (no session)"
            );
        }
    }
}

const FILE_TTL: Duration = Duration::from_secs(5);
const DIR_TTL: Duration = Duration::from_secs(30);
const BLOCK_SIZE: u32 = 512;

pub struct CloudMountFs {
    ops: CoreOps,
    uid: u32,
    gid: u32,
    notifier_slot: Arc<std::sync::Mutex<Option<Notifier>>>,
}

impl CloudMountFs {
    pub fn new(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        rt: Handle,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
    ) -> Self {
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        // Shared slot: populated after mount, used by both the filesystem
        // callbacks (open_file metadata refresh) and the delta sync observer.
        let notifier_slot: Arc<std::sync::Mutex<Option<Notifier>>> =
            Arc::new(std::sync::Mutex::new(None));

        let slot_for_invalidator = notifier_slot.clone();
        let invalidator: crate::core_ops::InodeInvalidator = Arc::new(move |ino: u64| {
            let guard = slot_for_invalidator.lock().unwrap();
            if let Some(ref n) = *guard {
                let _ = n.inval_inode(INodeNo(ino), 0, -1);
            }
        });

        let mut ops = CoreOps::new(graph, cache, inodes, drive_id, rt);
        ops = ops.with_inode_invalidator(invalidator);
        if let Some(tx) = event_tx {
            ops = ops.with_event_sender(tx);
        }
        Self {
            ops,
            uid,
            gid,
            notifier_slot,
        }
    }

    /// Creates a delta sync observer that shares the open file table and
    /// notifier slot with this filesystem.
    ///
    /// Call this before `mount()` (which consumes `self`). After mount, call
    /// `FuseDeltaObserver::set_notifier()` with `BackgroundSession::notifier()`
    /// — this populates the shared slot for both the observer and the
    /// filesystem's inode invalidator.
    pub fn create_delta_observer(&self) -> Arc<FuseDeltaObserver> {
        Arc::new(FuseDeltaObserver {
            open_files: self.ops.open_files().clone(),
            notifier: self.notifier_slot.clone(),
        })
    }

    pub fn mount(
        self,
        mountpoint: &str,
        auto_unmount: bool,
    ) -> cloudmount_core::Result<fuser::BackgroundSession> {
        let mut config = Config::default();
        config.mount_options = vec![
            MountOption::RW,
            MountOption::FSName("cloudmount".to_string()),
            MountOption::CUSTOM("max_read=1048576".into()),
            MountOption::NoAtime,
        ];
        if auto_unmount {
            config.mount_options.push(MountOption::AutoUnmount);
        }

        fuser::spawn_mount2(self, mountpoint, &config)
            .map_err(|e| cloudmount_core::Error::Filesystem(format!("mount failed: {e}")))
    }

    fn item_to_attr(&self, inode: u64, item: &DriveItem) -> FileAttr {
        let kind = if item.is_folder() {
            FileType::Directory
        } else {
            FileType::RegularFile
        };

        let perm = if item.is_folder() { 0o755 } else { 0o644 };
        let size = if item.is_folder() {
            0
        } else {
            item.size as u64
        };

        let mtime = item
            .last_modified
            .map(|dt| UNIX_EPOCH + Duration::from_secs(dt.timestamp().max(0) as u64))
            .unwrap_or(UNIX_EPOCH);

        let ctime = item
            .created
            .map(|dt| UNIX_EPOCH + Duration::from_secs(dt.timestamp().max(0) as u64))
            .unwrap_or(UNIX_EPOCH);

        FileAttr {
            ino: INodeNo(inode),
            size,
            blocks: size.div_ceil(BLOCK_SIZE as u64),
            atime: mtime,
            mtime,
            ctime,
            crtime: ctime,
            kind,
            perm,
            nlink: if item.is_folder() { 2 } else { 1 },
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    fn ttl_for(item: &DriveItem) -> Duration {
        if item.is_folder() { DIR_TTL } else { FILE_TTL }
    }

    fn ttl_for_attr(attr: &FileAttr) -> Duration {
        if attr.kind == FileType::Directory {
            DIR_TTL
        } else {
            FILE_TTL
        }
    }

    fn vfs_err_to_errno(e: VfsError) -> Errno {
        match e {
            VfsError::NotFound => Errno::ENOENT,
            VfsError::NotADirectory => Errno::ENOTDIR,
            VfsError::DirectoryNotEmpty => Errno::ENOTEMPTY,
            VfsError::PermissionDenied => Errno::EACCES,
            VfsError::TimedOut => Errno::ETIMEDOUT,
            VfsError::QuotaExceeded => Errno::ENOSPC,
            VfsError::IoError(_) => Errno::EIO,
        }
    }
}

impl Filesystem for CloudMountFs {
    fn init(&mut self, _req: &Request, config: &mut KernelConfig) -> std::io::Result<()> {
        let caps = InitFlags::FUSE_WRITEBACK_CACHE | InitFlags::FUSE_PARALLEL_DIROPS;
        match config.add_capabilities(caps) {
            Ok(()) => {
                tracing::info!("FUSE capabilities enabled: writeback cache, parallel dirops");
            }
            Err(unsupported) => {
                tracing::warn!(
                    ?unsupported,
                    "some FUSE capabilities not supported by kernel"
                );
            }
        }
        Ok(())
    }

    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        match self.ops.find_child(parent.0, name) {
            Some((inode, item)) => {
                let ttl = Self::ttl_for(&item);
                let attr = self.item_to_attr(inode, &item);
                reply.entry(&ttl, &attr, Generation(0));
            }
            None => reply.error(Errno::ENOENT),
        }
    }

    fn setattr(
        &self,
        _req: &Request,
        ino: INodeNo,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<FileHandle>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        if let Some(new_size) = size
            && let Err(e) = self.ops.truncate(ino.0, new_size)
        {
            reply.error(Self::vfs_err_to_errno(e));
            return;
        }

        match self.ops.lookup_item_for_getattr(ino.0) {
            Some((item, has_open_handle)) => {
                let ttl = if has_open_handle {
                    Duration::ZERO
                } else {
                    Self::ttl_for(&item)
                };
                let attr = self.item_to_attr(ino.0, &item);
                reply.attr(&ttl, &attr);
            }
            None => reply.error(Errno::ENOENT),
        }
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        match self.ops.lookup_item_for_getattr(ino.0) {
            Some((item, has_open_handle)) => {
                let ttl = if has_open_handle {
                    Duration::ZERO
                } else {
                    Self::ttl_for(&item)
                };
                let attr = self.item_to_attr(ino.0, &item);
                reply.attr(&ttl, &attr);
            }
            None => reply.error(Errno::ENOENT),
        }
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let ino = ino.0;
        let mut entries: Vec<(u64, FileType, String)> = vec![
            (ino, FileType::Directory, ".".to_string()),
            (ino, FileType::Directory, "..".to_string()),
        ];

        for (child_ino, item) in self.ops.list_children(ino) {
            let kind = if item.is_folder() {
                FileType::Directory
            } else {
                FileType::RegularFile
            };
            entries.push((child_ino, kind, item.name.clone()));
        }

        for (i, (inode, kind, name)) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(INodeNo(inode), (i + 1) as u64, kind, &name) {
                break;
            }
        }
        reply.ok();
    }

    fn readdirplus(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectoryPlus,
    ) {
        let ino_val = ino.0;

        let dir_attr = match self.ops.lookup_item(ino_val) {
            Some(item) => self.item_to_attr(ino_val, &item),
            None => {
                reply.error(Errno::ENOENT);
                return;
            }
        };

        let mut entries: Vec<(u64, FileAttr, String)> = vec![
            (ino_val, dir_attr, ".".to_string()),
            (ino_val, dir_attr, "..".to_string()),
        ];

        for (child_ino, item) in self.ops.list_children(ino_val) {
            let attr = self.item_to_attr(child_ino, &item);
            entries.push((child_ino, attr, item.name.clone()));
        }

        for (i, (inode, attr, name)) in entries.into_iter().enumerate().skip(offset as usize) {
            let ttl = Self::ttl_for_attr(&attr);
            if reply.add(
                INodeNo(inode),
                (i + 1) as u64,
                &name,
                &ttl,
                &attr,
                Generation(0),
            ) {
                break;
            }
        }
        reply.ok();
    }

    fn read(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        match self.ops.read_handle(fh.0, offset as usize, size as usize) {
            Ok(data) => reply.data(&data),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn open(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        match self.ops.open_file(ino.0) {
            Ok(fh) => reply.opened(FileHandle(fh), FopenFlags::empty()),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn write(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        _write_flags: WriteFlags,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        match self.ops.write_handle(fh.0, offset as usize, data) {
            Ok(written) => reply.written(written),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn flush(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        match self.ops.flush_handle(fh.0) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn release(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        match self.ops.release_file(fh.0) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn fsync(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        match self.ops.flush_handle(fh.0) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn create(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        let name_str = name.to_string_lossy().to_string();

        match self.ops.create_file(parent.0, &name_str) {
            Ok((fh, inode, item)) => {
                let attr = self.item_to_attr(inode, &item);
                reply.created(
                    &FILE_TTL,
                    &attr,
                    Generation(0),
                    FileHandle(fh),
                    FopenFlags::empty(),
                );
            }
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn mkdir(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let name_str = name.to_string_lossy().to_string();

        match self.ops.mkdir(parent.0, &name_str) {
            Ok((inode, item)) => {
                let attr = self.item_to_attr(inode, &item);
                reply.entry(&DIR_TTL, &attr, Generation(0));
            }
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let name_str = name.to_string_lossy();

        match self.ops.unlink(parent.0, &name_str) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let name_str = name.to_string_lossy();

        match self.ops.rmdir(parent.0, &name_str) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        let (blocks, bfree, bavail) = match self.ops.get_quota() {
            Some(quota) => {
                let total = quota.total.unwrap_or(0).max(0) as u64;
                let remaining = quota.remaining.unwrap_or(total as i64).max(0) as u64;
                let blk = total / BLOCK_SIZE as u64;
                let free = remaining / BLOCK_SIZE as u64;
                (blk, free, free)
            }
            None => {
                let fallback = 1u64 << 30;
                (fallback, fallback, fallback)
            }
        };
        reply.statfs(blocks, bfree, bavail, 0, 0, BLOCK_SIZE, 255, BLOCK_SIZE);
    }

    fn rename(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        _flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        let name_str = name.to_string_lossy();
        let newname_str = newname.to_string_lossy().to_string();

        match self
            .ops
            .rename(parent.0, &name_str, newparent.0, &newname_str)
        {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }

    fn copy_file_range(
        &self,
        _req: &Request,
        ino_in: INodeNo,
        fh_in: FileHandle,
        offset_in: u64,
        ino_out: INodeNo,
        fh_out: FileHandle,
        offset_out: u64,
        len: u64,
        _flags: CopyFileRangeFlags,
        reply: ReplyWrite,
    ) {
        match self.ops.copy_file_range(
            ino_in.0, fh_in.0, offset_in, ino_out.0, fh_out.0, offset_out, len,
        ) {
            Ok(n) => reply.written(n),
            Err(e) => reply.error(Self::vfs_err_to_errno(e)),
        }
    }
}
