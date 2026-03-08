# Implementation Plan — run-cloud-mount-008

**Intent:** fix-cross-platform-findings
**Scope:** wide
**Mode:** confirm

---

## Work Item 1: fix-mount-path-separator

### Approach
Replace all hardcoded `/` separators in path-building code with `PathBuf::join()`,
so path construction is OS-native on Windows, macOS, and Linux.

### Files to Modify
- `crates/cloudmount-core/src/config.rs`
  - `derive_mount_point()`: use `PathBuf` from `dirs::home_dir()` + `Path::join()` chain
  - `expand_mount_point()`: use `PathBuf` + `Path::join()` for `~/...` expansion
- `crates/cloudmount-app/src/main.rs`
  - `start_mount()` Windows branch (~line 784): change `std::path::Path::new(&mountpoint)` to `&std::path::PathBuf::from(&mountpoint)` for explicit normalisation

### Tests
- `cargo test -p cloudmount-core`
- `cargo clippy --all-targets --all-features` (zero warnings)

---

## Work Item 2: fix-windows-headless-mounts

### Approach
Replace the single generic `tracing::warn!` in the Windows headless branch with two
explicit per-feature warnings (crash recovery skipped, delta sync skipped), making
the degradation visible and diagnosable in logs.

### Files to Modify
- `crates/cloudmount-app/src/main.rs`
  - `run_headless()` Windows branch (~line 1235): replace one-liner warn with two targeted warns

### Tests
- `cargo test -p cloudmount-app`
- `cargo clippy --all-targets --all-features` (zero warnings)
