pub mod core_ops;
pub mod inode;
pub(crate) mod pending;

pub mod sync_processor;

pub use pending::recover_pending_writes;
pub use pending::retry_pending_writes_for_drive;
pub use sync_processor::{
    SyncHandle, SyncMetrics, SyncProcessorConfig, SyncProcessorDeps, SyncRequest,
    spawn_sync_processor,
};

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod fuse_fs;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod mount;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use fuse_fs::CarmineDesktopFs;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use fuse_fs::FuseDeltaObserver;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use mount::MountHandle;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use mount::cleanup_stale_mount;

#[cfg(target_os = "windows")]
pub mod winfsp_fs;

#[cfg(target_os = "windows")]
pub use winfsp_fs::WinFspMountHandle;

#[cfg(target_os = "windows")]
pub use winfsp_fs::WinFspDeltaObserver;
