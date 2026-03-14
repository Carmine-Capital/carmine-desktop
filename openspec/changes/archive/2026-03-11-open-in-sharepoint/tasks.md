## 1. DriveItem webUrl Foundation

- [x] 1.1 Add `web_url: Option<String>` field with `#[serde(rename = "webUrl")]` to `DriveItem` in `crates/carminedesktop-core/src/types.rs`
- [x] 1.2 Add `webUrl` to the `$select` parameter in `list_children()` at `crates/carminedesktop-graph/src/client.rs`
- [x] 1.3 Add `webUrl` to the `$select` parameter in `list_root_children()` at `crates/carminedesktop-graph/src/client.rs`
- [x] 1.4 Update existing graph integration tests to include `webUrl` in mock responses and verify it is deserialized

## 2. Office URI Scheme Mapping

- [x] 2.1 Create a `office_uri(extension: &str, web_url: &str) -> String` helper function that maps file extensions to Office URI schemes (`ms-word:ofe|u|...`, `ms-excel:ofe|u|...`, `ms-powerpoint:ofe|u|...`) and falls back to the plain URL for non-Office files. Use the plain URL unconditionally on Linux.
- [x] 2.2 Write unit tests for the Office URI mapping covering Word, Excel, PowerPoint extensions, unknown extensions, and Linux platform fallback

## 3. Path Resolution and Tauri Command

- [x] 3.1 Add a `resolve_web_url(local_path: &str) -> Result<String>` function that strips the mount prefix, splits into path components, calls `CoreOps::resolve_path()`, and returns `DriveItem.web_url`. If `web_url` is `None`, fall back to `graph.get_item()` to fetch it.
- [x] 3.2 Add a `#[tauri::command] open_online(path: String)` Tauri command that calls `resolve_web_url`, builds the Office URI via the helper from 2.1, and opens it via `open_with_clean_env` / `open::that`. On Office URI failure, fall back to the plain `webUrl`.
- [x] 3.3 Register `open_online` in the Tauri `invoke_handler!` macro in `main.rs`

## 4. Deep-Link Protocol Handler

- [x] 4.1 Add `tauri-plugin-deep-link` dependency to workspace `Cargo.toml` and `crates/carminedesktop-app/Cargo.toml`
- [x] 4.2 Register the `carminedesktop://` protocol scheme in the Tauri configuration (`tauri.conf.json` or equivalent)
- [x] 4.3 Add a deep-link event handler in the Tauri setup that parses `carminedesktop://open-online?path=<encoded>`, decodes the path, and calls the same resolution + open logic as the `open_online` command
- [x] 4.4 On invalid paths (not inside a mount) or unrecognized actions, show a desktop notification with the error

## 5. Windows Explorer Context Menu

- [x] 5.1 Create a helper function to write registry keys under `HKCU\Software\Classes\*\shell\carminedesktop.OpenInSharePoint` with the display text "Open in SharePoint" and command `cmd /c start carminedesktop://open-online?path=%1`
- [x] 5.2 Create a helper function to remove the registry keys on cleanup
- [x] 5.3 Call the registration helper during CfApi sync root setup in `cfapi.rs` mount flow
- [x] 5.4 Call the cleanup helper during CfApi sync root teardown in `cfapi.rs` unmount flow

## 6. Linux Nautilus Script

- [x] 6.1 Create a `Open in SharePoint` shell script that reads `NAUTILUS_SCRIPT_SELECTED_FILE_PATHS`, percent-encodes the path, and calls `xdg-open "carminedesktop://open-online?path=<encoded>"`
- [x] 6.2 Document installation instructions (copy to `~/.local/share/nautilus/scripts/`) in the script header or a README

## 7. Integration and Verification

- [ ] 7.1 Verify end-to-end on Windows: mount a drive, browse in Explorer, right-click a `.docx`, select "Open in SharePoint", confirm Word opens with SharePoint connection
- [ ] 7.2 Verify end-to-end on Linux: mount a drive, use Nautilus script or Tauri UI, confirm browser opens Office Online
- [ ] 7.3 Verify fallback: test with a non-Office file (`.pdf`) to confirm browser opens
- [ ] 7.4 Verify error handling: test with a path outside the mount to confirm error notification
- [ ] 7.5 Run CI checks (`make check`) to verify no warnings, clippy, or test failures
