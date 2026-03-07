## Context

`AuthManager::sign_in()` has no concurrency guard. When a user clicks Cancel in the wizard, `cancelSignIn()` resets the frontend state and tears down event listeners, but the `tokio::spawn` launched by `start_sign_in` in `commands.rs` continues running for up to 120 seconds. If the user immediately clicks Sign In again, a second spawn fires, creating two concurrent PKCE flows. Both can emit `auth-complete` or `auth-error`; the stale flow's events arrive after the new flow's listeners have been installed, producing spurious outcomes.

Additionally, `cancel_sign_in` does not exist as a Tauri command — the frontend Cancel button has no backend hook to stop the running flow. The only cancellation that happens is listener cleanup on the frontend side.

Code review of `oauth.rs` confirms that `url_tx.send()` already fires at line 72-76 **before** the opener call at line 79, so the "URL arrives late" scenario described in the initial bug report does not manifest in current code. The wizard's `auth-url` input field receives the URL before the browser is launched. This simplifies the scope: no ordering change is needed in `oauth.rs`.

## Goals / Non-Goals

**Goals:**

- Guarantee at most one PKCE flow runs at a time in `AuthManager`.
- Give `commands.rs` the ability to cancel a running flow on demand.
- Wire the wizard's Cancel button to actually terminate the backend flow.
- Abort an in-progress spawn when `start_sign_in` is called a second time.

**Non-Goals:**

- Changing the `url_tx` send order in `oauth.rs` (already correct).
- Modifying the token refresh, storage, or Graph layers.
- Adding UI beyond wiring the existing Cancel button to the new command.
- Supporting queuing of multiple sign-in requests (reject, not queue).

## Decisions

### D1: Cancellation via `tokio_util::sync::CancellationToken`

**Decision**: Add a `CancellationToken` to `AuthManager` that is created fresh for each `sign_in()` call and stored in `Arc<Mutex<Option<CancellationToken>>>`. `wait_for_callback` selects on the token alongside the TCP accept, returning a new `Error::Auth("sign-in cancelled")` variant if the token fires first.

**Alternatives considered**:

- `tokio::sync::oneshot` abort channel: requires plumbing through `run_pkce_flow` and `wait_for_callback` with the same complexity but without the ergonomic `cancelled()` future that `CancellationToken` provides.
- `JoinHandle::abort()` only: aborting the spawn at the `commands.rs` layer drops the task mid-flight, which may leave the TCP listener socket open briefly. It also does not compose with headless callers who call `sign_in()` directly. A token at the `AuthManager` level is more correct.
- `AtomicBool` flag: non-cancellable from async code without polling; `CancellationToken` integrates cleanly with `tokio::select!`.

`tokio-util` is already in `[workspace.dependencies]` with `features = ["rt"]`, so no new dependency is required. The `sync` feature (for `CancellationToken`) needs to be added to the existing entry.

### D2: Concurrency serialization via `Mutex<Option<CancellationToken>>`

**Decision**: Store the current flow's `CancellationToken` in `Arc<Mutex<Option<CancellationToken>>>` on `AuthManager`. On entry to `sign_in()`:

1. Lock the mutex.
2. If `Some(token)` exists, call `token.cancel()` to abort the previous flow.
3. Create a new `CancellationToken`, store it in the mutex, and release the lock.
4. Run the flow, passing a child token to `run_pkce_flow`.
5. On completion (success, error, or cancellation), lock the mutex and clear the stored token.

This approach is simple, avoids holding the lock across the async flow, and allows concurrent callers to see the cancellation immediately.

**Alternative**: `Mutex<bool>` "in_progress" flag that returns `Err` if already active. This is simpler but gives users no way to interrupt a stalled flow — they would be stuck until the 120-second timeout.

### D3: `cancel_sign_in` Tauri command stores `JoinHandle` in `AppState`

**Decision**: Add `active_sign_in: Mutex<Option<JoinHandle<()>>>` to `AppState`. In `start_sign_in`, before spawning:

1. Lock `active_sign_in`.
2. If a handle exists, call `handle.abort()` (belt-and-suspenders on top of the `CancellationToken`).
3. Replace with the new handle.

Add a `cancel_sign_in` command that:
1. Calls `auth.cancel()` (new public method on `AuthManager` that fires the stored `CancellationToken`).
2. Locks `active_sign_in`, aborts the handle, and clears it.

The wizard's `cancelSignIn()` adds `await invoke('cancel_sign_in')` before its existing cleanup.

**Alternative**: Fire `auth.cancel()` directly from `start_sign_in` before the new spawn, without a separate command. This fixes RACE-1 for the retry-immediately scenario but does nothing for the case where the user clicks Cancel without immediately retrying. A dedicated command is cleaner.

### D4: Error variant for cancellation

**Decision**: No new error variant is needed. `cancel()` on a `CancellationToken` causes the `tokio::select!` in `wait_for_callback` to produce an `Error::Auth("sign-in cancelled")` — the existing `Auth(String)` variant covers this. The spawned task in `commands.rs` catches this and emits `auth-error` with the message; the frontend's `auth-error` handler already suppresses processing if `signingIn` is false (which Cancel sets before invoking `cancel_sign_in`), so the stale event is silently dropped.

## Risks / Trade-offs

- [Risk: `CancellationToken` fires while token exchange is in progress] → The guard clears after the full `sign_in()` call completes; if cancellation arrives between `run_pkce_flow` returning and `exchange_code` starting, the exchange proceeds. This is an acceptable race window — it results in valid tokens being stored, not corruption.
- [Risk: `JoinHandle::abort()` in `commands.rs` and `CancellationToken` in `AuthManager` double-cancel] → Double-cancelling is safe: `cancel()` is idempotent, and `abort()` on an already-finished task is a no-op.
- [Risk: Mutex deadlock in `AuthManager`] → The mutex is held only for the short synchronous operations of reading and writing `Option<CancellationToken>`, never across `.await` points. No deadlock risk.
- [Risk: `tokio-util` `sync` feature not present] → The workspace already includes `tokio-util = { version = "0.7", features = ["rt"] }`. Adding `"sync"` to the features list is a safe, additive change.

## Migration Plan

1. Add `"sync"` to `tokio-util` features in root `Cargo.toml`.
2. Modify `cloudmount-auth`: add cancellation token field and `cancel()` method to `AuthManager`; thread the token into `run_pkce_flow` and `wait_for_callback`.
3. Modify `cloudmount-app/src/commands.rs`: add `active_sign_in` to `AppState`, implement abort-before-spawn in `start_sign_in`, add `cancel_sign_in` command, register it in `invoke_handler!`.
4. Modify `crates/cloudmount-app/dist/wizard.html`: add `await invoke('cancel_sign_in')` to `cancelSignIn()`.
5. Run `cargo clippy --all-targets --all-features` and `cargo test --all-targets` to verify no regressions.

No config changes. No database migration. Rollback: revert the four file changes above.

## Open Questions

- None. The implementation path is unambiguous given the existing code structure.
