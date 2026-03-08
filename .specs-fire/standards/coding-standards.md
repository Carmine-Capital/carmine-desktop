# Coding Standards

## Overview

Rust 2024 workspace. Zero warnings policy enforced in CI. Default rustfmt and Clippy settings — no config files. All code must compile clean with `RUSTFLAGS=-Dwarnings` and `cargo clippy --all-targets --all-features`.

## Code Formatting

**Tool**: rustfmt
**Config**: Default (no `rustfmt.toml`)
**Enforcement**: `cargo fmt --all -- --check` in CI

### Key Settings

- **Style**: Default rustfmt (Rust standard style)
- **Line Length**: 100 (rustfmt default)
- **Imports**: Grouped and sorted by rustfmt

## Linting

**Tool**: Clippy
**Base Config**: Default (no `clippy.toml`)
**Strictness**: `RUSTFLAGS=-Dwarnings` — all warnings treated as errors

### Key Rules

- `unused_imports`: error — clean up unused imports
- `unused_variables`: error — use `_` prefix for intentionally unused vars
- `dead_code`: error — remove or gate with `#[cfg]`
- No `#[allow(...)]` without explicit justification comment

## Naming Conventions

### Variables and Functions

| Element | Convention | Example |
|---------|------------|---------|
| Variables | snake_case | `drive_item` |
| Functions | snake_case | `fetch_data` |
| Types / Structs | PascalCase | `DriveItem` |
| Enums | PascalCase | `VfsError` |
| Enum variants | PascalCase | `NotFound` |
| Traits | PascalCase | `SyncFilter` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_RETRIES` |
| Modules | snake_case | `core_ops` |
| Lifetimes | short lowercase | `'a` |

### Files and Folders

- **Source files**: snake_case (e.g., `core_ops.rs`, `fuse_fs.rs`)
- **Crates**: kebab-case (e.g., `cloudmount-cache`)
- **Test files**: in `crates/<name>/tests/` directory (integration convention)

## File Organization

### Project Structure

```
crates/
├── cloudmount-app/         # Tauri entry point — runtime orchestration
├── cloudmount-auth/        # OAuth2 PKCE + token storage
├── cloudmount-cache/       # Multi-tier cache (memory → SQLite → disk)
├── cloudmount-core/        # Shared types, errors, config
├── cloudmount-graph/       # Microsoft Graph API client
└── cloudmount-vfs/         # VFS: FUSE (Linux/macOS), CfApi (Windows)
```

### Conventions

- **Errors**: Defined in `cloudmount-core::Error` via `thiserror`. Propagate with `cloudmount_core::Result<T>`. Use `anyhow` only for `Other` variant.
- **Platform gates**: `#[cfg(any(target_os = "linux", target_os = "macos"))]` for FUSE, `#[cfg(target_os = "windows")]` for CfApi, `#[cfg(feature = "desktop")]` for Tauri UI
- **Async boundary**: Tokio throughout. VFS uses `rt.block_on()` because FUSE/CfApi trait methods are sync.
- **Serde**: `#[serde(rename = "camelCase")]` to match Microsoft Graph API JSON field names
- **No inline test modules**: Integration tests live in `crates/<name>/tests/`, not `#[cfg(test)]` inline

## Import Order

```rust
// 1. Standard library
use std::collections::HashMap;

// 2. External crates
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

// 3. Internal workspace crates
use cloudmount_core::{DriveItem, Result};

// 4. Current crate modules
use crate::cache::MemoryCache;
```

**Rules**:
- rustfmt groups and sorts imports automatically — do not override
- Prefer `use` over `extern crate`
- Glob imports (`use foo::*`) only for test prelude patterns

## Error Handling

### Pattern

**Approach**: `thiserror` enum in `cloudmount-core::Error`. Propagate via `?`. Log at the boundary where context is available, not where errors are created.

### Guidelines

- Add error variants to `cloudmount_core::Error` enum
- Use `#[from]` for automatic conversions from external error types
- Use `#[error("...")]` with descriptive messages
- Avoid `.unwrap()` and `.expect()` in library code — use `?` or explicit handling
- In tests: `?` propagation via `-> cloudmount_core::Result<()>` return type

### Example

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("item not found: {0}")]
    NotFound(String),

    #[error("Graph API error: {0}")]
    Graph(#[from] GraphError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
```

## Logging

**Tool**: `tracing` crate
**Format**: Structured fields (`tracing::info!(field = value, "message")`)

### Log Levels

| Level | Usage |
|-------|-------|
| `error!` | Unrecoverable failures, data loss risk |
| `warn!` | Recoverable issues, degraded behavior |
| `info!` | Key lifecycle events (mount, unmount, auth) |
| `debug!` | Detailed flow for debugging |
| `trace!` | Very verbose, per-operation detail |

### Guidelines

**Always log**:
- Mount/unmount lifecycle events
- Auth state changes (sign-in, sign-out, token refresh)
- Errors before returning them to callers
- Significant cache misses / network calls

**Never log**:
- OAuth2 tokens or credentials
- User file content
- Passwords or secrets

## Comments and Documentation

### When to Comment

- Non-obvious algorithmic choices (e.g., why a specific timeout value)
- Platform-specific workarounds with reference to the bug/issue
- Safety invariants for `unsafe` blocks
- Do NOT comment obvious code — code should be self-documenting

### Documentation Format

**Functions**: `///` doc comments for public API functions
**Structs/Enums**: `///` doc comments on the type and each public field
**Private functions**: Comments only when logic is non-obvious

---
*Generated by specs.md - fabriqa.ai FIRE Flow*
