pub mod core_ops;
pub mod inode;
pub(crate) mod pending;

pub use pending::recover_pending_writes;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod fuse_fs;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod mount;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use fuse_fs::CloudMountFs;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use fuse_fs::FuseDeltaObserver;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use mount::MountHandle;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use mount::cleanup_stale_mount;

#[cfg(target_os = "windows")]
pub mod cfapi;

#[cfg(target_os = "windows")]
pub use cfapi::CfMountHandle;

#[cfg(target_os = "windows")]
pub use cfapi::apply_delta_placeholder_updates;

#[cfg(all(test, target_os = "windows"))]
pub use cfapi::active_mount_count;
