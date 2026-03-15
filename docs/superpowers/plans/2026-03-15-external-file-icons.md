# External File-Type Icons Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace embedded PE resource icons with external `.ico` files to eliminate Windows linker conflicts.

**Architecture:** Remove `embed_file_icons()` from build.rs, bundle `.ico` files via Tauri `resources`, and update `shell_integration.rs` to write file paths instead of ordinals to the Windows registry.

**Tech Stack:** Rust, Tauri v2 (bundle resources), Windows Registry (winreg)

**Spec:** `docs/superpowers/specs/2026-03-15-external-file-icons-design.md`

---

## Chunk 1: Remove icon embedding from build pipeline

### Task 1: Simplify `build.rs`

**Files:**
- Modify: `crates/carminedesktop-app/build.rs`

- [ ] **Step 1: Remove `embed_file_icons()` and its call**

Replace the entire `build.rs` with:

```rust
fn main() {
    // Ensure winfsp-x64.dll is delay-loaded so the process can start without
    // the DLL present (e.g. when launched from Explorer via context menu).
    // The winfsp-sys crate also emits these flags, but they may not propagate
    // reliably to the final binary — so we repeat them here for safety.
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        println!("cargo:rustc-link-lib=dylib=delayimp");
        #[cfg(target_arch = "x86_64")]
        println!("cargo:rustc-link-arg=/DELAYLOAD:winfsp-x64.dll");
        #[cfg(target_arch = "x86")]
        println!("cargo:rustc-link-arg=/DELAYLOAD:winfsp-x86.dll");
        #[cfg(target_arch = "aarch64")]
        println!("cargo:rustc-link-arg=/DELAYLOAD:winfsp-a64.dll");
    }

    #[cfg(feature = "desktop")]
    tauri_build::build();
}
```

- [ ] **Step 2: Delete `file_icons.rc`**

Delete file: `crates/carminedesktop-app/icons/files/file_icons.rc`

- [ ] **Step 3: Remove `embed-resource` build dependency**

In `crates/carminedesktop-app/Cargo.toml`, remove line 47:

```toml
embed-resource = { workspace = true }
```

In root `Cargo.toml`, remove line 90:

```toml
embed-resource = "3.0"
```

- [ ] **Step 4: Verify it compiles on Linux**

Run: `make check`
Expected: success, no warnings about unused embed-resource

### Task 2: Bundle `.ico` files via Tauri resources

**Files:**
- Modify: `crates/carminedesktop-app/tauri.conf.json`

- [ ] **Step 1: Add `resources` to `bundle` config**

In `tauri.conf.json`, add `"resources"` inside the `"bundle"` object (after the `"windows"` key at line 42):

```json
    "resources": {
      "icons/files/doc.ico": "icons/doc.ico",
      "icons/files/xls.ico": "icons/xls.ico",
      "icons/files/ppt.ico": "icons/ppt.ico",
      "icons/files/pdf.ico": "icons/pdf.ico"
    }
```

This tells Tauri's bundler (MSI) to copy each `.ico` from `icons/files/` into an `icons/` subdirectory next to the exe.

## Chunk 2: Update shell integration to use file paths

### Task 3: Replace ordinal-based `DefaultIcon` with file paths

**Files:**
- Modify: `crates/carminedesktop-app/src/shell_integration.rs:34-50,72-129`

- [ ] **Step 1: Replace `ICON_ORDINALS` with `ICON_FILES`**

Replace lines 34-50 (the `ICON_ORDINALS` constant and its doc comment):

```rust
/// Icon files bundled alongside the executable for Windows shell integration.
///
/// Each entry maps a file extension (with leading dot) to the `.ico` filename
/// in the `icons/` subdirectory next to the executable. Referenced in
/// `DefaultIcon` registry values as an absolute path to the `.ico` file.
#[cfg(target_os = "windows")]
const ICON_FILES: &[(&str, &str)] = &[
    (".doc", "doc.ico"),
    (".docx", "doc.ico"),
    (".xls", "xls.ico"),
    (".xlsx", "xls.ico"),
    (".ppt", "ppt.ico"),
    (".pptx", "ppt.ico"),
    (".pdf", "pdf.ico"),
];
```

- [ ] **Step 2: Update `register_file_associations()` to use file paths**

Replace lines 123-129 (the `DefaultIcon` block inside the `for ext` loop):

Before:
```rust
        // Set DefaultIcon to the embedded file-type icon resource (if mapped).
        // Uses negative ordinal syntax (,-N) which addresses the resource ordinal
        // directly, independent of icon group enumeration order.
        if let Some(&(_, ordinal)) = ICON_ORDINALS.iter().find(|(e, _)| e == ext) {
            let (icon_key, _) = progid_key.create_subkey("DefaultIcon")?;
            icon_key.set_value("", &format!("{exe_str},-{ordinal}"))?;
        }
```

After:
```rust
        // Set DefaultIcon to the bundled .ico file (if mapped).
        // In dev builds the icon may not exist — skip gracefully.
        if let Some(&(_, icon_name)) = ICON_FILES.iter().find(|(e, _)| e == ext) {
            let icon_path = exe_path.parent().unwrap().join("icons").join(icon_name);
            if icon_path.exists() {
                let (icon_key, _) = progid_key.create_subkey("DefaultIcon")?;
                icon_key.set_value("", &icon_path.to_string_lossy().as_ref())?;
            }
        }
```

Note: `exe_path` is already defined on line 72 as `std::env::current_exe()`. We use it directly instead of `exe_str` since we need the `Path` for `.parent().join()`.

- [ ] **Step 3: Verify it compiles**

Run: `make clippy`
Expected: success, zero warnings

- [ ] **Step 4: Commit all changes**

```bash
git add -A
git commit -m "fix(app): use external .ico files instead of embedded PE resources

- Remove embed_file_icons() from build.rs (eliminates CVT1100/LNK1107)
- Bundle .ico files via Tauri resources config
- Update shell_integration.rs to write file paths to DefaultIcon registry
- Remove embed-resource build dependency
- Delete file_icons.rc (no longer needed)"
```

## Chunk 3: Verify on Windows

### Task 4: Test on Windows

- [ ] **Step 1: Build and run on Windows**

Run: `cargo run -p carminedesktop-app --features desktop`
Expected: no linker errors, app starts normally

- [ ] **Step 2: Verify icon files are bundled (MSI build)**

Run: `cargo tauri build`
Check that the MSI installs `icons/doc.ico`, `icons/xls.ico`, `icons/ppt.ico`, `icons/pdf.ico` next to the exe.

- [ ] **Step 3: Verify file association icons**

After running the app (which calls `register_file_associations()`), check the registry:
- `HKCU\Software\Classes\CarmineDesktop.OfficeFile.docx\DefaultIcon` should contain `C:\...\icons\doc.ico`
- Create a `.docx` file and verify it shows the correct icon in Explorer
