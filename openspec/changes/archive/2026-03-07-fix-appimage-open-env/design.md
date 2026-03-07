## Context

CloudMount on Linux is distributed as an AppImage. The AppImage runtime sets two environment variables before launching the main binary:

- `LD_LIBRARY_PATH` — points to bundled `.so` files inside the AppImage mount
- `LD_PRELOAD` — may inject AppImage-bundled hooks

Any child process spawned without explicitly clearing these inherits them. `xdg-open`, the file manager it launches (Dolphin), and any application launched _from_ that file manager all carry the contaminated environment. Applications with their own bundled libs (LibreOffice, Chrome, etc.) load the AppImage's GLib/glibc instead of their own, hit an ABI mismatch, and crash silently.

The desktop OAuth URL opener (`main.rs:403-421`) already strips these vars via `Command::new("xdg-open").env_remove("LD_LIBRARY_PATH").env_remove("LD_PRELOAD")`. Two call sites were missed:

| Call site | Purpose | Bug |
|-----------|---------|-----|
| `tray.rs:72` | Open mount folder in file manager | `open::that()` — no scrubbing |
| `main.rs:942` | Open browser for OAuth (headless mode) | `open::that()` — no scrubbing |

## Goals / Non-Goals

**Goals:**
- Eliminate env var inheritance for all "open something with the OS" call sites in cloudmount-app
- Single definition of the Linux-specific scrubbing logic (DRY)
- No regression on macOS or Windows

**Non-Goals:**
- Fixing env contamination in child processes launched by other crates (auth, vfs, etc. don't spawn user-facing processes)
- Supporting platforms beyond Linux/macOS/Windows
- Changing the `OpenerFn` type or its contract with AuthManager

## Decisions

### D1 — Extract `pub(crate) fn open_with_clean_env`

Extract the xdg-open + env-scrubbing pattern into a single `pub(crate)` function in `main.rs`. Platform-gated:

```
Linux:      Command::new("xdg-open").arg(path)
              .env_remove("LD_LIBRARY_PATH")
              .env_remove("LD_PRELOAD")
              .status()
              → Ok(()) on success, Err(String) on failure

macOS/Win:  open::that(path).map_err(|e| e.to_string())
```

Returns `Result<(), String>` to match the existing `OpenerFn` contract, making it usable by all three callers.

**Alternatives considered:**

- **Inline the fix only in `tray.rs:72`** — fixes the reported bug but leaves `main.rs:942` broken and creates a third copy of the pattern. Rejected.
- **Add `opener: OpenerFn` to `AppState`** — would let `tray.rs` reuse the auth opener. Rejected: the auth opener is an auth concern; the tray opener is unrelated. AppState should stay lean.
- **New `platform.rs` module** — appropriate if other platform helpers accumulate. For now one function doesn't warrant a new file; `main.rs` is the right home since `OpenerFn` is already defined there.

### D2 — Use `.status()` (blocking) not `.spawn()` (fire-and-forget) in the helper

The existing desktop opener uses `.status()` for error observability. On KDE, `xdg-open` for a directory sends a D-Bus message to Dolphin and exits in milliseconds — blocking is acceptable. Using the same approach for all callers keeps the helper simple.

The tray menu event handler is a synchronous callback, but a sub-millisecond block is harmless. If this becomes an issue, the caller can trivially wrap the call in `std::thread::spawn`.

### D3 — Unify the desktop OpenerFn lambda with the new helper

The lambda at `main.rs:409-421` duplicates the pattern. After extracting `open_with_clean_env`, the lambda body becomes a single call to it. This is the canonical pattern going forward.

## Risks / Trade-offs

- **`.status()` blocks the tray event callback briefly** → Dolphin/xdg-open exits in <10ms in practice; acceptable. If measured to be a problem, wrap in `std::thread::spawn` at the call site.
- **`open::that()` on non-Linux platforms may have its own issues** → out of scope; existing behavior unchanged on macOS/Windows.
- **Future call sites could regress** → mitigated by the single helper being the obvious entry point for "open path externally".

## Migration Plan

1. Add `open_with_clean_env` to `main.rs` (platform-gated)
2. Update desktop opener lambda (`main.rs:409-421`) to call the helper
3. Update headless opener (`main.rs:942`) to call the helper
4. Update `tray.rs:72` to call `crate::open_with_clean_env`
5. Verify `cargo clippy --all-targets --all-features` passes (zero warnings)
6. Manual smoke test: launch AppImage, click tray mount item, open a file from Dolphin

No rollback complexity — changes are localized to two files, no schema/config/API changes.
