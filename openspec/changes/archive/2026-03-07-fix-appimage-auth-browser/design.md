## Context

The desktop auth flow triggers `xdg-open` via `tauri_plugin_opener.open_url()`. On Linux AppImages, the runtime prepends a bundled GLib/GTK lib path to `LD_LIBRARY_PATH`. The system `gio open` binary (called internally by `xdg-open`) picks up the bundled, older GLib instead of the system's, causing it to fail silently. `tauri_plugin_opener` spawns `xdg-open` in a fire-and-forget fashion — it returns `Ok(())` on successful spawn, not successful browser open — so the failure is completely invisible.

The secondary problem: even if `xdg-open` fails for any reason, the only fallback is `print_auth_url()` which writes to stderr — invisible in a GUI AppImage with no terminal.

Current opener in `run_desktop()`:
```
tauri_plugin_opener.open_url(url) → spawns xdg-open (inherits LD_LIBRARY_PATH) → gio fails silently → Ok(())
```

Target flow:
```
direct Command::new("xdg-open").env_remove("LD_LIBRARY_PATH") → status() → real error if gio fails
                                                                           ↓
                                              wizard UI shows auth URL as copy-paste fallback
```

## Goals / Non-Goals

**Goals:**
- Browser opens correctly when running as AppImage on Fedora Silverblue/Aurora
- If browser launch fails (for any reason), user sees the auth URL in the wizard and can copy-paste it
- Errors from `xdg-open` are observable in logs
- macOS and Windows behaviour unchanged

**Non-Goals:**
- Supporting headless auth URL display (already handled by `print_auth_url` → stderr)
- Fixing `xdg-open` itself or making it AppImage-aware globally
- Sandboxed/Flatpak packaging (separate problem)

## Decisions

### D1: Direct `std::process::Command` instead of `tauri_plugin_opener` on Linux

**Decision**: Replace `tauri_plugin_opener.open_url()` with a direct `Command::new("xdg-open")` call that strips `LD_LIBRARY_PATH` and `LD_PRELOAD` from the child environment, and uses `.status()` to capture the exit code.

**Alternatives considered**:
- *Keep `tauri_plugin_opener`, detect AppImage*: Would need to fall back to `Command` anyway for the AppImage case, so we'd have two paths on Linux with no benefit for the non-AppImage case. Simpler to always use the direct approach on Linux.
- *Fork before spawning, restore env in child*: Overkill. `env_remove` on `Command` is per-child and doesn't affect the current process.
- *Patch `LD_LIBRARY_PATH` to remove AppImage entries*: Fragile — requires parsing the path list and identifying which entries are AppImage-specific.

`tauri_plugin_opener` is kept for macOS and Windows where it handles platform-specific edge cases (macOS quarantine, Windows ShellExecute).

### D2: `.status()` instead of `.spawn()` for error observability

**Decision**: Wait for `xdg-open` to exit using `.status()`. `xdg-open` exits quickly (it delegates to the browser and returns), so this does not block the UI meaningfully.

**Alternatives considered**:
- *`.spawn()` (fire-and-forget)*: Current behaviour — unobservable failures. Rejected.
- *Spawn thread, wait with timeout*: Unnecessary complexity. `xdg-open` reliably exits in < 1s on GNOME.

Exit code interpretation: exit 0 → `Ok(())`, non-zero → `Err(format!("xdg-open exited with {s}"))`. When `Err` is returned, `oauth.rs` already logs a warn and calls `print_auth_url`. The wizard URL display (D3) provides the in-GUI path.

### D3: Auth URL displayed in wizard during sign-in

**Decision**: When the frontend calls `sign_in`, the command immediately returns the auth URL to the frontend before waiting for the PKCE callback. The wizard shows the URL with a copy button. The existing `wait_for_callback` continues to run in the background.

**Implementation approach**: Split the current `sign_in` command into two phases:
1. Frontend calls `sign_in` → backend starts PKCE flow, returns the auth URL immediately (new `start_sign_in` command)
2. Frontend displays URL + polls or waits for completion signal (Tauri event)
3. Backend emits `auth-complete` or `auth-error` event after PKCE callback resolves

**Alternatively** (simpler): `start_sign_in` returns `{ auth_url, ... }`, spawns a background task that emits an event on completion. Frontend displays URL immediately, listens for the completion event.

**Alternatives considered**:
- *Keep single blocking `sign_in` command, just log the URL*: Frontend never sees the URL. The wizard can't display it.
- *Pass URL via a Tauri event before blocking*: Requires `sign_in` to emit an event mid-execution. Doable but awkward with the current command structure.
- *Store URL in AppState, frontend polls*: Extra state, polling complexity. Events are cleaner.

### D4: `run_pkce_flow` returns auth URL for forwarding

**Decision**: `run_pkce_flow` already knows the auth URL before blocking on `wait_for_callback`. Extract the URL construction so `AuthManager::sign_in` can return or forward it.

Options:
- Add a callback/channel parameter to `run_pkce_flow` that receives the URL before blocking — clean, no API change to the returned type
- Return `(auth_url, code, verifier, port)` from a refactored inner function — requires splitting `run_pkce_flow`

The channel approach is preferred: add an `Option<oneshot::Sender<String>>` for the URL, sent right before `wait_for_callback` blocks. `AuthManager::sign_in` receives this via a channel and forwards to `start_sign_in`'s caller.

## Risks / Trade-offs

- **`xdg-open` not found**: If `xdg-open` is missing (minimal/headless system), `Command::new("xdg-open")` returns `Err`. The existing fallback (warn log + `print_auth_url`) handles this. The wizard URL display also covers it. Low risk.
- **`.status()` blocks briefly**: If `xdg-open` hangs (unusual), the opener closure blocks the async task. Acceptable since `xdg-open` reliably exits fast on all target systems. Could add a timeout if this proves to be a problem.
- **Wizard URL display complexity**: The split `start_sign_in` + event approach adds frontend state management. If the change feels too large, phase it — ship D1+D2 first (backend fix, auth works), D3 in a follow-up (wizard URL display).

## Open Questions

- Should `start_sign_in` replace `sign_in` entirely, or should `sign_in` be kept for the headless/non-wizard path? (Likely: `sign_in` stays for headless, `start_sign_in` is new for the wizard UI)
- What is the wizard state during the 120s PKCE wait? Show a spinner + URL, with a "Cancel" button that aborts the listener?
