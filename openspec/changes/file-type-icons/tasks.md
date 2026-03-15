## 1. Asset Preparation

- [x] 1.1 Convert SVG icons to multi-resolution ICO files (16, 32, 48, 256px) for doc, xls, ppt, pdf
- [x] 1.2 Commit `.ico` files to `crates/carminedesktop-app/icons/files/`

## 2. Build System

- [x] 2.1 Add `embed-resource` to `[workspace.dependencies]` in root `Cargo.toml`
- [x] 2.2 Add `embed-resource` to `[build-dependencies]` in `crates/carminedesktop-app/Cargo.toml`
- [x] 2.3 Create `crates/carminedesktop-app/icons/files/file_icons.rc` declaring icons at ordinals 101-104
- [x] 2.4 Add `embed_resource::compile()` call in `build.rs` (before `tauri_build::build()`, guarded by `#[cfg(target_os = "windows")]`)

## 3. Registration Logic

- [x] 3.1 Add `ICON_ORDINALS` const mapping table (extension → ordinal) in `shell_integration.rs`
- [x] 3.2 In `register_file_associations()`, create `DefaultIcon` subkey on each ProgID with value `"{exe_path},-{ordinal}"`
- [x] 3.3 Verify `unregister_file_associations()` already cleans up `DefaultIcon` via `delete_subkey_all` (no code change expected, just verify)

## 4. Verification

- [x] 4.1 Build on Linux/macOS to confirm `embed-resource` is a no-op (no build errors)
- [x] 4.2 Run `make clippy` to confirm zero warnings
