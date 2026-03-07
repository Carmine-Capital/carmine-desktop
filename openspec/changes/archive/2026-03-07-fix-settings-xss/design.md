## Context

`settings.html` renders the mount list by building HTML strings and assigning them to `innerHTML`. The data (`m.name`, `m.mount_point`, `m.id`) arrives from the Tauri IPC call `list_mounts`, which reads the user's config file at `~/.config/cloudmount/config.toml`. Because the config file is writable by any process running as the same user, any value in it can be attacker-controlled. When that value contains HTML or JavaScript, assigning it via `innerHTML` causes the browser engine to parse and execute it.

The attack is further amplified by the `onclick` handler construction pattern:

```javascript
'onclick="toggleMount(\'' + m.id + '\')"'
```

An `m.id` value of `x'); invoke('remove_mount',{id:'real-id'});//` would produce a syntactically valid handler that invokes privileged backend commands. The Tauri webview uses the same IPC bridge as legitimate frontend code, so there is no additional privilege boundary separating injected script from backend commands.

Neither `settings.html` nor `wizard.html` declare a Content-Security-Policy, so there is no browser-level defense to fall back on.

## Goals / Non-Goals

**Goals:**

- Eliminate all `innerHTML` assignments that include data from Tauri IPC responses in `settings.html`.
- Bind event handlers as JavaScript function closures, not as injected attribute strings.
- Add a restrictive `Content-Security-Policy` meta tag to `settings.html` and `wizard.html`.
- Ensure the fix survives future additions to the mount list renderer without re-introducing the pattern.

**Non-Goals:**

- Auditing or fixing other webview pages beyond `settings.html` and `wizard.html` (none exist at this time).
- Server-side sanitization in the Rust backend — the fix is entirely in the frontend rendering layer.
- Adopting a frontend framework (React, Vue, etc.) — the fix uses plain DOM APIs consistent with the existing codebase.
- Sanitizing config values at write time — defense-in-depth for the rendering layer is sufficient and avoids over-coupling.

## Decisions

### D1: DOM API over sanitization library

**Decision**: Use `document.createElement` and `textContent` exclusively for user-supplied data. Do not introduce an HTML sanitization library.

**Rationale**: The data being rendered is plain text (mount names, filesystem paths, IDs). It does not need to contain any HTML markup. Treating it as text via `textContent` is provably safe — the browser engine never parses it as HTML. A sanitization library would add a dependency, could have its own CVEs, and is unnecessary when the correct fix is to never parse the data as HTML in the first place.

**Alternatives considered**: DOMPurify or a hand-written escaper. Both are less safe than the DOM API approach because they still involve HTML parsing followed by filtering — the parsing step itself can be exploited by parser-differential attacks.

### D2: Closure-bound onclick handlers, not attribute strings

**Decision**: Set `button.onclick = () => toggleMount(m.id)` rather than generating an `onclick="..."` attribute string.

**Rationale**: Closure-bound handlers capture the value of `m.id` in JavaScript memory. The value is never serialized back to HTML and re-parsed. This completely eliminates the injection surface at `onclick` construction time. It also produces cleaner code and makes the handler logic independently testable.

**Alternatives considered**: `button.setAttribute('data-id', m.id)` with a delegated event listener on the list. Valid approach, but more complex than a direct closure for a list of this size.

### D3: CSP meta tag — script-src 'self', no unsafe-inline

**Decision**: Add `<meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; object-src 'none'">` to both HTML files.

**Rationale**: The existing HTML uses inline `<style>` blocks (acceptable) and inline `<script>` blocks. Tauri loads these files from the app bundle via a custom protocol (`tauri://`), not a remote URL, so `'self'` in this context refers to the bundle origin. The `script-src 'self'` directive prevents injected inline scripts from executing even if an `innerHTML` call were re-introduced in the future. `unsafe-inline` is required for `<style>` because the existing CSS is inline; this is acceptable since CSS injection has a narrower impact surface than JS injection. `object-src 'none'` blocks plugin-based injection vectors.

Note: Tauri v2 also enforces its own CSP layer in `tauri.conf.json`. The meta tag adds a redundant defense at the HTML level that works even in development mode and in any webview environment.

**Alternatives considered**: Configuring CSP exclusively in `tauri.conf.json`. This is valid and should also be done, but is a Tauri config change outside the scope of this purely HTML-layer fix. The meta tag approach requires no config changes and is self-contained in the files being modified.

## Risks / Trade-offs

- [Risk] Inline `<script>` blocks conflict with `script-src 'self'` CSP → The existing scripts are in `<script>` blocks without `src`, which are treated as inline by the CSP engine. If Tauri's webview enforces the meta CSP strictly, the scripts may be blocked. **Mitigation**: Test with the Tauri webview after adding the CSP tag. If inline scripts are blocked, move them to separate `.js` files loaded via `<script src="...">`. This is a clean improvement regardless.
- [Risk] Future developers add `innerHTML` with IPC data without knowing the policy → **Mitigation**: The CSP tag provides a runtime safety net. Code review and the spec requirement (added via delta spec) provide process-level guardrails.
- [Risk] The `m.id` format used in closures differs from what the backend expects if IDs contain special characters → No risk: the ID is passed directly to `invoke()` as a JavaScript string value, not interpolated into any HTML or shell context.

## Migration Plan

1. Apply the DOM API rewrite to `loadMounts()` in `settings.html`.
2. Add CSP meta tags to `settings.html` and `wizard.html`.
3. Test the settings window in the Tauri desktop app with a config containing HTML/JS in mount names and IDs to confirm no execution occurs.
4. Test normal mount list rendering (enable, disable, remove buttons) to confirm functional correctness.
5. If CSP blocks the inline `<script>` blocks: extract scripts to `settings.js` / `wizard.js` and update the `<script src="...">` references. This would be a follow-up commit.

No rollback complexity — the change is confined to two static HTML files. Reverting is a single git revert.

## Open Questions

- Should the CSP meta tag be complemented by a matching `csp` field in `tauri.conf.json`? This is out of scope for this fix but would be a valuable follow-up hardening step.
- Should `wizard.html` be audited for any `innerHTML` usage with IPC data beyond the CSP addition? A quick audit should accompany the fix (see tasks).
