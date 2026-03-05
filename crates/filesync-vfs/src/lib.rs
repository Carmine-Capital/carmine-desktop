pub mod core_ops;
pub mod inode;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod fuse_fs;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod mount;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use fuse_fs::FileSyncFs;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use mount::MountHandle;

#[cfg(target_os = "windows")]
pub mod cfapi;

#[cfg(target_os = "windows")]
pub use cfapi::CfMountHandle;
