---
id: ux-info-polish
title: OAuth page, wizard success, dirty-state, empty states, docs
intent: fix-comprehensive-review
complexity: low
mode: autopilot
status: completed
depends_on: []
created: 2026-03-09T18:00:00Z
run_id: run-cloud-mount-022
completed_at: 2026-03-09T19:31:02.228Z
---

# Work Item: OAuth page, wizard success, dirty-state, empty states, docs

## Description

Polish and documentation improvements:

1. **OAuth callback page unstyled** (`oauth.rs`): Browser callback shows plain text. Fix: return minimal styled HTML with CloudMount branding and "You can close this tab" message.

2. **Wizard success unexplained**: Success step shows "All Set" but doesn't explain what happens next (tray icon, file manager access). Fix: add brief explanation text.

3. **No dirty-state indicator** (`settings.html`): Settings page has no visual indicator when values differ from saved. Fix: track original values, add "unsaved changes" badge or asterisk in title.

4. **Empty SharePoint guidance** (`wizard.js`): When user has no followed sites, no guidance on how to follow them. Fix: add help text with link or instructions.

5. **SIGHUP undiscoverable**: Headless re-auth via SIGHUP not mentioned in `--help`. Fix: add to CLI help text.

6. **Config not self-documenting**: Config file has no comments. Fix: add `--print-default-config` flag that outputs annotated default config.

## Acceptance Criteria

- [ ] OAuth callback page has basic styling and CloudMount branding
- [ ] Wizard success step explains tray icon and file manager access
- [ ] Settings page shows unsaved changes indicator
- [ ] Empty SharePoint state shows guidance text
- [ ] SIGHUP documented in --help output
- [ ] --print-default-config outputs annotated defaults

## Technical Notes

OAuth callback HTML is returned from the local HTTP server in `oauth.rs`. Keep it minimal — inline styles are fine here since it's a standalone page not subject to Tauri CSP.

## Dependencies

(none)
