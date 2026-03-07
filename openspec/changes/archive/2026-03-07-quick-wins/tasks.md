## 1. FUSE mount options and capabilities

- [x] 1.1 Add `MountOption::CUSTOM("max_read=1048576".into())` and `MountOption::NoAtime` to mount options in `fuse_fs.rs::mount()`
- [x] 1.2 Implement `Filesystem::init()` on `CloudMountFs` to call `config.add_capabilities(InitFlags::FUSE_WRITEBACK_CACHE | InitFlags::FUSE_PARALLEL_DIROPS)` with graceful degradation (log warning on `Err`)
- [x] 1.3 Add `InitFlags` and `KernelConfig` to the fuser import list in `fuse_fs.rs`

## 2. SQLite prepared statement caching

- [x] 2.1 Replace all `conn.prepare(...)` calls with `conn.prepare_cached(...)` in `sqlite.rs` (5 call sites: `get_item_by_id`, `get_children`, `get_delta_token`, `max_inode`, and the `apply_delta` transaction queries)

## 3. Graph API $select

- [x] 3.1 Add `$select=id,name,size,lastModifiedDateTime,createdDateTime,eTag,parentReference,folder,file,@microsoft.graph.downloadUrl` parameter to `list_children` URL in `client.rs`
- [x] 3.2 Add the same `$select` parameter to `list_root_children` URL in `client.rs`

## 4. Tests

- [x] 4.1 Verify existing tests pass — no behavioral changes expected, only performance
- [x] 4.2 Verify `cargo clippy --all-targets --all-features` passes with no new warnings
