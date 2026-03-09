# Implementation Plan: Fix VFS Data-Loss Paths

**Run**: run-cloud-mount-016
**Work Item**: fix-vfs-data-safety
**Mode**: validate
**Design Doc**: `.specs-fire/intents/fix-comprehensive-review/work-items/fix-vfs-data-safety-design.md`

---

## Implementation Checklist

### Step 1: WritebackBuffer — persist on write (Fix 5)
**File**: `crates/cloudmount-cache/src/writeback.rs`

- [ ] In `write()`, after inserting into `self.buffers`, call `self.persist(drive_id, item_id).await?`
- [ ] Propagate the error (already returns `Result`)

### Step 2: StreamingBuffer size cap (Fix 3)
**File**: `crates/cloudmount-vfs/src/core_ops.rs`

- [ ] Add `const MAX_STREAMING_BUFFER_SIZE: u64 = 256 * 1024 * 1024;`
- [ ] Change `StreamingBuffer::new(total_size: u64) -> Self` to `-> Result<Self, VfsError>`
- [ ] Add validation: reject `total_size == 0` or `total_size > MAX_STREAMING_BUFFER_SIZE`
- [ ] Update the call site in `open_file()` to handle `Err` — return `VfsError::IoError` to FUSE

### Step 3: Conflict upload error propagation (Fix 1)
**File**: `crates/cloudmount-vfs/src/core_ops.rs`

- [ ] Replace `let _ = self.rt.block_on(self.graph.upload_small(...))` with `if let Err(e) = ...`
- [ ] On error: log at `error!` level, return `Err(VfsError::IoError(...))`

### Step 4: Shared recovery function + parent_id resolution (Fix 2 + Fix 4)
**File**: `crates/cloudmount-vfs/src/pending.rs`

- [ ] Add `use cloudmount_core::config::config_dir;` and necessary imports
- [ ] Add async fn `resolve_upload_params(cache, drive_id, item_id) -> (String, String)` — returns `(parent_id, name)` by looking up SQLite
- [ ] Add async fn `save_to_recovery(cache, drive_id, item_id, recovery_dir, label)` — reads content, writes to recovery folder with manifest
- [ ] Add pub async fn `recover_pending_writes(cache, graph, drive_id, recovery_dir, label)` — the shared recovery loop:
  - Lists pending for the given drive_id
  - For `local:*` items: calls `save_to_recovery`
  - For regular items: resolves parent_id, uploads with correct params
- [ ] Update `flush_pending` to call `recover_pending_writes`

### Step 5: Replace inline recovery loops in main.rs (Fix 2 + Fix 4)
**File**: `crates/cloudmount-app/src/main.rs`

- [ ] Replace desktop `run_crash_recovery` inline loop with call to `recover_pending_writes`
- [ ] Replace headless crash recovery inline loop with call to `recover_pending_writes`
- [ ] Replace re-auth flush (SIGHUP) inline loop with call to `recover_pending_writes`

### Step 6: Add recovery notification (Fix 4)
**File**: `crates/cloudmount-app/src/notify.rs`

- [ ] Add `pub fn files_recovered(app, count, path)` notification
- [ ] Call it from `run_crash_recovery` after `recover_pending_writes` completes (if files were recovered)

### Step 7: Tests
**File**: `crates/cloudmount-cache/tests/cache_tests.rs` and `crates/cloudmount-vfs/tests/`

- [ ] Test `WritebackBuffer::write()` persists to disk immediately
- [ ] Test `StreamingBuffer::new()` rejects sizes > 256MB and 0
- [ ] Test `StreamingBuffer::new()` succeeds for valid sizes
- [ ] Verify existing tests still pass

---

## Files to Modify
| File | Changes |
|------|---------|
| `crates/cloudmount-cache/src/writeback.rs` | Persist on write |
| `crates/cloudmount-vfs/src/core_ops.rs` | StreamingBuffer cap, conflict error propagation |
| `crates/cloudmount-vfs/src/pending.rs` | Shared recovery fn, parent_id resolution, save_to_recovery |
| `crates/cloudmount-app/src/main.rs` | Replace 3 inline recovery loops |
| `crates/cloudmount-app/src/notify.rs` | Add files_recovered notification |

## Files to Create
None.

## Tests
| Test | Location |
|------|----------|
| Writeback persist on write | `crates/cloudmount-cache/tests/cache_tests.rs` |
| StreamingBuffer cap | `crates/cloudmount-vfs/tests/` |

---

*This is Checkpoint 2 of Validate mode.*
*Approve implementation plan? [Y/n/edit]*
