## 1. Parameterize sync root ID

- [x] 1.1 Add `account_name: &str` parameter to `build_sync_root_id()` in `cfapi.rs` and pass it to `SyncRootIdBuilder::account_name()`
- [x] 1.2 Add `account_name: String` parameter to `CfMountHandle::mount()` and thread it through to `build_sync_root_id()`
- [x] 1.3 Store `account_name` in `CfMountHandle` struct (needed if we ever need to reconstruct the sync root ID)

## 2. Update callers

- [x] 2.1 Update `CfMountHandle::mount()` call in `cloudmount-app/src/main.rs` to pass drive ID (or mount label) as `account_name`

## 3. Fix test isolation

- [x] 3.1 Update `CfTestFixture::setup()` to pass the nanos-based `test_id` as `account_name` to `CfMountHandle::mount()`
- [x] 3.2 Replace bare `read_dir` + immediate assertion in `cfapi_browse_populates_placeholders` with a polling loop (retry every 100ms, timeout after 2s)
- [x] 3.3 Add a similar polling helper for tests that wait for placeholders before file operations (`cfapi_hydrate_file_on_read`, `cfapi_rename_file`, `cfapi_delete_file`)

## 4. Verify

- [x] 4.1 Run `cargo clippy --all-targets --all-features` — zero warnings
- [x] 4.2 Run `cargo fmt --all -- --check` — clean formatting
- [x] 4.3 Confirm tests compile on non-Windows (cfg gates intact)
