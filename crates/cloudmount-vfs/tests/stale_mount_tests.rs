#![cfg(any(target_os = "linux", target_os = "macos"))]

use cloudmount_vfs::cleanup_stale_mount;

#[test]
fn cleanup_stale_mount_nonexistent_path_returns_true() {
    // A path that doesn't exist is not a stale mount
    let result = cleanup_stale_mount("/tmp/cloudmount-test-nonexistent-path-abc123xyz");
    assert!(result, "non-existent path should return true (not stale)");
}

#[test]
fn cleanup_stale_mount_normal_directory_returns_true() {
    let dir = std::env::temp_dir().join(format!(
        "cloudmount-stale-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let result = cleanup_stale_mount(dir.to_str().unwrap());
    assert!(result, "normal directory should return true (not stale)");

    let _ = std::fs::remove_dir(&dir);
}
