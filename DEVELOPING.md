# Developing CloudMount

## System Dependencies

### Linux

```bash
# FUSE filesystem support
sudo apt install libfuse3-dev pkg-config

# Tauri GUI (only needed for desktop feature)
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev
```

### macOS

```bash
brew install macfuse
```

### Windows

Windows 10 1709+ (Cloud Files API is built-in). No additional dependencies for headless mode.

For desktop builds, install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

## Azure AD Setup

You need an Azure AD app registration to authenticate. See [docs/azure-ad-setup.md](docs/azure-ad-setup.md) for step-by-step instructions.

You'll get a **Client ID** and **Tenant ID** from the registration.

## Providing Credentials

Three ways to provide your Azure AD credentials (in order of convenience):

### 1. `.env` file (recommended for development)

```bash
cp .env.example .env
# Edit .env with your Client ID and Tenant ID
```

### 2. Environment variables

```bash
export CLOUDMOUNT_CLIENT_ID="your-client-id"
export CLOUDMOUNT_TENANT_ID="your-tenant-id"
```

### 3. CLI arguments

```bash
cargo run -p cloudmount-app -- --client-id "your-client-id" --tenant-id "your-tenant-id"
```

All three methods can be combined. Precedence: CLI args > env vars > `.env` file.

## Build-time Injection

CI pipelines can bake credentials into the binary at compile time:

```bash
CLOUDMOUNT_CLIENT_ID="your-id" CLOUDMOUNT_TENANT_ID="your-tenant" cargo build -p cloudmount-app --release
```

Values set during `cargo build` are embedded via `option_env!()` and used as fallbacks when no runtime override is provided.

## Build Commands

### Headless mode (no GUI)

```bash
cargo run -p cloudmount-app
```

### Desktop mode (Tauri GUI + system tray)

```bash
cargo run -p cloudmount-app --features desktop
```

### Headless mode with desktop binary

```bash
cargo run -p cloudmount-app --features desktop -- --headless
```

### Other useful flags

```bash
cargo run -p cloudmount-app -- --help        # Show all options
cargo run -p cloudmount-app -- --log-level debug  # Verbose logging
cargo run -p cloudmount-app -- --config /path/to/config.toml  # Custom config
```

## First Run

1. The app detects no credentials and initiates OAuth sign-in
2. Your browser opens to Microsoft's login page (or the URL is printed if no display)
3. After signing in, the app discovers your OneDrive and creates a default mount
4. Config is saved to `~/.config/cloudmount/config.toml`

If you see an error about the placeholder client ID, you haven't configured credentials yet — see [Providing Credentials](#providing-credentials) above.

## Org Builds

For building pre-configured binaries for your organization, see [docs/org-build-guide.md](docs/org-build-guide.md).
