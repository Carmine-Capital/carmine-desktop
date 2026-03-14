## 1. Rewrite loadMounts() with safe DOM API (X-001 / X-002)

- [x] 1.1 In `crates/carminedesktop-app/dist/settings.html`, replace the entire body of the `mounts.forEach(m => { ... })` block (lines 135-142) so that `li.innerHTML` is never used; instead use `document.createElement` and `element.textContent` for all text values (`m.name`, `m.mount_point`) and closure-bound `.onclick` assignments for button handlers that capture `m.id` and `m.enabled` in a JavaScript closure rather than serializing them into an HTML attribute string. Assign each toggle button `id="toggle-btn-${m.id}"` and each remove button `id="remove-btn-${m.id}"` so that async operations added by `fix-ui-feedback` can locate the correct button via `document.getElementById` to apply loading state.
- [x] 1.2 Verify the rewritten `loadMounts()` renders mount names and paths as plain text: structure the mount list item DOM to match the existing CSS classes (`mount-name`, `mount-path`, `mount-item`) so visual appearance is unchanged.
- [x] 1.3 Verify the rewritten toggle button correctly reflects `m.enabled` state (label "Disable" when enabled, "Enable" when disabled) using a direct property set (`button.textContent`), not string interpolation into innerHTML.
- [x] 1.4 Verify the rewritten Remove button's `.onclick` closure calls `removeMount(m.id)` with the actual `m.id` value, not a stringified version embedded in HTML.
- [x] 1.5 Confirm that `list.innerHTML = ''` on line 134 (used to clear the list before repopulating) is retained — this is safe because it assigns a static empty string, not user data.

## 2. Add Content-Security-Policy to settings.html

- [x] 2.1 Add `<meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; object-src 'none'">` as the third `<meta>` tag in the `<head>` of `crates/carminedesktop-app/dist/settings.html` (after the charset and viewport meta tags, before `<title>`).
- [x] 2.2 Test the settings window in the Tauri desktop build (`cargo run -p carminedesktop-app --features desktop`) to confirm the inline `<script>` block loads correctly under the CSP. If the webview blocks inline scripts under `script-src 'self'`, proceed to task 2.3; otherwise this task is complete.
- [x] 2.3 (Conditional — only if 2.2 reveals inline script blocking) Extract the inline `<script>` block from `settings.html` to `crates/carminedesktop-app/dist/settings.js` and update the `<script>` tag to `<script src="settings.js"></script>`. Ensure `settings.js` is included in any build/copy steps that deploy `settings.html`.

## 3. Add Content-Security-Policy to wizard.html

- [x] 3.1 Add the same `<meta http-equiv="Content-Security-Policy">` tag (identical policy as task 2.1) to the `<head>` of `crates/carminedesktop-app/dist/wizard.html`, after the existing charset and viewport meta tags.
- [x] 3.2 Confirm `wizard.html` already uses `textContent` for all IPC-derived values (audit lines 144, 154 and the full `init()` function) — no `innerHTML` with IPC data is present; document this as a no-change audit finding.
- [x] 3.3 Test the wizard window in the Tauri desktop build to confirm it still functions normally after adding the CSP tag. If inline scripts are blocked, apply the same extract-to-js approach as task 2.3 for `wizard.js`.

## 4. Manual security verification

- [ ] 4.1 Add a test mount entry to `~/.config/carminedesktop/config.toml` with `name = "<img src=x onerror=alert(1)>"` and confirm the settings window displays the literal string without triggering the image load or alert.
- [ ] 4.2 Add a test mount entry with `id = "x'); invoke('remove_mount',{id:'real-id'});//"` and confirm the Enable/Disable and Remove buttons still invoke the correct Tauri commands with the literal ID value, and no spurious invocations occur.
- [ ] 4.3 Add a test mount entry with `mount_point = "/home/user/<script>alert(2)</script>"` and confirm the path is displayed as literal text.
- [ ] 4.4 After verifying all three payloads are inert, remove the test entries from the config file.

## 5. Regression verification

- [ ] 5.1 With a normal config (no adversarial values), confirm the Mounts tab renders correctly: mount name and path display, Enable/Disable button reflects current mount state, Remove button removes the entry and refreshes the list.
- [ ] 5.2 Confirm that toggling a mount (Enable/Disable) correctly calls `invoke('toggle_mount', { id })` and that `loadMounts()` is called afterward to refresh the list.
- [x] 5.3 Run `cargo clippy --all-targets --all-features` and `cargo fmt --all -- --check` to confirm no Rust-side regressions were introduced (the fix is HTML/JS only, but confirm build is clean).
