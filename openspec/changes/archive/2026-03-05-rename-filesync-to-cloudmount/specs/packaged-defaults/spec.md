## MODIFIED Requirements

### Requirement: Packaged tenant and branding
The system SHALL use the packaged tenant and branding information to customize the authentication flow and UI.

#### Scenario: Pre-configured tenant ID
- **WHEN** `build/defaults.toml` contains a `[tenant]` section with `id`
- **THEN** the OAuth2 authorization URL includes `&domain_hint={tenant_id}` so the Microsoft login page skips organization selection and goes directly to the correct tenant login

#### Scenario: Pre-configured client ID
- **WHEN** `build/defaults.toml` contains a `[tenant]` section with `client_id`
- **THEN** the OAuth2 flow uses this client ID instead of the generic CloudMount app registration

#### Scenario: Custom app name
- **WHEN** `build/defaults.toml` contains `[branding]` with `app_name`
- **THEN** the system tray tooltip, window titles, notification titles, and wizard header all display the custom app name instead of "CloudMount"

#### Scenario: No branding configured
- **WHEN** `build/defaults.toml` does not contain a `[branding]` section
- **THEN** the application uses "CloudMount" as the default app name everywhere
