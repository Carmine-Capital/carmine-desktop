---
run: run-cloud-mount-020
work_item: fix-wizard-ux
intent: fix-comprehensive-review
---

# Walkthrough: Sanitize paths, back navigation, FUSE pre-check, auth timeout UX

## What Changed

Five wizard UX issues were fixed in a coordinated change across Rust backend (2 new commands) and frontend (JS, HTML, CSS).

## Changes

### 1. Path Sanitization (`wizard.js`)

**Problem:** SharePoint site names and library names from the Graph API can contain filesystem-unsafe characters (`/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`). These were used directly in mount path construction.

**Fix:** Added `sanitizePath()` that replaces unsafe chars with `_`. Applied before constructing mount paths in `confirmSelectedLibraries()`.

### 2. Back Navigation (`wizard.html`, `wizard.js`)

**Problem:** Once on step-sources, users had no way to switch accounts without restarting the app.

**Fix:** Added "Sign in with a different account" button (`switch-account-btn`) at the bottom of step-sources. Calls `sign_out`, resets all state, returns to step-welcome.

### 3. FUSE Pre-check (`commands.rs`, `main.rs`, `wizard.js`)

**Problem:** FUSE unavailability was detected only after auth, via a notification. Users went through the entire sign-in flow only to find out FUSE was missing.

**Fix:**
- Added `check_fuse_available` Tauri command that returns `bool` (platform-gated: calls `fuse_available()` on Linux/macOS, always `true` on Windows)
- Made `fuse_available` in main.rs `pub(crate)` so commands.rs can call it
- Wizard checks before starting auth; shows error via `showStatus()` if false

### 4. Platform-native Mount Root (`commands.rs`, `wizard.js`, `wizard.html`)

**Problem:** Mount paths were constructed with hardcoded `~/Cloud/` which is a Unix convention. On Windows this wouldn't resolve correctly.

**Fix:**
- Added `get_default_mount_root` Tauri command that reads `root_dir` from effective config and expands via `expand_mount_point()` (returns OS-native path like `/home/user/Cloud` or `C:\Users\user\Cloud`)
- Wizard fetches at init, uses it for all mount path construction
- OneDrive card shows the actual expanded path instead of `~/Cloud/OneDrive`

### 5. Auth Timeout Countdown (`wizard.js`, `wizard.html`, `styles.css`)

**Problem:** The 120-second auth timeout had no UI feedback. Users had no idea how much time remained.

**Fix:**
- Added `#auth-countdown` element in step-signing-in
- JS starts a `setInterval` countdown from 120s when sign-in begins
- Shows "Time remaining: Xs" with warning styling (amber) at <=30s
- Clears on auth-complete, auth-error, or cancel

## Files Modified

| File | Lines Changed | Purpose |
|------|-------------|---------|
| `commands.rs` | +18 | 2 new Tauri commands |
| `main.rs` | +3 | pub(crate) visibility, command registration |
| `wizard.js` | +75, -5 | All 5 fixes in JS |
| `wizard.html` | +3 | Countdown div, switch-account btn, mount-path id |
| `styles.css` | +25 | Countdown and btn-link styles |

## Decisions

1. **Expanded paths in config** — Mount points now stored as expanded absolute paths rather than `~/Cloud/...`. This is more portable and avoids tilde-expansion issues on Windows.
2. **FUSE check is non-blocking on error** — If the `check_fuse_available` IPC call itself fails, the wizard proceeds rather than blocking. This prevents issues on unsupported platforms.
3. **Countdown self-clears at 0** — Timer stops itself when reaching 0 rather than showing negative values, matching the 120s backend timeout.
