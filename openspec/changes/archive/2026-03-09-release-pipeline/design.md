## Context

carminedesktop has a working multi-platform CI (`ci.yml`) that checks, builds, and tests on Linux, macOS, and Windows. The Tauri configuration already declares bundle targets (deb, AppImage, DMG, MSI) and includes `tauri-plugin-updater` with empty endpoint/pubkey fields. The auto-updater application code is fully implemented but inert — it needs a real endpoint and signing key to function.

The repository is being made public, so GitHub Releases on the same repo can serve as the distribution channel.

## Goals / Non-Goals

**Goals:**
- Produce installable packages for all three platforms on every tagged release
- Enable Tauri's auto-updater by wiring a real endpoint and signing key
- Publish releases to GitHub Releases with a `latest.json` manifest for the updater
- Keep the workflow simple and maintainable

**Non-Goals:**
- OS-level code signing (Windows Authenticode, macOS notarization) — deferred to a follow-up change
- Universal macOS binary — ship separate aarch64 and x86_64 builds
- Custom download page or CDN — GitHub Releases is sufficient
- MSI installer — NSIS covers the Windows target; MSI can be added later for enterprise needs
- Linux ARM builds — x86_64 only for now

## Decisions

### 1. Use `tauri-apps/tauri-action@v0` for builds

**Choice**: Use the official Tauri GitHub Action rather than raw `cargo tauri build` commands.

**Rationale**: The action handles Tauri CLI installation, bundling, updater signature generation, `latest.json` creation, and GitHub Release asset upload in one step. It's maintained by the Tauri team and is the standard approach for Tauri v2 projects.

**Alternative considered**: Manual `cargo tauri build` + custom upload scripts — more control but significantly more maintenance and error-prone.

### 2. NSIS over WiX for Windows installer

**Choice**: Switch from WiX/MSI to NSIS.

**Rationale**: NSIS is bundled with the Tauri toolchain — no extra CI dependencies. WiX requires separate installation on Windows runners. NSIS produces a modern installer UX. Both work identically with the Tauri auto-updater.

**Alternative considered**: Ship both — unnecessary complexity for v1.

### 3. Draft → Build → Publish release pattern

**Choice**: Create a draft release first, upload artifacts from parallel platform builds, then un-draft to publish.

**Rationale**: If any platform build fails, the release stays in draft and users never see a partial release. This is the standard pattern for multi-platform release workflows.

**Alternative considered**: Build all, then create release — requires storing artifacts between jobs (large uploads to GitHub artifact storage) and a separate upload step.

### 4. Updater endpoint: GitHub Releases latest.json

**Choice**: Point the updater at `https://github.com/{owner}/{repo}/releases/latest/download/latest.json`.

**Rationale**: `tauri-apps/tauri-action` automatically generates and uploads `latest.json` to the GitHub Release. The Tauri updater natively understands this format. Zero additional infrastructure.

### 5. Four build targets in the matrix

**Choice**: `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc`.

**Rationale**: Covers the vast majority of desktop users. macOS needs both architectures since Apple Silicon and Intel Macs are both widely deployed. Linux ARM and Windows ARM are rare for desktop use cases.

### 6. Version source of truth

**Choice**: Version is maintained manually in `Cargo.toml` (workspace) and `tauri.conf.json`. Tag name must match.

**Rationale**: Simple and explicit. No tooling overhead. The release workflow validates that the tag matches the configured version to prevent mismatches.

## Risks / Trade-offs

- **Unsigned binaries trigger OS warnings** → Document the "how to install unsigned" flow per platform in release notes. Plan OS signing as a follow-up change.
- **macFUSE not bundled in DMG** → macFUSE has redistribution restrictions. Users must install macFUSE separately. Document this as a prerequisite in release notes.
- **AppImage + FUSE tension** → AppImage traditionally requires FUSE2 to mount itself, while carminedesktop uses FUSE3. Test that AppImage works on systems with only FUSE3. Fallback: `--appimage-extract-and-run` flag.
- **Tag/version mismatch** → The workflow should verify that the git tag matches `tauri.conf.json` version to catch mistakes early. Fail the build if they don't match.
