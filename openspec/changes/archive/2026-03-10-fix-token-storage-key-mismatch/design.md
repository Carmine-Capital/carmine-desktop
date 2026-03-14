## Context

OAuth2 PKCE requires the access token before the user's identity can be determined — you need the token to call Graph API to discover who the user is. This means `exchange_code` (which stores tokens) is necessarily called before `set_account_id`. The current code stores tokens under `client_id` as a fallback during exchange, then calls `set_account_id(drive_id)` afterwards. The in-memory state is updated but the on-disk/keyring storage is not migrated. On restart, `try_restore(drive_id)` finds nothing and opens the wizard.

**Current call sequence in `complete_sign_in` (commands.rs):**
```
auth.sign_in()                 → exchange_code() stores under client_id
get_my_drive()                 → first Graph call, discovers drive.id
auth.set_account_id(drive.id)  → updates in-memory only
```

**Storage backends involved:** OS keyring (primary) with AES-256-GCM encrypted file fallback (`tokens_{key}.enc`). Both are keyed by a string identifier. The migration must run against both backends transparently — `store_tokens`/`delete_tokens` in `carminedesktop-auth::storage` already handle this.

## Goals / Non-Goals

**Goals:**
- After a fresh sign-in, tokens are stored under `drive_id` before `complete_sign_in` returns.
- On restart, `try_restore(drive_id)` succeeds even if tokens were previously stored under `client_id` (repairs existing broken installs).
- Orphaned client-ID token entries are cleaned up automatically during migration.
- `set_account_id` remains a lightweight in-memory setter with no side effects.

**Non-Goals:**
- Changing the storage format or key derivation (Argon2id params, file structure).
- Supporting multiple accounts simultaneously.
- Migrating entries stored under arbitrary other keys (only client_id → drive_id path).

## Decisions

### D1 — New `finalize_sign_in` method rather than modifying `set_account_id`

**Decision:** Add `AuthManager::finalize_sign_in(id: &str) -> Result<()>` as a distinct method. `set_account_id` stays as a pure in-memory setter (used internally by `try_restore`).

**Rationale:** `set_account_id` is called in two contexts: (1) after a fresh sign-in in `commands.rs`, and (2) inside `try_restore` when loading previously stored tokens. In context (2), tokens are already under the correct key — running migration would be a no-op at best and an erroneous delete at worst if the old key happens to exist for a different reason. A dedicated `finalize_sign_in` carries clear intent: "sign-in is complete, I now know who the user is, consolidate storage."

**Alternatives considered:**
- Modify `set_account_id` with a `migrate: bool` parameter — adds noise to a simple setter, callers must remember to pass the right flag.
- Always migrate in `set_account_id` with a `old_key != new_key` guard — works but conflates two concerns and adds implicit I/O to what looks like a simple setter.

### D2 — Migration is store-then-delete, not delete-then-store

**Decision:** In `finalize_sign_in` and in the `try_restore` fallback path, always write to the new key first. Only delete the old key if the write succeeded.

**Rationale:** Prevents token loss if the storage backend fails mid-migration. The worst case on partial failure is a duplicate entry under the old key (client_id) that will be ignored on subsequent restores. This is safe — `try_restore` tries the account_id key first and only falls back if nothing is found there.

### D3 — `try_restore` fallback to `client_id` for existing broken installs

**Decision:** If `load_tokens(account_id)` returns `None`, `try_restore` attempts `load_tokens(client_id)` as a one-time migration path. If tokens are found, they are migrated and the restore proceeds normally.

**Rationale:** Without this, the fix only helps new sign-ins. All existing users who signed in before this fix would still be stuck in the re-auth loop until they sign in again manually. The fallback is cheap (one extra keyring/file lookup) and self-cleaning (migrates and removes the old entry on success).

**Alternatives considered:**
- Require users to sign in once to trigger `finalize_sign_in` migration — simpler code, but bad UX for existing users.
- One-shot migration script at startup — overkill for a two-line fallback.

## Risks / Trade-offs

- **[Risk] Storage write fails during migration** → Mitigation: store-then-delete ordering (D2). If write fails, old entry is preserved and the user sees a restore failure (wizard opens), which is the current behavior — no regression.
- **[Risk] `client_id` key collides with a legitimate `account_id`** → Mitigation: Client IDs are Azure app registration GUIDs (`8ebe3ef7-...`); drive IDs are Graph resource IDs (`b!...`). The format is structurally distinct — collision is not possible in practice.
- **[Risk] `try_restore` fallback masks unrelated auth failures** → Mitigation: Fallback only triggers when `load_tokens(account_id)` returns `Ok(None)` (token not found), not on `Err` (storage error). An actual storage error still propagates correctly.
- **[Trade-off] Two storage reads in `try_restore` on first post-fix restart** → Acceptable. This only happens once per installation (the second read finds and migrates the entry, subsequent restarts use the correct key).
