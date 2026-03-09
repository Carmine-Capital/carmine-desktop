## Context

The Windows CfApi mount path (`CfMountHandle::mount`) takes an `account_name: String` parameter that is fed directly into `SyncRootIdBuilder::account_name()`. The resulting sync root ID is `CloudMount!<SID>!<account_name>`, where `!` is the component separator. The `cloud-filter` crate validates `!` in provider_name and security_id, but **not** in account_name.

Currently, `main.rs:797` passes `drive_id.to_string()` as account_name. Microsoft Graph drive IDs for OneDrive Business and SharePoint always start with `b!`, injecting an extra separator into the sync root ID.

## Goals / Non-Goals

**Goals:**
- Fix sync root ID construction so CfApi registration succeeds on Windows
- Use a stable, deterministic account_name that uniquely identifies the mount

**Non-Goals:**
- Changing the mount path or directory structure
- Supporting re-registration migration from malformed IDs (no users affected yet)
- Fixing the `cloud-filter` crate's missing validation on `account_name`

## Decisions

### D1: Sanitize drive_id for account_name

**Choice:** Replace `!` with `_` in the drive_id before passing as account_name, reusing the existing `replace('!', "_")` pattern from `main.rs:775`.

**Alternatives considered:**
- *Use user email/display name*: Better semantically (matches CfApi docs), but requires threading user info through the mount call. The account_name has "no actual meaning" per the crate docs — it just needs to be unique and separator-free. Defer to a future UX polish change.
- *URL-encode or base64-encode*: Overengineered for removing a single problematic character.
- *Strip `b!` prefix*: Fragile — assumes a specific ID format that could change.

**Rationale:** Minimal change, proven pattern already in codebase, deterministic and reversible.

### D2: Sanitize inside `cfapi.rs`, not at call site

**Choice:** Add sanitization in `build_sync_root_id()` so any caller is protected.

**Rationale:** Defense-in-depth. The `cloud-filter` crate should validate this but doesn't. Our wrapper should be safe regardless of what the caller passes.

## Risks / Trade-offs

- **[Sync root ID change]** → If a user had somehow registered a sync root with the old (malformed) ID, they'd get a ghost registration. Not a concern: this is the first Windows test, no prior successful registrations exist. Future-proof: `unmount()` always unregisters, so ghost roots only survive crashes.
- **[Underscore collisions]** → Two drive IDs differing only in `!` position would collide after sanitization. In practice, Microsoft Graph drive IDs have a single `b!` prefix — no realistic collision risk.
