## MODIFIED Requirements

### Requirement: Branded UI elements
The system SHALL display the packaged branding throughout the UI when a custom app name is configured.

#### Scenario: Tray tooltip with custom name
- **WHEN** the packaged defaults define `app_name = "Contoso Drive"`
- **THEN** the system tray icon tooltip displays "Contoso Drive" instead of "CloudMount"

#### Scenario: Window titles with custom name
- **WHEN** the packaged defaults define a custom app name
- **THEN** the wizard window title, settings window title, and notification titles all use the custom name

#### Scenario: Default branding
- **WHEN** no custom app name is packaged
- **THEN** all UI elements display "CloudMount"
