## MODIFIED Requirements

### Requirement: Headless mode operation
The system SHALL support running without the `desktop` feature flag, performing the full mount lifecycle (authentication, mounting, sync, graceful shutdown) as a foreground terminal process without Tauri or any graphical UI. The system SHALL also support running in headless mode with the `desktop` feature when `--headless` is passed.

On Windows, headless mode is not supported because the Cloud Files API requires a desktop session. The `run_headless` function SHALL exit immediately with a clear error message on Windows. The remainder of the function body (runtime creation, authentication, mount iteration, sync loop, signal handling) SHALL NOT compile on Windows — it SHALL be gated with `#[cfg(not(target_os = "windows"))]` to prevent unused-variable and dead-code warnings under `RUSTFLAGS=-Dwarnings`.

#### Scenario: Headless startup with existing tokens
- **WHEN** the application starts in headless mode and valid tokens are found in the credential store
- **THEN** the system restores tokens, runs crash recovery for pending writes, starts all enabled mounts, starts the periodic delta sync loop, and logs "carminedesktop headless mode running — N mount(s) active"

#### Scenario: Headless startup without tokens
- **WHEN** the application starts in headless mode and no valid tokens are found in the credential store
- **THEN** the system attempts browser-based OAuth sign-in by opening the system default browser via the same PKCE flow used in desktop mode; if the browser cannot be opened (no display server), the system prints the auth URL to stdout for manual copy-paste; if sign-in fails after all attempts, the system logs the error and exits with a non-zero exit code

#### Scenario: Headless component initialization
- **WHEN** the application starts in headless mode
- **THEN** it initializes the same components as desktop mode (AuthManager, GraphClient, CacheManager, InodeTable) using the same configuration system (user config → effective config with built-in defaults), with the same CLI/env override chain, without creating any Tauri application context

#### Scenario: Headless graceful shutdown
- **WHEN** the headless process receives SIGTERM or SIGINT (Ctrl+C)
- **THEN** the system cancels the delta sync timer, flushes pending writes for all mounts (30-second timeout per mount), unmounts all FUSE/CfApi sessions, and exits the process with exit code 0

#### Scenario: Headless authentication degradation
- **WHEN** the headless process encounters an expired or revoked refresh token during operation
- **THEN** periodic sync continues running and logs "Re-authentication required — cached files remain accessible" once, further sync attempts skip the re-auth warning, and FUSE mounts remain accessible with cached data

#### Scenario: Windows headless rejection
- **WHEN** the application starts in headless mode on Windows
- **THEN** the system prints "Error: headless mode is not supported on Windows. Cloud Files API requires desktop mode." to stderr and exits with a non-zero exit code, without compiling the Unix mount/sync/signal logic on Windows
