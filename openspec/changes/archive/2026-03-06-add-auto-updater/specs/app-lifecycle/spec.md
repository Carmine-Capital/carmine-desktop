## MODIFIED Requirements

### Requirement: Component initialization
The system SHALL initialize all service components in dependency order during application startup, before accepting user interactions. Startup SHALL begin with .env file loading, CLI argument parsing, and pre-flight validation before proceeding to component creation.

#### Scenario: Initialization sequence
- **WHEN** the application starts
- **THEN** it initializes in this order: (1) load .env file if present, (2) parse CLI arguments (including env var fallbacks), (3) load config (packaged defaults + user config → effective config), (4) run pre-flight validation (client ID, FUSE availability), (5) create AuthManager with resolved client_id and tenant_id (from CLI > env > packaged > default), (6) create GraphClient with AuthManager's token provider, (7) create CacheManager with cache directory and SQLite database, (8) create shared InodeTable, (9) assemble AppState with all components, (10) register `tauri-plugin-updater` in the Tauri builder (desktop mode only)

#### Scenario: Initialization failure in config
- **WHEN** the packaged defaults or user config cannot be loaded
- **THEN** the system falls back to built-in defaults, logs a warning, and continues startup

#### Scenario: Initialization failure in cache
- **WHEN** the cache directory cannot be created or the SQLite database cannot be opened
- **THEN** the system logs an error and exits with a descriptive error message

#### Scenario: Pre-flight validation failure
- **WHEN** pre-flight checks detect a critical problem (e.g., placeholder client ID)
- **THEN** the system prints an actionable error message to stderr and exits with code 1 before attempting authentication or component initialization

## ADDED Requirements

### Requirement: Update checker lifecycle
The system SHALL start a background update checker after initialization in desktop mode, and cancel it during shutdown.

#### Scenario: Start update checker after mounts
- **WHEN** the application completes initialization and mount startup in desktop mode
- **THEN** the system spawns a background task that waits 10 seconds, performs an initial update check, then checks every 4 hours

#### Scenario: Cancel update checker on shutdown
- **WHEN** the application begins graceful shutdown (quit, signal, or restart-to-update)
- **THEN** the system cancels the periodic update checker task before proceeding with mount teardown

#### Scenario: Restart-to-update shutdown
- **WHEN** the user triggers "Restart to Update" from the tray menu
- **THEN** the system performs the standard graceful shutdown sequence (cancel sync, flush pending writes with 30-second timeout, unmount all drives), then delegates to the Tauri updater plugin to install the update and relaunch the process
