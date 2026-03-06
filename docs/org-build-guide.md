# Org Build Guide

Build and distribute pre-configured CloudMount binaries for your organization using a private config overlay repo.

## Overview

The config overlay pattern separates source code (public) from org configuration (private):

```
github.com/nyxa/cloudmount (public)
  └── Source code, defaults.toml.example, generic CI

gitlab.company.com/you/cloudmount-build (private, small)
  ├── defaults.toml          ← SharePoint mount definitions
  ├── .gitlab-ci.yml         ← Clones public repo, injects config, builds
  └── CI Variables: CLIENT_ID (masked), TENANT_ID, APP_NAME
```

No fork needed. No merge conflicts. Updating is a one-line version change.

## Prerequisites

- An Azure AD app registration ([setup guide](azure-ad-setup.md))
- A private Git repo (GitLab or GitHub) for your org config
- CI/CD with Rust toolchain support

## GitLab Setup

### 1. Create a private repo

Create a new private GitLab repo (e.g., `cloudmount-build`) with:

**`defaults.toml`** — your org mount definitions:

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

### 2. Configure CI variables

In **Settings > CI/CD > Variables**:

| Variable | Value | Options |
|----------|-------|---------|
| `CLOUDMOUNT_CLIENT_ID` | Your Azure AD client ID | Masked |
| `CLOUDMOUNT_TENANT_ID` | Your Azure AD tenant ID | |
| `CLOUDMOUNT_APP_NAME` | Your branded name | |
| `CLOUDMOUNT_VERSION` | Version tag (e.g., `v0.1.0`) | |

### 3. Add CI pipeline

Copy `docs/templates/gitlab-ci.yml` to `.gitlab-ci.yml` in your repo.

### 4. Build

Push to trigger the pipeline, or run it manually. The CI:

1. Clones the public CloudMount repo at the pinned version
2. Copies your `defaults.toml` into `build/`
3. Builds with `CLOUDMOUNT_CLIENT_ID` and `CLOUDMOUNT_TENANT_ID` env vars (baked in via `option_env!()`)
4. Produces branded binaries as pipeline artifacts

## GitHub Setup

### 1. Create a private repo

Same as GitLab — create a private repo with your `defaults.toml`.

### 2. Configure secrets and variables

In **Settings > Secrets and variables > Actions**:

- **Secret**: `CLOUDMOUNT_CLIENT_ID` (your Azure AD client ID)
- **Variable**: `CLOUDMOUNT_TENANT_ID` (your tenant ID)
- **Variable**: `CLOUDMOUNT_APP_NAME` (your branded name)
- **Variable**: `CLOUDMOUNT_VERSION` (version tag, e.g., `v0.1.0`)

### 3. Add workflow

Copy `docs/templates/github-build.yml` to `.github/workflows/build.yml` in your repo.

### 4. Build

Push to trigger the workflow, or run manually via Actions tab.

## Multi-Platform Builds

Both templates produce binaries for Linux, macOS, and Windows. Remove any platform job you don't need:

- **GitLab**: Delete the `build-linux`, `build-macos`, or `build-windows` job block
- **GitHub**: Remove the entry from `matrix.include`

macOS and Windows runners on GitLab require Premium+ (or self-hosted runners). Linux Docker runners are available on all tiers.

## Desktop vs Headless

The templates build with `--features desktop` to produce a full GUI app with system tray, wizard, and settings UI.

To build a headless/CLI-only binary instead (e.g., for servers), remove `--features desktop` from the `cargo build` command. The headless binary runs in the background without a GUI.

## Updating to a New Version

Change the version in your CI config:

- **GitLab**: Edit the `CLOUDMOUNT_VERSION` variable in `.gitlab-ci.yml`, or set it as a CI variable in Settings > CI/CD > Variables
- **GitHub**: Set `CLOUDMOUNT_VERSION` in repository variables (Settings > Secrets and variables > Actions > Variables), or edit the fallback in the workflow file

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
