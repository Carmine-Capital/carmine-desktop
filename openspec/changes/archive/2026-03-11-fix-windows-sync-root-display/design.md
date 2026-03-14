## Context

The Windows Cloud Files API requires each sync root to be registered with a `SyncRootInfo` before File Explorer will display it. That registration includes a display name (shown in the navigation pane) and an icon path (displayed next to it).

Currently `register_sync_root()` in `crates/carminedesktop-vfs/src/cfapi.rs` hardcodes both:

- **Display name**: `PROVIDER_NAME` = `"carminedesktop"` for every mount, regardless of which library it represents.
- **Icon**: `%SystemRoot%\system32\imageres.dll,0` — index 0 in imageres.dll, which is a generic document icon that renders as a black square in the navigation pane on modern Windows.

The `account_name` (derived from `mount_config.name`) already flows into `CfMountHandle::mount()` but is only forwarded to `build_sync_root_id()` for uniqueness; it is never passed to `register_sync_root()`, so the display name is always "carminedesktop".

A second compounding issue: the code only calls `register_sync_root()` when `is_registered` is false. Mounts already registered under the old buggy state are never corrected.

## Goals / Non-Goals

**Goals:**
- Each mount's File Explorer label shows its user-visible name (e.g., "Adelya", "Alpha Nova (Apollo Sparks)").
- Each mount's File Explorer icon shows the carminedesktop application icon.
- Already-registered sync roots (from prior buggy launches) are corrected on the next mount.

**Non-Goals:**
- No changes to the sync root ID format or uniqueness scheme.
- No changes to FUSE (Linux/macOS) mount behaviour.
- No per-mount custom icons (all mounts share the same app icon).

## Decisions

### 1. Thread `display_name` through the call stack

`register_sync_root()` gains a `display_name: &str` parameter. `CfMountHandle::mount()` gains a matching `display_name: String` parameter (separate from `account_name`). The caller in `main.rs` passes `mount_config.name` as `display_name` and continues to pass the `!`-sanitized string as `account_name`.

**Why separate from `account_name`?** `account_name` has `!` replaced with `_` to satisfy the SyncRootId three-component format. `display_name` should be the raw human-readable name so Explorer shows e.g. "Alpha Nova" rather than "Alpha Nova" (no difference here, but the semantics are distinct and the sanitization must not be applied to display strings).

**Alternative considered**: derive display_name from account_name inside `register_sync_root`. Rejected — it would tie display to an internal detail and make the function harder to test in isolation.

### 2. Always re-register (remove `is_registered` guard)

The conditional `if !is_registered { register_sync_root(...) }` is replaced with an unconditional call. The Windows `CfRegisterSyncProvider` API is idempotent when called with `CF_REGISTER_FLAG_UPDATE`; the `cloud-filter` crate's `SyncRootId::register()` wraps this and will overwrite an existing registration with the new `SyncRootInfo`.

**Why unconditional?** Users who already have broken "carminedesktop" registrations from prior versions must have them corrected without needing to fully unmount and remount. Unconditional re-registration is the only way to achieve this without a migration step.

**Alternative considered**: Version-stamp the registration and re-register only when the version changes. Rejected — unnecessary complexity for a one-time correction.

### 3. Icon path resolved from the running executable

The icon path is computed as `format!("{},0", current_exe_path)` where `current_exe_path` comes from `std::env::current_exe()`. Tauri embeds `icon.ico` into the Windows `.exe` at build time, so index 0 of the executable is the carminedesktop icon.

If `current_exe()` fails (unusual but possible in sandboxed environments), the code falls back to `%SystemRoot%\\system32\\shell32.dll,43` — the generic cloud folder icon in shell32 — rather than `imageres.dll,0` which renders incorrectly.

**Why the exe and not a separate `.ico` resource?** Pointing at the exe avoids having to ship or locate a separate icon file. The Tauri-generated installer already embeds the icon at index 0. The path is stable for the lifetime of the installation.

**Alternative considered**: Hardcode an absolute path to a bundled `.ico` file. Rejected — the bundle layout is installer-dependent and the path would need to be resolved from Tauri's resource directory, adding unnecessary complexity for what is ultimately a cosmetic feature.

## Risks / Trade-offs

- **Re-registration on every mount adds a small overhead** — `CfRegisterSyncProvider` is a registry write. Negligible for mounts that happen once at startup.
- **Icon missing in portable/uninstalled builds** — If the exe is run directly from the build output directory without Tauri packaging, the exe may not have an embedded icon. The shell32 fallback ensures Explorer still shows a recognisable cloud icon in this case.
- **Display name not validated for length** — Windows limits sync root display names; very long mount names could be truncated. Mount names in practice are short (user-defined, typically < 50 chars), so this is acceptable without additional validation.

## Migration Plan

No migration step required. Fixes take effect on the next app launch: all mounted drives are registered (or re-registered) with the corrected display name and icon. Users already running the app will see the correct names as soon as they restart carminedesktop.
