## 1. Graph Client

- [x] 1.1 Verify that `GraphClient::get_item(drive_id, "root")` correctly hits `/drives/{id}/items/root` — add a unit test with wiremock if not covered

## 2. VFS — Root Inode Initialization

- [x] 2.1 In `MountHandle::mount` (`crates/cloudmount-vfs/src/mount.rs`): call `rt.block_on(graph.get_item(drive_id, "root"))` before `spawn_mount2`; return `Err` if it fails
- [x] 2.2 Call `inodes.set_root(&root_item.id)` with the fetched item
- [x] 2.3 Seed the root item into `cache.memory.insert(ROOT_INODE, root_item.clone())`
- [x] 2.4 Seed the root item into `cache.sqlite` (upsert with inode = ROOT_INODE and parent = None)
- [x] 2.5 Apply the same initialization to `CfMountHandle::mount` (`crates/cloudmount-vfs/src/cfapi.rs`)

## 3. Tests

- [x] 3.1 Update `fuse_integration.rs`: remove manual `inodes.set_root(...)` call and verify the mount itself now sets root (or keep it and confirm both are consistent)
- [x] 3.2 Update `cfapi_integration.rs`: same as above
- [x] 3.3 Add test: mount with a mock Graph that returns a valid root item — verify `getattr(ROOT_INODE)` succeeds
- [x] 3.4 Add test: mount with a mock Graph that returns an error for the root fetch — verify `mount()` returns `Err`

## 4. Verify

- [x] 4.1 Run `cargo build --all-targets` with zero warnings
- [x] 4.2 Run `cargo test --all-targets` — all tests pass
- [x] 4.3 Run `cargo clippy --all-targets --all-features` — no lints
