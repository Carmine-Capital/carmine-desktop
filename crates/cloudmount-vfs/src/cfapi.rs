use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

use cloud_filter::error::{CResult, CloudErrorKind};
use cloud_filter::filter::{Request, SyncFilter, info, ticket};
use cloud_filter::metadata::Metadata;
use cloud_filter::placeholder::{Placeholder, UpdateOptions};
use cloud_filter::placeholder_file::PlaceholderFile;
use cloud_filter::root::{
    Connection, HydrationType, PopulationType, SecurityId, Session, SupportedAttribute, SyncRootId,
    SyncRootIdBuilder, SyncRootInfo,
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
const SAFE_SAVE_RECONCILE_TIMEOUT: Duration = Duration::from_secs(2);
const DEFERRED_INGEST_TTL: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct DeferredIngest {
    first_seen: Instant,
    attempts: u32,
    reason: &'static str,
}

#[derive(Clone)]
struct SafeSaveTxn {
    source_parent_ino: u64,
    source_name: String,
    target_parent_ino: u64,
    target_name: String,
    source_path: PathBuf,
    target_path: PathBuf,
    created_at: Instant,
}

#[cfg(target_os = "windows")]
static ACTIVE_CFAPI_MOUNTS: AtomicUsize = AtomicUsize::new(0);

pub struct CloudMountCfFilter {
    core: CoreOps,
    mount_path: PathBuf,
    deferred_ingest: Mutex<HashMap<PathBuf, DeferredIngest>>,
    safe_save_txns: Mutex<Vec<SafeSaveTxn>>,
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
        let mut ops =
            CoreOps::new(graph, cache, inodes, drive_id, rt).with_mount_path(mount_path.clone());
        if let Some(tx) = event_tx {
            ops = ops.with_event_sender(tx);
        }
        Self {
            core: ops,
            mount_path,
            deferred_ingest: Mutex::new(HashMap::new()),
            safe_save_txns: Mutex::new(Vec::new()),
        }
    }

    /// Return the path components relative to the mount root as lossless `OsString` values.
    /// Preserves NTFS filenames that may contain unpaired UTF-16 surrogates.
    fn relative_components(&self, absolute: &Path) -> Option<Vec<OsString>> {
        absolute
            .strip_prefix(&self.mount_path)
            .ok()
            .map(|p| p.iter().map(|c| c.to_os_string()).collect())
    }

    /// Split pre-resolved components into (parent_components, child_name).
    /// Returns `None` if `components` is empty.
    fn resolve_parent_and_name(components: &[OsString]) -> Option<(&[OsString], &OsString)> {
        components.split_last().map(|(name, parent)| (parent, name))
    }

    fn item_to_metadata(item: &DriveItem) -> Metadata {
        item_to_metadata(item)
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

    fn log_ingest_outcome(&self, outcome: &str, trigger: &str, path: &Path, reason: &str) {
        tracing::info!(
            outcome,
            trigger,
            path = %path.display(),
            reason,
            "cfapi: local-change ingest"
        );
    }

    fn temp_like_name(name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        lower.starts_with("~$")
            || lower.ends_with('~')
            || lower.ends_with(".tmp")
            || lower.ends_with(".bak")
            || lower.contains("autosave")
    }

    fn should_defer_rename(src_name: &str, dst_name: &str) -> bool {
        Self::temp_like_name(src_name) || Self::temp_like_name(dst_name)
    }

    fn defer_ingest(&self, path: &Path, reason: &'static str) {
        let mut deferred = self.deferred_ingest.lock().unwrap();
        let entry = deferred
            .entry(path.to_path_buf())
            .or_insert(DeferredIngest {
                first_seen: Instant::now(),
                attempts: 0,
                reason,
            });
        entry.attempts += 1;
        entry.reason = reason;
        self.log_ingest_outcome("deferred", "ingest", path, reason);
    }

    fn clear_deferred_ingest(&self, path: &Path) {
        self.deferred_ingest.lock().unwrap().remove(path);
    }

    fn process_deferred_timeouts(&self) {
        let now = Instant::now();
        self.deferred_ingest.lock().unwrap().retain(|path, state| {
            if now.duration_since(state.first_seen) >= DEFERRED_INGEST_TTL {
                tracing::warn!(
                    path = %path.display(),
                    attempts = state.attempts,
                    reason = state.reason,
                    "cfapi: local ingest deferred entry expired"
                );
                return false;
            }
            true
        });
    }

    fn process_safe_save_timeouts(&self) {
        let mut expired = Vec::new();
        {
            let now = Instant::now();
            let mut txns = self.safe_save_txns.lock().unwrap();
            txns.retain(|txn| {
                if now.duration_since(txn.created_at) >= SAFE_SAVE_RECONCILE_TIMEOUT {
                    expired.push(txn.clone());
                    return false;
                }
                true
            });
        }

        for txn in expired {
            match self.core.rename(
                txn.source_parent_ino,
                &txn.source_name,
                txn.target_parent_ino,
                &txn.target_name,
            ) {
                Ok(()) => tracing::info!(
                    path = %txn.source_path.display(),
                    target = %txn.target_path.display(),
                    "cfapi: safe-save reconciliation timeout committed as rename"
                ),
                Err(e) => tracing::warn!(
                    path = %txn.source_path.display(),
                    target = %txn.target_path.display(),
                    "cfapi: safe-save timeout rename commit failed: {e:?}"
                ),
            }
        }
    }

    fn reconcile_safe_save_replacement(&self, source: &Path, target: &Path) -> bool {
        let mut matched = false;
        let mut txns = self.safe_save_txns.lock().unwrap();
        txns.retain(|txn| {
            let is_match = txn.source_path == target || txn.target_path == source;
            if is_match {
                matched = true;
                tracing::info!(
                    source = %source.display(),
                    target = %target.display(),
                    original = %txn.source_path.display(),
                    "cfapi: safe-save transaction reconciled as content update"
                );
            }
            !is_match
        });
        matched
    }

    fn stage_writeback_from_disk(
        &self,
        abs_path: &Path,
        ino: u64,
        item: &DriveItem,
        trigger: &str,
    ) -> bool {
        let item_id = match self.core.inodes().get_item_id(ino) {
            Some(id) => id,
            None => {
                self.log_ingest_outcome("skipped", trigger, abs_path, "missing_item_id");
                return false;
            }
        };
        let drive_id = self.core.drive_id();
        let file_name = item.name.clone();

        let meta = match std::fs::metadata(abs_path) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(path = %abs_path.display(), "cfapi: ingest metadata failed: {e}");
                self.log_ingest_outcome("deferred", trigger, abs_path, "metadata_unavailable");
                self.defer_ingest(abs_path, "metadata_unavailable");
                return false;
            }
        };

        if meta.is_dir() {
            self.log_ingest_outcome("skipped", trigger, abs_path, "directory");
            return false;
        }

        // Skip the unmodified guard for local:* items — they haven't been
        // uploaded yet, so the cached DriveItem was created with the SAME mtime
        // and size as the local file, making the comparison always conclude
        // "unmodified".  The guard is only meaningful for items that already
        // exist on OneDrive.
        if !item_id.starts_with("local:")
            && let Some(server_mtime) = item.last_modified
            && let Ok(file_sys_time) = meta.modified()
        {
            let file_mtime = chrono::DateTime::<chrono::Utc>::from(file_sys_time);
            let diff = (file_mtime - server_mtime).num_seconds().unsigned_abs();
            if diff < 1 && meta.len() == item.size as u64 {
                self.log_ingest_outcome("skipped", trigger, abs_path, "unmodified");
                return false;
            }
        }

        if meta.len() <= SMALL_FILE_LIMIT as u64 {
            let disk_content = match std::fs::read(abs_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(
                        "failed to read file for writeback {}: {e}",
                        abs_path.display()
                    );
                    self.core.send_event(VfsEvent::WritebackFailed {
                        file_name: file_name.clone(),
                    });
                    self.log_ingest_outcome("deferred", trigger, abs_path, "read_failed");
                    self.defer_ingest(abs_path, "read_failed");
                    return false;
                }
            };
            if let Err(e) = block_on_compat(
                self.core.rt(),
                self.core
                    .cache()
                    .writeback
                    .write(drive_id, &item_id, &disk_content),
            ) {
                tracing::error!("writeback write failed for {}: {e}", abs_path.display());
                self.core.send_event(VfsEvent::WritebackFailed {
                    file_name: file_name.clone(),
                });
                self.log_ingest_outcome("deferred", trigger, abs_path, "writeback_write_failed");
                self.defer_ingest(abs_path, "writeback_write_failed");
                return false;
            }
        } else {
            const CHUNK_SIZE: usize = 64 * 1024;
            use std::io::Read;
            let file = match std::fs::File::open(abs_path) {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!(
                        "failed to open file for writeback {}: {e}",
                        abs_path.display()
                    );
                    self.core.send_event(VfsEvent::WritebackFailed {
                        file_name: file_name.clone(),
                    });
                    self.log_ingest_outcome("deferred", trigger, abs_path, "open_failed");
                    self.defer_ingest(abs_path, "open_failed");
                    return false;
                }
            };
            let mut reader = std::io::BufReader::with_capacity(CHUNK_SIZE, file);
            let mut buf = vec![0u8; CHUNK_SIZE];
            let mut offset: u64 = 0;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if let Err(e) = block_on_compat(
                            self.core.rt(),
                            self.core.cache().writeback.write_chunk(
                                drive_id,
                                &item_id,
                                offset,
                                &buf[..n],
                            ),
                        ) {
                            tracing::error!(
                                "writeback chunk write failed for {}: {e}",
                                abs_path.display()
                            );
                            self.core.send_event(VfsEvent::WritebackFailed {
                                file_name: file_name.clone(),
                            });
                            self.log_ingest_outcome(
                                "deferred",
                                trigger,
                                abs_path,
                                "writeback_chunk_failed",
                            );
                            self.defer_ingest(abs_path, "writeback_chunk_failed");
                            return false;
                        }
                        offset += n as u64;
                    }
                    Err(e) => {
                        tracing::error!(
                            "failed to read chunk for writeback {}: {e}",
                            abs_path.display()
                        );
                        self.core.send_event(VfsEvent::WritebackFailed {
                            file_name: file_name.clone(),
                        });
                        self.log_ingest_outcome("deferred", trigger, abs_path, "chunk_read_failed");
                        self.defer_ingest(abs_path, "chunk_read_failed");
                        return false;
                    }
                }
            }
            if let Err(e) = block_on_compat(
                self.core.rt(),
                self.core
                    .cache()
                    .writeback
                    .finish_chunked_write(drive_id, &item_id),
            ) {
                tracing::error!("writeback finalize failed for {}: {e}", abs_path.display());
                self.core.send_event(VfsEvent::WritebackFailed {
                    file_name: file_name.clone(),
                });
                self.log_ingest_outcome("deferred", trigger, abs_path, "writeback_finalize_failed");
                self.defer_ingest(abs_path, "writeback_finalize_failed");
                return false;
            }
        }

        self.mark_placeholder_pending(abs_path);

        match self.core.flush_inode(ino) {
            Ok(()) => {
                if let Some(updated_item) = self.core.lookup_item(ino) {
                    self.mark_placeholder_synced(abs_path, &updated_item);
                }
                self.clear_deferred_ingest(abs_path);
                self.log_ingest_outcome("enqueued", trigger, abs_path, "flushed");
                true
            }
            Err(e) => {
                tracing::error!(
                    "flush after ingest failed for {}: {e:?}",
                    abs_path.display()
                );
                self.core.send_event(VfsEvent::WritebackFailed {
                    file_name: item.name.clone(),
                });
                self.defer_ingest(abs_path, "flush_failed");
                false
            }
        }
    }

    fn ingest_local_change(&self, abs_path: &Path, trigger: &str) {
        self.process_safe_save_timeouts();
        self.process_deferred_timeouts();

        let Some(components) = self.relative_components(abs_path) else {
            self.log_ingest_outcome("skipped", trigger, abs_path, "outside_sync_root");
            return;
        };

        if components.is_empty() {
            self.log_ingest_outcome("skipped", trigger, abs_path, "sync_root");
            return;
        }

        if let Some((ino, item)) = self.core.resolve_path(&components) {
            if item.is_folder() {
                self.log_ingest_outcome("skipped", trigger, abs_path, "folder");
                return;
            }
            let _ = self.stage_writeback_from_disk(abs_path, ino, &item, trigger);
            return;
        }

        let Some((parent_components, child_name)) = Self::resolve_parent_and_name(&components)
        else {
            self.defer_ingest(abs_path, "missing_parent_components");
            return;
        };

        let Some((parent_ino, _)) = self.core.resolve_path(parent_components) else {
            self.defer_ingest(abs_path, "parent_unresolved");
            return;
        };

        let meta = match std::fs::metadata(abs_path) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!(path = %abs_path.display(), "cfapi: ingest metadata unavailable: {e}");
                self.defer_ingest(abs_path, "metadata_unavailable");
                return;
            }
        };

        if meta.is_dir() {
            self.log_ingest_outcome("skipped", trigger, abs_path, "directory");
            return;
        }

        let Some(name) = child_name.to_str() else {
            self.log_ingest_outcome("skipped", trigger, abs_path, "non_utf8_name");
            return;
        };

        let modified = meta
            .modified()
            .ok()
            .map(chrono::DateTime::<chrono::Utc>::from);
        match self
            .core
            .register_local_file(parent_ino, name, meta.len(), modified)
        {
            Ok((ino, item)) => {
                let _ = self.stage_writeback_from_disk(abs_path, ino, &item, trigger);
            }
            Err(e) => {
                tracing::warn!(
                    path = %abs_path.display(),
                    "cfapi: local ingest registration failed: {e:?}"
                );
                self.defer_ingest(abs_path, "register_local_file_failed");
            }
        }
    }

    fn retry_deferred_ingest(&self) {
        let pending: Vec<PathBuf> = self
            .deferred_ingest
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        for path in pending {
            self.log_ingest_outcome("retried", "state_changed", &path, "deferred_retry");
            self.ingest_local_change(&path, "state_changed_retry");
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
        let abs_path = request.path();
        let Some(components) = self.relative_components(&abs_path) else {
            tracing::warn!("cfapi: fetch_data called for path outside sync root");
            return Ok(());
        };

        let item_id = match std::str::from_utf8(request.file_blob()) {
            Ok(s) => s.to_owned(),
            Err(e) => {
                tracing::warn!(path = %abs_path.display(), "cfapi: fetch_data blob decode failed: {e:?}");
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
            match self.core.resolve_path(&components) {
                Some((ino, _)) => ino,
                None => {
                    // Item not in cache yet — allocate a fresh inode so
                    // read_range_direct can look up item_id and download.
                    tracing::debug!(
                        path = %abs_path.display(),
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
                tracing::warn!(path = %abs_path.display(), "cfapi: fetch_data download failed: {e:?}");
                return Ok(());
            }
        };

        if content.is_empty() {
            tracing::warn!(path = %abs_path.display(), "cfapi: fetch_data got empty content, skipping");
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
                tracing::warn!(path = %abs_path.display(), "cfapi: fetch_data write_at failed: {e:?}");
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
        let components = self
            .relative_components(&dir_path)
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        let (parent_ino, _) = self
            .core
            .resolve_path(&components)
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
        self.process_safe_save_timeouts();
        self.process_deferred_timeouts();
        if info.deleted() {
            tracing::debug!(path = %request.path().display(), "cfapi: closed guard skipped deleted file");
            return;
        }

        let abs_path = request.path();
        let Some(components) = self.relative_components(&abs_path) else {
            tracing::warn!(path = %abs_path.display(), "cfapi: closed guard skipped outside sync root");
            return;
        };
        let Some((ino, item)) = self.core.resolve_path(&components) else {
            tracing::warn!(path = %abs_path.display(), "cfapi: closed unresolved path, handing off to local-change ingest");
            self.ingest_local_change(&abs_path, "closed_unresolved");
            return;
        };

        if item.is_folder() {
            tracing::debug!(path = %abs_path.display(), "cfapi: closed guard skipped folder");
            return;
        }

        let _ = self.stage_writeback_from_disk(&abs_path, ino, &item, "closed");
    }

    fn dehydrate(
        &self,
        request: Request,
        ticket: ticket::Dehydrate,
        _info: info::Dehydrate,
    ) -> CResult<()> {
        let components = self
            .relative_components(&request.path())
            .unwrap_or_default();

        if let Some((ino, _)) = self.core.resolve_path(&components) {
            let item_id = self.core.inodes().get_item_id(ino);
            if let Some(ref id) = item_id {
                let _ = block_on_compat(
                    self.core.rt(),
                    self.core.cache().disk.remove(self.core.drive_id(), id),
                );
            }
        }

        if let Err(e) = ticket.pass() {
            tracing::warn!("cfapi: dehydrate ticket.pass() failed: {e:?}");
        }
        Ok(())
    }

    fn delete(&self, request: Request, ticket: ticket::Delete, _info: info::Delete) -> CResult<()> {
        let abs_path = request.path();
        let components = self
            .relative_components(&abs_path)
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        let Some((parent_components, child_name)) = Self::resolve_parent_and_name(&components)
        else {
            tracing::warn!(path = %abs_path.display(), "cfapi: delete on sync root");
            return Ok(());
        };

        let Some((parent_ino, _)) = self.core.resolve_path(parent_components) else {
            tracing::warn!(path = %abs_path.display(), "cfapi: delete parent not found");
            return Ok(());
        };

        let is_folder = self
            .core
            .find_child(parent_ino, child_name)
            .map(|(_, item)| item.is_folder())
            .unwrap_or(false);

        let Some(name_str) = child_name.to_str() else {
            tracing::warn!(path = %abs_path.display(), "cfapi: delete filename not valid UTF-8");
            return Ok(());
        };

        let result = if is_folder {
            self.core.rmdir(parent_ino, name_str)
        } else {
            self.core.unlink(parent_ino, name_str)
        };

        match result {
            Ok(()) => {
                if let Err(e) = ticket.pass() {
                    tracing::warn!(path = %abs_path.display(), "cfapi: delete ticket.pass() failed: {e:?}");
                }
            }
            Err(e) => {
                tracing::warn!(path = %abs_path.display(), "cfapi: delete failed: {e:?}");
            }
        }

        Ok(())
    }

    fn rename(&self, request: Request, ticket: ticket::Rename, info: info::Rename) -> CResult<()> {
        self.process_safe_save_timeouts();
        self.process_deferred_timeouts();

        let abs_path = request.path();
        let src_components = self
            .relative_components(&abs_path)
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        let target_path = info.target_path();
        let dst_components = self
            .relative_components(&target_path)
            .ok_or(CloudErrorKind::NotUnderSyncRoot)?;

        let Some((src_parent_comps, src_child)) = Self::resolve_parent_and_name(&src_components)
        else {
            tracing::warn!(path = %abs_path.display(), "cfapi: rename source is sync root");
            return Ok(());
        };

        let Some((dst_parent_comps, dst_child)) = Self::resolve_parent_and_name(&dst_components)
        else {
            tracing::warn!(path = %target_path.display(), "cfapi: rename target is sync root");
            return Ok(());
        };

        let Some((src_parent_ino, _)) = self.core.resolve_path(src_parent_comps) else {
            tracing::warn!(path = %abs_path.display(), "cfapi: rename source parent not found");
            return Ok(());
        };

        let Some((dst_parent_ino, _)) = self.core.resolve_path(dst_parent_comps) else {
            tracing::warn!(path = %target_path.display(), "cfapi: rename target parent not found");
            return Ok(());
        };

        let (Some(src_name), Some(dst_name)) = (src_child.to_str(), dst_child.to_str()) else {
            tracing::warn!("cfapi: rename filenames not valid UTF-8");
            return Ok(());
        };

        if self.reconcile_safe_save_replacement(&abs_path, &target_path) {
            if let Err(e) = ticket.pass() {
                tracing::warn!(
                    path = %abs_path.display(),
                    target = %target_path.display(),
                    "cfapi: rename safe-save replacement ticket.pass() failed: {e:?}"
                );
            }
            self.ingest_local_change(&target_path, "rename_safe_save_reconcile");
            return Ok(());
        }

        if Self::should_defer_rename(src_name, dst_name) {
            let txn = SafeSaveTxn {
                source_parent_ino: src_parent_ino,
                source_name: src_name.to_string(),
                target_parent_ino: dst_parent_ino,
                target_name: dst_name.to_string(),
                source_path: abs_path.clone(),
                target_path: target_path.clone(),
                created_at: Instant::now(),
            };
            self.safe_save_txns.lock().unwrap().push(txn);
            tracing::info!(
                source = %abs_path.display(),
                target = %target_path.display(),
                "cfapi: safe-save rename deferred for reconciliation"
            );
            if let Err(e) = ticket.pass() {
                tracing::warn!(
                    path = %abs_path.display(),
                    target = %target_path.display(),
                    "cfapi: rename deferred ticket.pass() failed: {e:?}"
                );
            }
            return Ok(());
        }

        match self
            .core
            .rename(src_parent_ino, src_name, dst_parent_ino, dst_name)
        {
            Ok(()) => {
                if let Err(e) = ticket.pass() {
                    tracing::warn!(
                        path = %abs_path.display(),
                        "cfapi: rename ticket.pass() failed: {e:?}"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    path = %abs_path.display(),
                    target = %target_path.display(),
                    "cfapi: rename failed: {e:?}"
                );
                if let Err(te) = ticket.pass() {
                    tracing::warn!(
                        path = %abs_path.display(),
                        "cfapi: rename ticket.pass() failed after Graph error: {te:?}"
                    );
                }
                tracing::info!(
                    path = %abs_path.display(),
                    target = %target_path.display(),
                    "cfapi: rename acknowledged to OS despite Graph API failure"
                );
            }
        }

        Ok(())
    }

    fn state_changed(&self, changes: Vec<PathBuf>) {
        for path in &changes {
            tracing::debug!("state changed: {}", path.display());
            if let Some(components) = self.relative_components(path) {
                if let Some((ino, _)) = self.core.resolve_path(&components) {
                    self.core.cache().memory.invalidate(ino);

                    if !components.is_empty() {
                        let parent_components = &components[..components.len() - 1];
                        if let Some((parent_ino, _)) = self.core.resolve_path(parent_components) {
                            self.core.cache().memory.invalidate(parent_ino);
                        }
                    }
                } else {
                    tracing::warn!(
                        path = %path.display(),
                        "cfapi: state_changed unresolved path, attempting best-effort ingest"
                    );
                }

                if !components.is_empty() {
                    self.ingest_local_change(path, "state_changed");
                }
            } else {
                tracing::warn!(
                    path = %path.display(),
                    "cfapi: state_changed ignored path outside sync root"
                );
            }
        }
        self.retry_deferred_ingest();
    }
}

/// Newtype wrapper to implement `SyncFilter` for `Arc<CloudMountCfFilter>`.
/// Required because the orphan rule prevents implementing a foreign trait
/// (`SyncFilter` from cloud_filter) for a foreign type (`Arc` from std).
struct FilterWrapper(Arc<CloudMountCfFilter>);

impl SyncFilter for FilterWrapper {
    fn fetch_data(
        &self,
        request: Request,
        ticket: ticket::FetchData,
        info: info::FetchData,
    ) -> CResult<()> {
        self.0.fetch_data(request, ticket, info)
    }

    fn fetch_placeholders(
        &self,
        request: Request,
        ticket: ticket::FetchPlaceholders,
        info: info::FetchPlaceholders,
    ) -> CResult<()> {
        self.0.fetch_placeholders(request, ticket, info)
    }

    fn closed(&self, request: Request, info: info::Closed) {
        self.0.closed(request, info)
    }

    fn dehydrate(
        &self,
        request: Request,
        ticket: ticket::Dehydrate,
        info: info::Dehydrate,
    ) -> CResult<()> {
        self.0.dehydrate(request, ticket, info)
    }

    fn delete(&self, request: Request, ticket: ticket::Delete, info: info::Delete) -> CResult<()> {
        self.0.delete(request, ticket, info)
    }

    fn rename(&self, request: Request, ticket: ticket::Rename, info: info::Rename) -> CResult<()> {
        self.0.rename(request, ticket, info)
    }

    fn state_changed(&self, changes: Vec<PathBuf>) {
        self.0.state_changed(changes)
    }
}

/// Convert a `DriveItem` into CfApi `Metadata` for placeholder creation/update.
pub(crate) fn item_to_metadata(item: &DriveItem) -> Metadata {
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

/// Apply post-delta-sync placeholder updates to CfApi NTFS placeholders.
///
/// For changed items: updates placeholder metadata (size, timestamps), dehydrates
/// the content so the next access triggers a fresh `fetch_data()`, and marks as in-sync.
/// For deleted items: removes the placeholder file/directory from the filesystem.
///
/// Skips items with pending writeback to avoid discarding local changes.
pub fn apply_delta_placeholder_updates(
    mount_path: &Path,
    changed: &[(PathBuf, DriveItem)],
    deleted: &[PathBuf],
    writeback: &cloudmount_cache::writeback::WriteBackBuffer,
    drive_id: &str,
) {
    // Process changed items
    for (relative_path, item) in changed {
        let abs_path = mount_path.join(relative_path);
        if !abs_path.exists() {
            tracing::debug!(
                "delta placeholder update: skipping non-existent {}",
                abs_path.display()
            );
            continue;
        }

        // Safety: skip dehydration for items with pending local writes
        if writeback.has_pending(drive_id, &item.id) {
            tracing::warn!(
                "delta placeholder update: skipping {} — pending writeback for item {}",
                abs_path.display(),
                item.id
            );
            continue;
        }

        match Placeholder::open(&abs_path) {
            Ok(mut ph) => {
                let mut update = UpdateOptions::default()
                    .metadata(item_to_metadata(item))
                    .mark_in_sync()
                    .blob(item.id.as_bytes());

                // Only dehydrate files, not folders (folders have no content)
                if !item.is_folder() {
                    update = update.dehydrate();
                }

                if let Err(e) = ph.update(update, None) {
                    tracing::warn!(
                        "delta placeholder update: failed to update {}: {e:?}",
                        abs_path.display()
                    );
                }
            }
            Err(e) => {
                tracing::warn!(
                    "delta placeholder update: failed to open {}: {e:?}",
                    abs_path.display()
                );
            }
        }
    }

    // Process deleted items
    for relative_path in deleted {
        let abs_path = mount_path.join(relative_path);
        if !abs_path.exists() {
            // Already absent — desired state achieved
            continue;
        }

        let result = if abs_path.is_dir() {
            std::fs::remove_dir(&abs_path)
        } else {
            std::fs::remove_file(&abs_path)
        };

        if let Err(e) = result {
            tracing::warn!(
                "delta placeholder delete: failed to remove {}: {e}",
                abs_path.display()
            );
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

/// Strip the `\\?\` prefix that `std::fs::canonicalize` adds on Windows.
/// WinRT `StorageFolder::GetFolderFromPathAsync` does not accept this prefix.
fn strip_win32_long_path_prefix(path: &Path) -> std::borrow::Cow<'_, Path> {
    let s = path.as_os_str().to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        std::borrow::Cow::Owned(PathBuf::from(stripped))
    } else {
        std::borrow::Cow::Borrowed(path)
    }
}

#[cfg(target_os = "windows")]
fn register_context_menu() -> cloudmount_core::Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let shell_path = r"Software\Classes\*\shell\CloudMount.OpenInSharePoint";
    let command_path = format!("{}\\command", shell_path);

    let (shell_key, _) = hkcu.create_subkey(shell_path).map_err(|e| {
        cloudmount_core::Error::Filesystem(format!("failed to create shell key: {e:?}"))
    })?;

    shell_key
        .set_value("", &"Open in SharePoint".to_string())
        .map_err(|e| {
            cloudmount_core::Error::Filesystem(format!("failed to set display name: {e:?}"))
        })?;

    let (command_key, _) = hkcu.create_subkey(&command_path).map_err(|e| {
        cloudmount_core::Error::Filesystem(format!("failed to create command key: {e:?}"))
    })?;

    let cmd_value = r#"powershell -NoProfile -WindowStyle Hidden -Command "$path = $args[0]; $encoded = [Uri]::EscapeDataString($path); Start-Process ('cloudmount://open-online?path=' + $encoded)" "%1""#;
    command_key
        .set_value("", &cmd_value.to_string())
        .map_err(|e| cloudmount_core::Error::Filesystem(format!("failed to set command: {e:?}")))?;

    tracing::info!("registered Windows Explorer context menu for 'Open in SharePoint'");
    Ok(())
}

#[cfg(target_os = "windows")]
fn unregister_context_menu() -> cloudmount_core::Result<()> {
    use winreg::RegKey;
    use winreg::enums::*;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let shell_path = r"Software\Classes\*\shell\CloudMount.OpenInSharePoint";

    let result = hkcu.delete_subkey_all(shell_path);
    match result {
        Ok(()) => {
            tracing::info!("unregistered Windows Explorer context menu");
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::debug!("context menu keys already absent, treating as success");
        }
        Err(e) => {
            return Err(cloudmount_core::Error::Filesystem(format!(
                "failed to remove context menu keys: {e:?}"
            )));
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn on_mount_added() {
    let prev = ACTIVE_CFAPI_MOUNTS.fetch_add(1, Ordering::SeqCst);
    if prev == 0
        && let Err(e) = register_context_menu()
    {
        tracing::error!("failed to register context menu on first mount: {e}");
    }
}

#[cfg(target_os = "windows")]
fn on_mount_removed() {
    let prev = ACTIVE_CFAPI_MOUNTS.fetch_sub(1, Ordering::SeqCst);
    if prev == 1
        && let Err(e) = unregister_context_menu()
    {
        tracing::error!("failed to unregister context menu on last mount: {e}");
    }
}

#[cfg(not(target_os = "windows"))]
fn on_mount_added() {}

#[cfg(not(target_os = "windows"))]
fn on_mount_removed() {}

#[cfg(target_os = "windows")]
pub fn active_mount_count() -> usize {
    ACTIVE_CFAPI_MOUNTS.load(Ordering::SeqCst)
}

#[cfg(not(target_os = "windows"))]
fn register_context_menu() -> cloudmount_core::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn unregister_context_menu() -> cloudmount_core::Result<()> {
    Ok(())
}

fn resolve_icon_path() -> String {
    match std::env::current_exe() {
        Ok(path) => format!("{},0", path.display()),
        Err(e) => {
            tracing::warn!("failed to resolve current executable path for icon: {e}");
            "%SystemRoot%\\system32\\shell32.dll,43".to_string()
        }
    }
}

fn register_sync_root(
    sync_root_id: &SyncRootId,
    mount_path: &Path,
    display_name: &str,
    icon_path: &str,
) -> cloudmount_core::Result<()> {
    let info = SyncRootInfo::default()
        .with_display_name(display_name)
        .with_version(PROVIDER_VERSION)
        .with_icon(icon_path)
        .with_hydration_type(HydrationType::Progressive)
        .with_population_type(PopulationType::Full)
        .with_supported_attribute(
            SupportedAttribute::FileLastWriteTime
                | SupportedAttribute::DirectoryLastWriteTime
                | SupportedAttribute::FileCreationTime
                | SupportedAttribute::DirectoryCreationTime,
        )
        .with_allow_pinning(true)
        .with_show_siblings_as_group(false)
        .with_path(mount_path)
        .map_err(|e| {
            cloudmount_core::Error::Filesystem(format!(
                "sync root path invalid (path={}, len={}): {e:?}",
                mount_path.display(),
                mount_path.as_os_str().len(),
            ))
        })?;

    sync_root_id.register(info).map_err(|e| {
        cloudmount_core::Error::Filesystem(format!("sync root registration failed: {e:?}"))
    })?;

    tracing::info!(
        display_name = %display_name,
        icon_path = %icon_path,
        "registered sync root at {}",
        mount_path.display()
    );
    Ok(())
}

/// Debounce window for filesystem watcher events. Events for the same path
/// within this interval are collapsed into a single `ingest_local_change()` call.
const WATCHER_DEBOUNCE: Duration = Duration::from_millis(500);

/// Buffer size for `ReadDirectoryChangesW` notification data (~64 KiB).
const WATCHER_BUF_SIZE: usize = 64 * 1024;

/// Spawn a dedicated OS thread that monitors `mount_path` for local filesystem
/// changes using `ReadDirectoryChangesW` (synchronous/blocking mode). Events are
/// debounced per-path and routed to [`CloudMountCfFilter::ingest_local_change`].
///
/// The thread runs until `stop_flag` is set to `true` or the directory handle
/// becomes invalid (e.g. sync root unmounted).
pub fn spawn_local_watcher(
    mount_path: PathBuf,
    filter: Arc<CloudMountCfFilter>,
    stop_flag: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("cfapi-local-watcher".into())
        .spawn(move || {
            local_watcher_loop(&mount_path, &filter, &stop_flag);
        })
        .expect("failed to spawn local watcher thread")
}

fn local_watcher_loop(mount_path: &Path, filter: &CloudMountCfFilter, stop_flag: &AtomicBool) {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, FILE_ACTION_ADDED, FILE_ACTION_MODIFIED, FILE_ACTION_REMOVED,
            FILE_ACTION_RENAMED_NEW_NAME, FILE_ACTION_RENAMED_OLD_NAME, FILE_FLAG_BACKUP_SEMANTICS,
            FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
            FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE, FILE_NOTIFY_INFORMATION,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
            ReadDirectoryChangesW,
        },
    };

    let wide_path: Vec<u16> = mount_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // Open directory handle with backup semantics for ReadDirectoryChangesW.
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        tracing::error!(
            path = %mount_path.display(),
            "cfapi watcher: failed to open directory handle"
        );
        return;
    }

    // Ensure the handle is closed when the function returns.
    struct HandleGuard(HANDLE);
    impl Drop for HandleGuard {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }
    let _guard = HandleGuard(handle);

    let notify_filter = FILE_NOTIFY_CHANGE_FILE_NAME
        | FILE_NOTIFY_CHANGE_DIR_NAME
        | FILE_NOTIFY_CHANGE_SIZE
        | FILE_NOTIFY_CHANGE_LAST_WRITE;

    let mut buffer = vec![0u8; WATCHER_BUF_SIZE];
    let mut debounce: HashMap<PathBuf, Instant> = HashMap::new();

    tracing::info!(
        path = %mount_path.display(),
        "cfapi watcher: started local filesystem watcher"
    );

    while !stop_flag.load(Ordering::Relaxed) {
        let mut bytes_returned: u32 = 0;

        // Synchronous (blocking) call — the dedicated thread is expected to block here.
        let ok = unsafe {
            ReadDirectoryChangesW(
                handle,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                1, // bWatchSubtree = TRUE (recursive)
                notify_filter,
                &mut bytes_returned,
                std::ptr::null_mut(), // no overlapped — synchronous
                None,                 // no completion routine
            )
        };

        if ok == 0 {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }
            tracing::warn!(
                path = %mount_path.display(),
                "cfapi watcher: ReadDirectoryChangesW failed, retrying in 1s"
            );
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }

        if bytes_returned == 0 {
            continue;
        }

        // Parse FILE_NOTIFY_INFORMATION entries and collect affected paths.
        let now = Instant::now();
        let mut offset: usize = 0;
        loop {
            if offset + std::mem::size_of::<FILE_NOTIFY_INFORMATION>() > bytes_returned as usize {
                break;
            }

            // Safety: offset is validated above and aligns to a FILE_NOTIFY_INFORMATION header.
            let info = unsafe { &*(buffer.as_ptr().add(offset) as *const FILE_NOTIFY_INFORMATION) };

            let action = info.Action;
            let name_len_bytes = info.FileNameLength as usize;
            // The FileName field sits at a fixed byte offset within FILE_NOTIFY_INFORMATION.
            let name_offset = std::mem::offset_of!(FILE_NOTIFY_INFORMATION, FileName);
            let name_ptr = unsafe { buffer.as_ptr().add(offset).add(name_offset) as *const u16 };
            let name_len_u16 = name_len_bytes / std::mem::size_of::<u16>();

            // Safety: name_ptr points into our buffer within bounds.
            let name_slice = unsafe { std::slice::from_raw_parts(name_ptr, name_len_u16) };
            let relative = OsString::from_wide(name_slice);
            let abs_path = mount_path.join(&relative);

            let trigger = match action {
                FILE_ACTION_ADDED => "watcher:added",
                FILE_ACTION_MODIFIED => "watcher:modified",
                FILE_ACTION_REMOVED => "watcher:removed",
                FILE_ACTION_RENAMED_OLD_NAME => "watcher:renamed_old",
                FILE_ACTION_RENAMED_NEW_NAME => "watcher:renamed_new",
                _ => "watcher:unknown",
            };

            // Per-path debouncing: skip if last event for this path was < 500ms ago.
            let should_ingest = debounce
                .get(&abs_path)
                .map_or(true, |last| now.duration_since(*last) >= WATCHER_DEBOUNCE);

            if should_ingest {
                debounce.insert(abs_path.clone(), now);
                filter.ingest_local_change(&abs_path, trigger);
            }

            // Advance to next entry or break.
            if info.NextEntryOffset == 0 {
                break;
            }
            offset += info.NextEntryOffset as usize;
        }

        // Flush expired debounce entries to prevent unbounded memory growth.
        debounce.retain(|_, last| now.duration_since(*last) < WATCHER_DEBOUNCE * 4);
    }

    tracing::info!(
        path = %mount_path.display(),
        "cfapi watcher: local filesystem watcher stopped"
    );
}

/// Spawns a background thread that periodically processes deferred operations.
///
/// Wakes every 500ms to run `process_safe_save_timeouts()`, `process_deferred_timeouts()`,
/// and `retry_deferred_ingest()`. This ensures deferred items are processed even when no
/// further CfApi callback fires within the timeout window.
pub fn spawn_periodic_timer(
    filter: Arc<CloudMountCfFilter>,
    stop_flag: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("cfapi-periodic-timer".into())
        .spawn(move || {
            tracing::info!("cfapi periodic timer: started");

            while !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));

                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }

                filter.process_safe_save_timeouts();
                filter.process_deferred_timeouts();
                filter.retry_deferred_ingest();
            }

            tracing::info!("cfapi periodic timer: stopped");
        })
        .expect("failed to spawn periodic timer thread")
}

pub struct CfMountHandle {
    /// Must be dropped before `sync_root_id` is unregistered. See `unmount()`.
    connection: Connection<FilterWrapper>,
    sync_root_id: SyncRootId,
    cache: Arc<CacheManager>,
    graph: Arc<GraphClient>,
    drive_id: String,
    rt: Handle,
    mount_path: PathBuf,
    stop_flag: Arc<AtomicBool>,
    watcher_handle: Option<std::thread::JoinHandle<()>>,
    timer_handle: Option<std::thread::JoinHandle<()>>,
}

impl CfMountHandle {
    #[allow(clippy::too_many_arguments)] // constructor — all params are required
    pub fn mount(
        graph: Arc<GraphClient>,
        cache: Arc<CacheManager>,
        inodes: Arc<InodeTable>,
        drive_id: String,
        mount_path: &Path,
        rt: Handle,
        account_name: String,
        display_name: String,
        event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
    ) -> cloudmount_core::Result<Self> {
        let sync_root_id = build_sync_root_id(&account_name)?;

        ensure_mount_dir(mount_path)?;

        // Canonicalize the path after creating the directory. WinRT's
        // StorageFolder::GetFolderFromPathAsync (used by cloud-filter to set
        // the sync root path) is stricter than Win32 file APIs — it rejects
        // mixed separators, relative components, and paths over MAX_PATH.
        // std::fs::canonicalize on Windows adds a \\?\ prefix that WinRT also
        // rejects, so we strip it.
        let canonical = std::fs::canonicalize(mount_path).map_err(|e| {
            cloudmount_core::Error::Filesystem(format!(
                "failed to canonicalize mount path {}: {e}",
                mount_path.display()
            ))
        })?;
        let mount_path = strip_win32_long_path_prefix(&canonical);
        let mount_path = mount_path.as_ref();

        tracing::debug!(
            path = %mount_path.display(),
            len = mount_path.as_os_str().len(),
            "canonicalized mount path"
        );

        let icon_path = resolve_icon_path();
        register_sync_root(&sync_root_id, mount_path, &display_name, &icon_path)?;

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

        let filter = Arc::new(CloudMountCfFilter::new(
            graph.clone(),
            cache.clone(),
            inodes,
            drive_id.clone(),
            rt.clone(),
            mount_path.to_path_buf(),
            event_tx,
        ));

        // Clone the Arc before Session::connect() takes ownership of the
        // outer Arc<CloudMountCfFilter> value. The watcher and timer threads
        // share the same filter instance to call ingest_local_change() and
        // process timeouts.
        let filter_for_threads = filter.clone();

        let connection = Session::new().connect(mount_path, FilterWrapper(filter)).map_err(|e| {
            cloudmount_core::Error::Filesystem(format!("CfApi connect failed: {e:?}"))
        })?;

        let stop_flag = Arc::new(AtomicBool::new(false));
        let watcher_handle = Some(spawn_local_watcher(
            mount_path.to_path_buf(),
            filter_for_threads.clone(),
            stop_flag.clone(),
        ));
        let timer_handle = Some(spawn_periodic_timer(filter_for_threads, stop_flag.clone()));

        on_mount_added();

        tracing::info!("mounted at {} via Cloud Files API", mount_path.display());

        Ok(Self {
            connection,
            sync_root_id,
            cache,
            graph,
            drive_id,
            rt,
            mount_path: mount_path.to_path_buf(),
            stop_flag,
            watcher_handle,
            timer_handle,
        })
    }

    pub fn mount_path(&self) -> &Path {
        &self.mount_path
    }

    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    pub fn unmount(mut self) -> cloudmount_core::Result<()> {
        tracing::info!(
            "unmounting Cloud Files API at {}",
            self.mount_path.display()
        );
        block_on_compat(
            &self.rt,
            crate::pending::flush_pending(&self.cache, &self.graph, &self.drive_id),
        );

        // Signal watcher and timer threads to stop, then join them before
        // disconnecting from the CfApi (which happens when Connection drops).
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(h) = self.watcher_handle.take() {
            if let Err(e) = h.join() {
                tracing::warn!("cfapi watcher thread panicked: {e:?}");
            }
        }
        if let Some(h) = self.timer_handle.take() {
            if let Err(e) = h.join() {
                tracing::warn!("cfapi timer thread panicked: {e:?}");
            }
        }

        drop(self.connection);
        let unregister_result = self.sync_root_id.unregister();
        on_mount_removed();
        unregister_result.map_err(|e| {
            cloudmount_core::Error::Filesystem(format!("sync root unregister failed: {e:?}"))
        })?;
        tracing::info!("unregistered sync root for {}", self.mount_path.display());
        Ok(())
    }
}

pub async fn shutdown_on_signal(mounts: Arc<Mutex<Vec<CfMountHandle>>>) {
    let _ = tokio::signal::ctrl_c().await;

    tracing::info!("shutdown signal received, unmounting all Cloud Files API drives");
    let handles = std::mem::take(&mut *mounts.lock().unwrap());
    for handle in handles {
        if let Err(e) = handle.unmount() {
            tracing::error!("unmount failed: {e}");
        }
    }
}
