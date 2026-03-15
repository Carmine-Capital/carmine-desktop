# Explorer Nav Pane Performance Fixes Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three remaining performance issues that cause Windows Explorer to open slowly when the Carmine Desktop navigation pane entry is registered.

**Architecture:** Three independent, small fixes in `shell_integration.rs` (attributes + conditional notify) and `main.rs` (execution order). Each fix is self-contained and can be committed separately.

**Tech Stack:** Rust, `winreg` crate, `windows` crate (`SHChangeNotify`), Windows Shell SFGAO flags.

---

## File Map

| File | Action | Responsibility |
|------|--------|---------------|
| `crates/carminedesktop-app/src/shell_integration.rs` | Modify | Fix 1: Change `ShellFolder\Attributes`. Fix 2: Add `ensure_nav_pane` to skip `SHChangeNotify` when registry is already correct. |
| `crates/carminedesktop-app/src/main.rs` | Modify | Fix 2: Call `ensure_nav_pane` instead of `register_nav_pane`. Fix 3: Move nav pane reconciliation after `start_all_mounts`. |

---

## Chunk 1: All Three Fixes

### Task 1: Add `SFGAO_ISSLOW` and remove `SFGAO_HASSUBFOLDER` from `ShellFolder\Attributes`

**Files:**
- Modify: `crates/carminedesktop-app/src/shell_integration.rs:471` (Attributes value)
- Modify: `crates/carminedesktop-app/src/shell_integration.rs:1511` (existing test assertion)

**Context:** The current `Attributes` value `0xF080004D` includes `SFGAO_HASSUBFOLDER` (`0x80000000`) which forces Explorer to enumerate the delegate folder's children eagerly (triggering Graph API calls via WinFsp). It also lacks `SFGAO_ISSLOW` (`0x00004000`) which would tell Explorer this is slow storage.

New value: `0x7080404D` = remove `0x80000000`, add `0x00004000`.

- [ ] **Step 1: Update `ShellFolder\Attributes` in `register_nav_pane`**

In `shell_integration.rs`, change line 471:

```rust
// Before:
shell_folder_key.set_value("Attributes", &0xF080004Du32)?;

// After:
shell_folder_key.set_value("Attributes", &0x7080404Du32)?;
```

- [ ] **Step 2: Update test assertion to match new value**

In the test `test_shell_integration_register_and_unregister_nav_pane`, add a verification for the new Attributes value. After the existing `HideDesktopIcons` verification block (around line 1556), add:

```rust
// Verify ShellFolder attributes include SFGAO_ISSLOW and exclude SFGAO_HASSUBFOLDER
let shell_folder = clsid_key.open_subkey_with_flags("ShellFolder", KEY_READ)?;
let attrs: u32 = shell_folder.get_value("Attributes")?;
assert_eq!(attrs, 0x7080404D);
```

- [ ] **Step 3: Run clippy to verify no warnings**

Run: `make clippy`
Expected: no warnings (CI enforces zero warnings)

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-app/src/shell_integration.rs
git commit -m "perf: use SFGAO_ISSLOW and drop SFGAO_HASSUBFOLDER in nav pane attributes

Removes SFGAO_HASSUBFOLDER (0x80000000) which caused Explorer to eagerly
enumerate WinFsp mount children (triggering Graph API calls). Adds
SFGAO_ISSLOW (0x00004000) to signal slow storage so Explorer avoids
aggressive prefetching.

0xF080004D -> 0x7080404D"
```

---

### Task 2: Skip `SHChangeNotify` when nav pane registry is already correct

**Files:**
- Modify: `crates/carminedesktop-app/src/shell_integration.rs` — add `ensure_nav_pane` function
- Modify: `crates/carminedesktop-app/src/main.rs:724-729` — call `ensure_nav_pane` instead of `register_nav_pane`

**Context:** `register_nav_pane` calls `notify_shell_change()` which fires `SHCNE_ASSOCCHANGED` — the heaviest Shell notification. This is called on every app startup even if nothing changed. We need a function that checks whether the registry already matches and only calls `register_nav_pane` when something differs.

- [ ] **Step 1: Add `ensure_nav_pane` function in `shell_integration.rs`**

Add this function after `update_nav_pane_target` (after line 616):

```rust
/// Ensure the navigation pane entry is registered and up-to-date.
///
/// Compares the current `TargetFolderPath` in the registry against `cloud_root`.
/// If the CLSID key exists and the target matches, this is a no-op — avoiding
/// the costly `SHChangeNotify(SHCNE_ASSOCCHANGED)` that a full
/// [`register_nav_pane`] call would trigger.
///
/// If the entry is missing or the target differs, delegates to
/// [`register_nav_pane`] (which sends the notification).
#[cfg(target_os = "windows")]
pub fn ensure_nav_pane(cloud_root: &std::path::Path) -> carminedesktop_core::Result<()> {
    let target = cloud_root.to_string_lossy();

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid_path = format!(r"Software\Classes\CLSID\{NAV_PANE_CLSID}");

    if let Ok(clsid_key) = hkcu.open_subkey_with_flags(&clsid_path, KEY_READ) {
        if let Ok(bag) = clsid_key.open_subkey_with_flags(r"Instance\InitPropertyBag", KEY_READ) {
            if let Ok(existing_target) = bag.get_value::<String, _>("TargetFolderPath") {
                if existing_target == target.as_ref() {
                    tracing::debug!("nav pane already registered with correct target, skipping");
                    return Ok(());
                }
            }
        }
    }

    register_nav_pane(cloud_root)
}
```

- [ ] **Step 2: Update `main.rs` to call `ensure_nav_pane`**

In `main.rs`, change lines 724-729:

```rust
// Before:
if nav_pane_enabled {
    if let Err(e) =
        shell_integration::register_nav_pane(std::path::Path::new(&cloud_root))
    {
        tracing::warn!("Explorer navigation pane registration failed: {e}");
    }

// After:
if nav_pane_enabled {
    if let Err(e) =
        shell_integration::ensure_nav_pane(std::path::Path::new(&cloud_root))
    {
        tracing::warn!("Explorer navigation pane registration failed: {e}");
    }
```

- [ ] **Step 3: Run clippy to verify no warnings**

Run: `make clippy`
Expected: no warnings

- [ ] **Step 4: Commit**

```bash
git add crates/carminedesktop-app/src/shell_integration.rs crates/carminedesktop-app/src/main.rs
git commit -m "perf: skip SHChangeNotify when nav pane registry is already correct

Add ensure_nav_pane() that checks TargetFolderPath before calling
register_nav_pane(). On normal startup (registry unchanged), this avoids
the costly SHCNE_ASSOCCHANGED notification that forces Explorer to
rebuild all cached file associations."
```

---

### Task 3: Move nav pane registration after `start_all_mounts`

**Files:**
- Modify: `crates/carminedesktop-app/src/main.rs:716-741` — reorder blocks

**Context:** Currently nav pane is registered at line 716-735, but mounts start at line 741. This means `SHChangeNotify` can fire before WinFsp is ready, causing Explorer to hit a non-mounted path. Moving the nav pane block after `start_all_mounts` ensures the VFS is serving before Explorer is notified.

- [ ] **Step 1: Move the nav pane reconciliation block after `start_all_mounts`**

In `main.rs`, move the `#[cfg(target_os = "windows")]` nav pane block (lines 716-735) to after `start_all_mounts(app);` (after line 741). The result should be:

```rust
        // Reconcile file association registration with config.
        {
            // ... (unchanged, lines 699-714)
        }

        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if !fuse_available() {
            notify::fuse_unavailable(app);
        }
        start_all_mounts(app);

        // Reconcile Explorer navigation pane registration with config.
        // Placed after start_all_mounts so WinFsp is serving before Explorer
        // is notified via SHChangeNotify (on first registration).
        #[cfg(target_os = "windows")]
        {
            let (nav_pane_enabled, cloud_root) = {
                let config = state.effective_config.lock().unwrap();
                let root = expand_mount_point(&format!("~/{}", config.root_dir));
                (config.explorer_nav_pane, root)
            };
            if nav_pane_enabled {
                if let Err(e) =
                    shell_integration::ensure_nav_pane(std::path::Path::new(&cloud_root))
                {
                    tracing::warn!("Explorer navigation pane registration failed: {e}");
                }
            } else if shell_integration::is_nav_pane_registered()
                && let Err(e) = shell_integration::unregister_nav_pane()
            {
                tracing::warn!("Explorer navigation pane unregistration failed: {e}");
            }
        }

        run_crash_recovery(app);
        start_delta_sync(app);
```

- [ ] **Step 2: Run clippy to verify no warnings**

Run: `make clippy`
Expected: no warnings

- [ ] **Step 3: Commit**

```bash
git add crates/carminedesktop-app/src/main.rs
git commit -m "perf: register nav pane after mounts are started

Moves Explorer nav pane reconciliation after start_all_mounts() so that
WinFsp is already serving when SHChangeNotify fires (on first
registration). Prevents Explorer from hitting a non-mounted path."
```
