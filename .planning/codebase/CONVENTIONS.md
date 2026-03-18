# Coding Conventions

**Analysis Date:** 2026-03-18

## Naming Patterns

**Files:**
- Use `snake_case.rs` for all Rust source files: `core_ops.rs`, `sync_processor.rs`, `pin_store.rs`
- Module files match their `pub mod` declaration in `lib.rs`
- Test files in `crates/<name>/tests/` use descriptive snake_case: `cache_tests.rs`, `auth_integration.rs`, `fuse_integration.rs`
- Frontend files use `snake_case.js`: `ui.js`, `settings.js`, `wizard.js`

**Functions:**
- Use `snake_case` for all Rust functions: `list_children()`, `upload_small()`, `get_delta_token()`
- Async functions use descriptive verbs: `download_content()`, `create_folder()`, `poll_copy_status()`
- Boolean-returning functions use `is_`/`has_` prefix: `is_folder()`, `is_locked()`, `has_pending()`, `is_pinned()`
- Test helper functions use `make_`/`test_` prefix: `make_client()`, `make_cache()`, `test_drive_item()`, `test_graph()`

**Variables:**
- Use `snake_case` for all variables and parameters
- Constants use `SCREAMING_SNAKE_CASE`: `GRAPH_BASE`, `MAX_RETRIES`, `BASE_DELAY_MS`, `UPLOAD_CHUNK_SIZE`, `SMALL_FILE_LIMIT`
- Drive and item IDs in tests use kebab-case strings: `"drive-123"`, `"file-1"`, `"root-id"`

**Types:**
- Structs and enums use `PascalCase`: `GraphClient`, `CacheManager`, `DriveItem`, `MountConfig`
- Error enum variants are domain-scoped nouns: `Auth(String)`, `GraphApi { status, message }`, `Cache(String)`, `Network(String)`
- Type aliases: `pub type Result<T> = std::result::Result<T, Error>`

**Crate Naming:**
- All crates use `carminedesktop-` prefix in Cargo.toml `name`: `carminedesktop-core`, `carminedesktop-graph`
- In Rust code, use underscores: `carminedesktop_core`, `carminedesktop_graph`

## Code Style

**Formatting:**
- `cargo fmt` with default `rustfmt` settings (no `.rustfmt.toml` or `rustfmt.toml` exists)
- Run via `make fmt` (or `make fmt-check` for CI mode)
- No custom formatting overrides

**Linting:**
- Clippy with `RUSTFLAGS=-Dwarnings` (zero warnings policy)
- CI runs clippy twice: once without features, once with `--features desktop`
- Run via `make clippy`
- **Collapse nested `if` blocks:** Use `if cond && let Err(e) = f() { ... }` instead of nested `if cond { if let Err(e) = f() { ... } }`
- No `.clippy.toml` configuration file

**CI Enforcement (`crates/carminedesktop-app/.github/workflows/ci.yml`):**
- Runs on all three platforms: `ubuntu-latest`, `macos-latest`, `windows-latest`
- Check order: `fmt --check` → `clippy` → `build` → `test`
- All cargo commands in local dev run inside `carminedesktop-build` toolbox container (`Makefile`)

## Import Organization

**Order:**
1. Standard library (`std::*`)
2. External crates (`serde`, `tokio`, `reqwest`, `chrono`, etc.)
3. Workspace crates (`carminedesktop_core`, `carminedesktop_graph`, etc.)
4. Crate-local modules (`crate::*`, `super::*`)

**Path Aliases:**
- No path aliases configured. Use full crate paths: `carminedesktop_core::types::DriveItem`
- Glob imports for types: `use carminedesktop_core::types::*;` (in `crates/carminedesktop-graph/src/client.rs`)
- Re-exports in `lib.rs` for public API: `pub use error::{Error, Result}`, `pub use client::GraphClient`

**Dependency Management:**
- ALL dependencies declared in workspace root `Cargo.toml` under `[workspace.dependencies]`
- Crate `Cargo.toml` files reference `{ workspace = true }` only
- Never add a dependency directly to a crate's `[dependencies]` section without first adding it to workspace root

## Error Handling

**Central Error Type (`crates/carminedesktop-core/src/error.rs`):**
```rust
#[derive(Debug, Error)]
pub enum Error {
    #[error("authentication failed: {0}")]
    Auth(String),
    #[error("Graph API error: {status} {message}")]
    GraphApi { status: u16, message: String },
    #[error("cache error: {0}")]
    Cache(String),
    #[error("filesystem error: {0}")]
    Filesystem(String),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("precondition failed: server content changed (412)")]
    PreconditionFailed,
    #[error("resource locked (423)")]
    Locked,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
```

**Patterns:**
- Use `thiserror` for the central `Error` enum in `carminedesktop-core`
- Use `anyhow` only for the `Other` variant catch-all
- Propagate errors via `carminedesktop_core::Result<T>` and the `?` operator
- Map external errors to domain-specific variants:
  - `reqwest` errors → `Error::Network(e.to_string())`
  - HTTP 412 → `Error::PreconditionFailed`
  - HTTP 423 → `Error::Locked`
  - HTTP 4xx/5xx → `Error::GraphApi { status, message }`
  - `rusqlite` errors → `Error::Cache(String)`
  - `toml` parse errors → `Error::Config(String)`
- Format error messages as lowercase with context: `"failed to parse user config: {e}"`, `"upload response parse failed: {e}"`
- In Tauri commands, map `Error` to `String` via `.map_err(|e| e.to_string())`

**Retry Logic (`crates/carminedesktop-graph/src/retry.rs`):**
- Retry on `Error::Network(_)` and `Error::GraphApi` with status 429 or >= 500
- 3 max retries with exponential backoff (1s, 2s, 4s) plus jitter
- Non-retryable errors propagated immediately (4xx except 429)

## Logging

**Framework:** `tracing` crate with `tracing-subscriber` (env-filter + fmt features)

**Patterns:**
- Use structured tracing macros: `tracing::warn!`, `tracing::info!`, `tracing::debug!`
- Use structured fields: `tracing::warn!(attempt, delay_ms = delay + jitter, "retrying after transient error: {e}")`
- **Never log token values** — `tracing::warn!` for errors only in auth code
- Log level configuration via `log_level` setting in `config.toml` (default: `"info"`)
- File-based logging via `tracing-appender`

## Comments

**When to Comment:**
- Doc comments (`///`) for all public functions, structs, and traits
- Module-level `//!` comments for test files explaining prerequisites: `//! FUSE integration tests — mount a real filesystem backed by wiremock.`
- Inline comments for non-obvious logic: platform-specific behavior, concurrency safety notes, format specifications
- Section separators in test files use block comments: `// ============================================================================`

**Doc Comments:**
- Use `///` for public API documentation
- Include examples of format/encoding: `/// Storage format: [16-byte salt][12-byte nonce][ciphertext]`
- Document why a trait exists in a specific crate to explain architecture decisions

## Serde Conventions

**Field Renaming:**
- Use `#[serde(rename = "camelCase")]` on individual fields to match Microsoft Graph API JSON:
  ```rust
  #[serde(rename = "lastModifiedDateTime")]
  pub last_modified: Option<DateTime<Utc>>,
  #[serde(rename = "@microsoft.graph.downloadUrl")]
  pub download_url: Option<String>,
  ```
- Use `#[serde(default)]` for optional fields that may be absent from API responses
- Use `#[serde(rename = "type")]` for reserved keywords: `pub mount_type: String`

## Platform Gating

**Patterns:**
- `#[cfg(any(target_os = "linux", target_os = "macos"))]` for FUSE code
- `#[cfg(target_os = "windows")]` for WinFsp code
- `#[cfg(feature = "desktop")]` for Tauri UI code (the `desktop` feature gate)
- `#[cfg(unix)]` / `#[cfg(windows)]` for OS-level helpers (mount point validation, etc.)
- Platform-specific defaults use paired cfg blocks:
  ```rust
  #[cfg(target_os = "windows")]
  let default_nav_pane = true;
  #[cfg(not(target_os = "windows"))]
  let default_nav_pane = false;
  ```

## Async Patterns

**Runtime:** Tokio with `features = ["full"]`

**Async/Sync Bridge:**
- VFS filesystem trait methods (FUSE `Filesystem`, WinFsp `FileSystemContext`) are sync
- Bridge to async via `rt.block_on()` where `rt` is a stored `tokio::runtime::Handle`
- **Never hold cache locks across `block_on` calls** — deadlock risk
- Use `tokio::task::spawn_blocking` in tests when testing sync code that calls `rt.block_on()` internally

**Concurrency:**
- `DashMap` for concurrent in-memory cache access
- `Mutex` (not `RwLock`) for SQLite connection — all ops take `&self`
- `RwLock` for `AuthState` — read lock for token check, write lock for refresh/exchange
- `AtomicBool` for flags: `authenticated`, `offline`, `auth_degraded`
- `Arc` wrapping for shared state across tasks

## Module Design

**Exports:**
- Each crate's `lib.rs` declares `pub mod` for all modules
- Key types re-exported at crate root: `pub use manager::CacheManager;`, `pub use client::GraphClient;`
- Internal modules use `pub(crate)`: `pub(crate) mod pending;`

**Barrel Files:**
- `lib.rs` serves as the barrel file for each crate
- Re-export only the public API, keep implementation details module-private

## Frontend (Vanilla JS)

**Location:** `crates/carminedesktop-app/dist/`

**Patterns:**
- No build step — vanilla JavaScript
- Tauri IPC via `window.__TAURI__.core.invoke()`
- Use `addEventListener` in `.js` files — **never use inline event handlers** (`onclick="..."`) due to CSP `script-src 'self'`
- All user-facing actions provide feedback via `showStatus()` in `ui.js`
- Never let a mutating operation complete silently

## Function Design

**Size:** Functions are typically 10-50 lines. Large functions (like `flush_inode` in VFS) contain essential sequential logic.

**Parameters:**
- Use `&str` for string parameters, `&[u8]` for byte slices
- Use `Option<&str>` for optional parameters (e.g., `if_match: Option<&str>` for ETags)
- Builder-style configuration: `CoreOps::new(...).with_sync_handle(handle).with_offline_flag(flag)`

**Return Values:**
- Use `carminedesktop_core::Result<T>` for fallible operations
- Use `Option<T>` for cache lookups that may miss
- Return owned types from cache lookups: `Option<DriveItem>` (clone from DashMap)

---

*Convention analysis: 2026-03-18*
