## 1. Backend — Linux Desktop Opener Fix

- [x] 1.1 In `run_desktop()` (`main.rs`), replace the `tauri_plugin_opener.open_url()` call with `#[cfg(target_os = "linux")]` / `#[cfg(not(target_os = "linux"))]` branches: on Linux, spawn `xdg-open {url}` via `std::process::Command` with `.env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD").status()`, mapping non-zero exit to `Err`; on other platforms, keep `handle.opener().open_url(url, None::<&str>)`
- [x] 1.2 Update the `microsoft-auth` delta spec scenario "Desktop mode browser opening on Linux" to match implementation
- [x] 1.3 Verify the fix manually: build AppImage, run on Fedora Silverblue, click "Sign In" → browser opens

## 2. Backend — Auth URL Forwarding

- [x] 2.1 In `oauth.rs`, add an `url_tx: Option<tokio::sync::oneshot::Sender<String>>` parameter to `run_pkce_flow`; send the auth URL on this channel (if `Some`) immediately after constructing `auth_url` and before calling `opener` / `wait_for_callback`
- [x] 2.2 In `manager.rs`, update `AuthManager::sign_in` to accept and thread through the `url_tx` parameter to `run_pkce_flow`
- [x] 2.3 Add a new Tauri command `start_sign_in(app: AppHandle) -> Result<String, String>` in `commands.rs` that: creates a `oneshot` channel, spawns a background task that calls `auth.sign_in(Some(url_tx)).await` and emits a `"auth-complete"` or `"auth-error"` Tauri event on the app handle when done, and returns the auth URL received on `url_rx` to the frontend
- [x] 2.4 Register `start_sign_in` in `invoke_handler!` in `main.rs`
- [x] 2.5 Keep the existing `sign_in` command unchanged (used internally or for headless path); `start_sign_in` is additive

## 3. Frontend — Wizard Auth URL Display

- [x] 3.1 In the wizard frontend, add a "signing-in" state that displays: a spinner/message ("Waiting for browser login…"), the auth URL in a read-only text field or code block, and a "Copy URL" button
- [x] 3.2 Wire "Sign In" button to call `start_sign_in` instead of `sign_in`; on response, transition to the "signing-in" state showing the returned auth URL
- [x] 3.3 Implement "Copy URL" button: copies auth URL to clipboard via `navigator.clipboard.writeText()`, shows brief "Copied!" confirmation
- [x] 3.4 Listen for the `"auth-complete"` Tauri event; on receipt, transition to the post-sign-in success flow (same as current `sign_in` success path)
- [x] 3.5 Listen for the `"auth-error"` Tauri event; on receipt, display the error message and return to the initial sign-in screen
- [x] 3.6 Add a "Cancel" button in the "signing-in" state (optional: cancels the PKCE wait by ignoring the eventual auth-complete/auth-error event and returning to sign-in screen)

## 4. Testing & Verification

- [x] 4.1 Verify on AppImage (Fedora Silverblue): browser opens on click "Sign In", auth URL visible in wizard
- [x] 4.2 Verify copy button works and copies correct URL
- [x] 4.3 Verify auth completes end-to-end after browser login (wizard advances, mounts start)
- [x] 4.4 Verify that if browser is already open and user completes login, wizard auto-advances without needing to interact with the URL display
- [x] 4.5 Verify non-AppImage desktop binary still works (macOS or non-immutable Linux)
- [x] 4.6 Run `cargo clippy --all-targets --all-features` and `cargo test --all-targets` with no errors
