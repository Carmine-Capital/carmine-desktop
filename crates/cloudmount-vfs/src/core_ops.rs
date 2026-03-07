//! Shared VFS operations used by both FUSE (Linux/macOS) and CfApi (Windows) backends.
//!
//! This module contains the core business logic for cache lookups, Graph API interactions,
//! inode management, and write-back operations. Platform-specific backends (FUSE callbacks,
//! CfApi sync filter) delegate to [`CoreOps`] instead of duplicating this logic.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use chrono::Utc;
use dashmap::DashMap;
use futures_util::StreamExt;
use tokio::runtime::Handle;
use tokio::sync::watch;

use crate::inode::InodeTable;
use cloudmount_cache::CacheManager;
use cloudmount_core::types::{DriveItem, FileFacet, ParentReference};
use cloudmount_graph::{CopyStatus, GraphClient, SMALL_FILE_LIMIT};

const COPY_POLL_INITIAL_MS: u64 = 500;
const COPY_POLL_MAX_MS: u64 = 5000;
const COPY_POLL_BACKOFF: u64 = 2;
const COPY_MAX_POLL_DURATION_SECS: u64 = 300;
const COPY_POLL_MAX_RETRIES: u32 = 3;

/// 2 MB threshold: if a read offset is within this distance of the download
/// frontier, block and wait for the sequential download to catch up.
/// Beyond this, issue an on-demand range request instead.
const RANDOM_ACCESS_THRESHOLD: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub enum DownloadProgress {
    InProgress(u64),
    Done,
    Failed(String),
}

pub struct StreamingBuffer {
    data: tokio::sync::RwLock<Vec<u8>>,
    progress: watch::Sender<DownloadProgress>,
    progress_rx: watch::Receiver<DownloadProgress>,
    pub total_size: u64,
}

impl StreamingBuffer {
    pub fn new(total_size: u64) -> Self {
        let (tx, rx) = watch::channel(DownloadProgress::InProgress(0));
        Self {
            data: tokio::sync::RwLock::new(vec![0u8; total_size as usize]),
            progress: tx,
            progress_rx: rx,
            total_size,
        }
    }

    pub async fn append_chunk(&self, chunk: &[u8]) {
        let mut data = self.data.write().await;
        let current = match *self.progress_rx.borrow() {
            DownloadProgress::InProgress(n) => n as usize,
            _ => return,
        };
        let end = std::cmp::min(current + chunk.len(), data.len());
        let copy_len = end - current;
        data[current..end].copy_from_slice(&chunk[..copy_len]);
        let _ = self.progress.send(DownloadProgress::InProgress(end as u64));
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
        let data = self.data.read().await;
        let downloaded = match *self.progress_rx.borrow() {
            DownloadProgress::InProgress(n) => n as usize,
            DownloadProgress::Done | DownloadProgress::Failed(_) => data.len(),
        };
        let end = std::cmp::min(offset + size, downloaded);
        if offset >= end {
            return Vec::new();
        }
        data[offset..end].to_vec()
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
}

/// Errors from core VFS operations.
///
/// Each platform backend maps these to its own error type
/// (e.g., `fuser::Errno` for FUSE, `CloudErrorKind` for CfApi).
#[derive(Debug)]
pub enum VfsError {
    /// Item not found (FUSE: ENOENT, Windows: STATUS_OBJECT_NAME_NOT_FOUND)
    NotFound,
    /// Target is not a directory (FUSE: ENOTDIR)
    NotADirectory,
    /// Directory is not empty (FUSE: ENOTEMPTY)
    DirectoryNotEmpty,
    /// I/O or network operation failed (FUSE: EIO, Windows: STATUS_DEVICE_NOT_READY)
    IoError(String),
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

/// Core VFS operations shared between platform backends.
///
/// Encapsulates cache lookups, Graph API calls, inode management, and write-back logic.
/// Each platform backend holds a `CoreOps` instance and delegates business logic to it,
/// keeping only platform-specific callback translation in the backend layer.
pub struct CoreOps {
    graph: Arc<GraphClient>,
    cache: Arc<CacheManager>,
    inodes: Arc<InodeTable>,
    drive_id: String,
    rt: Handle,
    open_files: OpenFileTable,
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
            open_files: OpenFileTable::new(),
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

    pub fn resolve_path(&self, relative_path: &str) -> Option<(u64, DriveItem)> {
        if relative_path.is_empty() {
            let item = self.lookup_item(crate::inode::ROOT_INODE)?;
            return Some((crate::inode::ROOT_INODE, item));
        }

        let mut current_ino = crate::inode::ROOT_INODE;
        for component in relative_path.split(['/', '\\']) {
            if component.is_empty() {
                continue;
            }
            let (child_ino, _) = self.find_child(current_ino, component)?;
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

    /// Find a child item by name under a given parent inode.
    /// Searches memory cache, SQLite, then falls back to Graph API.
    /// On Graph API fallback, also populates the parent's children list in memory cache.
    pub fn find_child(&self, parent_ino: u64, name: &str) -> Option<(u64, DriveItem)> {
        if let Some(children_map) = self.cache.memory.get_children(parent_ino)
            && let Some(&child_inode) = children_map.get(name)
            && let Some(item) = self.lookup_item(child_inode)
        {
            return Some((child_inode, item));
        }

        match self.cache.sqlite.get_children(parent_ino) {
            Ok(children) => {
                for (_, item) in children {
                    if item.name == name {
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
                    if item.name == name && found.is_none() {
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
                .unwrap_or(true);
            let etag_ok = match (&disk_etag, item.as_ref().and_then(|i| i.etag.as_ref())) {
                (Some(de), Some(ie)) => de == ie,
                _ => true,
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
            let buf = entry.content.as_complete_mut().unwrap();
            buf.resize(new_size, 0);
            entry.dirty = true;
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
    /// Before uploading an existing file, compares the cached eTag with the server eTag.
    /// On mismatch, saves the local content as `.conflict.{timestamp}` before proceeding.
    pub fn flush_inode(&self, ino: u64) -> VfsResult<()> {
        let item_id = match self.inodes.get_item_id(ino) {
            Some(id) => id,
            None => return Ok(()),
        };

        let content = match self
            .rt
            .block_on(self.cache.writeback.read(&self.drive_id, &item_id))
        {
            Some(data) => data,
            None => return Ok(()),
        };

        let item = match self.lookup_item(ino) {
            Some(item) => item,
            None => return Err(VfsError::IoError("item metadata not found".to_string())),
        };

        let parent_id = item
            .parent_reference
            .as_ref()
            .and_then(|p| p.id.as_deref())
            .unwrap_or("")
            .to_string();

        let is_new_file = item_id.starts_with("local:");

        if let Some(cached_etag) = item.etag.as_ref()
            && !is_new_file
        {
            match self
                .rt
                .block_on(self.graph.get_item(&self.drive_id, &item_id))
            {
                Ok(server_item) => {
                    if server_item.etag.as_deref() != Some(cached_etag) {
                        tracing::warn!(
                            "conflict detected for {}: cached={:?}, server={:?}",
                            item.name,
                            item.etag,
                            server_item.etag
                        );
                        let timestamp = Utc::now().timestamp();
                        let conflict_name = format!("{}.conflict.{timestamp}", item.name);
                        if !parent_id.is_empty() {
                            let _ = self.rt.block_on(self.graph.upload_small(
                                &self.drive_id,
                                &parent_id,
                                &conflict_name,
                                Bytes::from(content.clone()),
                            ));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("conflict check failed for {item_id}: {e}");
                }
            }
        }

        // Persist to disk for crash safety before the network upload
        let _ = self
            .rt
            .block_on(self.cache.writeback.persist(&self.drive_id, &item_id));

        let upload_result = if is_new_file {
            if parent_id.is_empty() {
                return Err(VfsError::IoError("no parent for new file".to_string()));
            }
            self.rt.block_on(self.graph.upload_small(
                &self.drive_id,
                &parent_id,
                &item.name,
                Bytes::from(content.clone()),
            ))
        } else {
            self.rt.block_on(self.graph.upload(
                &self.drive_id,
                &parent_id,
                Some(&item_id),
                &item.name,
                Bytes::from(content.clone()),
            ))
        };

        match upload_result {
            Ok(updated_item) => {
                if is_new_file {
                    self.inodes.reassign(ino, &updated_item.id);
                }
                self.cache.memory.insert(ino, updated_item.clone());
                let _ = self.rt.block_on(self.cache.disk.put(
                    &self.drive_id,
                    &updated_item.id,
                    &content,
                    updated_item.etag.as_deref(),
                ));
                let _ = self
                    .rt
                    .block_on(self.cache.writeback.remove(&self.drive_id, &item_id));
                Ok(())
            }
            Err(e) => {
                tracing::error!("flush upload failed for {item_id}: {e}");
                Err(VfsError::IoError(format!("upload failed: {e}")))
            }
        }
    }

    /// Open a file, loading its content into the open file table.
    /// Small files (< 4 MB) and cached files load eagerly.
    /// Large uncached files return immediately with a background streaming download.
    /// Validates disk cache freshness via dirty-inode set, eTag, and size checks.
    pub fn open_file(&self, ino: u64) -> VfsResult<u64> {
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;
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

        // Check disk cache with freshness validation
        if !self.cache.dirty_inodes.contains(&ino)
            && let Some((content, disk_etag)) = self
                .rt
                .block_on(self.cache.disk.get_with_etag(&self.drive_id, &item_id))
        {
            // Validate: size must match metadata
            let size_ok = item
                .as_ref()
                .map(|i| content.len() == i.size as usize)
                .unwrap_or(true);
            // Validate: eTag must match metadata (if both present)
            let etag_ok = match (&disk_etag, item.as_ref().and_then(|i| i.etag.as_ref())) {
                (Some(de), Some(ie)) => de == ie,
                _ => true,
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
            let buffer = Arc::new(StreamingBuffer::new(file_size as u64));
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
            let content = open_file
                .content
                .as_complete()
                .ok_or_else(|| VfsError::IoError("dirty file in non-complete state".to_string()))?;
            self.rt
                .block_on(
                    self.cache
                        .writeback
                        .write(&self.drive_id, &item_id, content),
                )
                .map_err(|e| VfsError::IoError(format!("release writeback failed: {e}")))?;
        }
        Ok(())
    }

    /// Read bytes from an open file handle's buffer.
    /// For streaming downloads, blocks until the requested range is available
    /// or issues an on-demand range request for random access.
    pub fn read_handle(&self, fh: u64, offset: usize, size: usize) -> VfsResult<Vec<u8>> {
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
        let new_size = {
            let buf = entry.content.as_complete_mut().unwrap();
            let needed = offset + data.len();
            if buf.len() < needed {
                buf.resize(needed, 0);
            }
            buf[offset..offset + data.len()].copy_from_slice(data);
            buf.len() as i64
        };
        entry.dirty = true;
        let ino = entry.ino;
        drop(entry);

        if let Some(mut item) = self.lookup_item(ino) {
            item.size = new_size;
            self.cache.memory.insert(ino, item);
        }

        Ok(data.len() as u32)
    }

    /// Flush an open file handle: push dirty content to writeback and upload.
    /// If streaming, waits for download to complete first.
    pub fn flush_handle(&self, fh: u64) -> VfsResult<()> {
        // Check dirty flag; if streaming, wait and transition to Complete
        {
            let mut entry = self.open_files.get_mut(fh).ok_or(VfsError::NotFound)?;
            if !entry.dirty {
                return Ok(());
            }
            ensure_complete(&mut entry, &self.rt)?;
        }

        let entry = self.open_files.get(fh).ok_or(VfsError::NotFound)?;
        let ino = entry.ino;
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;
        let content = entry.content.as_complete().unwrap().clone();
        drop(entry);

        self.rt
            .block_on(
                self.cache
                    .writeback
                    .write(&self.drive_id, &item_id, &content),
            )
            .map_err(|e| VfsError::IoError(format!("flush writeback failed: {e}")))?;

        self.flush_inode(ino)?;

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
            download_url: None,
        };

        let inode = self.inodes.allocate(&temp_item_id);

        self.cache.memory.insert(inode, item.clone());
        self.cache.memory.add_child(parent_ino, name, inode);

        let fh = self
            .open_files
            .insert(inode, DownloadState::Complete(Vec::new()));

        Ok((fh, inode, item))
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
            .find_child(parent_ino, name)
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
            .find_child(parent_ino, name)
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
            .find_child(parent_ino, name)
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
        if let Some((existing_ino, existing_item)) = self.find_child(new_parent_ino, new_name)
            && existing_item.id != item_id
        {
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

            self.cache.memory.insert(child_ino, updated_item);
        } else {
            let mut updated = child_item.clone();
            updated.name = new_name.to_string();
            if let (Some(new_pid), Some(pref)) =
                (&new_parent_item_id, &mut updated.parent_reference)
            {
                pref.id = Some(new_pid.clone());
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
            self.copy_file_range_server(ino_out, fh_out, &src_item)
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
        let start = std::time::Instant::now();
        let mut delay_ms = COPY_POLL_INITIAL_MS;
        let max_duration = std::time::Duration::from_secs(COPY_MAX_POLL_DURATION_SECS);

        loop {
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));

            if start.elapsed() > max_duration {
                tracing::warn!("server-side copy timed out after {COPY_MAX_POLL_DURATION_SECS}s");
                return Err(VfsError::IoError("server-side copy timed out".into()));
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
                            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
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
    /// Used by CfApi's fetch_data to avoid the streaming/open-file machinery.
    pub fn read_range_direct(&self, ino: u64, offset: u64, length: u64) -> VfsResult<Vec<u8>> {
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;

        // Check disk cache first
        if let Some(content) = self
            .rt
            .block_on(self.cache.disk.get(&self.drive_id, &item_id))
        {
            let start = offset as usize;
            let end = std::cmp::min(start + length as usize, content.len());
            if start >= end {
                return Ok(Vec::new());
            }
            return Ok(content[start..end].to_vec());
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
