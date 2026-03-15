# External File-Type Icons for Windows Shell Integration

**Date**: 2026-03-15
**Status**: Approved

## Problem

Embedding per-file-type icons (doc, xls, ppt, pdf) into the Windows executable
alongside Tauri's app icon causes linker conflicts:

1. Separate `.lib` files with auto-assigned internal ICON ordinals collide (CVT1100 ICON)
2. Same-stem compilation duplicates Tauri's link directive (CVT1100 VERSION)
3. Empty COFF archive workaround rejected by `link.exe` (LNK1107)

The root cause: Tauri owns the resource compilation pipeline (`resource.rc` →
`resource.lib`) and emits `cargo:rustc-link-lib=static=resource`. Any attempt to
merge our icons into that pipeline is fragile and version-dependent.

## Solution

Distribute file-type icons as **external `.ico` files** alongside the executable
instead of embedding them as PE resources. The Windows registry `DefaultIcon`
values point to the `.ico` file paths instead of exe ordinals.

## Changes

### 1. `tauri.conf.json` — bundle icons as resources

Add a `resources` map to `bundle` so the MSI installer copies `.ico` files:

```json
"resources": {
  "icons/files/doc.ico": "icons/doc.ico",
  "icons/files/xls.ico": "icons/xls.ico",
  "icons/files/ppt.ico": "icons/ppt.ico",
  "icons/files/pdf.ico": "icons/pdf.ico"
}
```

Installed layout:
```
<install_dir>/
├── Carmine Desktop.exe
└── icons/
    ├── doc.ico
    ├── xls.ico
    ├── ppt.ico
    └── pdf.ico
```

### 2. `shell_integration.rs` — file paths instead of ordinals

Replace `ICON_ORDINALS` (extension → resource ordinal) with `ICON_FILES`
(extension → ico filename):

```rust
const ICON_FILES: &[(&str, &str)] = &[
    (".doc", "doc.ico"),  (".docx", "doc.ico"),
    (".xls", "xls.ico"),  (".xlsx", "xls.ico"),
    (".ppt", "ppt.ico"),  (".pptx", "ppt.ico"),
    (".pdf", "pdf.ico"),
];
```

Note: `.pdf` is not in `OFFICE_EXTENSIONS` (no file association registered for
PDF), but the icon is mapped here so it's ready when PDF support is added.

Build the icon path from `current_exe().parent()` + `icons/{name}`.
Skip the `DefaultIcon` write if the icon file does not exist on disk (graceful
no-op in dev builds where Tauri resources are not deployed).

Registry `DefaultIcon` changes from:
```
"C:\...\Carmine Desktop.exe,-101"
```
to:
```
"C:\...\icons\doc.ico"
```

### 3. `build.rs` — remove `embed_file_icons()`

Delete the entire `embed_file_icons()` function and its call from `main()`.
This removes:
- RC file manipulation
- COFF archive generation
- `embed_resource::compile()` calls for file icons

### 4. Cleanup

- Delete `icons/files/file_icons.rc` (no longer needed)
- Remove `embed-resource` from build-dependencies if unused elsewhere

## Upgrade Path

Users upgrading from ordinal-based icons (pre-existing installs) to file-path
icons need no manual intervention. `register_file_associations()` overwrites
the `DefaultIcon` value on each call via `create_subkey` + `set_value`, so the
old `"exe_path,-101"` value is replaced by `"icons\doc.ico"` transparently.

## Trade-offs

**Advantages:**
- Eliminates all RC/COFF/linker complexity
- Zero risk of conflict with Tauri's resource pipeline
- Works across all Tauri and MSVC versions
- Easy to add new file types later (drop an `.ico`, add a map entry)

**Disadvantages:**
- Icons are separate files on disk (~4 files, few KB each)
- If exe is moved without the icons folder, Explorer shows no file-type icons
  (app still works, associations still work, just no custom icon)
- Negligible: MSI installation guarantees co-location

## Files Affected

| File | Action |
|------|--------|
| `crates/carminedesktop-app/tauri.conf.json` | Add `resources` to `bundle` |
| `crates/carminedesktop-app/src/shell_integration.rs` | Replace ordinal-based icons with file-path-based |
| `crates/carminedesktop-app/build.rs` | Remove `embed_file_icons()` |
| `crates/carminedesktop-app/icons/files/file_icons.rc` | Delete |
| `crates/carminedesktop-app/Cargo.toml` | Remove `embed-resource` build-dep (if unused) |
| Root `Cargo.toml` | Remove `embed-resource` workspace dep (if unused) |
