# Org Build Guide

Build and distribute pre-configured CloudMount binaries for your organization using a private config overlay repo.

## Overview

The config overlay pattern separates source code (public) from org configuration (private):

```
github.com/nyxa/cloudmount (public)
  └── Source code, defaults.toml.example, generic CI

github.com/you/cloudmount-build (private, small)
  ├── defaults.toml          <- SharePoint mount definitions
  ├── .github/workflows/build.yml  <- Clones public repo, injects config, builds
  └── GitHub Secrets/Variables: CLIENT_ID (secret), TENANT_ID, APP_NAME
```

No fork needed. No merge conflicts. Updating is a one-line version change.

## Prerequisites

- An Azure AD app registration ([setup guide](azure-ad-setup.md))
- A private GitHub repo for your org config
- GitHub Actions enabled (free for all tiers, all platforms)

## Setup

### 1. Create a private repo

Create a new private GitHub repo (e.g., `cloudmount-build`) with:

**`defaults.toml`** -- your org mount definitions:

```toml
[tenant]
id = "contoso.onmicrosoft.com"
client_id = "your-client-id"

[branding]
app_name = "Contoso Drive"

[defaults]
auto_start = true

[[mounts]]
id = "onedrive"
name = "OneDrive"
type = "onedrive"
mount_point = "{home}/OneDrive"
enabled = true
```

### 2. Configure secrets and variables

In **Settings > Secrets and variables > Actions**:

- **Secret**: `CLOUDMOUNT_CLIENT_ID` (your Azure AD client ID)
- **Variable**: `CLOUDMOUNT_TENANT_ID` (your tenant ID)
- **Variable**: `CLOUDMOUNT_APP_NAME` (your branded name)
- **Variable**: `CLOUDMOUNT_VERSION` (version tag, e.g., `v0.1.0`)

### 3. Add workflow

Copy `docs/templates/github-build.yml` to `.github/workflows/build.yml` in your repo.

### 4. Build

Push to trigger the workflow, or run manually via the Actions tab. The workflow:

1. Clones the public CloudMount repo at the pinned version
2. Copies your `defaults.toml` into `build/`
3. Builds with `CLOUDMOUNT_CLIENT_ID` and `CLOUDMOUNT_TENANT_ID` env vars (baked in via `option_env!()`)
4. Produces branded binaries as workflow artifacts

## Multi-Platform Builds

The template produces binaries for Linux, macOS, and Windows. All three platforms are available on GitHub Actions for free. Remove any platform you don't need by deleting its entry from `matrix.include`.

## Desktop vs Headless

The templates build with `--features desktop` to produce a full GUI app with system tray, wizard, and settings UI.

To build a headless/CLI-only binary instead (e.g., for servers), remove `--features desktop` from the `cargo build` command. The headless binary runs in the background without a GUI.

## Updating to a New Version

Set `CLOUDMOUNT_VERSION` in repository variables (Settings > Secrets and variables > Actions > Variables), or edit the fallback in the workflow file:

```yaml
CLOUDMOUNT_VERSION: "v0.2.0"
```

That's it. No source code changes, no merge conflicts.

## How It Works

CloudMount resolves configuration in this order (highest priority first):

1. **CLI arguments** (`--client-id`, `--tenant-id`)
2. **Runtime env vars** (`CLOUDMOUNT_CLIENT_ID`, etc.)
3. **Build-time env vars** (baked in via `option_env!()` during `cargo build`)
4. **`build/defaults.toml`** (embedded via `include_str!()`)
5. **Built-in defaults** (placeholder client ID, "CloudMount" name)

The org build pipeline injects values at layers 3 and 4, producing a binary that works out of the box for your organization.
