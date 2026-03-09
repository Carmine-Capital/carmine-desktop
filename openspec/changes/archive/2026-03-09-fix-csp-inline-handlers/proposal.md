## Why

All 5 action buttons in `settings.html` (Save General, Save Advanced, Add Mount, Sign Out, Clear Cache) are non-functional because they use inline `onclick="..."` handlers that are silently blocked by the page's CSP policy `script-src 'self'`. Users click Save and nothing happens — no feedback, no persistence.

## What Changes

- Remove all inline `onclick` attributes from `settings.html`
- Add `id` attributes to the 5 affected buttons
- Wire event listeners via `addEventListener` in `settings.js`
- No CSP policy changes — the existing `script-src 'self'` is correct and should stay

## Capabilities

### New Capabilities

(none)

### Modified Capabilities

- `settings-xss-hardening`: Add requirement that all event handlers must be wired via JS `addEventListener`, never inline HTML attributes, to comply with CSP `script-src 'self'`

## Impact

- `crates/cloudmount-app/dist/settings.html` — remove 5 inline `onclick` attributes, add button IDs
- `crates/cloudmount-app/dist/settings.js` — add `addEventListener` calls at end of file
- No Rust changes, no dependency changes, no API changes
