## Context

The VFS layer currently treats every `read()` and `write()` as a stateless operation keyed by inode. `open()` returns `FileHandle(0)` without loading content. Each `read()` call re-fetches the entire file from writeback → disk cache → network. Each `write()` call reads the full existing buffer, clones it, splices in new data, and writes the whole thing back to the `DashMap`-backed writeback buffer. This is O(n) per read and O(n) per write where n is file size — catastrophic for files over a few MB.

FUSE typically issues many small read/write calls (128KB reads, 4KB writes without `big_writes`). A 100MB file read generates ~800 calls each loading 100MB. A 100MB sequential write does ~25,000 clone-and-replace cycles totaling ~10GB of memcpy.

The writeback buffer (`WriteBackBuffer`) stores full content in a `DashMap<String, Vec<u8>>` keyed by `"{drive_id}\0{item_id}"`. This is fine for persistence but wrong for the hot write path — it forces a full replace on every small write.

## Goals / Non-Goals

**Goals:**
- Eliminate redundant file content loads on `read()` — load once on `open()`, slice on `read()`
- Eliminate O(n) buffer cloning on `write()` — mutate in-place within the open file buffer
- Implement proper file handle lifecycle: `open` → `read`/`write` → `flush` → `release`
- Keep writeback buffer as the persistence/crash-safety layer, but only write to it on flush/release
- Cross-platform: design works for both FUSE and CfApi backends

**Non-Goals:**
- Streaming/range downloads for large files (future optimization #3)
- FUSE mount option tuning like `big_writes`, `max_read` (separate change #4)
- Shared content buffers for multiple read-only handles to the same inode (premature optimization)
- Memory pressure management / eviction of open file buffers (files are bounded by what's actually open)

## Decisions

### D1: OpenFileTable as a DashMap in CoreOps

The `OpenFileTable` is a `DashMap<u64, OpenFile>` where the key is a file handle (u64), stored as a field on `CoreOps`. File handles are allocated by an `AtomicU64` counter.

**Why DashMap:** Consistent with the rest of the codebase (memory cache uses DashMap). Lock-free concurrent reads. FUSE can dispatch multiple read/write calls concurrently for different file handles.

**Why on CoreOps:** The open file table is VFS-level state shared between FUSE and CfApi backends. CoreOps already holds all shared VFS state (graph, cache, inodes).

**Alternative considered:** `RwLock<HashMap>` — simpler but creates contention on concurrent reads to different files. DashMap is already a dependency.

### D2: One buffer per file handle, not per inode

Each `open()` creates a fresh `OpenFile` with its own `Vec<u8>` content buffer, even if multiple handles point to the same inode. This is simple and correct for read-write workloads.

**Why not shared buffers:** Shared mutable buffers between handles require synchronization complexity (read-write locks per buffer, COW semantics). The common case is one handle per file. Memory cost is acceptable — users don't typically have hundreds of handles open to the same large file.

**Trade-off:** If two processes read the same 100MB file simultaneously, content is loaded twice. Acceptable — the current code loads it N*800 times for N readers. This is strictly better.

### D3: Content loaded eagerly on open(), not lazily on first read()

`open()` synchronously loads the full file content (from writeback → disk cache → network) into the `OpenFile` buffer before returning the file handle.

**Why eager:** Simple, correct, and handles the common case well. Most files opened are immediately read. Lazy loading adds complexity (tracking loaded ranges, blocking reads on download progress) for a scenario better served by the streaming optimization (non-goal #3).

**Why not lazy:** Would require a `Mutex<Option<Vec<u8>>>` or similar per-handle, turning every `read()` into a lock acquisition. The current bottleneck is redundant loads, not load latency.

**Trade-off:** Opening a large uncached file still blocks until download completes. This is the existing behavior — no regression. Future streaming work (change #3) will address this.

### D4: Dirty flag + flush-on-demand

Each `OpenFile` has a `dirty: bool` flag set on any `write()`. `flush()` and `release()` check the dirty flag — if clean, they're no-ops. If dirty, the buffer content is pushed to the writeback buffer and then the existing `flush_inode` upload path runs.

**Why:** Decouples the write hot path from writeback persistence. Writes become pure in-memory mutations. The writeback buffer remains the crash-safety and upload mechanism, but is only touched when explicitly flushed.

**Trade-off:** Crash between write and flush loses unflushed data. This is standard POSIX behavior — applications that need durability call `fsync()`. The writeback buffer is not the right place for write-ahead logging.

### D5: Truncate operates on OpenFile when file is open

`setattr` with a size change currently reads from writeback/disk, resizes, and writes back. With the open file table, if the file has an open handle, truncate should operate on the `OpenFile` buffer directly and mark it dirty.

**Implementation:** `CoreOps::truncate()` first checks if any open handle exists for the inode. If so, resize that buffer. If not, fall back to current behavior (for truncate-without-open, which POSIX allows).

## Risks / Trade-offs

**[Memory usage]** → Open files hold their full content in memory. A user opening a 1GB file consumes 1GB of RAM. This is inherent to the buffered approach and matches how most FUSE filesystems work. Mitigation: future streaming work for large files.

**[Crash safety]** → Data written but not flushed is lost on crash. Mitigation: This matches POSIX semantics. Applications that need durability call `fsync()`. We could add periodic auto-flush as a future enhancement.

**[Concurrent handle mutation]** → Two handles writing to the same inode produce last-writer-wins on flush. Mitigation: This matches POSIX behavior for independent file descriptors. Conflict detection on upload (eTag check) catches remote conflicts.

**[Handle leak]** → If FUSE never calls `release()` (process crash), the buffer stays in memory. Mitigation: The kernel guarantees `release()` is called when the last fd reference is closed, even on process crash. Kernel crash would lose state anyway.
