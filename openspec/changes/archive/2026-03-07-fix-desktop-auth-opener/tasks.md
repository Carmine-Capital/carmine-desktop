## 1. Dependencies

- [x] 1.1 Add `tauri-plugin-opener` to workspace `[workspace.dependencies]` in root `Cargo.toml`
- [x] 1.2 Add `tauri-plugin-opener` to `cloudmount-app/Cargo.toml` under the `desktop` feature

## 2. Auth crate: injectable opener

- [x] 2.1 Add `opener: Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>` field to `AuthManager` in `manager.rs`, update `new()` to accept it
- [x] 2.2 Pass the opener through `authorize()` into `run_pkce_flow()` in `oauth.rs`
- [x] 2.3 Replace `open::that()` and `has_display()` logic in `run_pkce_flow()` with a call to the opener, keeping stderr fallback on error
- [x] 2.4 Update auth crate tests to use a no-op opener

## 3. App crate: provide platform openers

- [x] 3.1 Register `tauri_plugin_opener::init()` in the Tauri builder in `main.rs`
- [x] 3.2 Construct `AuthManager` in desktop mode with a closure using `app.opener().open_url()` via `OpenerExt`, with stderr fallback on error
- [x] 3.3 Construct `AuthManager` in headless mode with a closure using `open::that()` + `has_display()` check + stderr fallback (preserving current behavior)

## 4. Tauri permissions

- [x] 4.1 Add `opener:default` (or `opener:allow-open-url` with `https://` and `http://` allowed) to Tauri capabilities configuration

## 5. Verification

- [x] 5.1 Build with `--features desktop` and verify no warnings (`RUSTFLAGS=-Dwarnings`)
- [x] 5.2 Build without desktop feature (headless) and verify no warnings
