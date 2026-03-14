## Why

A cross-platform review of `carminedesktop-vfs` and `carminedesktop-app` (verified against the working tree with `fix-cfapi-safety-parity` and `fix-vfs-residual-parity-gaps` applied) revealed six remaining defects that cause Windows CI failures, unnecessary Graph API traffic, silent data-race potential, and panic/debuggability issues across platforms. None are covered by existing in-progress changes.

## What Changes

- **InodeTable `allocate()` TOCTOU race** — the read-lock/write-lock split lets two concurrent calls allocate distinct inodes for the same `item_id`, creating ghost entries. Merge both maps under a single `RwLock` so lookup-or-insert is atomic.
- **CfApi `closed()` fires on every file close** — `cloud-filter-0.0.6` does not expose `CF_CALLBACK_CLOSE_COMPLETION_FLAG_MODIFIED`, so read-only opens trigger a full disk-read + Graph API upload cycle. Add a timestamp-based guard comparing file mtime against the last-synced value to skip unmodified files.
- **CfMountHandle `_connection` field naming** — the `_` prefix conventionally means "unused" but the field's drop order is safety-critical. Rename to `connection` and document the required drop sequence.
- **`run_headless` unconditional bindings on Windows** — `mounts_config`, `mount_entries`, and `mount_count` compile on Windows but are dead after the early `process::exit(1)`. Gate the entire post-exit body with `#[cfg(not(target_os = "windows"))]` to eliminate CI warnings.
- **`commands.rs` path separator** — `format!("~/{}/", config.root_dir)` hard-codes `/`; on Windows the returned path contains a mixed separator. Use `PathBuf::join` instead.
- **Minor robustness fixes** — `notify.rs` swallows the error object before logging it; `tray.rs` panics via `.unwrap()` on a missing icon in dev builds.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `virtual-filesystem`: InodeTable atomicity guarantee; CfApi `closed()` skips unmodified files; `_connection` drop-order correctness.
- `app-lifecycle`: `run_headless` Windows dead-code elimination; path construction fix in `commands.rs`.
- `ui-feedback`: `notify.rs` logs the actual error reason on notification failure.
- `tray-app`: icon loading returns an error instead of panicking.

## Impact

- **`crates/carminedesktop-vfs/src/inode.rs`** — `allocate()` rewritten with single-lock pattern.
- **`crates/carminedesktop-vfs/src/cfapi.rs`** — `closed()` gains mtime guard; `CfMountHandle._connection` renamed.
- **`crates/carminedesktop-app/src/main.rs`** — `run_headless` body restructured under `#[cfg(not(target_os = "windows"))]`.
- **`crates/carminedesktop-app/src/commands.rs`** — `get_default_mount_root` uses `PathBuf`.
- **`crates/carminedesktop-app/src/notify.rs`** — error included in warning log.
- **`crates/carminedesktop-app/src/tray.rs`** — `.unwrap()` replaced with `?` propagation.
