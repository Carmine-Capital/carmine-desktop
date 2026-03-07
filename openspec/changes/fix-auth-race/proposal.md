## Why

OAuth sign-in has a race condition that silently degrades user trust: cancelling and immediately retrying spawns a second concurrent auth flow while the first continues running unguarded. The Cancel button has no backend hook — it only resets frontend state, leaving the spawned task running for up to 120 seconds. This is the highest-stakes moment in the first-run experience.

## What Changes

- Add a concurrency guard to `AuthManager::sign_in()` so that at most one PKCE flow runs at a time; a second call while a flow is active either cancels the prior flow or returns an "already in progress" error, eliminating the orphaned-spawn scenario.
- Track the active `JoinHandle` in `commands.rs::start_sign_in` so that calling `start_sign_in` while a flow is already running aborts the previous spawn before starting a new one.
- Add a cancellation token (`CancellationToken` from `tokio-util`) to `run_pkce_flow` so that a cancel signal from the backend immediately stops `wait_for_callback` instead of waiting for the 120-second timeout. Note: code review confirms `url_tx.send()` already fires before the opener call at `oauth.rs:72-76` — no ordering change is needed there.
- Update the wizard's `cancelSignIn()` to invoke a new `cancel_sign_in` Tauri command that triggers the cancellation token, ensuring the backend flow is actually stopped when the user clicks Cancel.

## Capabilities

### New Capabilities

- None.

### Modified Capabilities

- `microsoft-auth`: The PKCE flow requirements gain one new behavioral constraint: concurrent calls to `sign_in` MUST be serialized — the prior call MUST be cancelled before a new one proceeds. The auth URL ordering guarantee (url_tx fires before opener) is already correct in the implementation and was already implied by the spec; this change makes no modification to that behavior.

## Impact

- `crates/cloudmount-auth/src/manager.rs`: Add `Arc<Mutex<Option<CancellationToken>>>` field; modify `sign_in()` to set/clear the token and select on it alongside `wait_for_callback`.
- `crates/cloudmount-auth/src/oauth.rs`: Thread the `CancellationToken` into `run_pkce_flow` and `wait_for_callback`; no change to the url_tx send order (already correct per code review — url_tx fires before opener call at line 72-76).
- `crates/cloudmount-app/src/commands.rs`: Add `cancel_sign_in` command; store the spawn `JoinHandle` in `AppState` under a `Mutex<Option<JoinHandle<()>>>` and abort it before spawning a new one.
- `crates/cloudmount-app/dist/wizard.html`: Wire `cancelSignIn()` to invoke `cancel_sign_in` command.
- No changes to Graph, cache, VFS, or config crates.
- No new external crate dependencies beyond `tokio-util` (for `CancellationToken`); check if `tokio-util` is already in `[workspace.dependencies]`.
