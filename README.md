# Carmine Desktop

Mount Microsoft OneDrive and SharePoint document libraries as local folders on Linux, macOS, and Windows.

## Features

- **FUSE filesystem** (Linux/macOS) and **WinFsp** (Windows) — files appear as native local folders
- **SharePoint support** — browse sites, select document libraries, mount as folders
- **Multi-tier cache** — memory → SQLite → disk for fast access with offline support
- **Write-back** — edit files locally, changes sync to cloud automatically
- **Conflict detection** — concurrent edits create `.conflict` copies instead of data loss
- **System tray** — runs in background with status indicators and quick actions
- **Pre-configured builds** — organizations can distribute branded installers with pre-set mounts

## Prerequisites

| Platform | Requirement |
|---|---|
| Linux | FUSE 3 (`libfuse3-dev` on Debian/Ubuntu, `fuse3` on Fedora) |
| macOS | [macFUSE](https://osxfuse.github.io/) |
| Windows | Windows 10+ — [WinFsp driver](https://winfsp.dev/) (installed automatically or download from winfsp.dev) |

## Installation

### From Installer

Download the latest release for your platform from the [Releases](../../releases) page:

- **Linux**: `.deb` (Debian/Ubuntu) or `.AppImage`
- **macOS**: `.dmg`
- **Windows**: `.msi`

### From Source

```bash
# Install Rust 1.85+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build headless (no GUI)
cargo build --release -p carminedesktop-app

# Build with desktop GUI (requires Tauri CLI + system deps)
cargo install tauri-cli --version "^2"
cargo tauri build --features desktop
```

Linux build dependencies:

```bash
sudo apt-get install -y libfuse3-dev pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

## First Run

1. Launch Carmine Desktop — a setup wizard appears
2. Click **Sign in with Microsoft** — your browser opens for authentication
3. Choose **OneDrive** or **SharePoint** as your source
4. For SharePoint: search for a site, select a document library
5. Choose a local mount point (e.g., `~/OneDrive`)
6. Done — your files appear as a local folder

Carmine Desktop minimizes to the system tray. Right-click the tray icon for options.

## Configuration

User settings are stored at:

- **Linux**: `~/.config/carminedesktop/config.toml`
- **macOS**: `~/Library/Application Support/carminedesktop/config.toml`
- **Windows**: `%APPDATA%\Carmine Desktop\config.toml`

Settings are also accessible from the tray icon → **Settings**.

## For Organizations

Organizations can build branded, pre-configured installers. See the [Builder Guide](docs/builder-guide.md) for:

- Azure AD app registration
- `build/defaults.toml` configuration
- Automated installer builds via GitHub Actions

## License

MIT
