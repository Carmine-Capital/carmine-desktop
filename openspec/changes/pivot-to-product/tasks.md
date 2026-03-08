## 1. Pre-deployment prerequisite (AAD portal)

- [ ] 1.1 Update the Azure AD app registration for client ID `8ebe3ef7-f509-4146-8fef-c9b5d7c22252` — set `signInAudience` to `AzureADandPersonalMicrosoftAccount` to enable both M365 org and personal MSA accounts

## 2. Config layer cleanup (cloudmount-core)

- [x] 2.1 Delete `PackagedDefaults`, `PackagedMount`, `BrandingConfig`, `TenantConfig`, `DefaultSettings` structs from `config.rs`
- [x] 2.2 Delete `MountOverride` struct from `config.rs`
- [x] 2.3 Remove `merge_mounts()`, `has_packaged_config()`, and `strip_comment_only_toml()` functions
- [x] 2.4 Remove `mount_overrides: Vec<MountOverride>`, `dismissed_packaged_mounts: Vec<String>`, and `restore_default_mounts()` from `UserConfig`
- [x] 2.5 Remove `tenant_id`, `client_id`, and `app_name` fields from `EffectiveConfig`
- [x] 2.6 Rewrite `EffectiveConfig::build()` — remove `PackagedDefaults` parameter; merge user config directly with built-in defaults only
- [x] 2.7 Update or remove any tests in `crates/cloudmount-core/tests/` that reference `PackagedDefaults`, `MountOverride`, or the merge logic

## 3. App cleanup (cloudmount-app main.rs)

- [x] 3.1 Replace `include_str!(build/defaults.toml)` / `PACKAGED_DEFAULTS_TOML` with `const CLIENT_ID: &str = "8ebe3ef7-f509-4146-8fef-c9b5d7c22252";`
- [x] 3.2 Remove `BUILD_CLIENT_ID`, `BUILD_TENANT_ID`, `BUILD_APP_NAME` `option_env!()` constants
- [x] 3.3 Delete `resolve_client_id()` and `resolve_tenant_id()` functions; replace call sites with `args.client_id.as_deref().unwrap_or(CLIENT_ID)` and `args.tenant_id` directly
- [x] 3.4 Remove `AppState.packaged` field and all references in `run_desktop()`, `run_headless()`, and command handlers
- [x] 3.5 Remove `packaged` parameter from `init_components()` and all call chains
- [x] 3.6 Replace all `packaged.app_name()` call sites with the `"CloudMount"` literal
- [x] 3.7 Remove the `has_packaged_config()` branch from pre-flight or first-run detection logic
- [x] 3.8 Update or remove tests in `crates/cloudmount-app/` that reference `PackagedDefaults` (e.g., `test_runtime_overrides_resolve_client_id`)

## 4. Build system cleanup

- [x] 4.1 Rewrite `crates/cloudmount-app/build.rs` — remove the `defaults.toml` file-copy logic; retain only `tauri_build::build()` for the `desktop` feature
- [x] 4.2 Delete `build/defaults.toml.example` from the repository
- [x] 4.3 Remove `build/defaults.toml` from `.gitignore` (the file no longer exists and is not generated)

## 5. Wizard — step-sources implementation (frontend)

- [x] 5.1 Create `step-sources` HTML structure: OneDrive card section, SharePoint section (browser panel + added-sources list), and "Get started" button
- [x] 5.2 On sign-in completion, call `get_drive_info` and `get_followed_sites` in parallel; hide/show sections based on results (OneDrive absent if drive call fails; SharePoint section absent if followed-sites call returns error or 403)
- [x] 5.3 Render OneDrive card pre-checked with proposed mount point `~/Cloud/OneDrive`; wire checkbox to enable/disable the source
- [x] 5.4 Render followed sites as clickable rows in the SharePoint browser panel
- [x] 5.5 Implement site search: debounced input calls `search_sites({ query })`, shows loading indicator, clears and re-renders results
- [x] 5.6 Implement site row click: call `list_drives({ siteId })`, render library rows with a Back affordance
- [x] 5.7 Implement library row click: call `add_mount({ mount_type, drive_id, site_id, site_name, library_name, mount_point })` where `mount_point` is auto-derived as `~/Cloud/<site_name> - <library_name>/`; on success append entry to added-sources list and reset browser to site list
- [x] 5.8 Implement Remove button on added-sources entries: call `remove_mount({ id })` and remove entry from list
- [x] 5.9 Wire "Get started" button active state: enabled when OneDrive is checked OR added-sources list is non-empty
- [x] 5.10 On "Get started": call `complete_wizard()`, then transition to `step-success`

## 6. Wizard routing cleanup (frontend)

- [x] 6.1 Remove `step-source` HTML/JS (the "Choose OneDrive or SharePoint" interstitial screen)
- [x] 6.2 Remove `step-sharepoint` HTML/JS
- [x] 6.3 Update wizard navigation so sign-in completion routes to `step-sources` instead of `step-source`
- [x] 6.4 Remove any references to `step-source` or `step-sharepoint` from wizard routing, back-link logic, and DOM reset on sign-out

## 7. Documentation and project metadata

- [x] 7.1 Delete `docs/builder-guide.md`
- [x] 7.2 Delete `docs/org-build-guide.md`
- [x] 7.3 Delete `docs/templates/` directory and its contents
- [x] 7.4 Rewrite `docs/azure-ad-setup.md` to be contributor-focused: remove org-builder sections, document the official CloudMount app registration setup for developers
- [x] 7.5 Update `CLAUDE.md` — remove `DEFAULT_CLIENT_ID` magic number entry, replace with `CLIENT_ID` constant and its value; remove build-time env var references; update wizard step list

## 8. Verification

- [x] 8.1 `cargo build --all-targets` — zero errors
- [x] 8.2 `cargo clippy --all-targets --all-features` — zero warnings (desktop feature requires GTK dev libs, not installed in dev env)
- [x] 8.3 `cargo test --all-targets` — all tests pass
- [ ] 8.4 Manual test (M365 org account): sign in → step-sources shows OneDrive checked + SharePoint browser with followed sites → add a SharePoint library → Get started → mounts activate
- [ ] 8.5 Manual test (personal MSA account): sign in → step-sources shows OneDrive only (no SharePoint section) → Get started → OneDrive mounts
- [x] 8.6 Verify `build/defaults.toml.example` is gone and `cargo build` succeeds on a clean checkout without it
