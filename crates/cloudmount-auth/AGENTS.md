# cloudmount-auth

OAuth2 PKCE flow for Microsoft identity platform. Secure token storage: OS keyring first, AES-256-GCM encrypted file fallback.

## STRUCTURE

```
src/
├── lib.rs       # Re-exports AuthManager
├── manager.rs   # AuthManager — token lifecycle, sign-in/out, auto-refresh
├── oauth.rs     # PKCE flow, local HTTP callback server, token exchange/refresh
└── storage.rs   # Token persistence: keyring + encrypted file fallback
tests/
└── auth_integration.rs  # Serialization roundtrips, PKCE validation, expiry detection
```

## WHERE TO LOOK

| Task | File | Notes |
|------|------|-------|
| Add OAuth scope | `oauth.rs` → `SCOPES` constant | Space-separated string |
| Change refresh buffer | `manager.rs` → `access_token()` | Currently 5min before expiry |
| Change auth timeout | `oauth.rs` → `wait_for_callback` | Currently 120s |
| Modify encryption | `storage.rs` → `store_tokens_encrypted` | AES-256-GCM + Argon2id |
| Add storage backend | `storage.rs` | Follow keyring → fallback pattern |

## AUTH FLOW

1. `sign_in()` → `run_pkce_flow()`:
   - Generate PKCE verifier + SHA-256 challenge
   - Bind `127.0.0.1:0` (random port) for redirect callback
   - Open browser to `login.microsoftonline.com`
   - Wait 120s for callback with auth code
2. `exchange_code()` → POST code + verifier to token endpoint
3. Store tokens: try keyring, fall back to encrypted file
4. `access_token()` → return cached if valid (5min buffer), else `refresh()`

## SECURITY

- **Key derivation**: Argon2id — 64KB memory, 3 iterations, from machine-specific password.
- **Encryption**: AES-256-GCM, random 12-byte nonce per write.
- **Storage format**: `[16-byte salt][12-byte nonce][ciphertext]`.
- **Machine password**: `cloudmount-fallback-{USER}@{config_dir}` — tied to user+machine.
- **Zeroization**: `zeroize` crate for key material.
- **Keyring service name**: `cloudmount`.
- **Token path**: `{config_dir}/cloudmount/tokens_{account_id}.enc`.

## CONVENTIONS

- `AuthState` behind `RwLock` — read lock for token check, write lock for refresh/exchange.
- `invalid_grant` in refresh response → specific error message ("re-authentication required").
- Tenant defaults to `common` if not specified (multi-tenant).
- Browser opened via `open::that()` crate.

## ANTI-PATTERNS

- Do NOT log token values — `tracing::warn!` for errors only.
- Do NOT change Argon2 params without migration path for existing `.enc` files.
- Do NOT skip keyring attempt — always try OS keychain before file fallback.
- Do NOT store plaintext tokens anywhere.
- Do NOT change `SCOPES` without verifying Microsoft Graph API permissions.
