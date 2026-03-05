# Azure AD App Registration

## Prerequisites

- Azure AD tenant with admin access (or ability to request admin consent)
- Access to the [Azure Portal](https://portal.azure.com)

## Steps

### 1. Register the Application

1. Navigate to **Azure Active Directory** → **App registrations** → **New registration**
2. Set the fields:
   - **Name**: `CloudMount` (or your branded name)
   - **Supported account types**: Accounts in this organizational directory only (Single tenant)
   - **Redirect URI**: Select **Public client/native (mobile & desktop)** and enter `http://localhost:8400/callback`
3. Click **Register**
4. Note the **Application (client) ID** and **Directory (tenant) ID** from the overview page

### 2. Configure API Permissions

1. Go to **API permissions** → **Add a permission** → **Microsoft Graph** → **Delegated permissions**
2. Add these permissions:
   - `User.Read` — sign-in and read user profile
   - `Files.ReadWrite.All` — read and write all files the user can access
   - `Sites.Read.All` — read SharePoint sites
   - `offline_access` — maintain access (refresh tokens)
3. Click **Grant admin consent for [your tenant]** (requires admin role)

### 3. Configure Authentication

1. Go to **Authentication**
2. Under **Advanced settings**, set **Allow public client flows** to **Yes**
3. Save

### 4. Use in CloudMount

Add the tenant ID and client ID to `build/defaults.toml`:

```toml
[tenant]
id = "your-tenant-id-here"
client_id = "your-client-id-here"
```

Build with `cargo tauri build --features desktop` to produce branded installers.

## Permissions Summary

| Permission | Type | Purpose |
|---|---|---|
| `User.Read` | Delegated | Read user profile for account display |
| `Files.ReadWrite.All` | Delegated | Mount OneDrive, read/write files |
| `Sites.Read.All` | Delegated | Browse and mount SharePoint document libraries |
| `offline_access` | Delegated | Obtain refresh tokens for background sync |
