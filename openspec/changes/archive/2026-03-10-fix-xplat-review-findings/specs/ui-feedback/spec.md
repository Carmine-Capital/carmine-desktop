## MODIFIED Requirements

### Requirement: In-page toast notification system
The backend notification subsystem SHALL log the actual error reason when a platform notification fails to display. The `send()` helper SHALL include the error value in the `tracing::warn!` message (e.g., "failed to send notification '{title}': {error}") instead of discarding the error object before logging. This ensures D-Bus failures on Linux and other platform-specific notification errors are diagnosable from logs.

#### Scenario: Notification failure logged with reason
- **WHEN** `tauri_plugin_notification` fails to display a notification (e.g., D-Bus unavailable on headless Linux)
- **THEN** the system logs a warning that includes both the notification title and the specific error message from the notification subsystem

#### Scenario: Notification success
- **WHEN** `tauri_plugin_notification` successfully displays a notification
- **THEN** no warning is logged
