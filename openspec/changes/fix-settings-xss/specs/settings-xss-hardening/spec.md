## ADDED Requirements

### Requirement: Safe DOM rendering for mount list
The settings UI SHALL render all mount list entries using safe DOM API calls exclusively. No user-supplied string received from Tauri IPC SHALL be assigned to `innerHTML`, `outerHTML`, or any property that causes the browser to parse the value as HTML markup.

#### Scenario: Mount name containing HTML is rendered as plain text
- **WHEN** a mount entry returned by `list_mounts` has a `name` field containing HTML markup (e.g., `<b>bold</b>`)
- **THEN** the text is displayed literally as `<b>bold</b>` in the mount list, and no HTML element is created or styled from that string

#### Scenario: Mount path containing HTML is rendered as plain text
- **WHEN** a mount entry returned by `list_mounts` has a `mount_point` field containing HTML markup
- **THEN** the path is displayed as literal text; no HTML elements are created from it

#### Scenario: Mount ID with JavaScript payload does not execute
- **WHEN** a mount entry returned by `list_mounts` has an `id` field containing a JavaScript expression (e.g., `'); alert(1);//`)
- **THEN** the settings window loads and renders the mount list without executing any script derived from that value; the Enable/Disable and Remove buttons remain functional using the actual ID value passed to the Tauri IPC call

### Requirement: Closure-bound event handlers for mount actions
The settings UI SHALL bind mount action buttons (enable/disable toggle, remove) using JavaScript function closures or `addEventListener`. Mount IDs SHALL NOT be serialized into HTML attribute strings to construct `onclick` handlers.

#### Scenario: Toggle button handler uses closure-captured ID
- **WHEN** the mount list is rendered and the user clicks the enable/disable button for a mount
- **THEN** the correct mount ID is passed to `invoke('toggle_mount', { id })` without any HTML serialization of the ID occurring

#### Scenario: Remove button handler uses closure-captured ID
- **WHEN** the mount list is rendered and the user clicks the Remove button for a mount
- **THEN** the correct mount ID is passed to `invoke('remove_mount', { id })` without any HTML serialization of the ID occurring

### Requirement: Content-Security-Policy in settings and wizard pages
Both `settings.html` and `wizard.html` SHALL declare a Content-Security-Policy that prohibits execution of scripts from untrusted sources and blocks plugin-based content.

#### Scenario: CSP meta tag present in settings.html
- **WHEN** `settings.html` is loaded in the Tauri webview
- **THEN** a `<meta http-equiv="Content-Security-Policy">` tag is present in the document `<head>` with a policy that includes `script-src 'self'` and `object-src 'none'`

#### Scenario: CSP meta tag present in wizard.html
- **WHEN** `wizard.html` is loaded in the Tauri webview
- **THEN** a `<meta http-equiv="Content-Security-Policy">` tag is present in the document `<head>` with a policy that includes `script-src 'self'` and `object-src 'none'`

#### Scenario: Injected inline script blocked by CSP
- **WHEN** an attacker-controlled value causes an inline script to be injected into the DOM (e.g., via a future regression to `innerHTML`)
- **THEN** the browser's CSP enforcement prevents that inline script from executing
