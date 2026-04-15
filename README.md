# Carmine Desktop

Mount Microsoft OneDrive and SharePoint document libraries as local folders on **Windows**.

## Features

- **WinFsp filesystem** — files appear as native local folders on Windows
- **SharePoint support** — browse sites, select document libraries, mount as folders
- **Multi-tier cache** — memory → SQLite → disk for fast access with offline support
- **Write-back** — edit files locally, changes sync to cloud automatically
- **Conflict detection** — concurrent edits create `.conflict` copies instead of data loss
- **System tray** — runs in background with status indicators and quick actions
- **Pre-configured builds** — organizations can distribute branded installers with pre-set mounts

## Prerequisites

| Platform | Requirement |
|---|---|
| Windows | Windows 10+ — [WinFsp driver](https://winfsp.dev/) (bundled by the installer, or download from winfsp.dev) |

## Installation

### From Installer

Download the latest Windows installer (`.exe`) from the [Releases](../../releases) page.

### From Source (on Windows)

```powershell
# Install Rust 1.85+
# https://rustup.rs/

# Install WinFsp (from https://winfsp.dev) and LLVM (for bindgen)
choco install winfsp llvm -y --params '/Developer'

# Build with the desktop GUI
cargo install tauri-cli --version "^2"
cargo tauri build --features desktop
```

## Developing on Linux/macOS

Carmine Desktop builds on Windows only. On Linux/macOS you can edit sources
and run git workflows, but `cargo build` / `cargo check` will fail because
`winfsp-sys` requires Windows. All authoritative checks (clippy, tests,
builds) run on GitHub Actions against `windows-latest`. Push to a branch
and let CI verify.

## First Run

1. Launch Carmine Desktop — a setup wizard appears
2. Click **Sign in with Microsoft** — your browser opens for authentication
3. Choose **OneDrive** or **SharePoint** as your source
4. For SharePoint: search for a site, select a document library
5. Choose a local mount point (e.g., `C:\OneDrive`)
6. Done — your files appear as a local folder

Carmine Desktop minimizes to the system tray. Right-click the tray icon for options.

## Configuration

User settings are stored at `%APPDATA%\Carmine Desktop\config.toml` and are
also accessible from the tray icon → **Settings**.

## For Organizations

Organizations can build branded, pre-configured installers. See the [Builder Guide](docs/builder-guide.md) for:

- Azure AD app registration
- `build/defaults.toml` configuration
- Automated installer builds via GitHub Actions

## License

MIT
