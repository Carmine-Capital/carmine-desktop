## Why

When CloudMount runs as an AppImage, the runtime sets `LD_LIBRARY_PATH` and `LD_PRELOAD` to point to bundled libraries. These environment variables are inherited by every child process. When the tray menu opens a mount folder (or the headless mode opens a browser for OAuth), the spawned `xdg-open` → file manager chain carries the contaminated env, causing applications launched from that file manager (e.g. LibreOffice) to load AppImage-bundled GLib/glibc instead of their own and crash silently.

The desktop OAuth URL opener already has the correct fix (`xdg-open` + `.env_remove("LD_LIBRARY_PATH") + .env_remove("LD_PRELOAD")`). Two other call sites were missed.

## What Changes

- Extract a `pub(crate) fn open_with_clean_env(path: &str) -> Result<(), String>` helper inside `cloudmount-app` that strips `LD_LIBRARY_PATH` and `LD_PRELOAD` on Linux before spawning `xdg-open`, and falls back to `open::that()` on other platforms.
- Replace `open::that(&expanded)` in `tray.rs:72` (mount folder opener) with the new helper.
- Replace `open::that(url)` in `main.rs:942` (headless OAuth URL opener) with the new helper.
- Simplify the desktop OAuth URL opener lambda (`main.rs:409-421`) to delegate to the same helper, eliminating the duplicate inline implementation.

## Capabilities

### New Capabilities

_(none — this is a bug fix with no new user-visible capabilities)_

### Modified Capabilities

_(none — spec-level behavior is unchanged; the fix is purely in subprocess spawning)_

## Impact

- **Files changed**: `crates/cloudmount-app/src/main.rs`, `crates/cloudmount-app/src/tray.rs`
- **No API or crate boundary changes** — helper is `pub(crate)`, internal to cloudmount-app
- **No dependency changes** — uses `std::process::Command` already in use
- **Platforms affected**: Linux AppImage deployments (the env scrubbing is a no-op on clean environments; other platforms are untouched)
