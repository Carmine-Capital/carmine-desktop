use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
use tokio::task::block_in_place;

use crate::core_ops::{CoreOps, VfsEvent};
use crate::inode::{InodeTable, ROOT_INODE};
use cloudmount_cache::CacheManager;
use cloudmount_core::types::DriveItem;
use cloudmount_graph::{GraphClient, SMALL_FILE_LIMIT};

const PROVIDER_NAME: &str = "CloudMount";
const PROVIDER_VERSION: &str = env!("CARGO_PKG_VERSION");
// ticket.write_at() requires 4KiB-aligned chunks (OS restriction)
const WRITE_CHUNK_SIZE: usize = 4096;

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
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
    ) -> Self {
        let mut ops = CoreOps::new(graph, cache, inodes, drive_id, rt);
        if let Some(tx) = event_tx {
            ops = ops.with_event_sender(tx);
        }
        Self {
            core: ops,
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
        let Some(rel_path) = self.relative_path(&request.path()) else {
            tracing::warn!("cfapi: fetch_data called for path outside sync root");
            return Ok(());
        };

        let item_id = match std::str::from_utf8(request.file_blob()) {
            Ok(s) => s.to_owned(),
            Err(e) => {
                tracing::warn!(path = %rel_path, "cfapi: fetch_data blob decode failed: {e:?}");
                return Ok(());
            }
        };

        // Fast path: item_id from blob is already in the inode table.
        // Fallback 1: resolve via path traversal (cache → Graph API).
        // Fallback 2: allocate a fresh inode for item_id so read_range_direct
        // can look it up and trigger a download. This handles the Windows Server
        // CI case where fetch_placeholders is unreliable and tests create
        // placeholders directly via PlaceholderFile without populating the table.
        //
        // NOTE: NEVER return Err from fetch_data. Write::fail in cloud-filter
        // 0.0.6 calls CfExecute(TRANSFER_DATA, length=0) which Windows rejects
        // with ERROR_CLOUD_FILE_INVALID_REQUEST for non-empty files, causing an
        // unwrap() panic across the FFI boundary (STATUS_STACK_BUFFER_OVERRUN).
        // Return Ok(()) on all error paths so the OS uses CANCEL_FETCH_DATA.
        let ino = if let Some(ino) = self.core.inodes().get_inode(&item_id) {
            ino
        } else {
            match self.core.resolve_path(&rel_path) {
                Some((ino, _)) => ino,
                None => {
                    // Item not in cache yet — allocate a fresh inode so
                    // read_range_direct can look up item_id and download.
                    tracing::debug!(
                        path = %rel_path,
                        "cfapi: fetch_data allocating inode for unknown item {item_id}"
                    );
                    self.core.inodes().allocate(&item_id)
                }
            }
        };

        let range = info.required_file_range();
        let offset = range.start;
        let length = range.end - range.start;

        let content = match self.core.read_range_direct(ino, offset, length) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(path = %rel_path, "cfapi: fetch_data download failed: {e:?}");
                return Ok(());
            }
        };

        if content.is_empty() {
            tracing::warn!(path = %rel_path, "cfapi: fetch_data got empty content, skipping");
            return Ok(());
        }

        let data = &content[..];
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

            if let Err(e) = ticket.write_at(&data[pos..pos + chunk_len], offset) {
                tracing::warn!(path = %rel_path, "cfapi: fetch_data write_at failed: {e:?}");
                break;
            }

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
        let dir_path = request.path();
        let rel_path = self
            .relative_path(&dir_path)
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        let (parent_ino, _) = self
            .core
            .resolve_path(&rel_path)
            .ok_or(CloudErrorKind::NotInSync)?;

        let children = self.core.list_children(parent_ino);

        // Pre-filter: skip items that already have a local placeholder on disk
        // (optimisation hint — the per-item error handler below is the safety net).
        let filtered: Vec<_> = children
            .iter()
            .filter(|(_ino, item)| !dir_path.join(&item.name).exists())
            .collect();

        for (_ino, item) in filtered {
            let placeholder = PlaceholderFile::new(&item.name)
                .metadata(Self::item_to_metadata(item))
                .blob(item.id.as_bytes().to_vec())
                .mark_in_sync();

            if let Err(e) = ticket.pass_with_placeholder(&mut [placeholder]) {
                if e.code().0 == 0x8007017cu32 as i32 {
                    // ERROR_CLOUD_FILE_INVALID_REQUEST: placeholder already exists
                    // (TOCTOU race between .exists() check and CfCreatePlaceholders).
                    // Treat as non-fatal to prevent STATUS_STACK_BUFFER_OVERRUN crash.
                    tracing::warn!(item = %item.name, "cfapi: placeholder already exists (TOCTOU skip)");
                    continue;
                }
                return Err(CloudErrorKind::Unsuccessful);
            }
        }

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
        let item_id = match self.core.inodes().get_item_id(ino) {
            Some(id) => id,
            None => return,
        };

        let drive_id = self.core.drive_id();

        // For large files, use chunked reading to avoid loading everything into memory.
        // SMALL_FILE_LIMIT (4MB) is the same threshold used for simple vs session upload.
        let file_size = match std::fs::metadata(&abs_path) {
            Ok(m) => m.len(),
            Err(_) => return,
        };

        if file_size <= SMALL_FILE_LIMIT as u64 {
            let disk_content = match std::fs::read(&abs_path) {
                Ok(c) => c,
                Err(_) => return,
            };
            let _ = self.core.rt().block_on(self.core.cache().writeback.write(
                drive_id,
                &item_id,
                &disk_content,
            ));
        } else {
            use std::io::Read;
            let file = match std::fs::File::open(&abs_path) {
                Ok(f) => f,
                Err(_) => return,
            };
            let mut reader = std::io::BufReader::with_capacity(SMALL_FILE_LIMIT, file);
            let mut buf = vec![0u8; SMALL_FILE_LIMIT];
            let mut all_content = Vec::with_capacity(file_size as usize);
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => all_content.extend_from_slice(&buf[..n]),
                    Err(_) => return,
                }
            }
            let _ = self.core.rt().block_on(self.core.cache().writeback.write(
                drive_id,
                &item_id,
                &all_content,
            ));
        }

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

        if let Err(e) = ticket.pass() {
            tracing::warn!("cfapi: dehydrate ticket.pass() failed: {e:?}");
        }
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

        if let Err(e) = ticket.pass() {
            tracing::warn!(path = %rel_path, "cfapi: delete ticket.pass() failed: {e:?}");
        }
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

        if let Err(e) = ticket.pass() {
            tracing::warn!(path = %rel_path, new_path = %new_rel, "cfapi: rename ticket.pass() failed: {e:?}");
        }
        Ok(())
    }

    fn state_changed(&self, changes: Vec<PathBuf>) {
        for path in &changes {
            tracing::debug!("state changed: {}", path.display());
            if let Some(rel_path) = self.relative_path(path)
                && let Some((ino, _)) = self.core.resolve_path(&rel_path)
            {
                self.core.cache().memory.invalidate(ino);
            }
        }
    }
}

/// Bridge async code from a context that may or may not already be inside a Tokio runtime.
///
/// - Inside an async context (e.g. tests): uses [`block_in_place`] + [`Handle::block_on`]
///   to avoid the "cannot start a runtime from within a runtime" panic.
/// - Outside a runtime (e.g. OS CfApi worker threads): uses plain [`Handle::block_on`].
fn block_on_compat<F: std::future::Future>(rt: &Handle, f: F) -> F::Output {
    match Handle::try_current() {
        Ok(_) => block_in_place(|| rt.block_on(f)),
        Err(_) => rt.block_on(f),
    }
}

fn build_sync_root_id(account_name: &str) -> cloudmount_core::Result<SyncRootId> {
    let sid = SecurityId::current_user().map_err(|e| {
        cloudmount_core::Error::Filesystem(format!("failed to get user SID: {e:?}"))
    })?;
    let sanitized = account_name.replace('!', "_");
    tracing::debug!(
        provider = PROVIDER_NAME,
        account_name = %sanitized,
        "building sync root ID"
    );
    Ok(SyncRootIdBuilder::new(PROVIDER_NAME)
        .user_security_id(sid)
        .account_name(&sanitized)
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
        account_name: String,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
    ) -> cloudmount_core::Result<Self> {
        let sync_root_id = build_sync_root_id(&account_name)?;

        ensure_mount_dir(mount_path)?;

        let is_registered = sync_root_id.is_registered().map_err(|e| {
            cloudmount_core::Error::Filesystem(format!(
                "sync root registration check failed: {e:?}"
            ))
        })?;

        if !is_registered {
            register_sync_root(&sync_root_id, mount_path)?;
        }

        let root_item = block_on_compat(&rt, graph.get_item(&drive_id, "root")).map_err(|e| {
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
            event_tx,
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
        block_on_compat(
            &self.rt,
            crate::pending::flush_pending(&self.cache, &self.graph, &self.drive_id),
        );
        // Drop order: connection drops first, then unregister
        drop(self._connection);
        self.sync_root_id.unregister().map_err(|e| {
            cloudmount_core::Error::Filesystem(format!("sync root unregister failed: {e:?}"))
        })?;
        tracing::info!("unregistered sync root for {}", self.mount_path.display());
        Ok(())
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
