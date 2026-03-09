## Context

The settings page declares `script-src 'self'` in its CSP meta tag, which correctly blocks inline scripts for XSS hardening. However, 5 buttons still use inline `onclick="..."` HTML attributes, which CSP silently blocks. The buttons appear interactive but do nothing when clicked. The dynamically-created mount buttons (toggle/remove) already use the correct pattern — closure-bound `.onclick` assignments in JS.

## Goals / Non-Goals

**Goals:**
- Make all 5 settings buttons functional by wiring them via `addEventListener`
- Maintain full CSP `script-src 'self'` compliance — no `'unsafe-inline'`
- Follow the same pattern already used by mount toggle/remove buttons

**Non-Goals:**
- Changing the CSP policy itself (it's correct as-is)
- Refactoring the save logic or Tauri command interface
- Adding new UI features or changing feedback behavior

## Decisions

### Wire buttons via `addEventListener` at DOM-ready time

**Decision**: Add `id` attributes to the 5 affected buttons in HTML, then call `addEventListener('click', ...)` in `settings.js` after the existing `loadSettings()` / `loadMounts()` calls.

**Rationale**: This matches the project's existing pattern (tab switching uses `addEventListener`, mount buttons use `.onclick` closures). No new patterns introduced. The `addEventListener` calls go at the bottom of `settings.js` alongside the existing `loadSettings(); loadMounts();` initialization.

**Alternative considered**: Adding `'unsafe-inline'` to `script-src` — rejected because it would undo the XSS hardening work and weaken the CSP for all future code.

### Use descriptive button IDs

**Decision**: Button IDs follow the pattern `btn-<action>` (e.g., `btn-save-general`, `btn-save-advanced`, `btn-sign-out`, `btn-add-mount`, `btn-clear-cache`).

**Rationale**: Keeps selectors simple and self-documenting. Avoids CSS class-based selection which is fragile when styling changes.

## Risks / Trade-offs

- [Minimal risk] The change is purely mechanical — moving handler references from HTML attributes to JS. No logic changes. If a button ID is mistyped, the `addEventListener` call silently fails (button stays inert) — same failure mode as today, but easier to debug since the handler registration is explicit in JS.
