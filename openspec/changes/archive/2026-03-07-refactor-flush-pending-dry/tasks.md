## 1. Create shared pending module

- [x] 1.1 Create `crates/carminedesktop-vfs/src/pending.rs` with `pub(crate) const UNMOUNT_FLUSH_TIMEOUT: Duration` (30 s)
- [x] 1.2 Implement `pub(crate) async fn flush_pending(cache: &CacheManager, graph: &GraphClient, drive_id: &str)` in `pending.rs` — copy body from either existing implementation (they are identical in logic)
- [x] 1.3 Declare `mod pending;` in `crates/carminedesktop-vfs/src/lib.rs` (no `#[cfg]` gate)

## 2. Update FUSE mount handle

- [x] 2.1 In `mount.rs`: remove the `UNMOUNT_FLUSH_TIMEOUT` constant
- [x] 2.2 In `mount.rs`: remove the `flush_pending` method body from `MountHandle`
- [x] 2.3 In `mount.rs`: replace the `self.flush_pending()` call in `MountHandle::unmount` with `tokio::task::block_in_place(|| self.rt.block_on(crate::pending::flush_pending(&self.cache, &self.graph, &self.drive_id)))`

## 3. Update CfApi mount handle

- [x] 3.1 In `cfapi.rs`: remove the `UNMOUNT_FLUSH_TIMEOUT` constant
- [x] 3.2 In `cfapi.rs`: remove the `flush_pending` method body from `CfMountHandle`
- [x] 3.3 In `cfapi.rs`: replace the `self.flush_pending()` call in `CfMountHandle::unmount` with the equivalent call via `block_on_compat(&self.rt, crate::pending::flush_pending(&self.cache, &self.graph, &self.drive_id))`

## 4. Verify

- [x] 4.1 `cargo build --all-targets` passes with no warnings (`RUSTFLAGS=-Dwarnings`)
- [x] 4.2 `cargo test --all-targets` passes unchanged
- [x] 4.3 `cargo clippy --all-targets --all-features` passes with no warnings
