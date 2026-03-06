## Why

Headless mode was implemented in the `fix-pre-release-bugs` change but a detailed audit against the spec and the desktop code path reveals 7 issues. The most critical is a token storage key mismatch in `cloudmount-auth` that breaks token restoration across restarts in BOTH modes: `exchange_code()` and `refresh()` store tokens under `self.client_id` (the Azure AD app ID), but `try_restore()` loads tokens using the `account_id` parameter (which is `drive.id` from the user config) — the keys never match, so tokens are never found on restart. Two further bugs break the headless first-time-use flow (no account metadata or OneDrive mount persisted after sign-in), two are behavioral defects (auth-degradation warning spam, crash recovery blocking mount startup), and two are missing capabilities (no re-authentication path in degraded mode, duplicated initialization code that will drift). These must be fixed before headless mode is usable for real-world deployment.

## What Changes

- **Fix token storage key mismatch**: In `AuthManager`, `exchange_code()` and `refresh()` store tokens under `self.client_id` (e.g., `"00000000-..."`) while `try_restore(account_id)` loads tokens using the passed `account_id` (e.g., `"b!xYz..."` — the OneDrive drive ID from config). The keys never match, so token restoration fails on every restart. Fix `try_restore()` to use `self.client_id` as the storage key, consistent with store/refresh/delete. This bug affects both desktop and headless modes.
- **Persist account metadata after headless sign-in**: After a successful headless sign-in, call `GET /me/drive` to discover the user's OneDrive, write `AccountMetadata` to `config.toml`, and auto-create a default OneDrive mount if none exists — mirroring the desktop `sign_in` command behavior.
- **Deduplicate auth-degradation warnings**: Add a local `auth_degraded` flag in the headless sync loop so the "Re-authentication required" warning is logged once, not on every sync cycle for every drive.
- **Non-blocking crash recovery**: Spawn crash recovery as a background task (matching desktop behavior) so mount startup is not delayed by pending-write uploads.
- **SIGHUP re-authentication**: Add a Unix SIGHUP handler that triggers a browser-based OAuth re-authentication attempt, clears the degraded flag on success, and flushes pending writes — giving headless users a recovery path without restarting the process.
- **Extract shared initialization**: Factor the duplicated component-initialization code (~30 lines) from `run_desktop()` and `run_headless()` into a shared `init_components()` function to eliminate drift risk.
- **Wire `user_config` in headless**: Pass `UserConfig` through to the post-sign-in logic so account metadata and mount config can be persisted, then drop the underscore-prefixed `_user_config` parameter.

## Capabilities

### New Capabilities

_(none)_

### Modified Capabilities

- `microsoft-auth`: Fix token restoration to use the same storage key as token storage — `try_restore()` SHALL use the application client_id (not the caller-provided account_id) as the credential store lookup key
- `app-lifecycle`: Add headless post-sign-in behavior (OneDrive auto-discovery, account persistence, auto-mount creation); add SIGHUP re-authentication scenario; tighten auth-degradation logging requirement (log once, not per-cycle); require non-blocking crash recovery in headless mode

## Impact

- **`crates/cloudmount-auth/src/manager.rs`**: Fix `try_restore()` to use `self.client_id` as storage key (1-line change, fixes both desktop and headless restart)
- **`crates/cloudmount-app/src/main.rs`**: Primary file — extract `init_components()`, rewrite `run_headless()` post-sign-in flow, add SIGHUP handler, add `auth_degraded` flag, spawn crash recovery
- **`openspec/specs/app-lifecycle/spec.md`**: Delta spec with new/updated scenarios for headless post-sign-in, SIGHUP re-auth, deduped degradation logging, non-blocking crash recovery
- **No new dependencies**: All required crates (`open`, `tokio::signal`, `cloudmount-auth`, `cloudmount-graph`, `cloudmount-core::config`) are already in scope
- **No API changes**: Internal refactor only, no user-facing config or command changes
