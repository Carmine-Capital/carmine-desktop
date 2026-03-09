---
run: run-cloud-mount-020
work_item: fix-wizard-ux
intent: fix-comprehensive-review
mode: confirm
checkpoint: plan
approved_at: pending
---

# Implementation Plan: Sanitize paths, back navigation, FUSE pre-check, auth timeout UX

## Approach

Fix 5 wizard UX issues in a single pass across frontend JS, HTML, and Rust backend:

1. **Sanitize display_name/library_name** — Add a `sanitizePath()` JS helper that strips `[/\\:*?"<>|]` chars before constructing mount paths. Apply in `confirmSelectedLibraries()`.

2. **Back navigation** — Add a "Sign in with a different account" link on `step-sources` that calls `sign_out` and returns to `step-welcome`.

3. **FUSE pre-check** — Add `check_fuse_available` Tauri command. Wizard calls it before starting auth on Linux/macOS; shows a blocking error if FUSE is missing.

4. **Platform-native mount root** — Add `get_default_mount_root` Tauri command that returns the expanded `~/Cloud/` path using `expand_mount_point()`. Wizard uses it instead of hardcoded `~/Cloud/`.

5. **Auth timeout countdown** — Add a countdown timer in `step-signing-in` that shows remaining time (120s). Warning style when <30s remain.

## Files to Create

| File | Purpose |
|------|---------|
| (none) | |

## Files to Modify

| File | Changes |
|------|---------|
| `crates/cloudmount-app/dist/wizard.js` | Add `sanitizePath()`, countdown timer, FUSE pre-check call, `get_default_mount_root` call, "different account" handler |
| `crates/cloudmount-app/dist/wizard.html` | Add countdown element in step-signing-in, "different account" link in step-sources |
| `crates/cloudmount-app/src/commands.rs` | Add `check_fuse_available` and `get_default_mount_root` Tauri commands |
| `crates/cloudmount-app/src/main.rs` | Register new commands in `invoke_handler!`, make `fuse_available` pub(crate) |

## Technical Details

### Path Sanitization
```js
function sanitizePath(name) {
  return name.replace(/[/\\:*?"<>|]/g, '_').trim();
}
```
Applied to both `site.display_name` and `library.name` before building mount path.

### FUSE Pre-check Command
```rust
#[tauri::command]
pub fn check_fuse_available() -> bool {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    { crate::fuse_available() }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    { true } // Windows uses CfApi, always available after preflight
}
```

### Mount Root Command
```rust
#[tauri::command]
pub fn get_default_mount_root(app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let config = state.effective_config.lock().map_err(|e| e.to_string())?;
    Ok(expand_mount_point(&format!("~/{}/", config.root_dir)))
}
```

### Countdown Timer
- `setInterval` in JS, counts from 120s
- Shows "Time remaining: Xs" below the spinner
- At <30s: warning color
- Clears on auth-complete, auth-error, or cancel

### Back Navigation
- "Sign in with a different account" link on step-sources
- Calls `sign_out` command, then shows step-welcome

---
*Plan approved at checkpoint. Execution follows.*
