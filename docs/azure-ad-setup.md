# Azure AD App Registration

This document describes the official Carmine Desktop app registration for developers contributing to or building the project.

## Official App Registration

Carmine Desktop uses a single, shared Azure AD app registration:

- **Client ID**: `70053421-2c1b-44fe-80f8-d258d0a81133`
- **Tenant ID**: `6a658318-4ef7-4de5-a2a6-d3c1698f272a` (Carmine Capital)
- **Supported account types**: Microsoft 365 org accounts only (`AzureADMyOrg` — single tenant, Carmine Capital)
- **Redirect URI**: `http://localhost:8400/callback` (Public client/native)

Both values are hardcoded in `crates/carminedesktop-app/src/main.rs` as `CLIENT_ID` and `TENANT_ID`. No build-time configuration is required.

## For Local Development

No Azure AD setup is needed for most contributors. The shared client ID works out of the box:

```bash
cargo run -p carminedesktop-app -- --help
cargo run -p carminedesktop-app  # headless mode
cargo run -p carminedesktop-app --features desktop  # with tray UI
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
cargo run -p carminedesktop-app -- --client-id <your-client-id>
# or
CARMINEDESKTOP_CLIENT_ID=<your-client-id> cargo run -p carminedesktop-app
```

## Permissions Summary

| Permission | Type | Purpose |
|---|---|---|
| `User.Read` | Delegated | Read user profile for account display |
| `Files.ReadWrite.All` | Delegated | Mount OneDrive, read/write files |
| `Sites.Read.All` | Delegated | Browse and mount SharePoint document libraries |
| `offline_access` | Delegated | Obtain refresh tokens for background sync |
