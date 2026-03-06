## ADDED Requirements

### Requirement: Headless mode operation
The system SHALL support running without the `desktop` feature flag, performing the full mount lifecycle (authentication, mounting, sync, graceful shutdown) as a foreground terminal process without Tauri or any graphical UI.

#### Scenario: Headless startup with existing tokens
- **WHEN** the application starts in headless mode and valid tokens are found in the credential store
- **THEN** the system restores tokens, runs crash recovery for pending writes, starts all enabled mounts, starts the periodic delta sync loop, and logs "CloudMount headless mode running — N mount(s) active"

#### Scenario: Headless startup without tokens
- **WHEN** the application starts in headless mode and no valid tokens are found in the credential store
- **THEN** the system attempts browser-based OAuth sign-in by opening the system default browser via the same PKCE flow used in desktop mode; if sign-in succeeds, the system proceeds with mount startup; if sign-in fails (browser unavailable, user cancels, timeout), the system logs the error and exits with a non-zero exit code

#### Scenario: Headless component initialization
- **WHEN** the application starts in headless mode
- **THEN** it initializes the same components as desktop mode (AuthManager, GraphClient, CacheManager, InodeTable) using the same configuration system (packaged defaults + user config → effective config), without creating any Tauri application context

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

## MODIFIED Requirements

### Requirement: Token restoration on startup
The system SHALL attempt to restore authentication tokens from secure storage on startup before requiring user sign-in.

#### Scenario: Valid tokens found in keyring
- **WHEN** the application starts and an account exists in user config with a stored account_id
- **THEN** the system attempts to load tokens from the OS keyring (or encrypted file fallback), and if valid tokens are found, skips the sign-in flow and proceeds directly to mounting drives

#### Scenario: Expired tokens found
- **WHEN** stored tokens are found but the access token is expired
- **THEN** the system attempts a silent token refresh using the stored refresh token, and if successful, proceeds to mounting drives without user interaction

#### Scenario: No tokens found or refresh fails (desktop mode)
- **WHEN** no stored tokens are found or the refresh token has been revoked, and the application is running in desktop mode
- **THEN** the system shows the sign-in wizard and waits for user authentication

#### Scenario: No tokens found or refresh fails (headless mode)
- **WHEN** no stored tokens are found or the refresh token has been revoked, and the application is running in headless mode
- **THEN** the system attempts browser-based OAuth sign-in via the PKCE flow; if the browser cannot be opened or sign-in fails, the system logs a descriptive error and exits with a non-zero exit code
