## Context

The project "carminedesktop" is being rebranded to "Carmine Desktop" as it moves under the Carmine Capital organization. The codebase is a Rust 2024 workspace with 6 crates, a Tauri desktop app, and CI/CD via GitHub Actions. The rename touches every layer: crate names, Rust identifiers, environment variables, system integration points (D-Bus, keyring), Tauri config, frontend assets, build tooling, and documentation.

Simultaneously, the distribution infrastructure is being moved off public GitHub Releases to a private update server, and the Windows installer format is switching from NSIS to MSI for enterprise compatibility.

## Goals / Non-Goals

**Goals:**

- Complete identity rename from carminedesktop to Carmine Desktop across all code, config, and documentation
- Move update distribution to private infrastructure (`static.carminecapital.com`)
- Switch Windows installer to MSI (WiX) for better enterprise/GPO deployment support
- Maintain full CI passing (fmt, clippy, build, 222 tests) after all changes
- Zero functional regressions — behavior identical, only names and distribution paths change

**Non-Goals:**

- Changing any application functionality or adding new features
- Migrating user data or configuration from old to new naming (clean break)
- Setting up the private server infrastructure (server-side nginx, SSH keys — separate ops task)
- macOS code signing or notarization changes
- Renaming the GitHub repository (already done separately)

## Decisions

### D1: Crate prefix `carminedesktop-*`

**Choice**: `carminedesktop-*` (e.g., `carminedesktop-core`, `carminedesktop-vfs`)

**Alternatives considered**:
- `carmine-*` — too generic, could conflict with other Carmine packages
- `carmine-desktop-*` — double hyphen makes import paths awkward (`carmine_desktop_core`)

**Rationale**: Single compound prefix keeps import paths clean (`carminedesktop_core::Error`) and matches the product name as one word.

### D2: Environment variable prefix `CARMINEDESKTOP_*`

**Choice**: `CARMINEDESKTOP_*` (e.g., `CARMINEDESKTOP_CLIENT_ID`)

**Rationale**: Directly mirrors the crate prefix convention. Unambiguous, no collision risk.

### D3: Bundle identifier `com.carmine-capital.desktop`

**Choice**: Reverse-domain using the organization name.

**Rationale**: Standard convention for Tauri/desktop apps. Unique enough to avoid conflicts.

### D4: Update endpoint on private server

**Choice**: `https://static.carminecapital.com/carmine-desktop/latest.json`

**Alternatives considered**:
- Keep GitHub Releases — requires public repository or GitHub Pro for private release assets
- S3/CloudFront — more complex, higher cost for a single-file manifest

**Rationale**: Simple static file server under org control. rsync deployment is trivial and does not require third-party services. Tauri's updater only needs an HTTPS endpoint serving `latest.json`.

### D5: rsync/SSH for release publishing

**Choice**: Replace the GitHub Release publish job with rsync over SSH to `static.carminecapital.com`.

**Rationale**: GitHub Releases requires the repo to be public for unauthenticated download URLs in `latest.json`. A private static server with rsync is simpler than managing GitHub token auth in the updater client.

### D6: MSI (WiX) instead of NSIS

**Choice**: Switch Windows bundle target from `"nsis"` to `"msi"` with WiX.

**Alternatives considered**:
- Keep NSIS — works but NSIS installers are less trusted in enterprise environments
- WiX burn bootstrapper — overkill for a single MSI

**Rationale**: MSI is the standard Windows installer format for enterprise deployment via Group Policy. Tauri has built-in WiX support. WinFsp is already an MSI, so embedding/chaining MSIs is more natural.

### D7: Swarm-based parallel implementation

**Choice**: Use a swarm of 7 parallel worker agents to apply the rename across different file categories.

**Rationale**: The rename is embarrassingly parallel — Rust sources, frontend files, Tauri config, CI/CD, and documentation can all be modified independently. A swarm minimizes total wall-clock time.

## Risks / Trade-offs

- **[Broken imports after rename]** → Mitigated by CI enforcement (fmt + clippy + build + 222 tests all pass). Swarm workers validated each file category independently.
- **[Stale references in non-code files]** → Mitigated by grepping for `carminedesktop` and `carminedesktop` across all file types, not just `.rs`.
- **[Toolbox container name mismatch]** → The local build container is still named `carminedesktop-build`. The Makefile now references `carminedesktop-build`. User must manually rename: `toolbox rename carminedesktop-build carminedesktop-build`.
- **[Server not yet provisioned]** → The private update server requires nginx setup and `DEPLOY_SSH_KEY` GitHub secret. Until provisioned, `release.yml` will fail on publish. This is an intentional deferred ops task.
- **[MSI/NSIS hooks.nsh stale content]** → The NSIS hooks file was updated but may need removal once MSI is fully validated on Windows CI.
- **[No user data migration]** → Users of the old "carminedesktop" builds will need to re-authenticate. Keyring entries and config paths use the new name. This is acceptable for a pre-release project.
