## MODIFIED Requirements

### Requirement: Component initialization
The system SHALL initialize all service components in dependency order during application startup, before accepting user interactions. Startup SHALL begin with .env file loading, CLI argument parsing, and pre-flight validation before proceeding to component creation.

#### Scenario: Initialization sequence
- **WHEN** the application starts
- **THEN** it initializes in this order: (1) load .env file if present, (2) parse CLI arguments (including env var fallbacks), (3) load config (packaged defaults + user config → effective config), (4) run pre-flight validation (client ID, FUSE availability), (5) create AuthManager with resolved client_id and tenant_id (from CLI > env > packaged > default), (6) create GraphClient with AuthManager's token provider, (7) create CacheManager with cache directory and SQLite database, (8) create shared InodeTable, (9) assemble AppState with all components

#### Scenario: Initialization failure in config
- **WHEN** the packaged defaults or user config cannot be loaded
- **THEN** the system falls back to built-in defaults, logs a warning, and continues startup

#### Scenario: Initialization failure in cache
- **WHEN** the cache directory cannot be created or the SQLite database cannot be opened
- **THEN** the system logs an error and exits with a descriptive error message

#### Scenario: Pre-flight validation failure
- **WHEN** pre-flight checks detect a critical problem (e.g., placeholder client ID)
- **THEN** the system prints an actionable error message to stderr and exits with code 1 before attempting authentication or component initialization

### Requirement: Headless mode operation
The system SHALL support running without the `desktop` feature flag, performing the full mount lifecycle (authentication, mounting, sync, graceful shutdown) as a foreground terminal process without Tauri or any graphical UI. The system SHALL also support running in headless mode with the `desktop` feature when `--headless` is passed.

#### Scenario: Headless startup with existing tokens
- **WHEN** the application starts in headless mode and valid tokens are found in the credential store
- **THEN** the system restores tokens, runs crash recovery for pending writes, starts all enabled mounts, starts the periodic delta sync loop, and logs "CloudMount headless mode running — N mount(s) active"

#### Scenario: Headless startup without tokens
- **WHEN** the application starts in headless mode and no valid tokens are found in the credential store
- **THEN** the system attempts browser-based OAuth sign-in by opening the system default browser via the same PKCE flow used in desktop mode; if the browser cannot be opened (no display server), the system prints the auth URL to stdout for manual copy-paste; if sign-in fails after all attempts, the system logs the error and exits with a non-zero exit code

#### Scenario: Headless component initialization
- **WHEN** the application starts in headless mode
- **THEN** it initializes the same components as desktop mode (AuthManager, GraphClient, CacheManager, InodeTable) using the same configuration system (packaged defaults + user config → effective config), with the same CLI/env override chain, without creating any Tauri application context

#### Scenario: Headless graceful shutdown
- **WHEN** the headless process receives SIGTERM or SIGINT (Ctrl+C)
- **THEN** the system cancels the delta sync timer, flushes pending writes for all mounts (30-second timeout per mount), unmounts all FUSE/CfApi sessions, and exits the process with exit code 0

#### Scenario: Headless authentication degradation
- **WHEN** the headless process encounters an expired or revoked refresh token during operation
- **THEN** the system logs a warning "Re-authentication required — cached files remain accessible", keeps all mounts alive in degraded mode (cached reads succeed, uncached reads fail with I/O error, writes buffer locally), and continues running until explicitly terminated

#### Scenario: Headless mode runs as foreground process
- **WHEN** the application starts in headless mode
- **THEN** the process SHALL remain in the foreground (not daemonize), blocking on a signal wait after completing initialization; all log output goes to stderr via the tracing subscriber

#### Scenario: Headless mode on Windows (limitation)
- **WHEN** the application starts in headless mode on Windows
- **THEN** the system logs a warning that Cloud Files API mounts are not supported in headless mode and skips mount startup for affected drives; Windows CfApi headless support is a future enhancement

#### Scenario: Headless via --headless flag
- **WHEN** the application is compiled with the `desktop` feature and started with `--headless`
- **THEN** the system runs in headless mode, bypassing Tauri initialization, using the same headless startup sequence as a non-desktop build
