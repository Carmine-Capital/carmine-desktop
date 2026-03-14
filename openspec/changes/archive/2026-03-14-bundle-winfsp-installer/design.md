## Context

carminedesktop on Windows depends on WinFsp (Windows File System Proxy) as a system-level filesystem driver. Currently, users must install WinFsp separately before carminedesktop can mount drives. The app has a runtime preflight check that detects missing WinFsp and shows an error, but this still requires the user to leave the app, find WinFsp, install it, and restart.

The Tauri NSIS bundler supports `installerHooks` — a `.nsh` file with macros that run at specific points during installation. This is the standard mechanism for installing prerequisites.

WinFsp's license (GPLv3 with FLOSS redistribution exception) permits bundling the unmodified MSI installer in open-source projects, provided a copyright notice and repo link appear in the UI.

## Goals / Non-Goals

**Goals:**
- Zero-friction WinFsp installation during carminedesktop setup
- Skip WinFsp install if already present (idempotent)
- Abort carminedesktop installation with clear message if WinFsp install fails
- Satisfy WinFsp redistribution license requirements

**Non-Goals:**
- WinFsp removal on carminedesktop uninstall (other apps may depend on it)
- WinFsp version upgrade management (if user has older WinFsp, leave it)
- Supporting WinFsp installation outside the NSIS installer flow (manual/portable installs rely on the existing preflight check)

## Decisions

### 1. Embed MSI via NSIS `File` directive, not Tauri `resources`

The WinFsp MSI is only needed during installation, not at runtime. Using Tauri's `resources` would leave a ~1.5 MB file permanently in the install directory. Instead, the NSIS hook extracts it to `$TEMP`, runs it, and deletes it.

**Alternative considered:** Tauri `bundle.resources` — simpler config but wastes disk space post-install.

### 2. Registry check for existing installation

Check `HKLM\SOFTWARE\WinFsp` for the `InstallDir` value (same logic as the Rust preflight check). This avoids re-installing over an existing WinFsp and is the canonical way to detect it.

**Alternative considered:** Checking for `winfsp-x64.dll` on PATH — less reliable, WinFsp doesn't add itself to PATH.

### 3. Silent install with `INSTALLLEVEL=1000`

Run `msiexec /i "$TEMP\winfsp.msi" /qn INSTALLLEVEL=1000`. The `/qn` flag suppresses all UI. `INSTALLLEVEL=1000` installs all features including the kernel driver.

**Alternative considered:** Interactive install (`/qr` or `/qb`) — adds unnecessary friction when the user already consented to the carminedesktop install.

### 4. Pin WinFsp version in CI

The CI workflow downloads a specific WinFsp release (e.g., `winfsp-2.1.25156.msi`). The version is defined as a workflow variable so it's easy to update alongside the `winfsp` crate dependency.

**Alternative considered:** Always downloading "latest" — risks breaking builds or shipping untested versions.

### 5. No uninstall hook

WinFsp is a shared system component. Other applications (Cryptomator, SSHFS-Win, rclone) may depend on it. Removing it during carminedesktop uninstall could break those apps.

### 6. Attribution in settings page

Add WinFsp copyright notice and GitHub link to `settings.html` in a "Third-party" section. This satisfies the redistribution exception's requirement for user-interface attribution.

## Risks / Trade-offs

- **WinFsp install requires admin privileges** → The NSIS installer already runs elevated (required for program files installation), so this is inherited naturally. No additional UAC prompt.
- **MSI install failure** → Show a message box explaining WinFsp is required, abort carminedesktop install. The user can install WinFsp manually and retry.
- **Pinned version drift** → If the `winfsp` crate is updated but the MSI pin isn't, there could be a mismatch. Mitigation: document the coupling in the workflow file with a comment.
- **Installer size increase** → ~1.5 MB for the WinFsp MSI. Negligible relative to the app binary.
