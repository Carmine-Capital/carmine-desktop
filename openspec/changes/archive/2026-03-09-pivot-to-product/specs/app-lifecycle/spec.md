## MODIFIED Requirements

### Requirement: Component initialization
The system SHALL initialize all service components in dependency order during application startup, before accepting user interactions. Startup SHALL begin with .env file loading, CLI argument parsing, and pre-flight validation before proceeding to component creation.

#### Scenario: Initialization sequence
- **WHEN** the application starts
- **THEN** it initializes in this order: (1) load .env file if present, (2) parse CLI arguments (including env var fallbacks), (3) load user config → derive effective config with built-in defaults, (4) run pre-flight validation (client ID sanity check, FUSE availability on Linux/macOS, CfApi version on Windows), (5) create AuthManager with the official carminedesktop client_id (overridden by `--client-id` CLI arg if provided), (6) create GraphClient with AuthManager's token provider, (7) create CacheManager with cache directory and SQLite database, (8) create shared InodeTable, (9) assemble AppState with all components, (10) register `tauri-plugin-updater` in the Tauri builder (desktop mode only)

#### Scenario: Initialization failure in config
- **WHEN** the user config cannot be loaded
- **THEN** the system falls back to built-in defaults, logs a warning, and continues startup

#### Scenario: Initialization failure in cache
- **WHEN** the cache directory cannot be created or the SQLite database cannot be opened
- **THEN** the system logs an error and exits with a descriptive error message

#### Scenario: Pre-flight validation failure
- **WHEN** pre-flight checks detect a critical problem (e.g., placeholder client ID or unsupported Windows version)
- **THEN** on Windows release desktop builds, the system displays a `MessageBoxW` dialog with the full actionable error message and exits with code 1; on all other platforms and build configurations, the system prints the actionable error message to stderr and exits with code 1
