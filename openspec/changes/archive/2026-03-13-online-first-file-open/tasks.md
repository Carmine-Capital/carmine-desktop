## 1. Simplify core types and config

- [x] 1.1 Remove `CollabOpenResponse::Cancel` variant from `cloudmount-core/src/types.rs`
- [x] 1.2 Remove `has_local_changes` field from `CollabOpenRequest` in `cloudmount-core/src/types.rs`
- [x] 1.3 Remove `CollabDefaultAction` enum from `cloudmount-core/src/config.rs`
- [x] 1.4 Simplify `CollaborativeOpenConfig` to only `timeout_seconds` and `shell_processes` in `cloudmount-core/src/config.rs`
- [x] 1.5 Update `EffectiveConfig` to use the simplified `CollaborativeOpenConfig`

## 2. Simplify VFS CollabGate

- [x] 2.1 Remove `VfsError::OperationCancelled` variant from `cloudmount-vfs/src/core_ops.rs`
- [x] 2.2 Remove `OperationCancelled` → `ECANCELED` mapping from `cloudmount-vfs/src/fuse_fs.rs`
- [x] 2.3 Remove `OperationCancelled` → `STATUS_CANCELLED` mapping from `cloudmount-vfs/src/winfsp_fs.rs`
- [x] 2.4 Remove `Cancel` arm from CollabGate match in `CoreOps::open_file()`
- [x] 2.5 Remove `has_local_changes` computation from CollabGate request construction in `CoreOps::open_file()`

## 3. Simplify Tauri CollabGate handler

- [x] 3.1 Remove `resolve_collab_preference()` function from `cloudmount-app/src/main.rs`
- [x] 3.2 Remove `show_collab_dialog()` function from `cloudmount-app/src/main.rs`
- [x] 3.3 Simplify `spawn_collab_handler()` to unconditionally open online with local fallback on error

## 4. Delete integration modules and scripts

- [x] 4.1 Delete `cloudmount-app/src/linux_integrations.rs` and remove `mod linux_integrations` from `main.rs`
- [x] 4.2 Delete `cloudmount-app/src/windows_integrations.rs` and remove `mod windows_integrations` from `main.rs`
- [x] 4.3 Delete `cloudmount-app/src/macos_integrations.rs` and remove `mod macos_integrations` from `main.rs`
- [x] 4.4 Delete all context menu scripts from `cloudmount-app/scripts/` (keep `README.md` only if still relevant, otherwise delete)
- [x] 4.5 Remove `reconcile_existing_installation()` call from `setup_after_launch()` in `main.rs`
- [x] 4.6 Remove `register_context_menus()` call from Windows mount path in `main.rs`
- [x] 4.7 Remove `unregister_context_menus()` call from Windows unmount path in `main.rs`

## 5. Remove tray menu integration toggle

- [x] 5.1 Remove `toggle_linux_integrations` menu item and its construction from `tray.rs`
- [x] 5.2 Remove `toggle_linux_integrations` handler from tray menu event handler in `tray.rs`
- [x] 5.3 Remove `linux_integrations_menu_label()` helper from `tray.rs`
- [x] 5.4 Remove `linux_integrations_installed`, `linux_integrations_removed`, and `linux_integrations_failed` from `notify.rs`

## 6. Remove collab config commands and settings UI

- [x] 6.1 Remove `get_collab_config` and `update_collab_config` Tauri commands from `commands.rs`
- [x] 6.2 Remove `CollabConfigInfo` and `ExtensionPref` structs from `commands.rs`
- [x] 6.3 Unregister `get_collab_config` and `update_collab_config` from the `invoke_handler!` in `main.rs`
- [x] 6.4 Remove "Collaborative Editing" section from `settings.html`
- [x] 6.5 Remove all collab config JS from `settings.js` (load, save, toggle, per-extension handlers)

## 7. Update tests

- [x] 7.1 Update CollabGate tests in `test_collab_gate.rs`: remove Cancel/dialog/preference scenarios, verify always-online behavior
- [x] 7.2 Remove or update any integration tests that reference `CollabDefaultAction::Ask`, `Cancel`, or `has_local_changes`
- [x] 7.3 Run full CI check (fmt, clippy, build, test) to catch any remaining references
