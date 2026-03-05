//! Shared VFS operations used by both FUSE (Linux/macOS) and CfApi (Windows) backends.
//!
//! This module contains the core business logic for cache lookups, Graph API interactions,
//! inode management, and write-back operations. Platform-specific backends (FUSE callbacks,
//! CfApi sync filter) delegate to [`CoreOps`] instead of duplicating this logic.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use chrono::Utc;
use tokio::runtime::Handle;

use crate::inode::InodeTable;
use cloudmount_cache::CacheManager;
use cloudmount_core::types::{DriveItem, FileFacet, ParentReference};
use cloudmount_graph::GraphClient;

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
        if let Some(children_inodes) = self.cache.memory.get_children(parent_ino) {
            for child_inode in children_inodes {
                if let Some(item) = self.lookup_item(child_inode)
                    && item.name == name
                {
                    return Some((child_inode, item));
                }
            }
        }

        if let Ok(children) = self.cache.sqlite.get_children(parent_ino) {
            for (child_inode, item) in children {
                if item.name == name {
                    self.cache.memory.insert(child_inode, item.clone());
                    return Some((child_inode, item));
                }
            }
        }

        let parent_item_id = self.inodes.get_item_id(parent_ino)?;
        if let Ok(children) = self
            .rt
            .block_on(self.graph.list_children(&self.drive_id, &parent_item_id))
        {
            let mut child_inodes = Vec::new();
            let mut found = None;

            for item in &children {
                let child_inode = self.inodes.allocate(&item.id);
                child_inodes.push(child_inode);
                self.cache.memory.insert(child_inode, item.clone());
                if item.name == name && found.is_none() {
                    found = Some((child_inode, item.clone()));
                }
            }

            if let Some(parent_item) = self.lookup_item(parent_ino) {
                self.cache
                    .memory
                    .insert_with_children(parent_ino, parent_item, child_inodes);
            }

            return found;
        }
        None
    }

    /// List all children of a directory, populating caches along the way.
    /// Checks memory cache → SQLite → Graph API in order.
    pub fn list_children(&self, parent_ino: u64) -> Vec<(u64, DriveItem)> {
        if let Some(children_inodes) = self.cache.memory.get_children(parent_ino) {
            let result: Vec<_> = children_inodes
                .iter()
                .filter_map(|&ino| self.lookup_item(ino).map(|item| (ino, item)))
                .collect();
            return result;
        }

        if let Ok(children) = self.cache.sqlite.get_children(parent_ino)
            && !children.is_empty()
        {
            for (ino, item) in &children {
                self.cache.memory.insert(*ino, item.clone());
            }
            return children;
        }

        let Some(item_id) = self.inodes.get_item_id(parent_ino) else {
            return Vec::new();
        };
        if let Ok(items) = self
            .rt
            .block_on(self.graph.list_children(&self.drive_id, &item_id))
        {
            return items
                .into_iter()
                .map(|item| {
                    let ino = self.inodes.allocate(&item.id);
                    self.cache.memory.insert(ino, item.clone());
                    (ino, item)
                })
                .collect();
        }

        Vec::new()
    }

    /// Read file content from disk cache or download from Graph API.
    pub fn read_content(&self, ino: u64) -> VfsResult<Vec<u8>> {
        let item_id = self.inodes.get_item_id(ino).ok_or(VfsError::NotFound)?;

        if let Some(content) = self
            .rt
            .block_on(self.cache.disk.get(&self.drive_id, &item_id))
        {
            return Ok(content);
        }

        match self
            .rt
            .block_on(self.graph.download_content(&self.drive_id, &item_id))
        {
            Ok(content) => {
                let _ = self
                    .rt
                    .block_on(self.cache.disk.put(&self.drive_id, &item_id, &content));
                Ok(content.to_vec())
            }
            Err(e) => {
                tracing::error!("download failed for {item_id}: {e}");
                Err(VfsError::IoError(format!("download failed: {e}")))
            }
        }
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

    /// Create a new file with a temporary `local:{nanos}` ID, reassigned on flush.
    pub fn create_file(&self, parent_ino: u64, name: &str) -> VfsResult<(u64, DriveItem)> {
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

        self.rt
            .block_on(
                self.cache
                    .writeback
                    .write(&self.drive_id, &temp_item_id, &[]),
            )
            .map_err(|e| VfsError::IoError(format!("create writeback failed: {e}")))?;

        self.cache.memory.insert(inode, item.clone());

        let mut children = self
            .cache
            .memory
            .get_children(parent_ino)
            .or_else(|| {
                self.cache.sqlite.get_children(parent_ino).ok().map(|c| {
                    c.iter()
                        .map(|(ino, item)| {
                            self.cache.memory.insert(*ino, item.clone());
                            *ino
                        })
                        .collect()
                })
            })
            .unwrap_or_default();
        children.push(inode);
        if let Some(parent_item) = self.lookup_item(parent_ino) {
            self.cache
                .memory
                .insert_with_children(parent_ino, parent_item, children);
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

        if let Some(mut children) = self.cache.memory.get_children(parent_ino) {
            children.push(inode);
            if let Some(parent_item) = self.lookup_item(parent_ino) {
                self.cache
                    .memory
                    .insert_with_children(parent_ino, parent_item, children);
            }
        }

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

        self.cleanup_deleted_item(&item_id, child_ino, parent_ino);
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
        self.cache.memory.invalidate(parent_ino);
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

        self.cache.memory.invalidate(parent_ino);
        if parent_ino != new_parent_ino {
            self.cache.memory.invalidate(new_parent_ino);
        }

        Ok(())
    }

    fn cleanup_deleted_item(&self, item_id: &str, ino: u64, parent_ino: u64) {
        self.cache.memory.invalidate(ino);
        self.cache.memory.invalidate(parent_ino);
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
