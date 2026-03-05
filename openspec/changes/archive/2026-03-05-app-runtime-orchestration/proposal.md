## Why

All six crates are individually complete and tested, but the application cannot actually function — sign-in is a no-op, SharePoint search returns empty results, and mounts defined in config are never started. The `filesync-app` crate is a UI skeleton (~495 lines) with no runtime wiring. A runtime orchestration layer is needed to connect Auth → Graph → Cache → VFS into a working application with a frictionless first-run experience.

## What Changes

- Add runtime orchestration to `filesync-app`: expand `AppState` to hold `AuthManager`, `GraphClient`, `CacheManager`, mount handles, and `DeltaSyncTimer`
- Implement component initialization sequence: Config → Auth → Graph → Cache → DeltaSync → VFS mounts
- Implement two first-run flows:
  - **With packaged defaults**: "Sign in with Microsoft" → auto-mount all pre-configured drives → minimize to tray
  - **Without packaged defaults**: "Sign in with Microsoft" → auto-discover and mount user's OneDrive → prompt for root directory name (`~/Cloud/` default) → skip SharePoint (add later from Settings) → minimize to tray
- Auto-discover user's OneDrive via `GET /me/drive` after sign-in (always mounted, no prompt)
- Default mount root directory `~/Cloud/` — OneDrive at `~/Cloud/OneDrive/`, SharePoint at `~/Cloud/{SiteName}/{LibName}/`
- Make all 5 stub Tauri commands functional: `sign_in`, `sign_out`, `search_sites`, `list_drives`, `refresh_mount`
- Wire tray menu actions: Sign Out unmounts all + clears tokens, Quit flushes pending writes + unmounts + exits
- On relaunch with valid tokens in keyring: skip auth, mount everything silently, go straight to tray
- Auth failure degradation: if refresh token is revoked mid-session, keep mounts alive in read-only/cached mode and show "Re-authentication required" notification
- Crash recovery: on startup, scan pending-writes directory and re-queue uploads from previous session
- Graceful shutdown: flush pending writes, stop delta sync, unmount all drives, then exit

**Out of scope**: headless mode, compile/packaging/installers (separate changes).

## Capabilities

### New Capabilities

- `app-lifecycle`: Runtime orchestration — component initialization sequence (Config → Auth → Graph → Cache → DeltaSync → VFS), AppState management with all service managers, mount lifecycle (start on auth, stop on sign-out, restart on config change), graceful shutdown coordination, crash recovery of pending writes, and auth-failure degradation to read-only cached mode

### Modified Capabilities

- `tray-app`: First-run wizard generic flow changes from 5-step manual setup to streamlined auto-mount — sign in then auto-mount OneDrive with root directory selection (`~/Cloud/`), skip SharePoint at first run; sign-out behavior needs full specification (unmount all, clear tokens, revert to first-run state)
- `config-persistence`: Add `root_dir` general setting for the default mount root directory (default `Cloud`, expanded to `~/Cloud/`); new mounts derive their `mount_point` from this root

## Impact

- **Primary crate**: `filesync-app` — major expansion (~495 → ~1200+ lines), all stub commands become functional
- **Config struct change**: new `root_dir` field in `UserGeneralSettings` and `DefaultSettings` structs in `filesync-core/src/config.rs`
- **Mount point convention**: default mounts use `{home}/Cloud/...` pattern instead of `{home}/OneDrive`
- **No changes to library crates**: filesync-auth, filesync-graph, filesync-cache, filesync-vfs remain unchanged — they are the building blocks, this change wires them together
- **No breaking changes**: existing config files continue to work; `root_dir` defaults to `Cloud` when absent
