## Context

The repository lives at `CloudMount/` but every internal identifier — crate names, module paths, config directories, service files, keyring entries, database filenames, Tauri metadata, CI scripts, and documentation — still uses the old "FileSync" / "filesync" naming. There are ~345 occurrences across 27 Rust source files, 30 occurrences in 8 Cargo.toml files, 4 in CI workflows, 2 in HTML templates, 2 in Tauri config, and ~144 across 27 Markdown files.

The application has not shipped a public release, so there are no external consumers to migrate. A handful of internal/dev users may have `~/.config/filesync/` directories and keyring entries, but no migration tooling is warranted at this stage.

## Goals / Non-Goals

**Goals:**
- Rename all 6 crate packages from `filesync-*` to `cloudmount-*`
- Rename all 6 crate directories from `crates/filesync-*` to `crates/cloudmount-*`
- Update all Rust module paths (`filesync_*` → `cloudmount_*`)
- Update all user-facing strings, config paths, and service names from `filesync`/`FileSync` to `cloudmount`/`CloudMount`
- Update Tauri product name and app identifier
- Update CI/CD workflows
- Update documentation (README, guides, specs)
- Update AGENTS.md knowledge base files
- Ensure `cargo build`, `cargo test`, `cargo clippy`, and `cargo fmt` pass cleanly after the rename

**Non-Goals:**
- Automated migration of existing user config directories (`~/.config/filesync/` → `~/.config/cloudmount/`) — pre-v1, not needed
- Automated migration of existing keyring entries — users re-authenticate
- Renaming the `openspec/changes/archive/` contents — historical records stay as-is
- Changing the Git repository name or remote URL — already named CloudMount
- Changing any Microsoft Graph API behavior, OAuth scopes, or functional logic

## Decisions

### D1: Rename scheme — consistent prefix swap

**Decision**: Apply a mechanical prefix swap across all naming variants.

| Old | New | Where |
|-----|-----|-------|
| `filesync-*` | `cloudmount-*` | Crate names, Cargo.toml, directory names |
| `filesync_*` | `cloudmount_*` | Rust module paths (`use` statements, types) |
| `"filesync"` | `"cloudmount"` | Keyring service name, config dir joins, FSName, DB name |
| `"FileSync"` | `"CloudMount"` | DEFAULT_APP_NAME, PROVIDER_NAME, display strings, docs |
| `"filesync-tray"` | `"cloudmount-tray"` | Tray icon ID |
| `com.filesync.*` | `com.cloudmount.*` | Tauri identifier, macOS LaunchAgent, macOS plist |
| `filesync.service` | `cloudmount.service` | systemd unit file |
| `filesync.db` | `cloudmount.db` | SQLite database filename |
| `FileSyncFs` | `CloudMountFs` | FUSE filesystem struct |
| `FileSyncCfFilter` | `CloudMountCfFilter` | CfApi filter struct |

**Rationale**: A mechanical swap is auditable, scriptable, and minimizes human error. Every occurrence follows one of the patterns above.

### D2: Execution order — directories first, then Cargo.toml, then source

**Decision**: Rename in this order:
1. `git mv` crate directories (`crates/filesync-*` → `crates/cloudmount-*`)
2. Update root `Cargo.toml` workspace members and dependency paths
3. Update individual crate `Cargo.toml` package names and dependency references
4. Update all `.rs` source files (module paths, constants, string literals, struct names)
5. Update Tauri config, HTML templates, CI workflows
6. Update documentation and specs
7. Regenerate AGENTS.md files
8. Run `cargo build --all-targets && cargo test --all-targets && cargo clippy --all-targets --all-features && cargo fmt --all -- --check`

**Rationale**: Cargo needs valid paths to resolve the workspace. Renaming directories and Cargo.toml first ensures the build system is consistent before touching source code. Doing it in this order means each intermediate step can be validated.

### D3: No backward-compatibility shim

**Decision**: Do not provide `pub use` re-exports or type aliases from old names to new names.

**Alternatives considered**:
- Add `pub use cloudmount_core as filesync_core;` in a compatibility crate → unnecessary complexity for pre-v1 software with no published crates on crates.io

**Rationale**: There are no external consumers of these crates. A clean break is simpler and avoids lingering references.

### D4: Historical archives untouched

**Decision**: Do not rename references inside `openspec/changes/archive/`. These are historical records of past changes.

**Rationale**: Archives document what was decided at the time. Changing them retroactively would misrepresent history and provide no functional benefit.

### D5: Auth storage password derivation keeps structure, changes prefix

**Decision**: The encrypted token fallback password changes from `"filesync-fallback-{user}@{home}"` to `"cloudmount-fallback-{user}@{home}"`.

**Rationale**: This is a machine-specific derivation string, not a user-visible password. Changing it is consistent with the rename. Existing `.enc` files from dev use will become unreadable — users simply re-authenticate, which is acceptable pre-v1.

## Risks / Trade-offs

- **[Risk] Missed occurrence** → Mitigation: Use `cargo build` + `cargo test` + `cargo clippy` as the final gate. Any missed `filesync_*` module path will be a compile error. String-level misses (e.g., a leftover `"filesync"` in a doc comment) are cosmetic and caught by post-rename grep sweep.
- **[Risk] Git blame disrupted for renamed files** → Mitigation: Git `--follow` tracks renames. Using `git mv` preserves rename detection. The alternative (copy+delete) would be worse.
- **[Risk] Existing dev config/tokens orphaned** → Mitigation: Acceptable pre-v1. Document in commit message that devs should `rm -rf ~/.config/filesync ~/.cache/filesync` and re-authenticate.
- **[Risk] Cargo incremental build cache invalidated** → Mitigation: Expected — `cargo clean` after rename. One-time cost.
