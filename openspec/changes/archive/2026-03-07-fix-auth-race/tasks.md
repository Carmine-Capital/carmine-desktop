## 1. Workspace Dependency Update

- [x] 1.1 In `/var/home/nyxa/Projets/carminedesktop/cloud-mount/Cargo.toml`, update the `tokio-util` entry in `[workspace.dependencies]` to include the `"sync"` feature alongside the existing `"rt"` feature: `tokio-util = { version = "0.7", features = ["rt", "sync"] }`

## 2. AuthManager: CancellationToken field and cancel() method

- [x] 2.1 In `crates/carminedesktop-auth/src/manager.rs`, add `tokio_util` to the crate's `Cargo.toml` dependencies as `{ workspace = true }` (if not already listed).
- [x] 2.2 In `crates/carminedesktop-auth/src/manager.rs`, add a field `active_cancel: Arc<Mutex<Option<tokio_util::sync::CancellationToken>>>` to `AuthManager` (use `std::sync::Mutex` to avoid holding an async lock across await points).
- [x] 2.3 In `AuthManager::new()`, initialize `active_cancel` as `Arc::new(Mutex::new(None))`.
- [x] 2.4 Add a public `cancel()` method to `AuthManager`: lock `active_cancel`, call `.cancel()` on the stored token if `Some`, and clear it to `None`.
- [x] 2.5 In `AuthManager::sign_in()`, before calling `self.authorize()`: lock `active_cancel`, cancel any existing token, create a fresh `CancellationToken`, store it, and drop the lock. After `sign_in()` returns (success or error), lock `active_cancel` and clear the token.

## 3. PKCE flow: thread CancellationToken into wait_for_callback

- [x] 3.1 In `crates/carminedesktop-auth/src/oauth.rs`, add a `cancel_token: tokio_util::sync::CancellationToken` parameter to `run_pkce_flow`.
- [x] 3.2 Thread `cancel_token` through to `wait_for_callback` as an additional parameter.
- [x] 3.3 In `wait_for_callback`, replace the bare `listener.accept().await` with a `tokio::select!` that races `listener.accept()` against `cancel_token.cancelled()`; if the token fires first, return `Err(carminedesktop_core::Error::Auth("sign-in cancelled".into()))`.
- [x] 3.4 Update the call site in `AuthManager::authorize()` to pass a child token cloned from the stored `CancellationToken` (use `child_token()` so cancelling the parent also cancels the child).

## 4. AppState: active_sign_in handle tracking

- [x] 4.1 In `crates/carminedesktop-app/src/main.rs`, add `active_sign_in: std::sync::Mutex<Option<tokio::task::JoinHandle<()>>>` to `AppState`.
- [x] 4.2 Initialize `active_sign_in` as `Mutex::new(None)` in the `AppState` construction site.

## 5. commands.rs: abort-before-spawn and cancel_sign_in command

- [x] 5.1 In `crates/carminedesktop-app/src/commands.rs`, at the top of `start_sign_in` (before `tokio::spawn`): lock `state.active_sign_in`, abort any existing handle, and clear it.
- [x] 5.2 After calling `tokio::spawn`, store the returned `JoinHandle` in `state.active_sign_in`.
- [x] 5.3 Add a new `#[tauri::command] pub async fn cancel_sign_in(app: AppHandle) -> Result<(), String>` that: (a) calls `state.auth.cancel()`, (b) locks `state.active_sign_in`, aborts the handle if present, and clears it.
- [x] 5.4 Register `cancel_sign_in` in the `invoke_handler!` macro in `crates/carminedesktop-app/src/main.rs`.

## 6. Wizard: wire Cancel button to cancel_sign_in command

- [x] 6.1 In `crates/carminedesktop-app/dist/wizard.html`, update `cancelSignIn()` to call `await invoke('cancel_sign_in')` (wrapped in a try/catch to swallow errors) before calling `cleanupListeners()` and `showStep('step-welcome')`.

## 7. Verification

- [x] 7.1 Run `cargo build --all-targets` and confirm it compiles cleanly.
- [x] 7.2 Run `cargo clippy --all-targets --all-features` and confirm zero warnings.
- [x] 7.3 Run `cargo test --all-targets` and confirm all existing tests pass.
- [x] 7.4 Manually test: start sign-in, click Cancel immediately, click Sign In again — verify only one auth flow runs and the second flow's URL appears in the wizard without error.
- [x] 7.5 Manually test: start sign-in, wait for URL to appear, click Cancel — verify the backend terminates promptly (no 120-second wait) and the app returns to the welcome step.
