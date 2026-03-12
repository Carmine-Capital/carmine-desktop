//! Shared VFS operations used by both FUSE (Linux/macOS) and WinFsp (Windows) backends.
//!
//! This module contains the core business logic for cache lookups, Graph API interactions,
//! inode management, and write-back operations. Platform-specific backends (FUSE callbacks,
//! WinFsp filesystem context) delegate to [`CoreOps`] instead of duplicating this logic.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use dashmap::DashMap;
use futures_util::StreamExt;
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::inode::InodeTable;
use cloudmount_cache::CacheManager;
use cloudmount_core::types::{DriveItem, DriveQuota, FileFacet, ParentReference};
use cloudmount_graph::{CopyStatus, GraphClient, SMALL_FILE_LIMIT};

/// Compare item names for child lookup.
/// Windows (NTFS/WinFsp) uses OrdinalIgnoreCase — ASCII case-insensitive.
/// FUSE on Linux/macOS uses exact (case-sensitive) comparison.
#[cfg(target_os = "windows")]
fn names_match(stored: &str, query: &str) -> bool {
    stored.eq_ignore_ascii_case(query)
}

#[cfg(not(target_os = "windows"))]
fn names_match(stored: &str, query: &str) -> bool {
    stored == query
}

const COPY_POLL_INITIAL_MS: u64 = 500;
const COPY_POLL_MAX_MS: u64 = 5000;
const COPY_POLL_BACKOFF: u64 = 2;
const COPY_MAX_POLL_DURATION_SECS: u64 = 10;
const COPY_POLL_MAX_RETRIES: u32 = 3;

/// 2 MB threshold: if a read offset is within this distance of the download
/// frontier, block and wait for the sequential download to catch up.
/// Beyond this, issue an on-demand range request instead.
const RANDOM_ACCESS_THRESHOLD: u64 = 2 * 1024 * 1024;

/// Maximum file size for in-memory streaming buffer (256 MB).
/// Files larger than this are downloaded to disk cache instead.
const MAX_STREAMING_BUFFER_SIZE: u64 = 256 * 1024 * 1024;

/// Chunk size for the streaming buffer's BTreeMap storage (256 KiB).
const STREAMING_CHUNK_SIZE: usize = 256 * 1024;

#[derive(Debug, Clone)]
pub enum DownloadProgress {
    InProgress(u64),
    Done,
    Failed(String),
}

pub struct StreamingBuffer {
    /// Chunk-based storage: key is chunk index (offset / STREAMING_CHUNK_SIZE).
    /// Each chunk is at most STREAMING_CHUNK_SIZE bytes.
    chunks: tokio::sync::RwLock<BTreeMap<u64, Vec<u8>>>,
    progress: watch::Sender<DownloadProgress>,
    progress_rx: watch::Receiver<DownloadProgress>,
    pub total_size: u64,
}

impl StreamingBuffer {
    pub fn new(total_size: u64) -> VfsResult<Self> {
        if total_size == 0 || total_size > MAX_STREAMING_BUFFER_SIZE {
            return Err(VfsError::IoError(format!(
                "file too large for streaming buffer: {total_size} bytes (max {MAX_STREAMING_BUFFER_SIZE})"
            )));
        }
        let (tx, rx) = watch::channel(DownloadProgress::InProgress(0));
        Ok(Self {
            chunks: tokio::sync::RwLock::new(BTreeMap::new()),
            progress: tx,
            progress_rx: rx,
            total_size,
        })
    }

    pub async fn append_chunk(&self, chunk: &[u8]) {
        let mut chunks = self.chunks.write().await;
        let current = match *self.progress_rx.borrow() {
            DownloadProgress::InProgress(n) => n as usize,
            _ => return,
        };
        let max_end = std::cmp::min(current + chunk.len(), self.total_size as usize);
        let usable = &chunk[..max_end - current];

        let mut offset = current;
        let mut remaining = usable;
        while !remaining.is_empty() {
            let chunk_key = (offset / STREAMING_CHUNK_SIZE) as u64;
            let chunk_offset = offset % STREAMING_CHUNK_SIZE;
            let space = STREAMING_CHUNK_SIZE - chunk_offset;
            let copy_len = std::cmp::min(space, remaining.len());

            let entry = chunks.entry(chunk_key).or_default();
            if entry.len() < chunk_offset + copy_len {
                entry.resize(chunk_offset + copy_len, 0);
            }
            entry[chunk_offset..chunk_offset + copy_len].copy_from_slice(&remaining[..copy_len]);

            remaining = &remaining[copy_len..];
            offset += copy_len;
        }
        let _ = self
            .progress
            .send(DownloadProgress::InProgress(offset as u64));
    }

    pub fn mark_done(&self) {
        let _ = self.progress.send(DownloadProgress::Done);
    }

    pub fn mark_failed(&self, msg: String) {
        let _ = self.progress.send(DownloadProgress::Failed(msg));
    }

    pub fn wait_for_range(&self, offset: u64, size: u64, rt: &Handle) -> VfsResult<()> {
        let needed = offset + size;
        let mut rx = self.progress_rx.clone();
        rt.block_on(async {
            loop {
                {
                    let progress = rx.borrow_and_update();
                    match &*progress {
                        DownloadProgress::InProgress(n) if *n >= needed => return Ok(()),
                        DownloadProgress::Done => return Ok(()),
                        DownloadProgress::Failed(msg) => {
                            return Err(VfsError::IoError(format!("download failed: {msg}")));
                        }
                        _ => {}
                    }
                }
                if rx.changed().await.is_err() {
                    return Err(VfsError::IoError("download channel closed".to_string()));
                }
            }
        })
    }

    pub async fn read_range(&self, offset: usize, size: usize) -> Vec<u8> {
        let chunks = self.chunks.read().await;
        let downloaded = match *self.progress_rx.borrow() {
            DownloadProgress::InProgress(n) => n as usize,
            DownloadProgress::Done | DownloadProgress::Failed(_) => self.total_size as usize,
        };
        let end = std::cmp::min(offset + size, downloaded);
        if offset >= end {
            return Vec::new();
        }
        let mut result = vec![0u8; end - offset];
        let mut pos = offset;
        let mut out_pos = 0;
        while pos < end {
            let chunk_key = (pos / STREAMING_CHUNK_SIZE) as u64;
            let chunk_offset = pos % STREAMING_CHUNK_SIZE;
            let copy_len = std::cmp::min(STREAMING_CHUNK_SIZE - chunk_offset, end - pos);

            if let Some(chunk) = chunks.get(&chunk_key) {
                let src_end = std::cmp::min(chunk_offset + copy_len, chunk.len());
                if chunk_offset < src_end {
                    result[out_pos..out_pos + (src_end - chunk_offset)]
                        .copy_from_slice(&chunk[chunk_offset..src_end]);
                }
            }

            pos += copy_len;
            out_pos += copy_len;
        }
        result
    }

    pub fn downloaded_bytes(&self) -> u64 {
        match *self.progress_rx.borrow() {
            DownloadProgress::InProgress(n) => n,
            DownloadProgress::Done => self.total_size,
            DownloadProgress::Failed(_) => 0,
        }
    }
}

pub enum DownloadState {
    Complete(Vec<u8>),
    Streaming {
        buffer: Arc<StreamingBuffer>,
        task: tokio::task::AbortHandle,
    },
}

impl DownloadState {
    pub fn is_complete(&self) -> bool {
        matches!(self, DownloadState::Complete(_))
    }

    pub fn as_complete(&self) -> Option<&Vec<u8>> {
        match self {
            DownloadState::Complete(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_complete_mut(&mut self) -> Option<&mut Vec<u8>> {
        match self {
            DownloadState::Complete(v) => Some(v),
            _ => None,
        }
    }

    pub fn into_complete(self) -> Option<Vec<u8>> {
        match self {
            DownloadState::Complete(v) => Some(v),
            _ => None,
        }
    }
}

pub struct OpenFile {
    pub ino: u64,
    pub content: DownloadState,
    pub dirty: bool,
    /// Set to `true` by the delta sync observer when remote content changes are detected.
    /// Does not interrupt active reads — the current content buffer continues to be served.
    pub stale: bool,
    /// Tracks the logical file size after truncation.
    ///
    /// When a file is truncated smaller, the underlying buffer may remain at its
    /// original capacity (to avoid reallocating on subsequent writes). This field
    /// records the intended size so that `flush_handle` truncates the buffer
    /// before uploading and `write_handle` uses the correct size for metadata.
    pub logical_size: Option<usize>,
}

pub struct OpenFileTable {
    files: DashMap<u64, OpenFile>,
    next_handle: AtomicU64,
}

impl Default for OpenFileTable {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenFileTable {
    pub fn new() -> Self {
        Self {
            files: DashMap::new(),
            next_handle: AtomicU64::new(1),
        }
    }

    pub fn insert(&self, ino: u64, content: DownloadState) -> u64 {
        let fh = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.files.insert(
            fh,
            OpenFile {
                ino,
                content,
                dirty: false,
                stale: false,
                logical_size: None,
            },
        );
        fh
    }

    pub fn get(&self, fh: u64) -> Option<dashmap::mapref::one::Ref<'_, u64, OpenFile>> {
        self.files.get(&fh)
    }

    pub fn get_mut(&self, fh: u64) -> Option<dashmap::mapref::one::RefMut<'_, u64, OpenFile>> {
        self.files.get_mut(&fh)
    }

    pub fn remove(&self, fh: u64) -> Option<OpenFile> {
        self.files.remove(&fh).map(|(_, v)| v)
    }

    pub fn find_by_ino(&self, ino: u64) -> Option<dashmap::mapref::one::RefMut<'_, u64, OpenFile>> {
        let fh = {
            let entry = self.files.iter().find(|e| e.value().ino == ino)?;
            *entry.key()
        };
        self.files.get_mut(&fh)
    }

    /// Returns the content size for the given inode from the first matching open handle.
    ///
    /// For `Complete` state, returns the content length. For `Streaming`, returns `total_size`.
    /// Returns `None` if no handle exists for this inode.
    pub fn get_content_size_by_ino(&self, ino: u64) -> Option<u64> {
        for entry in self.files.iter() {
            if entry.value().ino == ino {
                if let Some(ls) = entry.value().logical_size {
                    return Some(ls as u64);
                }
                return match &entry.value().content {
                    DownloadState::Complete(data) => Some(data.len() as u64),
                    DownloadState::Streaming { buffer, .. } => Some(buffer.total_size),
                };
            }
        }
        None
    }

    /// Marks all open handles for the given inode as stale.
    ///
    /// Called by the delta sync observer when remote content changes are detected.
    /// Active reads continue to serve the current content buffer without interruption.
    pub fn mark_stale_by_ino(&self, ino: u64) {
        let mut count = 0u32;
        for mut entry in self.files.iter_mut() {
            if entry.value().ino == ino {
                entry.value_mut().stale = true;
                count += 1;
            }
        }
        if count > 0 {
            tracing::debug!(ino, count, "marked open handles stale");
        }
    }

    /// Returns whether any open handle exists for the given inode.
    pub fn has_open_handles(&self, ino: u64) -> bool {
        self.files.iter().any(|e| e.value().ino == ino)
    }
}

/// Events emitted by VFS operations for the app layer to handle.
#[derive(Debug, Clone)]
pub enum VfsEvent {
    /// A conflict was detected and a conflict copy was uploaded.
    ConflictDetected {
        file_name: String,
        conflict_name: String,
    },
    /// A backend callback failed to persist file content to the writeback buffer.
    WritebackFailed { file_name: String },
    /// An upload failed (generic — not conflict or lock-specific).
    UploadFailed { file_name: String, reason: String },
    /// The file is locked on OneDrive (co-authoring or checkout).
    FileLocked { file_name: String },
}

/// Check if a filename matches known transient file patterns that should not
/// be uploaded to the server.
///
/// These files are meaningful only locally (Office lock files, Windows/macOS
/// system metadata, etc.). The check is a pure function of the filename.
pub fn is_transient_file(name: &str) -> bool {
    // Office lock files: ~$Book1.xlsx, ~$Report.docx
    if name.starts_with("~$") {
        return true;
    }
    // Office temp files: ~WRS0001.tmp, ~DF1234.tmp
    if name.starts_with('~') && name.to_ascii_lowercase().ends_with(".tmp") {
        return true;
    }
    // Windows/macOS system files (case-insensitive for cross-platform compat)
    let lower = name.to_ascii_lowercase();
    matches!(lower.as_str(), "thumbs.db" | "desktop.ini" | ".ds_store")
}

/// Generate a conflict filename that preserves the original extension.
///
/// `report.docx` → `report.conflict.1741...docx`
/// `notes` → `notes.conflict.1741...`
pub fn conflict_name(original: &str, timestamp: i64) -> String {
    match original.rfind('.') {
        Some(pos) => {
            let (stem, ext) = original.split_at(pos);
            format!("{stem}.conflict.{timestamp}{ext}")
        }
        None => format!("{original}.conflict.{timestamp}"),
    }
}

/// Errors from core VFS operations.
///
/// Each platform backend maps these to its own error type
/// (e.g., `fuser::Errno` for FUSE, `NTSTATUS` for WinFsp).
#[derive(Debug)]
pub enum VfsError {
    /// Item not found (FUSE: ENOENT, Windows: STATUS_OBJECT_NAME_NOT_FOUND)
    NotFound,
    /// Target is not a directory (FUSE: ENOTDIR)
    NotADirectory,
    /// Directory is not empty (FUSE: ENOTEMPTY)
    DirectoryNotEmpty,
    /// Permission denied (FUSE: EACCES)
    PermissionDenied,
    /// Operation timed out (FUSE: ETIMEDOUT)
    TimedOut,
    /// Storage quota exceeded (FUSE: ENOSPC)
    QuotaExceeded,
    /// I/O or network operation failed (FUSE: EIO, Windows: STATUS_DEVICE_NOT_READY)
    IoError(String),
}

impl VfsError {
    /// Map a `cloudmount_core::Error` to a specific `VfsError` variant.
    pub fn from_core_error(e: cloudmount_core::Error) -> Self {
        match &e {
            cloudmount_core::Error::GraphApi { status, message } => {
                if message.to_lowercase().contains("quota") {
                    return VfsError::QuotaExceeded;
                }
                match *status {
                    403 => VfsError::PermissionDenied,
                    404 => VfsError::NotFound,
                    507 => VfsError::QuotaExceeded,
                    _ => VfsError::IoError(e.to_string()),
                }
            }
            cloudmount_core::Error::Network(_) => VfsError::TimedOut,
            _ => VfsError::IoError(e.to_string()),
        }
    }
}

pub type VfsResult<T> = std::result::Result<T, VfsError>;

/// Ensure an open file's download is complete, transitioning Streaming → Complete.
fn ensure_complete(entry: &mut OpenFile, rt: &Handle) -> VfsResult<()> {
    match &entry.content {
        DownloadState::Complete(_) => Ok(()),
        DownloadState::Streaming { buffer, .. } => {
            buffer.wait_for_range(0, buffer.total_size, rt)?;
            let data = rt.block_on(buffer.read_range(0, buffer.total_size as usize));
            let old = std::mem::replace(&mut entry.content, DownloadState::Complete(data));
            if let DownloadState::Streaming { task, .. } = old {
                task.abort();
            }
            Ok(())
        }
    }
}

const QUOTA_CACHE_TTL_SECS: u64 = 60;

/// Core VFS operations shared between platform backends.
///
/// Encapsulates cache lookups, Graph API calls, inode management, and write-back logic.
/// Each platform backend holds a `CoreOps` instance and delegates business logic to it,
/// keeping only platform-specific callback translation in the backend layer.
/// Callback to invalidate kernel-cached attributes for an inode.
///
/// Set by the platform backend (e.g., FUSE `inval_inode`) to force the kernel
/// to discard its cached `i_size` / `mtime` when a metadata mismatch is detected
/// at open time. Without this, `FUSE_WRITEBACK_CACHE` keeps stale values.
pub type InodeInvalidator = Arc<dyn Fn(u64) + Send + Sync>;

pub struct CoreOps {
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,
    inodes: Arc<InodeTable>,
    drive_id: String,
    rt: Handle,
    open_files: Arc<OpenFileTable>,
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
    sync_handle: Option<crate::sync_processor::SyncHandle>,
    quota_cache: std::sync::Mutex<Option<(Instant, DriveQuota)>>,
    inode_invalidator: Option<InodeInvalidator>,
}

impl CoreOps {
    pub fn new(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        rt: Handle,
    ) -> Self {
        Self {
            graph,
            cache,
            inodes,
            drive_id,
            rt,
            open_files: Arc::new(OpenFileTable::new()),
            event_tx: None,
            sync_handle: None,
            quota_cache: std::sync::Mutex::new(None),
            inode_invalidator: None,
        }
    }

    pub fn with_event_sender(mut self, tx: tokio::sync::mpsc::UnboundedSender<VfsEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    pub fn with_sync_handle(mut self, handle: crate::sync_processor::SyncHandle) -> Self {
        self.sync_handle = Some(handle);
        self
    }

    pub fn with_inode_invalidator(mut self, f: InodeInvalidator) -> Self {
        self.inode_invalidator = Some(f);
        self
    }

    pub fn send_event(&self, event: VfsEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event);
        }
    }

    /// Get the drive quota, using a cached value if fresh enough.
    pub fn get_quota(&self) -> Option<DriveQuota> {
        {
            let cache = self.quota_cache.lock().unwrap();
            if let Some((fetched_at, ref quota)) = *cache
                && fetched_at.elapsed().as_secs() < QUOTA_CACHE_TTL_SECS
            {
                return Some(quota.clone());
            }
        }
        match self.rt.block_on(self.graph.get_drive(&self.drive_id)) {
            Ok(drive) => {
                if let Some(quota) = drive.quota {
                    let mut cache = self.quota_cache.lock().unwrap();
                    *cache = Some((Instant::now(), quota.clone()));
                    Some(quota)
                } else {
                    None
                }
            }
            Err(e) => {
                tracing::warn!("quota fetch failed: {e}");
                None
            }
        }
    }

    /// Check if a remote item has been modified on the server compared to our cache.
    fn has_server_conflict(&self, item: &DriveItem) -> bool {
        let Some(cached_etag) = item.etag.as_deref() else {
            return false;
        };
        match self
            .rt
            .block_on(self.graph.get_item(&self.drive_id, &item.id))
        {
            Ok(server_item) => server_item.etag.as_deref() != Some(cached_etag),
            Err(_) => false,
        }
    }

    pub fn graph(&self) -> &Arc<GraphClient> {
        &self.graph
    }

    pub fn cache(&self) -> &Arc<CacheManager> {
        &self.cache
    }

    pub fn inodes(&self) -> &Arc<InodeTable> {
        &self.inodes
    }

    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    /// Returns the shared open file table for use by the delta sync observer.
    pub fn open_files(&self) -> &Arc<OpenFileTable> {
        &self.open_files
    }

    pub fn mark_dirty(&self, ino: u64) {
        self.cache.dirty_inodes.insert(ino);
    }

    pub fn is_dirty(&self, ino: u64) -> bool {
        self.cache.dirty_inodes.contains(&ino)
    }

    pub fn clear_dirty(&self, ino: u64) {
        self.cache.dirty_inodes.remove(&ino);
    }

    pub fn rt(&self) -> &Handle {
        &self.rt
    }

    pub fn resolve_path(&self, components: &[impl AsRef<OsStr>]) -> Option<(u64, DriveItem)> {
        if components.is_empty() {
            let item = self.lookup_item(crate::inode::ROOT_INODE)?;
            return Some((crate::inode::ROOT_INODE, item));
        }

        let mut current_ino = crate::inode::ROOT_INODE;
        for component in components {
            let (child_ino, _) = self.find_child(current_ino, component.as_ref())?;
            current_ino = child_ino;
        }

        let item = self.lookup_item(current_ino)?;
        Some((current_ino, item))
    }

    /// Look up a [`DriveItem`] by inode from cache (memory → SQLite).
    /// Does NOT fall back to the Graph API.
    pub fn lookup_item(&self, inode: u64) -> Option<DriveItem> {
        if let Some(item) = self.cache.memory.get(inode) {
            return Some(item);
        }

        let item_id = self.inodes.get_item_id(inode)?;
        if let Ok(Some((_, item))) = self.cache.sqlite.get_item_by_id(&item_id) {
            self.cache.memory.insert(inode, item.clone());
            return Some(item);
        }

        None
    }

    /// Look up a [`DriveItem`] for `getattr`, returning handle-consistent size.
    ///
    /// When an open file handle exists for the inode, clones the `DriveItem` from
    /// cache but overrides `size` with the handle's content size. Returns
    /// `(item, has_open_handle)` — when `has_open_handle` is `true`, the caller
    /// should use a TTL of 0 to ensure the kernel re-queries on every `stat()`.
    pub fn lookup_item_for_getattr(&self, ino: u64) -> Option<(DriveItem, bool)> {
        let mut item = self.lookup_item(ino)?;

        if let Some(handle_size) = self.open_files.get_content_size_by_ino(ino) {
            tracing::debug!(
                ino,
                handle_size,
                cache_size = item.size,
                "getattr: returning handle size instead of cache size"
            );
            item.size = handle_size as i64;
            Some((item, true))
        } else {
            Some((item, false))
        }
    }

    /// Find a child item by name under a given parent inode.
    /// Searches memory cache, SQLite, then falls back to Graph API.
    /// On Graph API fallback, also populates the parent's children list in memory cache.
    ///
    /// Accepts `&OsStr` for lossless path handling on Windows (NTFS filenames may contain
    /// unpaired UTF-16 surrogates). Returns `None` if the name cannot be converted to UTF-8
    /// (Graph API stores only valid Unicode names, so no match can exist).
    pub fn find_child(&self, parent_ino: u64, name: &OsStr) -> Option<(u64, DriveItem)> {
        let name = name.to_str()?;

        if let Some(children_map) = self.cache.memory.get_children(parent_ino) {
            // On Windows, NTFS uses case-insensitive names so we must iterate.
            // On Linux/macOS, exact HashMap::get is sufficient.
            #[cfg(not(target_os = "windows"))]
            let child_ino = children_map.get(name).copied();
            #[cfg(target_os = "windows")]
            let child_ino = children_map
                .iter()
                .find(|(k, _)| names_match(k, name))
                .map(|(_, &v)| v);

            if let Some(child_inode) = child_ino
                && let Some(item) = self.lookup_item(child_inode)
            {
                return Some((child_inode, item));
            }
        }

        match self.cache.sqlite.get_children(parent_ino) {
            Ok(children) => {
                for (_, item) in children {
                    if names_match(&item.name, name) {
                        let resolved_ino = self.inodes.allocate(&item.id);
                        self.cache.memory.insert(resolved_ino, item.clone());
                        return Some((resolved_ino, item));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(parent_ino, name, "find_child sqlite lookup failed: {e}");
            }
        }

        let parent_item_id = self.inodes.get_item_id(parent_ino)?;
        match self
            .rt
            .block_on(self.graph.list_children(&self.drive_id, &parent_item_id))
        {
            Ok(children) => {
                let mut children_map = std::collections::HashMap::new();
                let mut found = None;

                for item in &children {
                    let child_inode = self.inodes.allocate(&item.id);
                    children_map.insert(item.name.clone(), child_inode);
                    self.cache.memory.insert(child_inode, item.clone());
                    if names_match(&item.name, name) && found.is_none() {
                        found = Some((child_inode, item.clone()));
                    }
                }

                if let Some(parent_item) = self.lookup_item(parent_ino) {
                    self.cache
                        .memory
                        .insert_with_children(parent_ino, parent_item, children_map);
                }

                return found;
            }
            Err(e) => {
                tracing::warn!(parent_ino, name, "find_child graph fallback failed: {e}");
            }
        }
        None
    }

    /// List all children of a directory, populating caches along the way.
    /// Checks memory cache → SQLite → Graph API in order.
    pub fn list_children(&self, parent_ino: u64) -> Vec<(u64, DriveItem)> {
        if let Some(children_map) = self.cache.memory.get_children(parent_ino) {
            let result: Vec<_> = children_map
                .values()
                .filter_map(|&ino| self.lookup_item(ino).map(|item| (ino, item)))
                .collect();
            return result;
        }

        match self.cache.sqlite.get_children(parent_ino) {
            Ok(children) if !children.is_empty() => {
                return children
                    .into_iter()
                    .map(|(_, item)| {
                        let resolved_ino = self.inodes.allocate(&item.id);
                        self.cache.memory.insert(resolved_ino, item.clone());
                        (resolved_ino, item)
                    })
                    .collect();
            }
            Ok(_) => {
                tracing::debug!(parent_ino, "list_children: no children in sqlite");
            }
            Err(e) => {
                tracing::warn!(parent_ino, "list_children sqlite lookup failed: {e}");
            }
        }

        let Some(item_id) = self.inodes.get_item_id(parent_ino) else {
            tracing::warn!(parent_ino, "list_children: no item_id for inode");
            return Vec::new();
        };
        match self
            .rt
            .block_on(self.graph.list_children(&self.drive_id, &item_id))
        {
            Ok(items) => {
                let mut children_map = std::collections::HashMap::new();
                let result: Vec<_> = items
                    .into_iter()
                    .map(|item| {
                        let ino = self.inodes.allocate(&item.id);
                        children_map.insert(item.name.clone(), ino);
                        self.cache.memory.insert(ino, item.clone());
                        (ino, item)
                    })
                    .collect();

                if let Some(parent_item) = self.lookup_item(parent_ino) {
                    self.cache
                        .memory
                        .insert_with_children(parent_ino, parent_item, children_map);
                }

                result
            }
            Err(e) => {
                tracing::error!(parent_ino, %item_id, "list_children graph fallback failed: {e}");
                Vec::new()
            }
        }
    }

    /// Read file content from disk cache or download from Graph API.
    pub fn read_content(&self, ino: u64) -> VfsResult<Vec<u8>> {
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;
        let item = self.lookup_item(ino);

        // Check writeback buffer first (pending local writes)
        if let Some(content) = self
            .rt
            .block_on(self.cache.writeback.read(&self.drive_id, &item_id))
        {
            return Ok(content);
        }

        // Check disk cache with freshness validation
        if !self.cache.dirty_inodes.contains(&ino)
            && let Some((content, disk_etag)) = self
                .rt
                .block_on(self.cache.disk.get_with_etag(&self.drive_id, &item_id))
        {
            let size_ok = item
                .as_ref()
                .map(|i| content.len() == i.size as usize)
                .unwrap_or(false);
            let etag_ok = match (&disk_etag, item.as_ref().and_then(|i| i.etag.as_ref())) {
                (Some(de), Some(ie)) => de == ie,
                _ => false,
            };
            if size_ok && etag_ok {
                return Ok(content);
            }
            let _ = self
                .rt
                .block_on(self.cache.disk.remove(&self.drive_id, &item_id));
        }

        let item_etag = item.as_ref().and_then(|i| i.etag.clone());
        match self
            .rt
            .block_on(self.graph.download_content(&self.drive_id, &item_id))
        {
            Ok(content) => {
                self.cache.dirty_inodes.remove(&ino);
                let _ = self.rt.block_on(self.cache.disk.put(
                    &self.drive_id,
                    &item_id,
                    &content,
                    item_etag.as_deref(),
                ));
                Ok(content.to_vec())
            }
            Err(e) => {
                tracing::error!("download failed for {item_id}: {e}");
                Err(VfsError::IoError(format!("download failed: {e}")))
            }
        }
    }

    /// Truncate or extend a file to the given size.
    /// If the file has an open handle, resizes that buffer directly.
    pub fn truncate(&self, ino: u64, new_size: u64) -> VfsResult<()> {
        let new_size = new_size as usize;

        // If the file is open, operate on the open file buffer directly
        if let Some(mut entry) = self.open_files.find_by_ino(ino) {
            ensure_complete(&mut entry, &self.rt)?;
            let buf_before = entry.content.as_complete().unwrap().len();
            let buf = entry.content.as_complete_mut().unwrap();
            buf.resize(new_size, 0);
            entry.dirty = true;
            entry.logical_size = Some(new_size);
            tracing::debug!(
                "[DIAG:truncate] ino={ino} buf {buf_before}->{new_size} logical_size=Some({new_size})"
            );
            drop(entry);
        } else {
            // Fallback: truncate via writeback buffer
            let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;

            let mut content = if new_size == 0 {
                Vec::new()
            } else {
                self.rt
                    .block_on(self.cache.writeback.read(&self.drive_id, &item_id))
                    .or_else(|| {
                        self.rt
                            .block_on(self.cache.disk.get(&self.drive_id, &item_id))
                    })
                    .unwrap_or_default()
            };

            content.resize(new_size, 0);

            self.rt
                .block_on(
                    self.cache
                        .writeback
                        .write(&self.drive_id, &item_id, &content),
                )
                .map_err(|e| VfsError::IoError(format!("truncate writeback failed: {e}")))?;
        }

        if let Some(mut item) = self.lookup_item(ino) {
            item.size = new_size as i64;
            self.cache.memory.insert(ino, item);
        }

        Ok(())
    }

    /// Write data to the writeback buffer at the given offset, returning bytes written.
    pub fn write_to_buffer(&self, ino: u64, offset: usize, data: &[u8]) -> VfsResult<u32> {
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;

        let existing = self
            .rt
            .block_on(self.cache.writeback.read(&self.drive_id, &item_id))
            .or_else(|| {
                self.rt
                    .block_on(self.cache.disk.get(&self.drive_id, &item_id))
            })
            .unwrap_or_default();

        let needed = offset + data.len();
        let mut buffer = existing;
        if buffer.len() < needed {
            buffer.resize(needed, 0);
        }
        buffer[offset..offset + data.len()].copy_from_slice(data);

        self.rt
            .block_on(
                self.cache
                    .writeback
                    .write(&self.drive_id, &item_id, &buffer),
            )
            .map_err(|e| VfsError::IoError(format!("write buffer failed: {e}")))?;

        Ok(data.len() as u32)
    }

    /// Upload pending writes for an inode with conflict detection.
    ///
    /// Delegates to the shared `flush_inode_async` free function via `block_on`.
    /// Both this method (sync fallback path) and the `SyncProcessor` use the same
    /// underlying upload logic.
    pub fn flush_inode(&self, ino: u64) -> VfsResult<()> {
        let success = self.rt.block_on(crate::sync_processor::flush_inode_async(
            ino,
            &self.graph,
            &self.cache,
            &self.inodes,
            &self.drive_id,
            self.event_tx.as_ref(),
        ));
        if success {
            Ok(())
        } else {
            Err(VfsError::IoError("flush_inode upload failed".to_string()))
        }
    }

    /// Open a file, loading its content into the open file table.
    /// Small files (< 4 MB) and cached files load eagerly.
    /// Large uncached files return immediately with a background streaming download.
    /// Validates disk cache freshness via dirty-inode set, eTag, and size checks.
    pub fn open_file(&self, ino: u64) -> VfsResult<u64> {
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;

        // Local files haven't been uploaded yet — Graph API would reject any download
        // attempt with 400. If another handle is already open (e.g. LibreOffice opens
        // the same file twice during its save flow), clone its in-memory content;
        // otherwise return an empty buffer.
        if item_id.starts_with("local:") {
            let content = self
                .open_files
                .find_by_ino(ino)
                .and_then(|e| e.content.as_complete().cloned())
                .or_else(|| {
                    self.rt
                        .block_on(self.cache.writeback.read(&self.drive_id, &item_id))
                })
                .unwrap_or_default();
            return Ok(self
                .open_files
                .insert(ino, DownloadState::Complete(content)));
        }

        let item = self.lookup_item(ino);

        // Check writeback buffer first (pending local writes)
        if let Some(content) = self
            .rt
            .block_on(self.cache.writeback.read(&self.drive_id, &item_id))
        {
            return Ok(self
                .open_files
                .insert(ino, DownloadState::Complete(content)));
        }

        // Refresh metadata from the server BEFORE checking the disk cache.
        // With FUSE_WRITEBACK_CACHE the kernel ignores size/mtime updates from
        // getattr, so a stale cached size causes reads to be truncated.
        // Without this, the disk cache validation compares against memory-cached
        // metadata which may also be stale — both have the old eTag, so the
        // stale content passes validation and is served as-is (corruption).
        let item = match self
            .rt
            .block_on(self.graph.get_item(&self.drive_id, &item_id))
        {
            Ok(fresh) => {
                // Check if file is locked (co-authoring, checkout)
                if fresh.is_locked() {
                    let file_name = fresh.name.clone();
                    self.send_event(VfsEvent::FileLocked { file_name });
                }

                let stale = item.as_ref().and_then(|i| i.etag.as_ref()) != fresh.etag.as_ref();
                if stale {
                    tracing::debug!(
                        ino,
                        old_size = item.as_ref().map(|i| i.size),
                        new_size = fresh.size,
                        "open_file: server metadata differs, refreshing caches"
                    );
                    self.cache.memory.insert(ino, fresh.clone());
                    let parent_ino = fresh
                        .parent_reference
                        .as_ref()
                        .and_then(|pr| pr.id.as_deref())
                        .map(|pid| self.inodes.allocate(pid));
                    let _ = self
                        .cache
                        .sqlite
                        .upsert_item(ino, &self.drive_id, &fresh, parent_ino);
                    if let Some(ref invalidator) = self.inode_invalidator {
                        invalidator(ino);
                    }
                }
                Some(fresh)
            }
            Err(e) => {
                tracing::warn!(
                    ino,
                    "open_file: get_item refresh failed: {e}, using cached metadata"
                );
                item
            }
        };

        // Check disk cache with freshness validation (now against fresh server metadata)
        if !self.cache.dirty_inodes.contains(&ino)
            && let Some((content, disk_etag)) = self
                .rt
                .block_on(self.cache.disk.get_with_etag(&self.drive_id, &item_id))
        {
            // Validate: size must match metadata
            let size_ok = item
                .as_ref()
                .map(|i| content.len() == i.size as usize)
                .unwrap_or(false);
            // Validate: eTag must match metadata (if both present)
            let etag_ok = match (&disk_etag, item.as_ref().and_then(|i| i.etag.as_ref())) {
                (Some(de), Some(ie)) => de == ie,
                _ => false,
            };
            if size_ok && etag_ok {
                return Ok(self
                    .open_files
                    .insert(ino, DownloadState::Complete(content)));
            }
            // Stale — remove and fall through to download
            let _ = self
                .rt
                .block_on(self.cache.disk.remove(&self.drive_id, &item_id));
        }

        // Not cached or stale — check file size for streaming decision
        let file_size = item.as_ref().map(|i| i.size).unwrap_or(0) as usize;
        let item_etag = item.as_ref().and_then(|i| i.etag.clone());

        if file_size < SMALL_FILE_LIMIT {
            // Small file: download fully (read_content handles dirty-inode + freshness)
            let content = self.read_content(ino)?;
            Ok(self
                .open_files
                .insert(ino, DownloadState::Complete(content)))
        } else {
            // Large file: stream in background
            let buffer = Arc::new(StreamingBuffer::new(file_size as u64)?);
            let buf_clone = buffer.clone();
            let graph = self.graph.clone();
            let cache = self.cache.clone();
            let drive_id = self.drive_id.clone();
            let item_id_clone = item_id.clone();
            let dirty_ino = ino;

            let task = self.rt.spawn(async move {
                match graph.download_streaming(&drive_id, &item_id_clone).await {
                    Ok(mut stream) => {
                        while let Some(chunk_result) = stream.next().await {
                            match chunk_result {
                                Ok(chunk) => buf_clone.append_chunk(&chunk).await,
                                Err(e) => {
                                    buf_clone.mark_failed(e.to_string());
                                    return;
                                }
                            }
                        }
                        buf_clone.mark_done();
                        // Populate disk cache with completed download and eTag
                        let data = buf_clone.read_range(0, buf_clone.total_size as usize).await;
                        let _ = cache
                            .disk
                            .put(&drive_id, &item_id_clone, &data, item_etag.as_deref())
                            .await;
                        cache.dirty_inodes.remove(&dirty_ino);
                    }
                    Err(e) => {
                        buf_clone.mark_failed(e.to_string());
                    }
                }
            });

            let fh = self.open_files.insert(
                ino,
                DownloadState::Streaming {
                    buffer,
                    task: task.abort_handle(),
                },
            );
            Ok(fh)
        }
    }

    /// Release a file handle, flushing dirty content to writeback if needed.
    /// Cancels any in-progress streaming download.
    pub fn release_file(&self, fh: u64) -> VfsResult<()> {
        let open_file = match self.open_files.remove(fh) {
            Some(f) => f,
            None => return Ok(()),
        };
        // Cancel any in-progress streaming download
        if let DownloadState::Streaming { task, .. } = &open_file.content {
            task.abort();
        }
        if open_file.dirty {
            let item_id = self
                .inodes
                .get_item_id(open_file.ino)
                .ok_or(VfsError::NotFound)?;
            let mut content = open_file
                .content
                .as_complete()
                .ok_or_else(|| VfsError::IoError("dirty file in non-complete state".to_string()))?
                .clone();
            // Apply logical_size truncation to match flush_handle behaviour —
            // otherwise a failed flush followed by release would overwrite the
            // correctly-truncated writeback content with the full raw buffer.
            if let Some(size) = open_file.logical_size {
                content.truncate(size);
            }
            self.rt
                .block_on(
                    self.cache
                        .writeback
                        .write(&self.drive_id, &item_id, &content),
                )
                .map_err(|e| VfsError::IoError(format!("release writeback failed: {e}")))?;
        }
        Ok(())
    }

    /// Re-downloads content for a stale open file handle.
    /// Skips refresh for dirty handles (local writes take precedence — conflict
    /// detection in flush_handle will resolve divergence).
    fn refresh_stale_handle(&self, fh: u64, ino: u64) -> VfsResult<()> {
        let new_content = self.read_content(ino)?;
        let mut entry = self.open_files.get_mut(fh).ok_or(VfsError::NotFound)?;
        if entry.stale {
            entry.content = DownloadState::Complete(new_content);
            entry.stale = false;
            tracing::debug!(fh, ino, "refreshed stale handle with new content");
        }
        Ok(())
    }

    /// Read bytes from an open file handle's buffer.
    /// For streaming downloads, blocks until the requested range is available
    /// or issues an on-demand range request for random access.
    pub fn read_handle(&self, fh: u64, offset: usize, size: usize) -> VfsResult<Vec<u8>> {
        // Refresh stale handles (remote content changed while handle is open)
        {
            let entry = self.open_files.get(fh).ok_or(VfsError::NotFound)?;
            if entry.stale && !entry.dirty {
                let ino = entry.ino;
                drop(entry);
                self.refresh_stale_handle(fh, ino)?;
            }
        }

        let entry = self.open_files.get(fh).ok_or(VfsError::NotFound)?;
        match &entry.content {
            DownloadState::Complete(content) => {
                if offset >= content.len() {
                    return Ok(Vec::new());
                }
                let end = std::cmp::min(offset + size, content.len());
                Ok(content[offset..end].to_vec())
            }
            DownloadState::Streaming { buffer, .. } => {
                let downloaded = buffer.downloaded_bytes();
                let needed_end = offset as u64 + size as u64;
                let ino = entry.ino;

                if downloaded >= needed_end {
                    // Data already available
                    let data = self.rt.block_on(buffer.read_range(offset, size));
                    Ok(data)
                } else if (offset as u64) <= downloaded + RANDOM_ACCESS_THRESHOLD {
                    // Near the download frontier — wait for sequential download
                    buffer.wait_for_range(offset as u64, size as u64, &self.rt)?;
                    let data = self.rt.block_on(buffer.read_range(offset, size));
                    Ok(data)
                } else {
                    // Random access — issue on-demand range request
                    let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;
                    drop(entry);
                    let bytes = self
                        .rt
                        .block_on(self.graph.download_range(
                            &self.drive_id,
                            &item_id,
                            offset as u64,
                            size as u64,
                        ))
                        .map_err(|e| VfsError::IoError(format!("range download failed: {e}")))?;
                    Ok(bytes.to_vec())
                }
            }
        }
    }

    /// Write data into an open file handle's buffer in-place.
    /// If the file is still streaming, blocks until download completes first.
    pub fn write_handle(&self, fh: u64, offset: usize, data: &[u8]) -> VfsResult<u32> {
        let mut entry = self.open_files.get_mut(fh).ok_or(VfsError::NotFound)?;
        ensure_complete(&mut entry, &self.rt)?;
        let write_end = offset + data.len();
        let buf_before = entry.content.as_complete().unwrap().len();
        {
            let buf = entry.content.as_complete_mut().unwrap();
            if buf.len() < write_end {
                buf.resize(write_end, 0);
            }
            buf[offset..write_end].copy_from_slice(data);
        }
        entry.dirty = true;

        // Update logical_size: if set (post-truncate), expand to cover the write;
        // otherwise leave None so flush uses the full buffer length.
        let reported_size = if let Some(ls) = entry.logical_size {
            let new_ls = ls.max(write_end);
            entry.logical_size = Some(new_ls);
            new_ls as i64
        } else {
            entry.content.as_complete().unwrap().len() as i64
        };

        let ino = entry.ino;
        let buf_after = entry.content.as_complete().unwrap().len();
        let ls = entry.logical_size;
        drop(entry);
        tracing::debug!(
            "[DIAG:write_handle] ino={ino} fh={fh} offset={offset} data_len={} buf {buf_before}->{buf_after} logical_size={ls:?} reported={reported_size}",
            data.len()
        );

        if let Some(mut item) = self.lookup_item(ino) {
            item.size = reported_size;
            self.cache.memory.insert(ino, item);
        }

        Ok(data.len() as u32)
    }

    /// Flush an open file handle: push dirty content to writeback and upload.
    /// If streaming, waits for download to complete first.
    ///
    /// When `wait_for_completion` is true and a sync processor is available,
    /// sends a `FlushSync` request and blocks until the upload completes
    /// (with a 60-second timeout). This is used by WinFsp where the OS
    /// expects flush to guarantee data is persisted.
    pub fn flush_handle(&self, fh: u64, wait_for_completion: bool) -> VfsResult<()> {
        // Check dirty flag; if streaming, wait and transition to Complete
        {
            let mut entry = self.open_files.get_mut(fh).ok_or(VfsError::NotFound)?;
            if !entry.dirty {
                tracing::debug!("[DIAG:flush_handle] fh={fh} not dirty, skip");
                return Ok(());
            }
            ensure_complete(&mut entry, &self.rt)?;
        }

        let entry = self.open_files.get(fh).ok_or(VfsError::NotFound)?;
        let ino = entry.ino;
        let logical_size = entry.logical_size;
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;
        let mut content = entry.content.as_complete().unwrap().clone();
        let buf_len = content.len();
        drop(entry);

        if let Some(size) = logical_size {
            content.truncate(size);
        }
        tracing::debug!(
            "[DIAG:flush_handle] fh={fh} ino={ino} item_id={item_id} buf_len={buf_len} logical_size={logical_size:?} upload_len={}",
            content.len()
        );

        self.rt
            .block_on(
                self.cache
                    .writeback
                    .write(&self.drive_id, &item_id, &content),
            )
            .map_err(|e| VfsError::IoError(format!("flush writeback failed: {e}")))?;

        if let Some(ref sync_handle) = self.sync_handle {
            if wait_for_completion {
                // Synchronous flush: block until the upload completes or times out.
                let (tx, rx) = tokio::sync::oneshot::channel();
                sync_handle.send(crate::sync_processor::SyncRequest::FlushSync { ino, done: tx });
                match self.rt.block_on(async {
                    tokio::time::timeout(std::time::Duration::from_secs(60), rx).await
                }) {
                    Ok(Ok(true)) => {} // success
                    Ok(Ok(false)) => {
                        return Err(VfsError::IoError("flush upload failed".to_string()));
                    }
                    Ok(Err(_)) => {
                        return Err(VfsError::IoError("sync processor closed".to_string()));
                    }
                    Err(_) => return Err(VfsError::TimedOut),
                }
            } else {
                // Fire-and-forget: delegate upload to the async sync processor
                sync_handle.send(crate::sync_processor::SyncRequest::Flush { ino });
            }
        } else {
            // Fallback: synchronous inline upload (tests or processor disabled)
            self.flush_inode(ino)?;
        }

        if let Some(mut entry) = self.open_files.get_mut(fh) {
            entry.dirty = false;
        }

        Ok(())
    }

    /// Create a new file with a temporary `local:{nanos}` ID, reassigned on flush.
    /// Returns `(file_handle, inode, DriveItem)`.
    pub fn create_file(&self, parent_ino: u64, name: &str) -> VfsResult<(u64, u64, DriveItem)> {
        let parent_item_id = self
            .inodes
            .get_item_id(parent_ino)
            .ok_or(VfsError::NotFound)?;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_item_id = format!("local:{nanos}");

        let now = Utc::now();
        let item = DriveItem {
            id: temp_item_id.clone(),
            name: name.to_string(),
            size: 0,
            last_modified: Some(now),
            created: Some(now),
            etag: None,
            parent_reference: Some(ParentReference {
                drive_id: Some(self.drive_id.clone()),
                id: Some(parent_item_id),
                path: None,
            }),
            folder: None,
            file: Some(FileFacet {
                mime_type: None,
                hashes: None,
            }),
            publication: None,
            download_url: None,
            web_url: None,
        };

        let inode = self.inodes.allocate(&temp_item_id);

        self.cache.memory.insert(inode, item.clone());
        self.cache.memory.add_child(parent_ino, name, inode);

        let fh = self
            .open_files
            .insert(inode, DownloadState::Complete(Vec::new()));

        Ok((fh, inode, item))
    }

    /// Register a locally-created file in VFS metadata so it can be flushed via
    /// the existing writeback/upload pipeline.
    pub fn register_local_file(
        &self,
        parent_ino: u64,
        name: &str,
        file_size: u64,
        modified: Option<chrono::DateTime<chrono::Utc>>,
    ) -> VfsResult<(u64, DriveItem)> {
        if let Some((child_ino, child_item)) = self.find_child(parent_ino, OsStr::new(name)) {
            return Ok((child_ino, child_item));
        }

        let parent_item_id = self
            .inodes
            .get_item_id(parent_ino)
            .ok_or(VfsError::NotFound)?;

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_item_id = format!("local:{nanos}");
        let now = Utc::now();

        let item = DriveItem {
            id: temp_item_id.clone(),
            name: name.to_string(),
            size: file_size as i64,
            last_modified: modified.or(Some(now)),
            created: Some(now),
            etag: None,
            parent_reference: Some(ParentReference {
                drive_id: Some(self.drive_id.clone()),
                id: Some(parent_item_id),
                path: None,
            }),
            folder: None,
            file: Some(FileFacet {
                mime_type: None,
                hashes: None,
            }),
            publication: None,
            download_url: None,
            web_url: None,
        };

        let inode = self.inodes.allocate(&temp_item_id);
        self.cache.memory.insert(inode, item.clone());
        self.cache.memory.add_child(parent_ino, name, inode);

        if let Err(e) =
            self.cache
                .sqlite
                .upsert_item(inode, &self.drive_id, &item, Some(parent_ino))
        {
            tracing::warn!("register_local_file: sqlite upsert failed: {e}");
        }

        Ok((inode, item))
    }

    pub fn mkdir(&self, parent_ino: u64, name: &str) -> VfsResult<(u64, DriveItem)> {
        let parent_item_id = self
            .inodes
            .get_item_id(parent_ino)
            .ok_or(VfsError::NotFound)?;

        let folder_item = self
            .rt
            .block_on(
                self.graph
                    .create_folder(&self.drive_id, &parent_item_id, name),
            )
            .map_err(|e| VfsError::IoError(format!("mkdir failed: {e}")))?;

        let inode = self.inodes.allocate(&folder_item.id);
        self.cache.memory.insert(inode, folder_item.clone());
        self.cache.memory.add_child(parent_ino, name, inode);

        Ok((inode, folder_item))
    }

    pub fn unlink(&self, parent_ino: u64, name: &str) -> VfsResult<()> {
        let (child_ino, child_item) = self
            .find_child(parent_ino, OsStr::new(name))
            .ok_or(VfsError::NotFound)?;
        let item_id = child_item.id.clone();

        if !item_id.starts_with("local:") {
            self.rt
                .block_on(self.graph.delete_item(&self.drive_id, &item_id))
                .map_err(|e| VfsError::IoError(format!("unlink failed: {e}")))?;
        }

        self.cache.memory.remove_child(parent_ino, name);
        self.cleanup_deleted_item(&item_id, child_ino);
        Ok(())
    }

    pub fn rmdir(&self, parent_ino: u64, name: &str) -> VfsResult<()> {
        let (child_ino, child_item) = self
            .find_child(parent_ino, OsStr::new(name))
            .ok_or(VfsError::NotFound)?;

        if !child_item.is_folder() {
            return Err(VfsError::NotADirectory);
        }

        let item_id = child_item.id.clone();

        if !item_id.starts_with("local:") {
            match self
                .rt
                .block_on(self.graph.list_children(&self.drive_id, &item_id))
            {
                Ok(children) if !children.is_empty() => {
                    return Err(VfsError::DirectoryNotEmpty);
                }
                Err(e) => {
                    return Err(VfsError::IoError(format!(
                        "rmdir list_children failed: {e}"
                    )));
                }
                _ => {}
            }

            self.rt
                .block_on(self.graph.delete_item(&self.drive_id, &item_id))
                .map_err(|e| VfsError::IoError(format!("rmdir delete failed: {e}")))?;
        } else if self
            .cache
            .memory
            .get_children(child_ino)
            .is_some_and(|children| !children.is_empty())
        {
            return Err(VfsError::DirectoryNotEmpty);
        }

        self.cache.memory.invalidate(child_ino);
        self.cache.memory.remove_child(parent_ino, name);
        self.inodes.remove_by_item_id(&item_id);
        let _ = self.cache.sqlite.delete_item(&item_id);

        Ok(())
    }

    pub fn rename(
        &self,
        parent_ino: u64,
        name: &str,
        new_parent_ino: u64,
        new_name: &str,
    ) -> VfsResult<()> {
        let (child_ino, child_item) = self
            .find_child(parent_ino, OsStr::new(name))
            .ok_or(VfsError::NotFound)?;
        let item_id = child_item.id.clone();

        let new_parent_item_id = if parent_ino == new_parent_ino {
            None
        } else {
            Some(
                self.inodes
                    .get_item_id(new_parent_ino)
                    .ok_or(VfsError::NotFound)?,
            )
        };

        // POSIX rename replaces destination if it exists
        if let Some((existing_ino, existing_item)) =
            self.find_child(new_parent_ino, OsStr::new(new_name))
            && existing_item.id != item_id
        {
            // Before deleting a remote file, check if it has different server content
            // (different eTag or pending writes). Save as conflict copy if so.
            if !existing_item.id.starts_with("local:")
                && !existing_item.is_folder()
                && (self.is_dirty(existing_ino) || self.has_server_conflict(&existing_item))
            {
                let timestamp = Utc::now().timestamp();
                let cname = conflict_name(&existing_item.name, timestamp);
                let parent_id = existing_item
                    .parent_reference
                    .as_ref()
                    .and_then(|p| p.id.as_deref())
                    .unwrap_or("");
                if !parent_id.is_empty() {
                    // Download current server content and upload as conflict copy
                    if let Ok(content) = self.rt.block_on(
                        self.graph
                            .download_content(&self.drive_id, &existing_item.id),
                    ) {
                        if let Err(e) = self.rt.block_on(self.graph.upload_small(
                            &self.drive_id,
                            parent_id,
                            &cname,
                            content,
                            None,
                        )) {
                            tracing::error!(
                                "conflict copy upload failed for '{}', aborting rename: {e}",
                                existing_item.name
                            );
                            return Err(VfsError::IoError(format!(
                                "conflict copy upload failed: {e}"
                            )));
                        }
                        self.send_event(VfsEvent::ConflictDetected {
                            file_name: existing_item.name.clone(),
                            conflict_name: cname,
                        });
                    }
                }
            }

            if !existing_item.id.starts_with("local:") {
                let _ = self
                    .rt
                    .block_on(self.graph.delete_item(&self.drive_id, &existing_item.id));
            }
            self.cache.memory.remove_child(new_parent_ino, new_name);
            self.cleanup_deleted_item(&existing_item.id, existing_ino);
        }

        if !item_id.starts_with("local:") {
            let updated_item = self
                .rt
                .block_on(self.graph.update_item(
                    &self.drive_id,
                    &item_id,
                    Some(new_name),
                    new_parent_item_id.as_deref(),
                ))
                .map_err(|e| VfsError::IoError(format!("rename failed: {e}")))?;

            if let Err(e) = self.cache.sqlite.upsert_item(
                child_ino,
                &self.drive_id,
                &updated_item,
                Some(new_parent_ino),
            ) {
                tracing::warn!("rename: sqlite upsert failed: {e}");
            }
            self.cache.memory.insert(child_ino, updated_item);
        } else {
            let mut updated = child_item.clone();
            updated.name = new_name.to_string();
            if let (Some(new_pid), Some(pref)) =
                (&new_parent_item_id, &mut updated.parent_reference)
            {
                pref.id = Some(new_pid.clone());
            }
            if let Err(e) = self.cache.sqlite.upsert_item(
                child_ino,
                &self.drive_id,
                &updated,
                Some(new_parent_ino),
            ) {
                tracing::warn!("rename: sqlite upsert failed: {e}");
            }
            self.cache.memory.insert(child_ino, updated);
        }

        self.cache.memory.remove_child(parent_ino, name);
        self.cache
            .memory
            .add_child(new_parent_ino, new_name, child_ino);

        Ok(())
    }

    #[allow(clippy::too_many_arguments)] // Mirrors FUSE copy_file_range signature
    pub fn copy_file_range(
        &self,
        ino_in: u64,
        fh_in: u64,
        offset_in: u64,
        ino_out: u64,
        fh_out: u64,
        offset_out: u64,
        len: u64,
    ) -> VfsResult<u32> {
        let src_item = self.lookup_item(ino_in).ok_or(VfsError::NotFound)?;
        let src_item_id = src_item.id.clone();
        let src_size = src_item.size as u64;

        let eligible = !src_item_id.starts_with("local:") && offset_in == 0 && len >= src_size;

        if eligible {
            match self.copy_file_range_server(ino_out, fh_out, &src_item) {
                Ok(n) => Ok(n),
                Err(VfsError::TimedOut) => {
                    tracing::info!("server-side copy timed out, falling back to read/write copy");
                    self.copy_file_range_fallback(
                        fh_in, offset_in, fh_out, offset_out, len, ino_out,
                    )
                }
                Err(e) => Err(e),
            }
        } else {
            self.copy_file_range_fallback(fh_in, offset_in, fh_out, offset_out, len, ino_out)
        }
    }

    fn copy_file_range_server(
        &self,
        ino_out: u64,
        fh_out: u64,
        src_item: &DriveItem,
    ) -> VfsResult<u32> {
        let src_size = src_item.size as u64;
        let src_drive_id = src_item
            .parent_reference
            .as_ref()
            .and_then(|p| p.drive_id.as_deref())
            .unwrap_or(&self.drive_id);
        let src_item_id = &src_item.id;

        let dst_item = self.lookup_item(ino_out).ok_or(VfsError::NotFound)?;
        let old_dst_id = dst_item.id.clone();
        let dst_parent_id = dst_item
            .parent_reference
            .as_ref()
            .and_then(|p| p.id.as_deref())
            .ok_or_else(|| VfsError::IoError("destination has no parent reference".into()))?
            .to_string();
        let dst_name = dst_item.name.clone();

        let monitor_url = self
            .rt
            .block_on(self.graph.copy_item(
                src_drive_id,
                src_item_id,
                &self.drive_id,
                &dst_parent_id,
                &dst_name,
            ))
            .map_err(|e| VfsError::IoError(format!("copy_item failed: {e}")))?;

        // Poll with exponential backoff
        let start = Instant::now();
        let mut delay_ms = COPY_POLL_INITIAL_MS;
        let max_duration = std::time::Duration::from_secs(COPY_MAX_POLL_DURATION_SECS);

        loop {
            self.rt
                .block_on(tokio::time::sleep(std::time::Duration::from_millis(
                    delay_ms,
                )));

            if start.elapsed() > max_duration {
                tracing::warn!("server-side copy timed out after {COPY_MAX_POLL_DURATION_SECS}s");
                return Err(VfsError::TimedOut);
            }

            let poll_result = {
                let mut retries = 0;
                loop {
                    match self.rt.block_on(self.graph.poll_copy_status(&monitor_url)) {
                        Ok(status) => break Ok(status),
                        Err(e) => {
                            retries += 1;
                            if retries > COPY_POLL_MAX_RETRIES {
                                break Err(e);
                            }
                            tracing::warn!(
                                "copy poll retry {retries}/{COPY_POLL_MAX_RETRIES}: {e}"
                            );
                            self.rt
                                .block_on(tokio::time::sleep(std::time::Duration::from_millis(
                                    delay_ms,
                                )));
                        }
                    }
                }
            };

            match poll_result {
                Ok(CopyStatus::Completed { resource_id }) => {
                    let new_item = self
                        .rt
                        .block_on(self.graph.get_item(&self.drive_id, &resource_id))
                        .map_err(|e| VfsError::IoError(format!("get copied item failed: {e}")))?;

                    self.inodes.reassign(ino_out, &new_item.id);
                    self.cache.memory.insert(ino_out, new_item.clone());
                    let _ = self
                        .rt
                        .block_on(self.cache.writeback.remove(&self.drive_id, &old_dst_id));

                    if let Some(mut entry) = self.open_files.get_mut(fh_out) {
                        if let Some(buf) = entry.content.as_complete_mut() {
                            buf.resize(new_item.size as usize, 0);
                        }
                        entry.dirty = false;
                    }

                    return Ok(src_size as u32);
                }
                Ok(CopyStatus::InProgress { percentage }) => {
                    tracing::debug!("server-side copy {percentage:.0}% complete");
                    delay_ms = (delay_ms * COPY_POLL_BACKOFF).min(COPY_POLL_MAX_MS);
                }
                Ok(CopyStatus::Failed { message }) => {
                    tracing::error!("server-side copy failed: {message}");
                    return Err(VfsError::IoError(format!(
                        "server-side copy failed: {message}"
                    )));
                }
                Err(e) => {
                    tracing::error!("copy poll failed: {e}");
                    return Err(VfsError::IoError(format!("copy poll failed: {e}")));
                }
            }
        }
    }

    fn copy_file_range_fallback(
        &self,
        fh_in: u64,
        offset_in: u64,
        fh_out: u64,
        offset_out: u64,
        len: u64,
        ino_out: u64,
    ) -> VfsResult<u32> {
        // Read from source — use read_handle which handles streaming
        let offset_in = offset_in as usize;
        let len = len as usize;
        let data = self.read_handle(fh_in, offset_in, len)?;
        let to_copy = data.len();
        if to_copy == 0 {
            return Ok(0);
        }

        // Write to destination — ensure complete first
        let mut dst_entry = self.open_files.get_mut(fh_out).ok_or(VfsError::NotFound)?;
        ensure_complete(&mut dst_entry, &self.rt)?;
        let new_size = {
            let buf = dst_entry.content.as_complete_mut().unwrap();
            let offset_out = offset_out as usize;
            let needed = offset_out + to_copy;
            if buf.len() < needed {
                buf.resize(needed, 0);
            }
            buf[offset_out..offset_out + to_copy].copy_from_slice(&data);
            buf.len() as i64
        };
        dst_entry.dirty = true;
        drop(dst_entry);

        if let Some(mut item) = self.lookup_item(ino_out) {
            item.size = new_size;
            self.cache.memory.insert(ino_out, item);
        }

        Ok(to_copy as u32)
    }

    /// Read a byte range directly from disk cache or via a range download.
    /// Used by WinFsp's read callback to avoid the streaming/open-file machinery.
    pub fn read_range_direct(&self, ino: u64, offset: u64, length: u64) -> VfsResult<Vec<u8>> {
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;

        // Check disk cache first — read only the needed range
        if let Some(data) = self
            .cache
            .disk
            .get_range(&self.drive_id, &item_id, offset, length)
        {
            return Ok(data);
        }

        // Fall back to range download
        let bytes = self
            .rt
            .block_on(
                self.graph
                    .download_range(&self.drive_id, &item_id, offset, length),
            )
            .map_err(|e| VfsError::IoError(format!("range download failed: {e}")))?;
        Ok(bytes.to_vec())
    }

    fn cleanup_deleted_item(&self, item_id: &str, ino: u64) {
        self.cache.memory.invalidate(ino);
        self.inodes.remove_by_item_id(item_id);
        let _ = self.cache.sqlite.delete_item(item_id);
        let _ = self
            .rt
            .block_on(self.cache.disk.remove(&self.drive_id, item_id));
        let _ = self
            .rt
            .block_on(self.cache.writeback.remove(&self.drive_id, item_id));
    }
}
