## 1. Fix fetch_data error paths in cfapi.rs

- [x] 1.1 Read `crates/cloudmount-vfs/src/cfapi.rs` and locate the `fetch_data` method
- [x] 1.2 Replace the `resolve_path` lookup with `request.file_blob()` decode: parse the blob bytes as UTF-8 to get the item ID string; return `Err(CloudErrorKind::Unsuccessful)` if decode fails
- [x] 1.3 Look up the decoded item ID in `self.core.inodes()` to get the inode; return `Err(CloudErrorKind::Unsuccessful)` if not found
- [x] 1.4 Change the "path outside sync root" early return from `Ok(())` to `Err(CloudErrorKind::Unsuccessful)`
- [x] 1.5 Change the `read_range_direct` error return from `Ok(())` to `Err(CloudErrorKind::Unsuccessful)`
- [x] 1.6 Change the "content is empty" early return from `Ok(())` to `Err(CloudErrorKind::Unsuccessful)`
- [x] 1.7 Change the `write_at` loop failure `break` to `return Err(CloudErrorKind::Unsuccessful)`

## 2. Verify inode table access from cfapi.rs

- [x] 2.1 Confirm `CoreOps` exposes a method to look up inode by item ID (e.g., `inodes().get_inode(item_id)` or equivalent); add one if missing

## 3. Verify compilation and tests

- [x] 3.1 Run `cargo build -p cloudmount-vfs --target x86_64-pc-windows-msvc` (or cross-compile check) to confirm the change compiles with no warnings
- [ ] 3.2 Confirm `cfapi_hydrate_file_on_read` passes in Windows CI (no longer times out with error 426)
- [ ] 3.3 Confirm `cfapi_edit_and_sync_file` passes in Windows CI
- [ ] 3.4 Confirm all other cfapi integration tests still pass: `cfapi_mount_and_unmount_lifecycle`, `cfapi_browse_populates_placeholders`, `cfapi_rename_file`, `cfapi_delete_file`
