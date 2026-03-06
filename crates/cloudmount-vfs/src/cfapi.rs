use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cloud_filter::error::{CResult, CloudErrorKind};
use cloud_filter::filter::{Request, SyncFilter, info, ticket};
use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder::{Placeholder, UpdateOptions};
use cloud_filter::placeholder_file::PlaceholderFile;
use cloud_filter::root::{
    Connection, HydrationType, PopulationType, SecurityId, Session, SyncRootId, SyncRootIdBuilder,
    SyncRootInfo,
};
use cloud_filter::utility::WriteAt;
use nt_time::FileTime;
use tokio::runtime::Handle;

use crate::core_ops::CoreOps;
use crate::inode::{InodeTable, ROOT_INODE};
use cloudmount_cache::CacheManager;
use cloudmount_core::types::DriveItem;
use cloudmount_graph::GraphClient;

const PROVIDER_NAME: &str = "CloudMount";
const PROVIDER_VERSION: &str = env!("CARGO_PKG_VERSION");
// ticket.write_at() requires 4KiB-aligned chunks (OS restriction)
const WRITE_CHUNK_SIZE: usize = 4096;
const UNMOUNT_FLUSH_TIMEOUT: Duration = Duration::from_secs(30);

pub struct CloudMountCfFilter {
    core: CoreOps,
    mount_path: PathBuf,
}

impl CloudMountCfFilter {
    pub fn new(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        rt: Handle,
        mount_path: PathBuf,
    ) -> Self {
        Self {
            core: CoreOps::new(graph, cache, inodes, drive_id, rt),
            mount_path,
        }
    }

    fn relative_path(&self, absolute: &Path) -> Option<String> {
        absolute
            .strip_prefix(&self.mount_path)
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
    }

    fn resolve_parent_item_id(&self, rel_path: &str) -> Option<String> {
        let parent_rel = {
            let p = Path::new(rel_path);
            let parent = p.parent()?;
            parent.to_string_lossy().into_owned()
        };
        let (parent_ino, _) = self.core.resolve_path(&parent_rel)?;
        self.core.inodes().get_item_id(parent_ino)
    }

    fn item_to_metadata(item: &DriveItem) -> Metadata {
        let base = if item.is_folder() {
            Metadata::directory()
        } else {
            Metadata::file()
        };

        let mut meta = base.size(item.size as u64);

        if let Some(mtime) = item.last_modified
            && let Ok(ft) = FileTime::try_from(mtime)
        {
            meta = meta.written(ft).changed(ft);
        }
        if let Some(ctime) = item.created
            && let Ok(ft) = FileTime::try_from(ctime)
        {
            meta = meta.created(ft);
        }

        meta
    }

    fn mark_placeholder_synced(&self, abs_path: &Path, item: &DriveItem) {
        match Placeholder::open(abs_path) {
            Ok(mut ph) => {
                let update = UpdateOptions::default()
                    .metadata(Self::item_to_metadata(item))
                    .mark_in_sync()
                    .blob(item.id.as_bytes());

                if let Err(e) = ph.update(update, None) {
                    tracing::warn!(
                        "failed to update placeholder sync status for {}: {e:?}",
                        abs_path.display()
                    );
                }
            }
            Err(e) => {
                tracing::warn!("failed to open placeholder {}: {e:?}", abs_path.display());
            }
        }
    }

    fn mark_placeholder_pending(&self, abs_path: &Path) {
        match Placeholder::open(abs_path) {
            Ok(mut ph) => {
                let _ = ph.mark_in_sync(false, None);
            }
            Err(e) => {
                tracing::debug!(
                    "failed to open placeholder for pending mark {}: {e:?}",
                    abs_path.display()
                );
            }
        }
    }
}

impl SyncFilter for CloudMountCfFilter {
    fn fetch_data(
        &self,
        request: Request,
        ticket: ticket::FetchData,
        info: info::FetchData,
    ) -> CResult<()> {
        let rel_path = self
            .relative_path(&request.path())
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        let (ino, _item) = self
            .core
            .resolve_path(&rel_path)
            .ok_or(CloudErrorKind::NotInSync)?;

        let content = self
            .core
            .read_content(ino)
            .map_err(|_| CloudErrorKind::Unsuccessful)?;

        let range = info.required_file_range();
        let start = range.start as usize;
        let end = std::cmp::min(range.end as usize, content.len());

        if start >= content.len() {
            return Ok(());
        }

        let data = &content[start..end];
        let total_len = data.len();
        let mut offset = range.start;

        let mut pos = 0;
        while pos < total_len {
            let remaining = total_len - pos;
            let chunk_len = if pos + WRITE_CHUNK_SIZE <= total_len {
                WRITE_CHUNK_SIZE
            } else {
                remaining
            };

            ticket
                .write_at(&data[pos..pos + chunk_len], offset)
                .map_err(|_| CloudErrorKind::Unsuccessful)?;

            pos += chunk_len;
            offset += chunk_len as u64;
        }

        Ok(())
    }

    fn fetch_placeholders(
        &self,
        request: Request,
        ticket: ticket::FetchPlaceholders,
        _info: info::FetchPlaceholders,
    ) -> CResult<()> {
        let rel_path = self
            .relative_path(&request.path())
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        let (parent_ino, _) = self
            .core
            .resolve_path(&rel_path)
            .ok_or(CloudErrorKind::NotInSync)?;

        let children = self.core.list_children(parent_ino);

        let mut placeholders: Vec<PlaceholderFile> = children
            .iter()
            .map(|(_ino, item)| {
                PlaceholderFile::new(&item.name)
                    .metadata(Self::item_to_metadata(item))
                    .blob(item.id.as_bytes().to_vec())
                    .mark_in_sync()
            })
            .collect();

        ticket
            .pass_with_placeholder(&mut placeholders)
            .map_err(|_| CloudErrorKind::Unsuccessful)?;

        Ok(())
    }

    fn closed(&self, request: Request, info: info::Closed) {
        if info.deleted() {
            return;
        }

        let Some(rel_path) = self.relative_path(&request.path()) else {
            return;
        };
        let Some((ino, item)) = self.core.resolve_path(&rel_path) else {
            return;
        };

        if item.is_folder() {
            return;
        }

        let abs_path = request.path();
        let disk_content = match std::fs::read(&abs_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let item_id = match self.core.inodes().get_item_id(ino) {
            Some(id) => id,
            None => return,
        };

        let drive_id = self.core.drive_id();
        let _ = self.core.rt().block_on(self.core.cache().writeback.write(
            drive_id,
            &item_id,
            &disk_content,
        ));

        self.mark_placeholder_pending(&abs_path);

        match self.core.flush_inode(ino) {
            Ok(()) => {
                if let Some(updated_item) = self.core.lookup_item(ino) {
                    self.mark_placeholder_synced(&abs_path, &updated_item);
                }
            }
            Err(e) => {
                tracing::error!("flush after close failed for {}: {e:?}", rel_path);
            }
        }
    }

    fn dehydrate(
        &self,
        request: Request,
        ticket: ticket::Dehydrate,
        _info: info::Dehydrate,
    ) -> CResult<()> {
        let rel_path = self.relative_path(&request.path()).unwrap_or_default();

        if let Some((ino, _)) = self.core.resolve_path(&rel_path) {
            let item_id = self.core.inodes().get_item_id(ino);
            if let Some(ref id) = item_id {
                let _ = self
                    .core
                    .rt()
                    .block_on(self.core.cache().disk.remove(self.core.drive_id(), id));
            }
        }

        ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)?;
        Ok(())
    }

    fn delete(&self, request: Request, ticket: ticket::Delete, _info: info::Delete) -> CResult<()> {
        let rel_path = self
            .relative_path(&request.path())
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        if let Some((ino, item)) = self.core.resolve_path(&rel_path) {
            let item_id = item.id.clone();
            if !item_id.starts_with("local:") {
                let _ = self.core.rt().block_on(
                    self.core
                        .graph()
                        .delete_item(self.core.drive_id(), &item_id),
                );
            }
            self.core.cache().memory.invalidate(ino);
            self.core.inodes().remove_by_item_id(&item_id);
            let _ = self.core.cache().sqlite.delete_item(&item_id);
            let _ = self.core.rt().block_on(
                self.core
                    .cache()
                    .disk
                    .remove(self.core.drive_id(), &item_id),
            );
            let _ = self.core.rt().block_on(
                self.core
                    .cache()
                    .writeback
                    .remove(self.core.drive_id(), &item_id),
            );
        }

        ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)?;
        Ok(())
    }

    fn rename(&self, request: Request, ticket: ticket::Rename, info: info::Rename) -> CResult<()> {
        let rel_path = self
            .relative_path(&request.path())
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        let target_path = info.target_path();
        let new_rel = self
            .relative_path(&target_path)
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        if let Some((ino, item)) = self.core.resolve_path(&rel_path) {
            let item_id = item.id.clone();
            if !item_id.starts_with("local:") {
                let new_name = target_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| new_rel.clone());

                let new_parent_item_id = self.resolve_parent_item_id(&new_rel);

                let _ = self.core.rt().block_on(self.core.graph().update_item(
                    self.core.drive_id(),
                    &item_id,
                    Some(&new_name),
                    new_parent_item_id.as_deref(),
                ));
            }

            self.core.cache().memory.invalidate(ino);
        }

        ticket.pass().map_err(|_| CloudErrorKind::Unsuccessful)?;
        Ok(())
    }

    fn state_changed(&self, changes: Vec<PathBuf>) {
        for path in &changes {
            tracing::debug!("state changed: {}", path.display());
        }
    }
}

fn build_sync_root_id() -> cloudmount_core::Result<SyncRootId> {
    let sid = SecurityId::current_user().map_err(|e| {
        cloudmount_core::Error::Filesystem(format!("failed to get user SID: {e:?}"))
    })?;
    Ok(SyncRootIdBuilder::new(PROVIDER_NAME)
        .user_security_id(sid)
        .build())
}

fn ensure_mount_dir(path: &Path) -> cloudmount_core::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|e| {
            cloudmount_core::Error::Filesystem(format!(
                "failed to create mount directory {}: {e}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

fn register_sync_root(sync_root_id: &SyncRootId, mount_path: &Path) -> cloudmount_core::Result<()> {
    let info = SyncRootInfo::default()
        .with_display_name(PROVIDER_NAME)
        .with_version(PROVIDER_VERSION)
        .with_icon("%SystemRoot%\\system32\\imageres.dll,0")
        .with_hydration_type(HydrationType::Progressive)
        .with_population_type(PopulationType::Full)
        .with_allow_pinning(true)
        .with_show_siblings_as_group(false)
        .with_path(mount_path)
        .map_err(|e| {
            cloudmount_core::Error::Filesystem(format!("sync root path invalid: {e:?}"))
        })?;

    sync_root_id.register(info).map_err(|e| {
        cloudmount_core::Error::Filesystem(format!("sync root registration failed: {e:?}"))
    })?;

    tracing::info!("registered sync root at {}", mount_path.display());
    Ok(())
}

pub struct CfMountHandle {
    // Drop order matters: connection must be dropped before sync_root_id is unregistered
    _connection: Connection<CloudMountCfFilter>,
    sync_root_id: SyncRootId,
    cache: Arc<CacheManager>,
    graph: Arc<GraphClient>,
    drive_id: String,
    rt: Handle,
    mount_path: PathBuf,
}

impl CfMountHandle {
    pub fn mount(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        mount_path: &Path,
        rt: Handle,
    ) -> cloudmount_core::Result<Self> {
        let sync_root_id = build_sync_root_id()?;

        ensure_mount_dir(mount_path)?;

        let is_registered = sync_root_id.is_registered().map_err(|e| {
            cloudmount_core::Error::Filesystem(format!(
                "sync root registration check failed: {e:?}"
            ))
        })?;

        if !is_registered {
            register_sync_root(&sync_root_id, mount_path)?;
        }

        let root_item = rt
            .block_on(graph.get_item(&drive_id, "root"))
            .map_err(|e| {
                cloudmount_core::Error::Filesystem(format!(
                    "failed to fetch root item for drive {drive_id}: {e}"
                ))
            })?;

        inodes.set_root(&root_item.id);
        cache.memory.insert(ROOT_INODE, root_item.clone());
        cache
            .sqlite
            .upsert_item(ROOT_INODE, &drive_id, &root_item, None)?;

        let filter = CloudMountCfFilter::new(
            graph.clone(),
            cache.clone(),
            inodes,
            drive_id.clone(),
            rt.clone(),
            mount_path.to_path_buf(),
        );

        let connection = Session::new().connect(mount_path, filter).map_err(|e| {
            cloudmount_core::Error::Filesystem(format!("CfApi connect failed: {e:?}"))
        })?;

        tracing::info!("mounted at {} via Cloud Files API", mount_path.display());

        Ok(Self {
            _connection: connection,
            sync_root_id,
            cache,
            graph,
            drive_id,
            rt,
            mount_path: mount_path.to_path_buf(),
        })
    }

    pub fn mount_path(&self) -> &Path {
        &self.mount_path
    }

    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    pub fn unmount(self) -> cloudmount_core::Result<()> {
        tracing::info!(
            "unmounting Cloud Files API at {}",
            self.mount_path.display()
        );
        self.flush_pending();
        // Drop order: connection drops first, then unregister
        drop(self._connection);
        self.sync_root_id.unregister().map_err(|e| {
            cloudmount_core::Error::Filesystem(format!("sync root unregister failed: {e:?}"))
        })?;
        tracing::info!("unregistered sync root for {}", self.mount_path.display());
        Ok(())
    }

    fn flush_pending(&self) {
        let pending = match self.rt.block_on(self.cache.writeback.list_pending()) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("failed to list pending writes on unmount: {e}");
                return;
            }
        };

        let drive_pending: Vec<_> = pending
            .into_iter()
            .filter(|(d, _)| d == &self.drive_id)
            .collect();

        if drive_pending.is_empty() {
            return;
        }

        tracing::info!(
            "flushing {} pending writes for drive {}",
            drive_pending.len(),
            self.drive_id
        );

        let graph = self.graph.clone();
        let cache = self.cache.clone();
        let drive_id = self.drive_id.clone();

        let flush_result = self.rt.block_on(async {
            tokio::time::timeout(UNMOUNT_FLUSH_TIMEOUT, async {
                for (_, item_id) in &drive_pending {
                    if let Some(content) = cache.writeback.read(&drive_id, item_id).await {
                        match graph
                            .upload(
                                &drive_id,
                                "",
                                Some(item_id),
                                item_id,
                                bytes::Bytes::from(content),
                            )
                            .await
                        {
                            Ok(_) => {
                                let _ = cache.writeback.remove(&drive_id, item_id).await;
                            }
                            Err(e) => {
                                tracing::error!("flush upload failed for {item_id}: {e}");
                            }
                        }
                    }
                }
            })
            .await
        });

        if flush_result.is_err() {
            tracing::warn!(
                "unmount flush timed out after {}s, {} writes may be pending",
                UNMOUNT_FLUSH_TIMEOUT.as_secs(),
                drive_pending.len()
            );
        }
    }
}

pub async fn shutdown_on_signal(mounts: Arc<Mutex<Vec<CfMountHandle>>>) {
    let _ = tokio::signal::ctrl_c().await;

    tracing::info!("shutdown signal received, unmounting all Cloud Files API drives");
    let mut handles = mounts.lock().unwrap();
    while let Some(handle) = handles.pop() {
        if let Err(e) = handle.unmount() {
            tracing::error!("unmount failed: {e}");
        }
    }
}
