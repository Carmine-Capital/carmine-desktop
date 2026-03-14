## Context

carminedesktop is a system tray app (Tauri v2) that mounts OneDrive/SharePoint as local filesystems. Branded builds (e.g., Carmine Drive) are produced by private overlay repos that inject org config and build with `cargo tauri build`. Currently, the build pipeline produces raw debug binaries with `cargo build --release` — no installers, no signing, no update mechanism.

The primary deployment target is Carmine Capital (~40 users, Windows/macOS/Linux). Updates must be seamless — no manual downloads, no walking to desks.

Distribution model:
- `cloud-mount` (public) — source code + quality gate CI + git tags
- `cloud-mount-carmine` (private) — org config + build pipeline
- `carmine-drive-releases` (public, locked down) — GitHub Releases hosting installers + update manifests

## Goals / Non-Goals

**Goals:**
- App checks for updates automatically and installs them without user intervention
- Branded builds produce signed platform installers (`.deb`, `.AppImage`, `.dmg`, `.msi`)
- Each branded build has its own update endpoint, signing keys, and release channel
- Main repo provides the updater infrastructure; branded repos configure the endpoint
- Update flow is non-disruptive — no forced restarts during work

**Non-Goals:**
- Auto-updates in headless mode (no Tauri runtime — headless users update manually or via package manager)
- OS-level code signing (macOS notarization, Windows Authenticode) — future enhancement
- Delta/differential updates — full bundle replacement is fine for this app size
- Multiple release channels (stable/beta) — single channel per branded build
- Custom update server logic — static JSON file on GitHub Releases is sufficient

## Decisions

### D1: Use `tauri-plugin-updater` (not custom updater)

Tauri v2's built-in updater plugin handles the complete update lifecycle: endpoint polling, version comparison, download with progress, ed25519 signature verification, and platform-native installation. Building a custom updater would duplicate all of this.

**Alternative considered**: Custom update checker with direct download links. Rejected — would need to reimplement signature verification, platform-specific install logic, and atomic replacement. The Tauri plugin is well-tested and maintained.

### D2: Update checks from Rust backend (not JavaScript frontend)

carminedesktop is primarily a system tray app. The settings/wizard webview is shown rarely. Update checks must run even when no webview is open. The `tauri-plugin-updater` exposes a Rust API via `UpdaterExt` trait that works without any frontend.

```rust
use tauri_plugin_updater::UpdaterExt;

let update = app.updater()?.check().await?;
```

**Alternative considered**: JavaScript-based update check in the webview. Rejected — the webview isn't always open, and we'd need IPC plumbing to trigger from the tray.

### D3: Check timing — startup + periodic + manual

- **On startup**: Check after a 10-second delay (let mounts initialize first)
- **Periodic**: Every 4 hours while running
- **Manual**: "Check for Updates" tray menu item

This ensures users get updates within a work day without excessive polling.

### D4: Non-disruptive update flow

When an update is found:
1. Download in background (silent)
2. Send notification: "{app_name} v{version} is ready — restart to update"
3. User restarts at their convenience (via "Restart to Update" tray menu item)
4. On next app quit (natural or via tray), the update is installed before relaunch

This avoids interrupting active filesystem operations. A forced restart during writes could cause data loss.

**Alternative considered**: Fully automatic download + restart. Rejected — restarting unmounts all filesystems, which disrupts active work. The user should choose when.

### D5: Placeholder endpoint in main repo, branded repos override

The main repo's `tauri.conf.json` includes the updater plugin with an empty/placeholder endpoint. Branded build repos patch this with their actual release URL before building.

```
Main repo tauri.conf.json:
  plugins.updater.endpoints = []        ← no endpoint = updater disabled
  plugins.updater.pubkey = ""           ← no key = can't verify

Carmine patches before build:
  plugins.updater.endpoints = ["https://github.com/.../releases/latest/download/update.json"]
  plugins.updater.pubkey = "dW50cnV..."  ← Carmine's public key
```

When endpoints is empty, the updater gracefully does nothing. Generic/dev builds skip updates automatically.

### D6: GitHub Releases for update hosting

Each branded build's release workflow:
1. `cargo tauri build` — produces signed installers + `.sig` files
2. Generates `update.json` manifest from build output
3. `gh release create` on the public release repo with installers + `update.json`

The updater endpoint URL points at `https://github.com/{owner}/{release-repo}/releases/latest/download/update.json`.

GitHub Releases provides:
- CDN-backed downloads (fast, reliable)
- Immutable release assets
- Zero infrastructure to maintain
- Works with fine-grained PAT for cross-repo publishing

### D7: Ed25519 signing via Tauri's built-in signer

Tauri uses ed25519 signatures (not OS code signing). Each branded build has its own key pair:
- **Private key**: stored as `TAURI_SIGNING_PRIVATE_KEY` GitHub Secret
- **Password**: stored as `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` GitHub Secret
- **Public key**: embedded in `tauri.conf.json` via the branded repo's patch

The `cargo tauri build` command automatically signs bundles when these env vars are set, producing `.sig` files alongside each installer.

### D8: New `update.rs` module in `carminedesktop-app`

Update logic lives in a new `crates/carminedesktop-app/src/update.rs` module (gated behind `#[cfg(feature = "desktop")]`):
- `check_for_update(app: &AppHandle)` — polls the endpoint, returns update info
- `spawn_update_checker(app: AppHandle)` — background task: startup delay + periodic checks
- Integration with tray menu for manual check and "Restart to Update" item

This keeps update concerns separate from mount lifecycle, auth, and tray code.

## Risks / Trade-offs

**[Risk] Update during active writes** → Mitigation: Updates are never auto-installed. The user triggers restart, which runs the standard graceful shutdown (flush pending writes, unmount, then exit). The updater installs during the restart transition.

**[Risk] Signing key compromise** → Mitigation: Private key stored only in GitHub Secrets (never in repo). Rotation requires a new build with the new public key shipped first, then subsequent updates signed with the new key. No remote revocation mechanism — but acceptable for 40 users.

**[Risk] GitHub Releases unavailable** → Mitigation: Update check failure is logged and silently retried on next cycle. The app continues working normally. Users aren't blocked from using the filesystem.

**[Risk] Platform differences in update installation** → Mitigation: Tauri handles this per-platform. On Windows, the `.msi`/`.nsis` installer runs. On macOS, the `.app` bundle is replaced. On Linux, the `.AppImage` is replaced. We rely on Tauri's tested behavior here.

**[Risk] Headless mode has no auto-updates** → Mitigation: Documented as a non-goal. Headless is for development/servers. Production users use the desktop app.

**[Trade-off] No delta updates** → Full bundle downloads (~15-30MB) on every update. Acceptable for the app size and update frequency. Saves significant complexity.

**[Trade-off] No staged rollouts** → All users get the same update at the same time. With 40 users, staged rollouts add complexity without meaningful risk reduction.
