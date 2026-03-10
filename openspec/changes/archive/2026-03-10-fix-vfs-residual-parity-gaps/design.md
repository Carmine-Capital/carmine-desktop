## Context

The `fix-cfapi-safety-parity` change (37/54 tasks done) addressed most CfApi parity issues found in the VFS audit. A verification pass against the current codebase confirmed three residual defects:

1. **`cfapi.rs:210`** — `rel_path` is referenced in a `tracing::warn!` inside `fetch_data` but is never defined. The only path variable in scope is `abs_path`. This is a compile error on `x86_64-pc-windows-msvc` (CI runs Windows builds).

2. **`cfapi.rs:400-402`** — When `flush_inode(ino)` fails after `closed()` completes the writeback write, the error is logged but no `VfsEvent::WritebackFailed` is emitted. The UI never shows the failure. All six other error branches in `closed()` correctly emit the event.

3. **`cfapi.rs:542-551`** — `state_changed()` invalidates the changed item's cache entry but not its parent directory's children map. After a placeholder state change, `list_children` on the parent returns stale results until TTL expiry. FUSE and `core_ops` mutation methods all invalidate the parent explicitly.

## Goals / Non-Goals

**Goals:**
- Fix the Windows compile error so CI passes on `windows-latest`
- Ensure all failure paths in `closed()` surface feedback to the user
- Bring `state_changed()` parent cache handling in line with FUSE parity

**Non-Goals:**
- Restructuring `closed()` control flow (already addressed by `fix-cfapi-safety-parity`)
- Adding `ticket.fail()` calls for delete/rename errors (the cloud-filter `Delete`/`Rename` tickets use a pass/no-pass model — not calling `ticket.pass()` already signals failure to Windows)
- Addressing the `let _ = delete_item` in the rename clobber path in `core_ops.rs` (the cascading 409 from `update_item` prevents actual data loss; a separate change can tighten this)

## Decisions

### D1: Fix `rel_path` by using `abs_path.display()`

The `fetch_data` function has `abs_path` (from `request.path()`) in scope. The existing spec ("Resilient CfApi callback error handling") requires logging "sufficient context (callback name, file path, error details)". Using `abs_path.display()` matches all other `tracing` calls in the same function (lines 138, 145, 186, 192) and satisfies the spec.

**Alternative considered:** Define `rel_path` by stripping the mount prefix. Rejected — no other log line in `fetch_data` uses a relative path, and the absolute path is more useful for debugging on Windows (includes drive letter and sync root).

### D2: Emit `WritebackFailed` on `flush_inode` error in `closed()`

Add `self.core.send_event(VfsEvent::WritebackFailed { file_name: file_name.clone() })` in the `Err(e)` branch at line 401. This is the same pattern used in all six other error branches of `closed()`.

`file_name` is already bound at line 292 (`let file_name = item.name.clone()`). However, it is moved by earlier `return` branches (the `WritebackFailed` event takes ownership). A `.clone()` is needed because `file_name` may have already been partially moved in a branch not taken. In practice, since the `return` branches exit early, `file_name` is still available at line 401 — but the compiler requires the clone because `send_event` takes `file_name` by value.

**Alternative considered:** Change `VfsEvent::WritebackFailed` to take `&str`. Rejected — that would change the event enum signature and affect `main.rs` event forwarding, expanding scope beyond this fix.

### D3: Invalidate parent in `state_changed()`

After resolving the changed item's inode, resolve the parent by taking `components[..components.len()-1]` and calling `self.core.resolve_path()` on the parent slice. If the parent resolves, call `self.core.cache().memory.invalidate(parent_ino)`.

Edge case: if `components` is empty (the sync root itself changed), there is no parent to invalidate — skip. This matches the guard already present in `delete` and `rename` via `resolve_parent_and_name()`.

**Alternative considered:** Call `cache.memory.remove_child(parent_ino, child_name)` for surgical removal instead of full invalidation. Rejected — `state_changed` receives a list of paths but no mutation type (create/delete/rename). Without knowing the operation, surgical update is error-prone. Full invalidation of the parent is the safe choice and consistent with how `core_ops` handles mutations whose exact nature is ambiguous.

## Risks / Trade-offs

- **[Risk: parent invalidation is overly aggressive]** Full invalidation forces a re-fetch of all children on next `list_children`. For directories with thousands of items, this adds latency after each `state_changed` event. Mitigation: `state_changed` typically fires for 1-2 items after a user action; the re-fetch is bounded by the directory's TTL and is the same cost as a cache miss.

- **[Risk: `file_name.clone()` adds a small allocation]** The clone happens only on the error path (flush failure), which is already an exceptional case. No performance concern.
