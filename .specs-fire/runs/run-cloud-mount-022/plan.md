# Plan: ux-info-polish

**Mode**: autopilot | **Work Item**: ux-info-polish | **Intent**: fix-comprehensive-review

## Approach

Six small polish changes across auth, wizard, settings, and CLI:

1. **OAuth callback page** — Style the plain HTML in `oauth.rs` with CloudMount branding
2. **Wizard success step** — Add explanation about tray icon and file manager access
3. **Settings dirty-state** — Track original values, show unsaved changes indicator
4. **Empty SharePoint guidance** — Show help text when no followed sites
5. **SIGHUP in --help** — Add `after_help` to CLI to document SIGHUP
6. **--print-default-config** — Add CLI flag to output annotated default config

## Files to Modify

- `crates/cloudmount-auth/src/oauth.rs` — Style success/error callback HTML
- `crates/cloudmount-app/dist/wizard.html` — Add explanation text to success step
- `crates/cloudmount-app/dist/wizard.js` — Add empty followed-sites guidance
- `crates/cloudmount-app/dist/settings.js` — Track dirty state, show indicator
- `crates/cloudmount-app/dist/settings.html` — Add unsaved-changes indicator element
- `crates/cloudmount-app/dist/styles.css` — Style dirty-state indicator
- `crates/cloudmount-app/src/main.rs` — Add SIGHUP docs + --print-default-config

## Tests

- Existing tests must pass (no new logic requiring dedicated tests)
- Manual verification for frontend changes
