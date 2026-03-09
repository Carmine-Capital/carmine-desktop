## ADDED Requirements

### Requirement: CSP-compliant event handler wiring
All interactive elements in settings and wizard HTML pages SHALL have their event handlers wired exclusively via JavaScript `addEventListener` or programmatic property assignment (e.g., `.onclick = () => ...`). No HTML element SHALL use inline event handler attributes (`onclick`, `onsubmit`, `onchange`, or any `on<event>` attribute), as these are blocked by the CSP `script-src 'self'` policy.

#### Scenario: Settings page buttons respond to clicks
- **WHEN** the settings page is loaded in the Tauri webview with CSP `script-src 'self'` active
- **THEN** all action buttons (Save General, Save Advanced, Add Mount, Sign Out, Clear Cache) execute their associated handler functions when clicked

#### Scenario: Inline onclick attribute is absent from all HTML
- **WHEN** any HTML file in `crates/cloudmount-app/dist/` is inspected
- **THEN** no element contains an `onclick`, `onsubmit`, `onchange`, or other inline `on<event>` attribute

#### Scenario: New button added with inline handler is caught
- **WHEN** a developer adds a new button with an inline `onclick="..."` attribute to settings.html
- **THEN** the UX reviewer agent flags it as a BLOCKED issue due to CSP non-compliance
