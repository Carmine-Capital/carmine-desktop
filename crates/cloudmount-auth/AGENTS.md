# cloudmount-auth

OAuth2 PKCE flow for Microsoft identity platform. Secure token storage: OS keyring first, AES-256-GCM encrypted file fallback.

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

## ANTI-PATTERNS

- Do NOT log token values — `tracing::warn!` for errors only.
- Do NOT change Argon2 params without migration path for existing `.enc` files.
- Do NOT skip keyring attempt — always try OS keychain before file fallback.
- Do NOT store plaintext tokens anywhere.
- Do NOT change `SCOPES` without verifying Microsoft Graph API permissions.
