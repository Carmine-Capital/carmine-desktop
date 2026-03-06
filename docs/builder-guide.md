# Builder Guide

Build and distribute pre-configured CloudMount installers for your organization.

## Quick Start (GitHub Actions)

1. Fork this repository
2. Go to **Actions** → **Build Installer** → **Run workflow**
3. Fill in:
   - **app_name**: Your branded name (e.g., "Contoso Drive")
   - **tenant_id**: Azure AD tenant ID (see [Azure AD Setup](azure-ad-setup.md))
   - **client_id**: Azure AD application client ID
   - **mounts_json**: Optional pre-configured mounts (see below)
4. Download installers from the workflow artifacts

## Manual Build

### Prerequisites

- Rust 1.85+ with `cargo`
- [Tauri CLI](https://v2.tauri.app/): `cargo install tauri-cli --version "^2"`
- Platform dependencies:
  - **Linux**: `libfuse3-dev`, `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
  - **macOS**: macFUSE (`brew install macfuse`)
  - **Windows**: Windows 10 1709+ (Cloud Files API is built-in)

### Configure defaults

Copy the template and edit it:

```bash
cp build/defaults.toml.example build/defaults.toml
```

Edit `build/defaults.toml`:

```toml
[tenant]
id = "your-tenant-id"
client_id = "your-client-id"

[branding]
app_name = "Contoso Drive"

[defaults]
auto_start = true
cache_max_size = "10GB"
sync_interval_secs = 60

[[mounts]]
id = "contoso-docs"
name = "Shared Documents"
type = "sharepoint"
mount_point = "{home}/Contoso Documents"
enabled = true
site_id = "contoso.sharepoint.com,guid,guid"
library_name = "Documents"
```

The `{home}` placeholder resolves to the user's home directory at runtime.

### Build

```bash
cargo tauri build --features desktop
```

Installers are written to `target/release/bundle/`.

## Pre-configured Mounts JSON

When using the GitHub Actions workflow, pass mounts as a JSON array:

```json
[
  {
    "id": "shared-docs",
    "name": "Shared Documents",
    "type": "sharepoint",
    "mount_point": "{home}/Contoso Documents",
    "enabled": true,
    "site_id": "contoso.sharepoint.com,abc,def",
    "library_name": "Documents"
  }
]
```

## What Happens at Runtime

1. **Generic build** (no `build/defaults.toml` config): User sees a full setup wizard — sign in → choose source → configure mount point
2. **Pre-configured build**: User sees a simplified wizard — branded welcome → sign in → all mounts auto-activated → done

Users can always add more mounts or change settings after the initial setup.

## Config Overlay Pattern

For automated CI builds without forking, see the [Org Build Guide](org-build-guide.md). This pattern uses a small private repo with just your `defaults.toml` and CI config — no source code to maintain.

Build-time environment variables (`CLOUDMOUNT_CLIENT_ID`, `CLOUDMOUNT_TENANT_ID`, `CLOUDMOUNT_APP_NAME`) can also inject values directly via `option_env!()`, useful for CI pipelines that manage secrets natively.
