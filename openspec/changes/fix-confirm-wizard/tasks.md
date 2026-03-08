## 1. Capabilities: Add dialog permission

- [x] 1.1 Add `"dialog:allow-confirm"` to `crates/cloudmount-app/capabilities/default.json`

## 2. Backend: Add `is_authenticated` command

- [x] 2.1 Add `pub fn is_authenticated(app: AppHandle) -> bool` to `crates/cloudmount-app/src/commands.rs`, reading `app.state::<AppState>().authenticated.load(Ordering::Relaxed)`
- [x] 2.2 Register `is_authenticated` in the `invoke_handler!` macro in `crates/cloudmount-app/src/main.rs`

## 3. Backend: Add `open_or_focus_wizard` helper

- [x] 3.1 Add `pub fn open_or_focus_wizard(app: &AppHandle, add_mount: bool)` to `crates/cloudmount-app/src/tray.rs`: if window exists and `add_mount` is true, call `win.eval("goToAddMount()")` before focusing; otherwise call existing focus logic; if window does not exist, create it normally
- [x] 3.2 Update `handle_menu_event` in `tray.rs` to call `open_or_focus_wizard(app, false)` for `"sign_in"` and `"re_authenticate"`, and `open_or_focus_wizard(app, true)` for `"add_mount"`
- [x] 3.3 Update `open_wizard` command in `commands.rs` to call `open_or_focus_wizard(&app, true)` so the settings "Add Mount" button also navigates to `step-sources`

## 4. Frontend: Wizard auth-aware routing

- [x] 4.1 Add `async function goToAddMount()` to `wizard.js` that calls `onSignInComplete()` (exposes it as a callable global for `win.eval`)
- [x] 4.2 In `wizard.js init()`, call `invoke('is_authenticated')` and if `true`, call `goToAddMount()` before returning, so any wizard opened while authenticated starts at `step-sources`

## 5. Frontend: Replace `window.confirm()` with Tauri dialog

- [x] 5.1 In `settings.js signOut()`, replace `if (!confirm('Sign out? All mounts will stop.')) return;` with `const ok = await window.__TAURI__.dialog.confirm('Sign out? All mounts will stop.', { title: 'Sign Out', kind: 'warning' }); if (!ok) return;`
- [x] 5.2 In `settings.js removeMount()`, replace `if (!confirm('Remove this mount? This cannot be undone.')) return;` with the same Tauri dialog pattern, title `'Remove Mount'`
- [x] 5.3 Make both `signOut()` and `removeMount()` properly `async` if not already (they are — verify no caller awaits them without handling the async nature)

## 6. Verify

- [x] 6.1 Build with `cargo build -p cloudmount-app --features desktop` — zero warnings
- [ ] 6.2 Manually verify: Sign Out button in Account tab shows OS dialog and signs out on confirm
- [ ] 6.3 Manually verify: Remove mount button shows OS dialog and removes on confirm
- [ ] 6.4 Manually verify: Settings "Add Mount" opens wizard at sources step (not sign-in screen)
- [ ] 6.5 Manually verify: Tray "Add Mount…" opens wizard at sources step when already authenticated
- [ ] 6.6 Manually verify: Tray "Add Mount…" when wizard already open at step-welcome navigates it to step-sources
- [ ] 6.7 Manually verify: Opening wizard when NOT authenticated still shows step-welcome
