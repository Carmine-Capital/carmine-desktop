# Coding Conventions

**Analysis Date:** 2026-03-10

## Naming Patterns

**Files:**
- Crate structure: lowercase with hyphens (`cloudmount-vfs`, `cloudmount-cache`)
- Module files: lowercase with underscores (`core_ops.rs`, `open_file_table_tests.rs`)
- Test files: `<module>_tests.rs` or `<module>_test.rs` suffix (e.g., `cache_tests.rs`)
- Helper functions in tests: prefix with `test_` or `setup_` or descriptive verb (`make_client`, `make_cache`, `unique_cache_dir`, `mock_file_download`)

**Functions:**
- Async functions: no special prefix, use `async fn` keyword
- Public functions: `snake_case` (e.g., `get_my_drive`, `list_children`, `download_content`, `open_file`)
- Private/internal: `snake_case` with leading underscore if intentionally hidden (e.g., `_helper_fn`)
- Trait methods: match trait requirement exactly (e.g., `get_json`, `handle_error`, `append_chunk`)
- Constructors: use `new()` for primary, `with_base_url()` for variants with specific parameters
- Boolean predicates: prefix with `is_` or `has_` (e.g., `is_folder()`, `is_locked()`)

**Variables:**
- Local variables: `snake_case` (e.g., `drive_id`, `file_ino`, `cache_dir`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `GRAPH_BASE`, `STREAMING_CHUNK_SIZE`, `MAX_STREAMING_BUFFER_SIZE`)
- Type aliases and type parameters: `PascalCase` (e.g., `TokenFuture`, `OpenFile`)
- Struct fields: `snake_case` (e.g., `parent_reference`, `last_modified`, `download_url`)

**Types:**
- Structs: `PascalCase` (e.g., `DriveItem`, `StreamingBuffer`, `OpenFileTable`, `CacheManager`)
- Enums: `PascalCase` (e.g., `DownloadProgress`, `DownloadState`, `CopyStatus`, `Error`)
- Enum variants: `PascalCase` (e.g., `InProgress(u64)`, `Failed(String)`)
- Traits: `PascalCase` (e.g., `DeltaSyncObserver`)

## Code Style

**Formatting:**
- Tool: `rustfmt` (Rust standard formatter)
- Run via: `make fmt` or `cargo fmt --all`
- CI enforces formatting: `make fmt-check` validates against repository standard

**Linting:**
- Tool: `clippy` with strict warnings-as-errors
- CI command: `RUSTFLAGS=-Dwarnings cargo clippy --all-targets` (both standard and with `--features desktop`)
- All warnings must be fixed or explicitly justified with `#[allow(...)]` comments
- Never suppress lints without written justification in a comment above the suppression

**Indentation:** 4 spaces (rustfmt default)

**Line length:** No hard limit enforced by formatter, but aim for readability (<120 columns typical)

**Braces:** Always on same line as declaration (K&R style, enforced by rustfmt):
```rust
fn example() {
    if condition {
        // code
    }
}
```

## Import Organization

**Order:**
1. Standard library (`std::`, `core::`, etc.)
2. External crates (`tokio::`, `serde::`, `wiremock::`, etc.)
3. Internal workspace crates (`cloudmount_core::`, `cloudmount_cache::`, etc.)
4. Relative imports and module-scoped items (use `crate::` for workspace paths)

**Path Aliases:**
- No path aliases configured in workspace root — use full qualified names
- Crate external paths use full module hierarchy: `use cloudmount_core::types::DriveItem;`
- Relative paths within same crate: `use crate::module::Item;`

**Example from `cloudmount-graph/src/client.rs`:**
```rust
use std::pin::Pin;

use bytes::Bytes;
use cloudmount_core::types::*;
use futures_util::{Stream, StreamExt};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, RANGE};
use reqwest::{Client, StatusCode};

use crate::retry::with_retry;
```

**Glob imports:**
- Used sparingly when importing multiple types from same module (e.g., `use cloudmount_core::types::*;`)
- Prefer explicit imports for clarity unless many items needed from same module

## Error Handling

**Error Type:**
- All fallible operations return `cloudmount_core::Result<T>` (= `std::result::Result<T, cloudmount_core::Error>`)
- Defined in `crates/cloudmount-core/src/error.rs` using `#[derive(thiserror::Error)]`
- Variants: `Auth(String)`, `GraphApi { status, message }`, `Cache(String)`, `Filesystem(String)`, `Config(String)`, `Network(String)`, `PreconditionFailed`, `Locked`, `Io(std::io::Error)`, `Other(anyhow::Error)`

**Propagation:**
- Use `?` operator throughout (requires returning `Result` type)
- Async contexts: `with_retry()` wrapper in `cloudmount-graph` for automatic retries on transient errors
- Convert external error types via `#[from]` attribute on enum variants

**Pattern in tests (integration tests return Result):**
```rust
#[test]
fn test_something() -> cloudmount_core::Result<()> {
    let result = some_fallible_operation()?;
    assert_eq!(result, expected);
    Ok(())
}
```

**VFS platform-specific:**
- FUSE/CfApi trait methods are sync and cannot use `?` directly
- Bridge to async via `rt.block_on()` wrapper
- Return platform error types: `Errno` for FUSE, `HRESULT` for CfApi
- Map `cloudmount_core::Error` to appropriate platform errno: `ENOENT` for missing items, `EIO` for server errors

## Logging

**Framework:** `tracing` (ecosystem: `tracing`, `tracing-subscriber`)

**Configuration:**
- Initialized in `cloudmount-app/src/main.rs` via `tracing_subscriber::fmt().with_env_filter(...).init()`
- Log level controlled by `--log-level` CLI argument or `CLOUDMOUNT_LOG_LEVEL` environment variable
- Default: `info` level
- Subscriber initialized once at application startup

**Patterns:**
- Structured logging via `tracing::info!`, `tracing::debug!`, `tracing::warn!`, `tracing::error!` macros
- Each macro accepts format string + interpolation: `tracing::info!("Action completed: {status}")`
- No explicit module prefixing in messages — `tracing` adds source context automatically
- For detailed debugging: use `tracing::debug!` (only visible at debug log level)

## Comments

**When to Comment:**
- Non-obvious algorithmic choices (e.g., why streaming buffer uses watch channels instead of simpler synchronization)
- Platform-specific conditionals explaining the reason for platform gate
- References to external specifications or bug reports
- Complex calculations or multi-step processes

**When NOT to Comment:**
- Method names that are self-documenting (e.g., `is_locked()`, `mark_done()`, `download_streaming()`)
- Straightforward assignments and control flow
- Loop iterations that mirror their intent

**Example from `crates/cloudmount-vfs/src/core_ops.rs`:**
```rust
/// Compare item names for child lookup.
/// Windows (NTFS/CfApi) uses OrdinalIgnoreCase — ASCII case-insensitive.
/// FUSE on Linux/macOS uses exact (case-sensitive) comparison.
#[cfg(target_os = "windows")]
fn names_match(stored: &str, query: &str) -> bool {
    stored.eq_ignore_ascii_case(query)
}

#[cfg(not(target_os = "windows"))]
fn names_match(stored: &str, query: &str) -> bool {
    stored == query
}
```

## Documentation Comments

**JSDoc/RustDoc:**
- Use `///` for public items (public functions, trait methods, structs, enums)
- Use `//!` for module-level documentation (at top of file)
- Documentation should explain *why*, not just *what*
- Include examples for complex public types/functions

**Pattern:**
```rust
/// Observer for delta sync content change notifications.
///
/// Implemented by the VFS layer to react when delta sync detects that a file's
/// content has changed on the server (eTag mismatch). This enables the VFS to
/// mark open file handles as stale and optionally invalidate the kernel page cache.
///
/// The trait lives in `cloudmount-core` (shared dependency) to avoid a circular
/// dependency between `cloudmount-cache` and `cloudmount-vfs`.
pub trait DeltaSyncObserver: Send + Sync {
    /// Called when delta sync detects that the content of the given inode has changed.
    fn on_inode_content_changed(&self, ino: u64);
}
```

**Module-level documentation:**
```rust
//! Shared VFS operations used by both FUSE (Linux/macOS) and CfApi (Windows) backends.
//!
//! This module contains the core business logic for cache lookups, Graph API interactions,
//! inode management, and write-back operations. Platform-specific backends delegate to
//! [`CoreOps`] instead of duplicating this logic.
```

## Serde & Serialization

**Naming:**
- Use `#[serde(rename = "camelCase")]` to match Microsoft Graph API JSON field names
- Applied to struct fields that map to external API responses (e.g., `last_modified_datetime` → `"lastModifiedDateTime"`)
- Example from `crates/cloudmount-core/src/types.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveItem {
    pub id: String,
    #[serde(rename = "lastModifiedDateTime")]
    pub last_modified: Option<DateTime<Utc>>,
    #[serde(rename = "createdDateTime")]
    pub created: Option<DateTime<Utc>>,
    #[serde(rename = "eTag")]
    pub etag: Option<String>,
}
```

**Defaults:**
- Use `#[serde(default)]` for optional fields that deserialize to empty/default when missing
- Applied to fields like `name` where absence should become empty string rather than None

## Platform-Gating

**Attributes:**
- `#[cfg(any(target_os = "linux", target_os = "macos"))]` — for FUSE-only code (Unix platforms)
- `#[cfg(target_os = "windows")]` — for CfApi-only code
- `#[cfg(target_os = "linux")]` — Linux-specific only
- `#[cfg(feature = "desktop")]` — Tauri/GUI code (conditional build feature)

**Pattern:**
- Platform gates appear on entire function definitions or module sections
- Multiple gates combined: `#[cfg(any(feature = "desktop", not(target_os = "windows")))]`
- Paired with `#[allow(dead_code)]` when code is referenced by tests on all platforms but only used conditionally

## Function Signatures

**Parameter Order:**
1. `&self` or `&mut self` (if method)
2. Required parameters (larger types first for better memory layout)
3. Optional/context parameters (lifetime params, generic bounds)

**Return Types:**
- Fallible operations: `Result<T>` (always `cloudmount_core::Result<T>`)
- Async operations: `async fn` syntax (no explicit future wrapping unless higher-order)
- Trait object lifetimes: use `'static` for Send+Sync bounds (e.g., `Box<dyn Fn() -> TokenFuture + Send + Sync + 'static>`)

**Example from `crates/cloudmount-graph/src/client.rs`:**
```rust
pub fn new<F, Fut>(token_fn: F) -> Self
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = cloudmount_core::Result<String>> + Send + 'static,
{
    Self {
        http: Client::new(),
        base_url: GRAPH_BASE.to_string(),
        token_fn: Box::new(move || Box::pin(token_fn())),
    }
}
```

## Module Design

**Exports:**
- Private by default; explicitly `pub` items form the public API
- Module `lib.rs` / `main.rs` re-exports important types: `pub use module::{Type1, Type2};`
- Example from `crates/cloudmount-graph/src/lib.rs`:
```rust
pub mod client;
pub mod retry;

pub use client::{CopyStatus, GraphClient, SMALL_FILE_LIMIT};
```

**Barrel Files:**
- Re-export pattern: each crate's root exposes key public types
- Consumers import from crate root: `use cloudmount_cache::CacheManager;`
- Internal modules remain private unless explicitly exported

**Visibility Rules:**
- `pub fn` — public function, part of stable API
- `pub(crate) fn` — visible within crate only
- Private (no `pub`) — internal to module or file
- Used throughout to enforce module boundaries

## Async/Await

**Runtime:**
- Tokio throughout the workspace
- Async entry point: `#[tokio::main]` in executable crates
- Async runtime handle available in sync contexts: `tokio::runtime::Handle::current()` for `block_on()`

**Convention:**
- VFS layer (FUSE/CfApi) is sync; bridges to async via `rt.block_on()`
- All I/O and Graph API calls use `async fn`
- Helper spawning: `tokio::task::spawn()` for background tasks, `tokio::task::spawn_blocking()` for CPU-bound work in async contexts

## Workspace Dependencies

**Requirement:**
- ALL external dependencies declared in workspace root `Cargo.toml` under `[workspace.dependencies]`
- Individual crates reference via `{ workspace = true }` (never duplicate version pins)
- Enforces consistent versions across all crates

**Location:** `/var/home/nyxa/Projets/CloudMount/cloud-mount/Cargo.toml`

---

*Convention analysis: 2026-03-10*
