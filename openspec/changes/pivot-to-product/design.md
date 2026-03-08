## Context

CloudMount's config layer was built around `PackagedDefaults` — a TOML file embedded at compile time via `include_str!()` that let organizations ship pre-configured binaries with baked-in credentials and mount definitions. This model is being abandoned. CloudMount is now a multi-tenant product with official releases. The cleanup removes ~200 lines from `config.rs`, the entire `build/defaults.toml` mechanism, and several docs. The wizard is redesigned to let users self-configure their OneDrive and SharePoint mounts.

Key facts from code exploration:
- `cloudmount-auth/src/oauth.rs:17` already does `tenant_id.unwrap_or("common")` — the auth endpoint is already multi-tenant-capable
- `cloudmount-graph` already has `search_sites()`, `get_followed_sites()`, `list_site_drives()` — no new Graph calls needed
- The Tauri commands wrapping these (`get_followed_sites`, `search_sites`) were implemented by the preceding `fix-sharepoint-wizard` change
- Existing `config.toml` files with `mount_overrides`/`dismissed_packaged_mounts` fields are silently ignored by serde — no migration needed

## Goals / Non-Goals

**Goals:**
- Remove all branded-build infrastructure from code, build system, and docs
- Hardcode the official CloudMount client ID as a Rust constant
- Support personal Microsoft accounts (MSA) in addition to M365 org accounts
- Redesign the first-run wizard to let users add OneDrive and SharePoint sources themselves
- Simplify `EffectiveConfig` and `UserConfig` (remove fields that only existed to support packaged builds)

**Non-Goals:**
- No changes to `cloudmount-auth`, `cloudmount-graph`, `cloudmount-cache`, or `cloudmount-vfs`
- No new Tauri commands — the wizard uses commands from `fix-sharepoint-wizard`
- No Tauri auto-updater setup (separate concern)
- No change to how existing mounts are persisted or mounted at startup
- No pagination in the SharePoint browser — search covers the gap

## Decisions

### D1 — Client ID as a Rust constant, not config

**Decision:** Replace `include_str!(build/defaults.toml)` + `BUILD_CLIENT_ID` env var with a single constant:
```rust
const CLIENT_ID: &str = "8ebe3ef7-f509-4146-8fef-c9b5d7c22252";
```

The `--client-id` CLI argument is kept for developer/testing overrides. The `--tenant-id` CLI argument is also kept to allow testing against specific tenants during development.

**Alternatives considered:**
- Keep `option_env!(CLOUDMOUNT_CLIENT_ID)` as a build-time override — adds complexity with no benefit for a single-product model
- Move client ID to a runtime config file — wrong layer; it's an app identity, not user config

### D2 — Remove PackagedDefaults entirely, not simplify

**Decision:** Delete all structs and logic associated with `PackagedDefaults`. The only remaining value it could hold (client_id) becomes D1's constant. Keeping a vestigial struct would add confusion.

Deleted: `PackagedDefaults`, `PackagedMount`, `BrandingConfig`, `TenantConfig`, `DefaultSettings`, `MountOverride`, `merge_mounts()`, `has_packaged_config()`, `strip_comment_only_toml()`.

Removed from `UserConfig`: `mount_overrides`, `dismissed_packaged_mounts`, `restore_default_mounts()`.

### D3 — EffectiveConfig simplified to user-only fields

**Decision:** Remove `tenant_id`, `client_id`, and `app_name` from `EffectiveConfig`. These were sourced from `PackagedDefaults`; without it, they have no meaningful source. `app_name` is hardcoded as "CloudMount" where needed (e.g., notifications). `EffectiveConfig::build()` no longer takes a `PackagedDefaults` parameter.

### D4 — AAD app registration: AzureADandPersonalMicrosoftAccount

**Decision:** The AAD app registration for client ID `8ebe3ef7-f509-4146-8fef-c9b5d7c22252` must be configured with `signInAudience: AzureADandPersonalMicrosoftAccount`. This is a portal-side action, not a code change. The Rust code already uses the `common` endpoint and requires no modification.

Personal MSA users will not have SharePoint access. The wizard detects this gracefully (see D5).

### D5 — MSA detection via graceful Graph failure, not explicit account-type check

**Decision:** Do not inspect the user's `userPrincipalName` or tenant ID to determine account type. Instead, call `GET /me/followedSites` after sign-in; if it fails or returns an error, hide the SharePoint browser section entirely.

**Rationale:** Simpler, and correctly handles edge cases (M365 accounts without SharePoint license also hit this path). No special-casing needed for MSA — Graph tells us what the user has access to.

**Alternatives considered:**
- Check tenant ID against MSA tenant (`9188040d-6c67-4c5b-b112-36a304b66dad`) — fragile, one more thing to maintain
- Always show SharePoint section but show "not available" message — worse UX, confusing for personal users

### D6 — Unified step-sources wizard screen

**Decision:** Replace `step-source` + `step-sharepoint` with a single `step-sources` screen. After sign-in completion:

1. `GET /me/drive` and `GET /me/followedSites` called in parallel
2. If OneDrive detected → rendered as a pre-checked source (user can uncheck)
3. If followed sites succeed → SharePoint browser section visible with recent sites + search
4. If followed sites fail → SharePoint section absent
5. User may add multiple SharePoint libraries via the browser (click site → see libraries → click Add)
6. Added libraries accumulate in a list; each shows the proposed mount point
7. "Get started" button is active when ≥ 1 source is selected or added

The SharePoint browser opens inline (not a new wizard step). Site search uses `search_sites(query)` with a debounce. Library listing uses `list_site_drives(site_id)`.

### D7 — No Sites.Read.All scope change for MSA

**Decision:** Keep `Sites.Read.All` in the SCOPES constant. For personal MSA accounts, Graph will return a 403 on site calls — this is handled gracefully by D5. Removing the scope would break it for org accounts. Conditionally adding it based on account type is unnecessary complexity.

## Risks / Trade-offs

- **AAD registration config is a prerequisite** → The code changes work without it, but MSA sign-in will fail until the portal is updated. `docs/azure-ad-setup.md` must be updated to reflect the new audience setting. This is the only deployment gate.

- **Users with mount_overrides lose customizations** → Effectively zero users (branded build feature). No mitigation needed. Serde silently drops unknown fields so config files don't break.

- **Personal MSA users with SharePoint access via guest** → A personal account can be a guest in an org SharePoint. `get_followed_sites` will likely return empty for these users even though they do have some SP access. Mitigation: the search box is always shown first in the browser panel, allowing them to find sites manually regardless.

- **`step-sources` is more complex to implement than the current split steps** → Offset by removing two steps entirely and the wizard being one screen shorter overall.

## Migration Plan

1. **Pre-deployment (portal):** Update AAD app registration `signInAudience` to `AzureADandPersonalMicrosoftAccount`
2. **Phase 1 — Rust cleanup (no user-visible change):**
   - Delete `PackagedDefaults` and related structs/functions from `config.rs`
   - Remove `mount_overrides`, `dismissed_packaged_mounts` from `UserConfig`
   - Simplify `EffectiveConfig` and its `build()` method
   - Replace `include_str!` + `BUILD_*` constants in `main.rs` with `CLIENT_ID` constant
   - Simplify `AppState` (remove `packaged` field)
   - Simplify `build.rs`
3. **Phase 2 — Wizard (user-visible):**
   - Implement `step-sources` HTML/JS
   - Wire up parallel Graph calls on sign-in completion
   - Remove `step-source` and `step-sharepoint` routing
   - Update wizard navigation logic
4. **Phase 3 — Docs + cleanup:**
   - Delete `docs/builder-guide.md`, `docs/org-build-guide.md`, `docs/templates/`
   - Update `docs/azure-ad-setup.md` (contributor-focused)
   - Delete `build/defaults.toml.example`

Rollback: all changes are in a single branch; revert is a git revert. No database migrations, no schema changes in persistent storage.

## Open Questions

- None — decisions above are sufficient to proceed to implementation.
