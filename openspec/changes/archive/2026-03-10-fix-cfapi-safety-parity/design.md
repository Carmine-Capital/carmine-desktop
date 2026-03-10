## Context

The CfApi backend (`cfapi.rs`) was originally written as a minimal Windows prototype that called Graph API methods directly from SyncFilter callbacks. Over time, the FUSE backend accumulated safety logic inside `CoreOps` (conflict detection, directory guards, parent cache invalidation, error propagation), but the CfApi backend was never updated to delegate to these shared methods. A cross-platform audit revealed 18 issues spanning data-loss risks, OOM risks, silent corruption, and UX gaps.

The FUSE backend delegates `unlink`, `rmdir`, `rename`, `flush` to `CoreOps`. The CfApi backend only delegates `flush_inode` (via `closed`) and read operations. This asymmetry is the root cause of most findings.

## Goals / Non-Goals

**Goals:**

- CfApi mutation callbacks (`delete`, `rename`) delegate to `CoreOps` methods, achieving behavioral parity with FUSE
- Eliminate `to_string_lossy()` on Windows paths to prevent silent surrogate corruption
- Fix the `drive_id`-as-`account_name` production bug
- Close the TOCTOU window in `flush_inode` with `If-Match` conditional upload
- Eliminate unbounded memory allocations in writeback, streaming buffer, range reads, and crash recovery
- Propagate writeback errors in CfApi `closed()` instead of silently discarding
- Prevent deadlock in `shutdown_on_signal` by releasing mutex before blocking unmounts
- Make Windows headless mode fail clearly instead of running as a silent no-op
- Clean up dead code, stale dependencies, and misleading comments

**Non-Goals:**

- Implementing full headless CfApi mounting (deferred to a future change)
- Changing `CoreOps` public method signatures beyond what's needed for `OsStr` support
- Adding integration tests for Windows CfApi callbacks (no Windows CI runner available)
- Refactoring the open file table or streaming download architecture

## Decisions

### D1. CfApi `delete` and `rename` delegate to `CoreOps` via resolved parent inodes

**Decision:** Resolve the parent inode from the CfApi relative path, extract the child name, and call `self.core.unlink(parent_ino, name)` / `self.core.rmdir(parent_ino, name)` / `self.core.rename(src_parent_ino, src_name, dst_parent_ino, dst_name)`.

**Why over alternatives:**
- *Alt: Duplicate CoreOps logic in CfApi* — Violates DRY, same divergence will recur. Rejected.
- *Alt: Make CoreOps accept a relative path* — Would add a second entry point to CoreOps for every mutation, complicating the API. Rejected. The parent+name pattern matches FUSE's natural calling convention and keeps CoreOps clean.

**On errors:** When the `CoreOps` call returns `Err`, the CfApi callback logs at `warn` level and returns `Ok(())` (per the existing "Resilient CfApi callback error handling" spec). It does NOT call `ticket.pass()`, so the OS sees the operation as incomplete and may retry.

### D2. `relative_path()` returns `Vec<OsString>` path components instead of `String`

**Decision:** Replace `relative_path(&self, absolute: &Path) -> Option<String>` with `relative_components(&self, absolute: &Path) -> Option<Vec<OsString>>` that returns the pre-split path components as `OsString` values. `CoreOps::resolve_path()` changes signature to accept `&[OsString]` (or `&[impl AsRef<OsStr>]`) and compares using `OsStr` throughout.

**Why:** `to_string_lossy()` replaces unpaired UTF-16 surrogates (valid on NTFS) with U+FFFD. This corruption is silent and irrecoverable — the real filename is lost. By keeping `OsString` all the way to the `names_match` comparison, we preserve lossless round-tripping. The `find_child` lookup in memory cache compares `OsStr` against stored `String` names (from Graph API, which are always valid Unicode), so an `OsStr::to_str()` conversion at that boundary is safe — if it fails, the child simply won't match, which is correct (Graph API cannot store unpaired surrogates).

**Impact:** `resolve_path`, `find_child`, `resolve_parent_item_id`, and all CfApi callers change. FUSE callers pass `OsStr` from the kernel (already lossless). The `names_match` function works on `&str` from the cache side and `&OsStr` from the caller side.

### D3. `flush_inode` uses `If-Match` conditional upload

**Decision:** Pass the server eTag obtained during conflict check as an `If-Match` header to `graph.upload_small()` and `graph.upload_large()`. If the server returns 412 Precondition Failed, treat it as a conflict (same as eTag mismatch).

**Why:** Closes the TOCTOU window between the eTag fetch (step 2) and the upload (step 4). The Graph API supports `If-Match` on PUT for content upload.

**Impact:** `GraphClient::upload_small()` and `upload_large()` gain an optional `if_match: Option<&str>` parameter. Existing callers pass `None` to preserve current behavior.

### D4. `Bytes::from(content)` move instead of clone in `flush_inode`

**Decision:** Restructure `flush_inode` so the conflict copy path clones content only when a conflict is actually detected (lazy clone), then the main upload path moves the `content` Vec into `Bytes::from(content)`.

**Why:** Eliminates the triple-memory-copy pattern (writeback buffer + content Vec + Bytes clone). At peak, only 2 copies exist (content + Bytes, which shares the allocation via move). On conflict, a third copy is created but only in the rare conflict path.

### D5. CfApi `closed()` streams large files to writeback in chunks

**Decision:** Replace the accumulate-then-write pattern with a loop that reads 64 KiB chunks from the `BufReader` and writes each chunk directly to the writeback layer via a new `writeback.write_chunk(drive_id, item_id, offset, chunk)` method. The writeback layer appends to a file on disk. For files below `SMALL_FILE_LIMIT` (4 MB), keep the current `std::fs::read()` + single-write pattern.

**Why:** Eliminates the `Vec::with_capacity(file_size)` OOM risk for multi-GB files.

### D6. `disk.get_range()` for partial reads

**Decision:** Add `DiskCache::get_range(drive_id, item_id, offset, length) -> Option<Vec<u8>>` that opens the cached file and reads only the requested byte range using `seek()` + `read_exact()`. `read_range_direct` calls this instead of `disk.get()`.

**Why:** Avoids loading a 500 MB file into memory to serve a 4 KB range request.

### D7. `StreamingBuffer` uses sparse/chunked allocation

**Decision:** Replace `vec![0u8; total_size]` with a `BTreeMap<u64, Vec<u8>>` of fixed-size chunks (e.g., 256 KiB each). Chunks are allocated on first write. The `read` path returns data from populated chunks or waits for the download to fill them.

**Why:** 4 concurrent 256 MB streaming opens currently consume 1 GB upfront. With chunked allocation, memory grows proportionally to downloaded bytes.

### D8. `shutdown_on_signal` drains handles before unmount

**Decision:** Both `mount.rs::shutdown_on_signal` and `cfapi.rs::shutdown_on_signal` drain the handles Vec/HashMap out of the mutex first (`std::mem::take`), then iterate and unmount without holding the lock.

**Why:** Each unmount's `flush_pending` has a 30-second timeout. Holding the mutex for N × 30s blocks any concurrent access.

### D9. `start_mount` shared preamble extraction

**Decision:** Extract the shared 70% of `start_mount` (drive validation, cache dir, CacheManager, InodeTable, event channel, state insertion, notification) into a private `start_mount_common()` that returns a `MountContext` struct. Each platform's `start_mount` calls the helper, then constructs the platform-specific mount handle.

**Why:** Fixes the `drive_id`-as-`account_name` bug (line 909) as a side effect — the helper computes the correct `account_name` from the mount config's display name. Also eliminates the code duplication maintenance burden.

### D10. Windows headless exits with clear error

**Decision:** Replace the `warn!` + silent skip with `tracing::error!` + `eprintln!` + `std::process::exit(1)` early in `run_headless()` when the platform is Windows.

**Why:** A process that authenticates then sits idle is worse than a clear failure. Users can run desktop mode instead.

## Risks / Trade-offs

- **[Risk] `OsStr` path changes touch many call sites** → Mitigated by keeping the change within `cfapi.rs` callers and `CoreOps::resolve_path`. FUSE already passes `OsStr` from the kernel. The `find_child` cache lookup converts `OsStr → &str` at the comparison boundary, which is a safe fallible conversion.

- **[Risk] Chunked `StreamingBuffer` adds complexity to read/write paths** → Mitigated by using a well-tested `BTreeMap<u64, Vec<u8>>` pattern. Existing tests for `open_file_table` cover the read/write semantics. The chunk size (256 KiB) is a tunable constant.

- **[Risk] `If-Match` upload may fail spuriously on Graph API lag** → The 412 response is treated as a conflict, which triggers the conflict copy path. This is the correct behavior — if the server version changed, we should not overwrite it.

- **[Risk] Streaming writeback for large files adds a new `write_chunk` method** → The writeback layer already operates on files on disk. Adding append-mode writing is a small change.

- **[Trade-off] Windows headless exits instead of degraded operation** → Acceptable for v1. Headless CfApi support is a future enhancement tracked separately.

- **[Trade-off] `pending.rs` large-file recovery needs `upload_large` session** → The `GraphClient` already has `upload_large()`. The change is to use it when content exceeds `SMALL_FILE_LIMIT` instead of always using `upload_small`.
