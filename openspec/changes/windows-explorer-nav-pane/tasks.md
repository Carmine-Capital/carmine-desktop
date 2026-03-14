## 1. Config layer

- [x] 1.1 Add `explorer_nav_pane: Option<bool>` field to `UserGeneralSettings` in `config.rs`, with default resolution to `true` on Windows, `false` elsewhere (same pattern as `register_file_associations`)
- [x] 1.2 Add `explorer_nav_pane: bool` field to `EffectiveConfig` and wire it in `EffectiveConfig::from_user_config()`
- [x] 1.3 Add `"explorer_nav_pane"` to the `clear_setting` match arm in `UserGeneralSettings`

## 2. Shell integration — navigation pane functions

- [x] 2.1 Add the hardcoded CLSID GUID constant `NAV_PANE_CLSID` (`{E4B3F4A1-7C2D-4A8E-B5D6-9F1E2A3C4B5D}`) in `shell_integration.rs`
- [x] 2.2 Implement `register_nav_pane(cloud_root: &Path) -> Result<()>` — creates CLSID key tree (DefaultIcon, InProcServer32, Instance/InitPropertyBag with TargetFolderPath and Attributes, ShellFolder, shell\open\command), Desktop\NameSpace entry, HideDesktopIcons entry, then calls SHChangeNotify
- [x] 2.3 Implement `unregister_nav_pane() -> Result<()>` — removes all three registry key trees (resilient to missing keys), then calls SHChangeNotify
- [x] 2.4 Implement `is_nav_pane_registered() -> bool` — checks if the CLSID key exists in HKCU
- [x] 2.5 Implement `update_nav_pane_target(cloud_root: &Path) -> Result<()>` — updates TargetFolderPath and DefaultIcon (exe path may have changed), then calls SHChangeNotify

## 3. App lifecycle integration

- [x] 3.1 Add navigation pane reconciliation in `setup_after_launch()` — after token restoration, before mount startup: if `explorer_nav_pane` enabled, call `register_nav_pane()` with expanded cloud root path; if disabled but registered, call `unregister_nav_pane()`. Log warnings on failure, non-fatal.
- [x] 3.2 Wire `explorer_nav_pane` toggle in `save_settings` command — on save, if value changed, call `register_nav_pane()` or `unregister_nav_pane()` accordingly. Also call `update_nav_pane_target()` if `root_dir` changed and nav pane is registered.

## 4. Tauri command and frontend

- [x] 4.1 Ensure `save_settings` Tauri command passes the `explorer_nav_pane` value through to config persistence and shell integration (extend existing save_settings flow)
- [x] 4.2 Add `explorer_nav_pane` toggle to the settings UI in `dist/` — checkbox with label "Show in Explorer navigation pane", only visible on Windows (feature-gated via a Tauri command that returns platform info or a config field)

## 5. Testing

- [x] 5.1 Add integration tests in `crates/carminedesktop-app/tests/` for `register_nav_pane`, `unregister_nav_pane`, `is_nav_pane_registered`, and `update_nav_pane_target` — verify registry keys are created/removed correctly (Windows CI only, `#[cfg(target_os = "windows")]`)
- [x] 5.2 Add test for config resolution: `explorer_nav_pane` defaults to true on Windows, false elsewhere
- [x] 5.3 Add test for resilient unregistration: calling `unregister_nav_pane()` when keys are already missing does not error
