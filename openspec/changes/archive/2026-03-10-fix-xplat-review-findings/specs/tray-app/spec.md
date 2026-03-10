## MODIFIED Requirements

### Requirement: System tray presence
The system SHALL display a system tray icon with the CloudMount logo. The tray icon SHALL be loaded from the application's default window icon. If the default window icon is not configured (e.g., during development builds without bundled assets), the tray setup SHALL return an error instead of panicking, and the application SHALL log a warning and continue without a tray icon.

#### Scenario: Tray icon loads successfully
- **WHEN** the application starts with a configured default window icon
- **THEN** the system tray icon is displayed using the bundled icon

#### Scenario: Missing tray icon in development
- **WHEN** the application starts without a configured default window icon (e.g., unbundled dev build)
- **THEN** the tray setup returns an error, the application logs a warning, and the application continues operating without a tray icon instead of panicking
