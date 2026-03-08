# Azure AD App Registration

This document describes the official CloudMount app registration for developers contributing to or building the project.

## Official App Registration

CloudMount uses a single, shared Azure AD app registration:

- **Client ID**: `8ebe3ef7-f509-4146-8fef-c9b5d7c22252`
- **Supported account types**: Microsoft 365 org accounts and personal Microsoft accounts (`AzureADandPersonalMicrosoftAccount`)
- **Redirect URI**: `http://localhost:8400/callback` (Public client/native)

This client ID is hardcoded in `crates/cloudmount-app/src/main.rs` as `CLIENT_ID`. No build-time configuration is required.

## For Local Development

No Azure AD setup is needed for most contributors. The shared client ID works out of the box:

```bash
cargo run -p cloudmount-app -- --help
cargo run -p cloudmount-app  # headless mode
cargo run -p cloudmount-app --features desktop  # with tray UI
```

## If You Need Your Own App Registration

For forks, testing, or enterprise deployments requiring a separate app registration:

### 1. Register the Application

1. Navigate to **Azure Active Directory** → **App registrations** → **New registration**
2. Set the fields:
   - **Name**: your app name
   - **Supported account types**: Select based on your use case (see below)
   - **Redirect URI**: Select **Public client/native (mobile & desktop)** and enter `http://localhost:8400/callback`
3. Click **Register**
4. Note the **Application (client) ID** from the overview page

**Supported account types:**
- `AzureADandPersonalMicrosoftAccount` — M365 org accounts + personal MSA (recommended for public releases)
- `AzureADMyOrg` — Single tenant only

### 2. Configure API Permissions

1. Go to **API permissions** → **Add a permission** → **Microsoft Graph** → **Delegated permissions**
2. Add these permissions:
   - `User.Read` — sign-in and read user profile
   - `Files.ReadWrite.All` — read and write all files the user can access
   - `Sites.Read.All` — browse and mount SharePoint document libraries
   - `offline_access` — obtain refresh tokens for background sync
3. Click **Grant admin consent** if required by your tenant policy

### 3. Configure Authentication

1. Go to **Authentication**
2. Under **Advanced settings**, set **Allow public client flows** to **Yes**
3. Save

### 4. Use Your Client ID

Pass it at runtime — no rebuild required:

```bash
cargo run -p cloudmount-app -- --client-id <your-client-id>
# or
CLOUDMOUNT_CLIENT_ID=<your-client-id> cargo run -p cloudmount-app
```

## Permissions Summary

| Permission | Type | Purpose |
|---|---|---|
| `User.Read` | Delegated | Read user profile for account display |
| `Files.ReadWrite.All` | Delegated | Mount OneDrive, read/write files |
| `Sites.Read.All` | Delegated | Browse and mount SharePoint document libraries |
| `offline_access` | Delegated | Obtain refresh tokens for background sync |
