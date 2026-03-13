## 1. NSIS Hook

- [x] 1.1 Create `crates/cloudmount-app/windows/hooks.nsh` with `NSIS_HOOK_PREINSTALL` macro: registry check (`HKLM\SOFTWARE\WinFsp` → `InstallDir`), extract MSI via `File` directive to `$TEMP\winfsp.msi`, run `msiexec /i "$TEMP\winfsp.msi" /qn INSTALLLEVEL=1000`, check exit code, delete temp MSI, abort with message box on failure
- [x] 1.2 Add `"installerHooks": "windows/hooks.nsh"` to the `nsis` section of `tauri.conf.json`

## 2. CI Workflow

- [x] 2.1 Add a WinFsp version variable (e.g., `WINFSP_MSI_VERSION`) and download step to `.github/workflows/release.yml` (Windows job only): download the pinned WinFsp MSI from `https://github.com/winfsp/winfsp/releases/download/...` to `crates/cloudmount-app/resources/winfsp.msi`
- [x] 2.2 Add the same WinFsp MSI download step to `.github/workflows/build-installer.yml` (Windows job only)
- [x] 2.3 Add `crates/cloudmount-app/resources/` to `.gitignore`

## 3. Attribution

- [x] 3.1 Add WinFsp copyright notice and GitHub repository link to `crates/cloudmount-app/dist/settings.html` in a "Third-party" section

## 4. Verification

- [ ] 4.1 Verify the NSIS hook syntax is valid by running a test build on Windows (`cargo tauri build --features desktop` with the MSI present in `resources/`)
- [ ] 4.2 Verify the installer correctly skips WinFsp install when already present, and installs it when absent
