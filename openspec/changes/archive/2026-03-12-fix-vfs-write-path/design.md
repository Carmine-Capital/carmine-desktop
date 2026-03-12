## Context

The VFS write path flows through `CoreOps` methods shared by FUSE and WinFsp backends:
- `write_handle()` → mutates in-memory buffer
- `flush_handle()` → persists buffer to writeback cache, delegates upload to `SyncProcessor`
- `SyncProcessor::flush_inode_async()` → debounces, uploads, handles conflicts

Three bugs exist in this path:
1. `flush_handle()` returns before upload completes (fire-and-forget), breaking Windows app save verification
2. `write_handle()` never shrinks the buffer — trailing stale bytes survive when new content is shorter
3. `flush_inode_async()` skips transient files in writeback but leaves orphaned memory cache entries

## Goals / Non-Goals

**Goals:**
- Windows apps (Word, Excel, Notepad) can save files without re-save prompts or permission errors
- File content integrity is preserved when writes produce shorter content than what's in the buffer
- Directory listings on Windows are free of ghost 0-byte unnamed entries
- FUSE behavior on Linux/macOS remains unchanged (fire-and-forget flush is fine for POSIX apps)

**Non-Goals:**
- Refactoring the SyncProcessor architecture (debounce, retry, concurrency — all fine as-is)
- Changing the writeback or disk cache storage format
- Adding new user-visible features or UI changes
- Fixing the online-edit-then-local-open corruption (separate investigation needed — likely a disk cache or stale-handle re-download issue, not `write_handle`)

## Decisions

### Decision 1: Synchronous flush via oneshot completion channel

**Choice:** Add a `SyncRequest::FlushSync { ino, done: oneshot::Sender<bool> }` variant. The SyncProcessor sends `true`/`false` on the oneshot when the upload completes. `flush_handle` gets a `wait_for_completion: bool` parameter — when `true`, it blocks on the oneshot receiver after sending the request.

**Why not bypass SyncProcessor entirely?** The SyncProcessor owns conflict detection, retry logic, and inode reassignment. Duplicating this in a synchronous path creates two upload code paths to maintain. Reusing the same path with a completion signal keeps a single upload implementation.

**Why not always wait?** FUSE apps on Linux/macOS don't verify saves — waiting would add unnecessary latency (500ms debounce + upload time) for no benefit. Only WinFsp callers need the synchronous path.

**Implementation detail:** When `FlushSync` arrives, the SyncProcessor skips the debounce pending map and immediately spawns the upload (acquiring a semaphore permit). The oneshot is stored alongside the in-flight entry and resolved when the upload result arrives. This means FlushSync requests bypass debounce entirely — correct because the caller is actively waiting and wants immediate execution.

**Alternatives considered:**
- *Poll writeback until entry disappears:* Fragile — upload failure leaves writeback intact, would spin forever. No way to distinguish "upload in progress" from "upload failed."
- *Always call `flush_inode()` synchronously from WinFsp:* Works but duplicates the upload path. Misses conflict detection improvements added to SyncProcessor later.

### Decision 2: Track logical file size in OpenFile entry

**Choice:** Add a `logical_size: Option<usize>` field to `OpenFile`. When `truncate()` resizes a buffer smaller, set `logical_size = Some(new_size)`. When `write_handle()` writes data, update `logical_size = Some(max(logical_size.unwrap_or(0), offset + data.len()))`. On `flush_handle()`, truncate the buffer to `logical_size` before writing to writeback.

**Why not just truncate the buffer in `write_handle()`?** POSIX semantics: writing N bytes at offset 0 to a file of size M (N < M) does NOT truncate the file. The file remains M bytes. We must preserve this behavior for FUSE compatibility. The logical_size only comes into play when an explicit `truncate()` or `overwrite()` has set a smaller size — after that, writes extend the logical size from the truncated point, not from the original buffer length.

**Why `Option<usize>`?** `None` means "no explicit truncation happened, use buffer length as file size" — preserving current behavior for the common case. `Some(n)` means "file was explicitly sized to n, clamp buffer on flush."

**Alternatives considered:**
- *Always truncate buffer on write:* Breaks POSIX semantics where writing at offset 0 doesn't shrink the file.
- *Truncate buffer immediately in `truncate()`:* Already done — `buf.resize(new_size, 0)`. But `write_handle` then grows it back if it writes at an offset. The issue is that `item.size = buf.len()` after write, which over-reports.

### Decision 3: Clean up memory cache when skipping transient files

**Choice:** In `flush_inode_async`, after removing a transient file from writeback, also:
1. Remove the inode from the memory cache via `cache.memory.remove(ino)`
2. Remove the child entry from the parent's children map via `cache.memory.remove_child(parent_ino, &item.name)`
3. Remove the inode mapping from the InodeTable via `inodes.remove_by_item_id(&item_id)`

**Why full cleanup?** The transient file was never uploaded — it has no server-side counterpart. Leaving any cache entry creates a ghost that appears in directory listings until the memory cache TTL expires or a refresh overwrites it.

**Why in flush_inode_async and not in create_file?** We can't know at creation time whether a file is transient — the app creates `~$document.docx` with a normal create call. Only at flush time, when we have the final name, can we decide to skip. And the SyncProcessor is the right place because it already handles the writeback cleanup.

**Alternatives considered:**
- *Filter transient files in `list_children`:* Hides the symptom, not the cause. Entries still consume cache memory and confuse other lookups.
- *Never cache `local:` items:* Breaks the create→write→flush flow where the file needs to be visible before upload.

## Risks / Trade-offs

**[FlushSync adds blocking in WinFsp callbacks]** → WinFsp callbacks run on a thread pool managed by the WinFsp driver. Blocking one thread during upload is acceptable — WinFsp allocates multiple threads, and saves are inherently sequential per-file. If the upload fails, the oneshot returns `false` and we propagate the error to the app. Timeout risk: we should cap the wait (e.g., 60 seconds) to avoid permanently blocking a WinFsp thread on a hung upload.

**[FlushSync bypasses debounce]** → For rapid saves, each one blocks. But this matches Windows app expectations: save = persisted. If an app saves 10 times in 1 second, each save blocks for its upload. In practice, Word/Excel don't do rapid-fire saves — they save once on Ctrl+S and once on close. The debounce remains active for the fire-and-forget path (FUSE).

**[logical_size adds complexity to OpenFile]** → Minimal impact — one `Option<usize>` field. The flush path already clones the buffer; adding a truncation before the clone is trivial. Risk of stale logical_size if truncate/write interleaving is unexpected — mitigated by tests covering: truncate→write, write→truncate→write, overwrite→write sequences.

**[Transient cleanup removes parent_ino lookup]** → We need the parent inode to call `remove_child`. The `DriveItem.parent_reference.id` gives us the parent item_id, from which we can resolve parent_ino via the InodeTable. If the parent inode can't be resolved (edge case: parent was deleted), we skip the child removal — the parent's cache entry is already gone anyway.
