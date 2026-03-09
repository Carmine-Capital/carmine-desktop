# Test Report — run-cloud-mount-011

## Work Item: fix-cfapi-double-sanitize

### Test Results
- `cargo check --all-targets`: PASS
- `cargo clippy --all-targets --all-features`: PASS (0 warnings)

### Acceptance Criteria Validation
- [x] main.rs no longer calls `replace('!', "_")` on drive_id before passing to CfMountHandle
- [x] build_sync_root_id in cfapi.rs still sanitizes `!` to `_` (unchanged)
- [x] cargo check and cargo clippy pass clean with -Dwarnings

---

## Work Item: fix-settings-error-feedback

### Test Results
- Manual review: All 5 changes verified in settings.js
- No inline event handlers (CSP compliant)
- Button IDs verified present in settings.html

### Acceptance Criteria Validation
- [x] loadSettings failure shows error status to user via showStatus()
- [x] loadMounts failure shows error status to user via showStatus()
- [x] saveAdvanced uses getElementById('btn-save-advanced')
- [x] clearCache uses getElementById('btn-clear-cache')
- [x] signOut uses getElementById('btn-sign-out')
- [x] All errors use showStatus() from ui.js

---

## Work Item: fix-wizard-error-feedback

### Test Results
- `cargo check --all-targets`: PASS
- `cargo clippy --all-targets --all-features`: PASS (0 warnings)
- UX review: All 6 changes verified, no CSP violations, no regressions
- Manual code review: All error paths restore button state correctly

### Acceptance Criteria Validation
- [x] B1: startSignIn catch shows `showStatus('Sign-in failed', 'error')` (L43)
- [x] S6: copyAuthUrl clipboard failure shows `showStatus('Could not copy URL', 'error')` (L69)
- [x] S1: removeMount only removes DOM row on success; shows error on failure (L369-376)
- [x] D1: getStarted disables button with "Setting up…" during async, restores on error (L404-407, L424-425, L434-435)
- [x] S2: complete_wizard catch shows error and returns to prevent proceeding (L430-437)
- [x] S3: list_mounts on success step wrapped in try/catch with fallback (L439-451)
- [x] No inline event handlers (CSP compliant)
- [x] All error feedback uses showStatus() from ui.js

---

## Work Item: add-accessibility-support

### Test Results
- `cargo check --all-targets`: PASS
- `cargo clippy --all-targets --all-features`: PASS (0 warnings)
- UX review: All changes verified, no CSP violations, focus-visible styles added
- D3 fix: role/tabindex cleaned up on mounted rows after cloneNode
- M7 fix: lib rows use role="checkbox" with aria-checked instead of role="button"

### Acceptance Criteria Validation
- [x] All form inputs in settings.html have associated labels via `for` attribute (sync-interval, cache-dir, cache-max-size, metadata-ttl, log-level)
- [x] Settings tabs are keyboard-navigable with arrow keys (+ Home/End/Enter/Space) and have proper ARIA tab roles (tablist, tab, tabpanel)
- [x] Error divs in wizard.html have role="alert" (#auth-error, #sources-sp-error, #sources-error)
- [x] Status bar in both HTML files has role="status" aria-live="polite"
- [x] Interactive card elements (.sp-result-row) are focusable and activatable via keyboard (role="button", tabindex="0", Enter/Space)
- [x] Library rows (.sp-lib-row) use role="checkbox" with aria-checked for toggle semantics
- [x] No inline event handlers (CSP compliant)
- [x] Focus-visible styles added for .tab, .sp-result-row, .sp-lib-row in styles.css

---

## Work Item: ux-polish

### Test Results
- `cargo check --all-targets`: PASS
- `cargo clippy --all-targets --all-features`: PASS (0 warnings)
- Manual code review: All 5 changes verified across 4 files
- No inline event handlers (CSP compliant)

### Acceptance Criteria Validation
- [x] Error status bar shows a dismiss/close button (× via `.status-dismiss`); clicking it hides the bar
- [x] Clearing search field in wizard restores cached sites via `cachedFollowedSites` without network re-fetch
- [x] Switching sites in wizard shows "Previous selections cleared" info notification when selections exist
- [x] Settings mount list shows "No mounts configured" when empty (`.mount-empty` class)
- [x] Wizard document title updates per step via `_stepTitles` map
- [x] No inline event handlers (CSP compliant)
