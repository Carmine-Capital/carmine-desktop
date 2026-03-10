## 1. Fix fetch_data compile error (D1)

- [x] 1.1 In `cfapi.rs::fetch_data`, replace `path = %rel_path` with `path = %abs_path.display()` in the `tracing::warn!` at the `write_at` failure path (line 210)

## 2. Emit WritebackFailed on flush_inode failure in closed() (D2)

- [x] 2.1 In `cfapi.rs::closed()`, add `self.core.send_event(VfsEvent::WritebackFailed { file_name: file_name.clone() })` in the `Err(e)` branch of the `flush_inode` match (line 401), before the closing brace
- [x] 2.2 Verify `file_name` is still available at that point (it is bound at line 292 and not consumed by earlier branches that `return` early); add `.clone()` if the compiler requires it due to move analysis

## 3. Invalidate parent directory cache in state_changed() (D3)

- [x] 3.1 In `cfapi.rs::state_changed()`, after resolving the changed item's inode, compute the parent components as `&components[..components.len()-1]`
- [x] 3.2 If parent components is non-empty, call `self.core.resolve_path(parent_components)` to get the parent inode, then call `self.core.cache().memory.invalidate(parent_ino)`
- [x] 3.3 If parent components is empty (sync root changed), skip parent invalidation

## 4. Verification

- [x] 4.1 `make clippy` passes with zero warnings (Linux native + Windows cross-check if available)
- [x] 4.2 `make test` passes — existing tests still green
