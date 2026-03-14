use std::sync::Arc;

use tokio::runtime::Handle;

use crate::core_ops::VfsEvent;
use crate::fuse_fs::{CarmineDesktopFs, FuseDeltaObserver};
use crate::inode::{InodeTable, ROOT_INODE};
use carminedesktop_cache::CacheManager;
use carminedesktop_core::DeltaSyncObserver;
use carminedesktop_graph::GraphClient;

/// Detect and clean up a stale FUSE mount at `path`.
///
/// Returns `true` if the path is usable (not stale, or cleanup succeeded).
/// Returns `false` if the path is a stale mount that could not be cleaned up.
pub fn cleanup_stale_mount(path: &str) -> bool {
    let meta = std::fs::metadata(path);
    match meta {
        Ok(_) => true, // Path exists and is accessible — not stale
        Err(e) => {
            let raw = e.raw_os_error();
            // ENOTCONN = "Transport endpoint is not connected"
            // EIO = "Input/output error"
            if raw == Some(libc::ENOTCONN) || raw == Some(libc::EIO) {
                tracing::warn!(
                    "stale FUSE mount detected at {path} (errno {:?}), attempting cleanup",
                    raw.unwrap()
                );
                if try_unmount(path) {
                    tracing::info!("stale mount at {path} cleaned up successfully");
                    true
                } else {
                    tracing::warn!(
                        "failed to clean up stale mount at {path} — run `fusermount -u {path}` manually"
                    );
                    false
                }
            } else {
                // Path doesn't exist or other benign error — not stale
                true
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn try_unmount(path: &str) -> bool {
    // Try fusermount3 first (Fedora 43+ default), then fusermount
    for cmd in &["fusermount3", "fusermount"] {
        match std::process::Command::new(cmd).arg("-u").arg(path).output() {
            Ok(output) if output.status.success() => return true,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::debug!("{cmd} -u {path} failed: {stderr}");
            }
            Err(e) => {
                tracing::debug!("{cmd} not available: {e}");
            }
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn try_unmount(path: &str) -> bool {
    match std::process::Command::new("umount").arg(path).output() {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::debug!("umount {path} failed: {stderr}");
            false
        }
        Err(e) => {
            tracing::debug!("umount not available: {e}");
            false
        }
    }
}

pub struct MountHandle {
    session: fuser::BackgroundSession,
    cache: Arc<CacheManager>,
    graph: Arc<GraphClient>,
    drive_id: String,
    rt: Handle,
    mountpoint: String,
    delta_observer: Arc<FuseDeltaObserver>,
    sync_handle: Option<crate::sync_processor::SyncHandle>,
    sync_join: Option<tokio::task::JoinHandle<()>>,
}

impl MountHandle {
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
        collab_tx: Option<crate::core_ops::CollabSender>,
        collab_config: Option<carminedesktop_core::config::CollaborativeOpenConfig>,
        file_associations_registered: bool,
    ) -> carminedesktop_core::Result<Self> {
        let root_item =
            tokio::task::block_in_place(|| rt.block_on(graph.get_item(&drive_id, "root")))
                .map_err(|e| {
                    carminedesktop_core::Error::Filesystem(format!(
                        "failed to fetch root item for drive {drive_id}: {e}"
                    ))
                })?;

        inodes.set_root(&root_item.id);
        cache.memory.insert(ROOT_INODE, root_item.clone());
        cache
            .sqlite
            .upsert_item(ROOT_INODE, &drive_id, &root_item, None)?;

        // Helper: create filesystem, extract observer, and mount.
        // Returns (session, observer) on success.
        let try_mount = |auto_unmount: bool,
                         event_tx: Option<tokio::sync::mpsc::UnboundedSender<VfsEvent>>,
                         sync_handle: Option<crate::sync_processor::SyncHandle>,
                         collab_tx: Option<crate::core_ops::CollabSender>,
                         collab_config: Option<
            carminedesktop_core::config::CollaborativeOpenConfig,
        >| {
            let fs = CarmineDesktopFs::new(
                graph.clone(),
                cache.clone(),
                inodes.clone(),
                drive_id.clone(),
                mountpoint,
                rt.clone(),
                event_tx,
                sync_handle,
                collab_tx,
                collab_config,
                file_associations_registered,
            );
            let observer = fs.create_delta_observer();
            let session = fs.mount(mountpoint, auto_unmount)?;
            observer.set_notifier(session.notifier());
            Ok::<_, carminedesktop_core::Error>((session, observer))
        };

        // Try with auto_unmount first (crash safety net), fall back without it
        // since it requires fusermount3 + non-Owner ACL which isn't always available.
        let stored_handle = sync_handle.clone();
        let (session, delta_observer) = match try_mount(
            true,
            event_tx.clone(),
            sync_handle.clone(),
            collab_tx.clone(),
            collab_config.clone(),
        ) {
            Ok(result) => result,
            Err(_) => {
                tracing::warn!("auto_unmount not supported, mounting without it");
                try_mount(false, event_tx, sync_handle, collab_tx, collab_config)?
            }
        };

        tracing::info!("mounted drive {drive_id} at {mountpoint}");

        Ok(Self {
            session,
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

    pub fn mountpoint(&self) -> &str {
        &self.mountpoint
    }

    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    /// Returns the delta sync observer for this mount.
    pub fn delta_observer(&self) -> Arc<dyn DeltaSyncObserver> {
        self.delta_observer.clone()
    }

    pub fn unmount(self) -> carminedesktop_core::Result<()> {
        // Send shutdown to sync processor and await drain
        if let Some(ref sh) = self.sync_handle {
            sh.send(crate::sync_processor::SyncRequest::Shutdown);
        }
        if let Some(join) = self.sync_join {
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
        drop(self.session);
        tracing::info!("unmounted {}", self.mountpoint);
        Ok(())
    }
}

pub async fn shutdown_on_signal(mounts: Arc<std::sync::Mutex<Vec<MountHandle>>>) {
    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).ok();
        let ctrl_c = tokio::signal::ctrl_c();

        tokio::select! {
            _ = ctrl_c => {},
            _ = async {
                if let Some(ref mut s) = sigterm {
                    s.recv().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {},
        }
    }

    tracing::info!("shutdown signal received, unmounting all drives");
    let handles = std::mem::take(&mut *mounts.lock().unwrap());
    for handle in handles {
        if let Err(e) = handle.unmount() {
            tracing::error!("unmount failed: {e}");
        }
    }
}
