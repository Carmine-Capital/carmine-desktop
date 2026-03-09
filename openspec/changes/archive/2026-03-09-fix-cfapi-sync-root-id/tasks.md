## 1. Sanitize account_name in cfapi.rs

- [x] 1.1 In `build_sync_root_id()`, replace `!` with `_` in the `account_name` parameter before passing to `SyncRootIdBuilder::account_name()`
- [x] 1.2 Add a `tracing::debug!` log line showing the constructed sync root ID components (provider, sanitized account_name) for Windows mount diagnostics

## 2. Fix call site in main.rs

- [x] 2.1 In the Windows `start_mount()` function (`main.rs`), pass sanitized `drive_id.replace('!', "_")` as `account_name` to `CfMountHandle::mount()` instead of raw `drive_id`

## 3. Verify

- [x] 3.1 Run `cargo clippy --all-targets --all-features` — no warnings
- [x] 3.2 Run `cargo test --all-targets` — all tests pass
