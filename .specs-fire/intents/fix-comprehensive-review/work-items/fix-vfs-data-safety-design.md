# Design Doc: Fix VFS Data-Loss Paths

## Problem Summary

Five critical data-loss paths exist in the VFS and cache layers. Each can silently lose user data under specific conditions (conflict, crash, large files, new files).

---

## Fix 1: Conflict Upload Error Must Abort Main Upload

### Current Behavior
`core_ops.rs:660` — `let _ =` discards the conflict copy upload error. If the conflict copy fails to upload, the main upload proceeds and overwrites the server version. The user's local divergent copy is lost.

### Design
- Replace `let _ =` with proper error propagation
- If conflict copy upload fails, log the error and return `VfsError::IoError` to the caller — do NOT proceed with the main upload
- The user retains their local copy in the writeback buffer (it was not removed), so they can retry on next flush

### Code Change
```rust
// Before (core_ops.rs:659-666)
if !parent_id.is_empty() {
    let _ = self.rt.block_on(self.graph.upload_small(...));
}

// After
if !parent_id.is_empty() {
    if let Err(e) = self.rt.block_on(self.graph.upload_small(
        &self.drive_id, &parent_id, &conflict_name,
        Bytes::from(content.clone()),
    )) {
        tracing::error!(
            "conflict copy upload failed for {}, aborting flush: {e}",
            item.name
        );
        return Err(VfsError::IoError(format!(
            "conflict copy upload failed: {e}"
        )));
    }
}
```

### Risk Assessment
- **Low risk**: Returning an error is strictly safer than silently proceeding. The writeback buffer retains the data, so the next flush attempt can retry.

---

## Fix 2: `flush_pending` and Crash Recovery Must Resolve parent_id

### Current Behavior
`pending.rs:33-38` passes `parent_id=""` and uses `item_id` as the `name` parameter. The `upload()` dispatcher:
- Small files → `upload_small(drive_id, "", item_id, content)` — empty parent_id in Graph URL = invalid request
- Large files → `upload_large(drive_id, item_id, content)` — uses item_id directly, so it works by accident

Three additional call sites in `main.rs` (desktop crash recovery, headless crash recovery, re-auth flush) have the same `parent_id=""` bug.

### Design
1. Add a helper function `resolve_parent_id` in `pending.rs` that looks up the item's parent_id from the SQLite cache
2. Before uploading, call `cache.sqlite.get_item_by_id(item_id)` to get the `DriveItem`, extract `parent_reference.id`
3. If parent_id cannot be resolved (item not in SQLite), use `item_id` as the name and pass empty parent_id — this is the existing fallback behavior and only affects `upload_small` (which will likely fail, but we log it)
4. Extract the duplicated recovery loop into a shared async function in `pending.rs` to DRY the 4 call sites

### Shared Recovery Function
```rust
// pending.rs — new function
pub(crate) async fn recover_pending_writes(
    cache: &CacheManager,
    graph: &GraphClient,
    recovery_dir: Option<&std::path::Path>,
    label: &str,
) {
    let pending = match cache.writeback.list_pending().await { ... };
    for (drive_id, item_id) in &pending {
        if item_id.starts_with("local:") {
            // → Fix 4: save to recovery folder instead of discarding
            save_to_recovery(cache, &drive_id, &item_id, recovery_dir, label).await;
            continue;
        }
        // Resolve parent_id from SQLite
        let (parent_id, name) = resolve_upload_params(cache, &drive_id, &item_id);
        // Upload with correct params
        ...
    }
}
```

### Call Sites Updated
| Location | Current | After |
|----------|---------|-------|
| `pending.rs::flush_pending` | inline loop with `""` parent_id | calls `recover_pending_writes` |
| `main.rs::run_crash_recovery` (desktop) | inline loop with `""` parent_id | calls `recover_pending_writes` |
| `main.rs` headless crash recovery | inline loop with `""` parent_id | calls `recover_pending_writes` |
| `main.rs` re-auth flush (SIGHUP) | inline loop with `""` parent_id | calls `recover_pending_writes` |

### Risk Assessment
- **Medium risk**: The SQLite lookup may fail for items not yet synced. Fallback to existing behavior (empty parent_id) preserves current functionality — no regression.

---

## Fix 3: StreamingBuffer Size Cap and Validation

### Current Behavior
`core_ops.rs:52` — `vec![0u8; total_size as usize]` allocates full file size in RAM. A 2GB file = 2GB allocation. A negative `item.size` (from Graph API returning -1 for unknown sizes) cast via `as usize` causes a massive allocation → OOM.

### Design
1. Add `const MAX_STREAMING_BUFFER_SIZE: u64 = 256 * 1024 * 1024;` (256 MB) to `core_ops.rs`
2. In `StreamingBuffer::new()`, validate `total_size`:
   - If `total_size == 0` or `total_size > MAX_STREAMING_BUFFER_SIZE`: return `Err(VfsError)` instead of panicking
   - Change `new()` to return `Result<Self, VfsError>`
3. Update all call sites (there should be 1 in `open_file`) to handle the error

### Code Change
```rust
const MAX_STREAMING_BUFFER_SIZE: u64 = 256 * 1024 * 1024;

impl StreamingBuffer {
    pub fn new(total_size: u64) -> Result<Self, VfsError> {
        if total_size > MAX_STREAMING_BUFFER_SIZE {
            return Err(VfsError::IoError(format!(
                "file too large for streaming buffer: {total_size} bytes (max {MAX_STREAMING_BUFFER_SIZE})"
            )));
        }
        // total_size is validated ≤ 256MB, safe to cast
        let (tx, rx) = watch::channel(DownloadProgress::InProgress(0));
        Ok(Self {
            data: tokio::sync::RwLock::new(vec![0u8; total_size as usize]),
            progress: tx,
            progress_rx: rx,
            total_size,
        })
    }
}
```

### Fallback for Large Files
Files exceeding 256 MB should fall back to disk-based streaming (download to disk cache directly, read from disk). The existing `open_file` code path already handles cached files via disk — the streaming buffer is only used for uncached large files. For files >256 MB, we'll download to disk cache first, then serve reads from disk.

### Risk Assessment
- **Low risk**: Adds a clear upper bound. Files >256 MB are uncommon in typical OneDrive usage and can be served from disk cache.

---

## Fix 4: Crash Recovery Saves `local:*` Files Instead of Discarding

### Current Behavior
3 code paths in `main.rs` (desktop crash recovery, headless crash recovery, re-auth flush) discard `local:*` files with `let _ = cache.writeback.remove(drive_id, item_id).await` and a log warning. User's unsaved work vanishes silently.

### Design
1. Add a recovery folder at `config_dir()/recovered/{YYYY-MM-DD_HH-MM-SS}/`
2. Before removing `local:*` entries, read their content and save to the recovery folder as `{item_id_sanitized}.bin`
3. Write a `manifest.txt` alongside with drive_id, item_id, and original timestamp
4. Log at `error!` level (not `warn!`) to ensure visibility
5. Send a desktop notification (if AppHandle available) informing the user files were recovered

### Recovery Folder Structure
```
~/.config/cloudmount/recovered/
└── 2026-03-09_14-30-00/
    ├── manifest.txt       # drive_id, item_id, timestamp per file
    ├── local_123456.bin   # recovered file content
    └── local_789012.bin
```

### Implementation Location
The `save_to_recovery` function lives in `pending.rs` alongside `flush_pending` and the new `recover_pending_writes`. It takes an optional `recovery_dir: Option<&Path>` — if `None`, falls back to `config_dir()/recovered/`.

### Notification
Add `notify::files_recovered(app, count, path)` to `notify.rs` for desktop mode. Headless mode relies on log output.

### Risk Assessment
- **Low risk**: Strictly additive. Worst case: recovery folder write fails, falls back to existing behavior (discard + warn).

---

## Fix 5: WritebackBuffer Persists on Write

### Current Behavior
`writeback.rs:30` — `write()` stores content only in `DashMap` (in-memory). Content is only persisted to disk when `persist()` is explicitly called in `flush_inode()`. A crash between `write()` and `flush()` loses the data.

### Design
**Option chosen: Persist immediately in `write()`.**

Rationale: The debounce-timer approach adds complexity (background task, cancellation tokens) for minimal benefit. The `pending/` directory already exists for crash safety. Writing to disk on every FUSE `write()` call adds I/O but guarantees no data loss window.

### Code Change
```rust
pub async fn write(
    &self,
    drive_id: &str,
    item_id: &str,
    content: &[u8],
) -> cloudmount_core::Result<()> {
    let key = Self::buffer_key(drive_id, item_id);
    self.buffers.insert(key, content.to_vec());
    // Persist to disk immediately for crash safety
    self.persist(drive_id, item_id).await?;
    Ok(())
}
```

### Performance Consideration
Each FUSE `write()` call triggers a disk write. For sequential writes to the same file (common pattern: `write(offset=0, 4KB)`, `write(offset=4096, 4KB)`, ...), this means N disk writes. However:
- FUSE write calls are already blocking (`rt.block_on`)
- The pending files use the OS page cache, so sequential writes to the same file are mostly absorbed
- Data safety is more important than throughput for a cloud filesystem

The redundant `persist()` call in `flush_inode()` (core_ops.rs:676-678) can remain — it's a no-op if content hasn't changed, and provides defense-in-depth.

### Risk Assessment
- **Low risk**: Adds disk I/O but guarantees crash safety. The pending dir write is a simple `fs::write` — failure is propagated to the caller.

---

## Cross-Cutting Concerns

### Error Variants
No new error variants needed. All fixes use existing `VfsError::IoError(String)` and `cloudmount_core::Error::Cache(String)`.

### Test Strategy
1. **Conflict upload failure**: Unit test in `cloudmount-vfs/tests/` — mock graph client to fail conflict upload, verify main upload is NOT attempted
2. **flush_pending parent_id**: Unit test — verify SQLite lookup is called before upload, verify correct parent_id is passed
3. **StreamingBuffer cap**: Unit test — verify `new()` returns error for sizes > 256MB and for 0
4. **Crash recovery**: Integration test — create pending `local:*` files, run recovery, verify files exist in recovery folder
5. **WritebackBuffer persist**: Unit test — call `write()`, verify file exists on disk before `flush()`

### Files to Modify
| File | Changes |
|------|---------|
| `crates/cloudmount-vfs/src/core_ops.rs` | Fix 1 (conflict error), Fix 3 (StreamingBuffer cap) |
| `crates/cloudmount-vfs/src/pending.rs` | Fix 2 (parent_id resolution), Fix 4 (recovery function), shared recovery helper |
| `crates/cloudmount-cache/src/writeback.rs` | Fix 5 (persist on write) |
| `crates/cloudmount-app/src/main.rs` | Fix 2+4 (replace inline recovery loops with shared function) |
| `crates/cloudmount-app/src/notify.rs` | Fix 4 (add `files_recovered` notification) |

### Files to Create
None — all changes are modifications to existing files.

---

## Implementation Order
1. Fix 5 (WritebackBuffer persist) — standalone, no dependencies
2. Fix 3 (StreamingBuffer cap) — standalone, no dependencies
3. Fix 1 (conflict error propagation) — standalone, small change
4. Fix 2 (flush_pending parent_id + shared recovery function) — creates the shared function
5. Fix 4 (crash recovery saves local files) — uses the shared function from Fix 2

---
*Design doc for work item fix-vfs-data-safety*
