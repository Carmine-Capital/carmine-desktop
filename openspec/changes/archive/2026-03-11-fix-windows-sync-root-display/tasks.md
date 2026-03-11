## 1. Update `register_sync_root` signature and logic

- [x] 1.1 Add `display_name: &str` and `icon_path: &str` parameters to `register_sync_root()` in `crates/cloudmount-vfs/src/cfapi.rs`
- [x] 1.2 Replace `.with_display_name(PROVIDER_NAME)` with `.with_display_name(display_name)` in `SyncRootInfo` construction
- [x] 1.3 Replace `.with_icon(...)` with `.with_icon(icon_path)` in `SyncRootInfo` construction

## 2. Resolve icon path from the running executable

- [x] 2.1 Add a helper function `resolve_icon_path() -> String` in `cfapi.rs` that calls `std::env::current_exe()`, formats the result as `"<path>,0"`, and falls back to `"%SystemRoot%\\system32\\shell32.dll,43"` on error
- [x] 2.2 Call `resolve_icon_path()` in `CfMountHandle::mount()` and pass the result to `register_sync_root()`

## 3. Add `display_name` parameter to `CfMountHandle::mount`

- [x] 3.1 Add `display_name: String` as a new parameter to `CfMountHandle::mount()` (after `account_name`)
- [x] 3.2 Pass `display_name` and the resolved `icon_path` as arguments to `register_sync_root()`

## 4. Remove the `is_registered` guard (always re-register)

- [x] 4.1 Remove the `let is_registered = sync_root_id.is_registered()?` check and the `if !is_registered` guard in `CfMountHandle::mount()`
- [x] 4.2 Call `register_sync_root()` unconditionally so stale registrations from prior launches are overwritten

## 5. Update caller in `cloudmount-app`

- [x] 5.1 In `crates/cloudmount-app/src/main.rs` `start_mount()` (Windows path), pass `mount_config.name.clone()` as the new `display_name` argument to `CfMountHandle::mount()`

## 6. Update integration tests

- [x] 6.1 Update `CfMountHandle::mount()` call sites in `crates/cloudmount-vfs/tests/cfapi_integration.rs` to pass a `display_name` argument (e.g., `"Test Mount"`)
- [x] 6.2 Update `CfMountHandle::mount()` call site in `crates/cloudmount-app/tests/integration_tests.rs` to pass a `display_name` argument

## 7. Verify

- [x] 7.1 Run `make clippy` and confirm zero warnings
- [x] 7.2 Run `make test` and confirm all CfApi integration tests pass
