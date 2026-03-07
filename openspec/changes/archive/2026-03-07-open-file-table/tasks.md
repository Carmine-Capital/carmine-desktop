## 1. OpenFileTable data structure

- [x] 1.1 Add `OpenFile` struct and `OpenFileTable` to `core_ops.rs`: `OpenFile { ino: u64, content: Vec<u8>, dirty: bool }`, `OpenFileTable` as `DashMap<u64, OpenFile>` with `AtomicU64` handle counter, and methods `open() -> u64`, `get(&fh) -> Ref`, `get_mut(&fh) -> RefMut`, `remove(fh) -> Option<OpenFile>`
- [x] 1.2 Add `open_files: OpenFileTable` field to `CoreOps` and initialize it in `CoreOps::new()`

## 2. CoreOps open/release lifecycle

- [x] 2.1 Implement `CoreOps::open_file(ino: u64) -> VfsResult<u64>`: resolve item_id, load content from writeback → disk → network (reusing existing `read_content` logic), insert into `OpenFileTable`, return file handle
- [x] 2.2 Implement `CoreOps::release_file(fh: u64) -> VfsResult<()>`: if dirty, push buffer to writeback via `cache.writeback.write()`, remove entry from `OpenFileTable`

## 3. CoreOps read/write via file handle

- [x] 3.1 Implement `CoreOps::read_handle(fh: u64, offset: usize, size: usize) -> VfsResult<Vec<u8>>`: get `OpenFile` by handle, slice `content[offset..min(offset+size, len)]`, return bytes
- [x] 3.2 Implement `CoreOps::write_handle(fh: u64, offset: usize, data: &[u8]) -> VfsResult<u32>`: get_mut `OpenFile` by handle, resize buffer if needed, `copy_from_slice` at offset, set `dirty = true`, update in-memory metadata size, return `data.len()`

## 4. CoreOps flush via file handle

- [x] 4.1 Implement `CoreOps::flush_handle(fh: u64) -> VfsResult<()>`: if not dirty → no-op. If dirty → push buffer content to writeback, then delegate to existing `flush_inode()` logic. Clear dirty flag on success.

## 5. CoreOps truncate integration

- [x] 5.1 Update `CoreOps::truncate()` to check if any open handle exists for the inode (scan `OpenFileTable` by ino). If found, resize that handle's buffer and mark dirty. If no open handle, fall back to current writeback-based truncate.

## 6. CoreOps create integration

- [x] 6.1 Update `CoreOps::create_file()` to also create an `OpenFile` entry with empty content buffer and return the file handle alongside `(inode, DriveItem)`

## 7. FUSE backend wiring

- [x] 7.1 Update `fuse_fs.rs::open()` to call `self.ops.open_file(ino)` and return the real file handle instead of `FileHandle(0)`
- [x] 7.2 Update `fuse_fs.rs::read()` to call `self.ops.read_handle(fh, offset, size)` instead of `self.ops.read_content(ino)`
- [x] 7.3 Update `fuse_fs.rs::write()` to call `self.ops.write_handle(fh, offset, data)` instead of `self.ops.write_to_buffer(ino, ...)`
- [x] 7.4 Update `fuse_fs.rs::flush()` to call `self.ops.flush_handle(fh)` instead of `self.ops.flush_inode(ino)`
- [x] 7.5 Implement `fuse_fs.rs::release()` in the `Filesystem` trait to call `self.ops.release_file(fh)`
- [x] 7.6 Update `fuse_fs.rs::create()` to return the real file handle from `create_file()` instead of `FileHandle(0)`
- [x] 7.7 Update `fuse_fs.rs::fsync()` to call `self.ops.flush_handle(fh)` instead of `self.ops.flush_inode(ino)`

## 8. CfApi backend wiring

- [x] 8.1 Update `cfapi.rs` hydration/dehydration callbacks to use `CoreOps::open_file` / `read_handle` / `release_file` where applicable

## 9. Tests

- [x] 9.1 Update integration tests in `cloudmount-app/tests/integration_tests.rs` for handle-based read/write/flush semantics
- [x] 9.2 Add unit tests for `OpenFileTable`: open returns unique handles, read slices correctly, write mutates in-place, flush pushes to writeback, release cleans up, truncate on open file works
