## Why

A project status audit revealed 5 issues that block a clean build, passing CI, and correct runtime behavior. Two are bugs (token storage data loss, unwired UI command), two are missing assets/code quality (icons, clippy warnings), and one is an incomplete feature (headless mode is a stub that logs and exits). These must be resolved before packaging and distribution work can begin.

## What Changes

- **Fix silent token loss in keyring storage**: `store_tokens()` trusts the keyring's `Ok(())` return without verifying the data actually persists. On some systems (locked keyring, null backend), `set_password` succeeds but `get_password` returns `NoEntry` — and since keyring didn't error, the encrypted file fallback never runs. Add verify-after-write: read back immediately after keyring store, fall through to encrypted file if the read fails.
- **Generate application icon formats**: `tauri.conf.json` references `icons/32x32.png`, `icons/128x128.png`, `icons/icon.icns`, `icons/icon.ico` — source SVG provided at `icons/icon.svg`, needs conversion to required platform formats.
- **Fix clippy warnings**: 4 warnings (3 `nonminimal_bool` in auth tests, 1 in auth lib) — CI runs `RUSTFLAGS=-Dwarnings` so these are build failures.
- **Wire cache clear command**: The Advanced settings tab has a "Clear Cache" button (`settings.html:178`) that calls an empty `clearCache()` function. The tray-app spec already requires this (line 121). Add the backend Tauri command and wire it to the frontend.
- **Implement minimal headless mode**: `run_headless()` currently creates a tokio runtime, logs "ready", and exits. Implement a functional headless mode: load config, authenticate, mount drives, run sync loop, handle signals for graceful shutdown — the same lifecycle as desktop mode but without Tauri/WebView.

## Capabilities

### New Capabilities

_(none — all changes fit within existing capability boundaries)_

### Modified Capabilities

- `microsoft-auth`: Token storage must verify persistence after keyring write — if verify fails, fall through to encrypted file fallback (new scenario under "Secure token storage")
- `app-lifecycle`: Add headless mode operation — the application must support running without the `desktop` feature, performing the full mount lifecycle (auth, mount, sync, shutdown) via CLI/terminal instead of Tauri

## Impact

- **`crates/carminedesktop-auth/src/storage.rs`**: Modify `store_tokens()` to verify-after-write on keyring path
- **`crates/carminedesktop-auth/tests/auth_integration.rs`**: Fix clippy warnings (`nonminimal_bool`)
- **`crates/carminedesktop-app/src/main.rs`**: Rewrite `run_headless()` with full lifecycle
- **`crates/carminedesktop-app/src/commands.rs`**: Add `clear_cache` Tauri command
- **`crates/carminedesktop-app/dist/settings.html`**: Wire `clearCache()` to invoke backend
- **`crates/carminedesktop-app/icons/`**: Convert source SVG to 4 platform formats (PNG, ICNS, ICO)
- **`crates/carminedesktop-app/tauri.conf.json`**: No changes needed (already references correct paths)
- **CI**: All existing checks should pass after fixes (fmt, clippy, tests)
