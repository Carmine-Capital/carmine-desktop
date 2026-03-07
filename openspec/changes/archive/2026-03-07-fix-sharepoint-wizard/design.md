## Context

CloudMount's backend for SharePoint is complete: `search_sites`, `list_drives`, and `add_mount` commands are all implemented, registered in `invoke_handler!`, and tested. The gap is entirely in the frontend.

Two entry points for adding a mount exist, both broken:

1. **wizard.html `step-source`** — shown after sign-in when `list_mounts` returns results (i.e., additional mounts wanted). `selectSource()` is a two-line stub that jumps straight to `step-done` regardless of what the user clicks.
2. **settings.html `addMount()`** — a one-line comment, does nothing.

The tray "Add Mount" item already calls `open_or_focus_window(app, "wizard", ...)` — so the wizard is the correct and consistent place to handle mount addition. The settings path should route there too.

`complete_sign_in` in `commands.rs` auto-adds an OneDrive mount during sign-in if none exists. This means when `step-source` is displayed, OneDrive is already configured. The OneDrive button in `step-source` should therefore add a *second* OneDrive mount (edge case, rarely useful) or be removed. The primary purpose of `step-source` post-sign-in is SharePoint.

## Goals / Non-Goals

**Goals:**
- `step-source` → SharePoint path leads to a working site-search → site-select → library-select → confirm-mount flow inside the wizard.
- `step-source` → OneDrive path works: auto-derives a mount point and calls `add_mount` with the drive ID from `list_mounts` (the existing OneDrive mount's drive ID), or skips if already mounted.
- `addMount()` in settings opens the wizard window (focus if already open, create if not).
- `step-done` always reflects the current mount list (refreshed after each `add_mount`).
- No backend changes — all required commands already exist.

**Non-Goals:**
- Mount-point path customization UI (the `add_mount` command accepts a `mount_point`; derive it automatically as the backend already does via `derive_mount_point`).
- Subfolder-within-library mount (spec scenario exists, deferred — requires additional browse step).
- Mount editing or renaming.
- Pagination of site or library search results.

## Decisions

### D1: SharePoint flow lives entirely in wizard.html as a new `step-sharepoint` DOM step

**Alternatives considered:**
- Inline modal in settings.html: duplicates UI, diverges from the tray "Add Mount" path which already routes to the wizard.
- Separate HTML file for SharePoint browser: extra webview window, cross-window communication complexity.

**Chosen:** Single new `<div id="step-sharepoint">` in wizard.html. The wizard's existing `showStep()` pattern handles navigation. Back button returns to `step-source`. This keeps all mount-addition logic in one place and reuses existing wizard chrome (CSS, Tauri bridge, event plumbing).

### D2: `step-sharepoint` implements a two-phase flow (site list → library list) in one step

The step renders two sub-panels in sequence rather than introducing two more DOM steps (`step-site` and `step-library`). State is held in JS variables (`selectedSite`). This avoids step proliferation while keeping the DOM simple.

Phase 1: Search input + site results list. User types → debounced `search_sites` call → results rendered as clickable rows.
Phase 2: Library results list + back link. After site selection → `list_drives` call → results rendered. Single library auto-selected per spec.

**Alternatives considered:**
- Two separate DOM steps: cleaner separation but more `showStep` calls and more DOM to maintain.
- Accordion in one step: harder to style consistently with existing wizard CSS.

### D3: `addMount()` in settings invokes a new `open_wizard` Tauri command

**Decision**: Register `open_wizard` as a `#[tauri::command]` in `commands.rs`. It calls `crate::tray::open_or_focus_window(&app, "wizard", "Setup", "wizard.html")` and maps any error to `String`. `addMount()` in settings calls `invoke('open_wizard')` via the standard Tauri JS bridge.

**Rationale**: `open_or_focus_window` is the single authoritative code path for opening windows in this application — it applies the `min_inner_size` constraint (added by `fix-window-state`) and the settings-refresh `win.eval()` call for the settings window. Bypassing it via the raw JS `window.__TAURI__.window.WebviewWindow` API creates a second, divergent wizard-opening path that will not pick up future behavior added to `open_or_focus_window`. The tray "Add Mount" path already goes through `open_or_focus_window`; the settings "Add Mount" path must be consistent.

The `open_wizard` command is a one-line wrapper — the concern about adding commands "for something the JS API handles" is outweighed by the correctness requirement that all wizard-open paths be identical.

**Alternatives considered:**
- `window.__TAURI__.window.WebviewWindow` JS API: bypasses `open_or_focus_window`, skips `min_inner_size` and any future per-window behavior. Tauri v2 label-deduplication behavior (focus vs. throw on duplicate label) is also underdocumented and requires a try/catch workaround. Rejected.
- `window.location` navigation to wizard.html: breaks the two-window architecture. Rejected.

### D4: OneDrive path in `step-source` calls `list_mounts`, finds the existing drive ID, derives a new mount point, and calls `add_mount`

`complete_sign_in` always adds an OneDrive mount on sign-in. If `step-source` is shown post-sign-in, OneDrive is already present. The OneDrive button therefore:
1. Calls `list_mounts` to get the existing drive's `id` (which encodes the drive ID for `add_mount`).
2. Derives a mount point label (e.g., "OneDrive 2") to avoid collision.
3. Calls `add_mount` with `mount_type: 'drive'` and the drive ID.

If OneDrive is not already mounted (edge case: `step-source` reached via settings "Add Mount" path before sign-in completes), show an error.

**Alternative:** Remove the OneDrive button from `step-source` entirely, since OneDrive is auto-configured. Rejected — the step-source spec and existing DOM both show it; removing it would require spec changes outside this change's scope.

### D5: Mount point auto-derived as `~/Cloud/<LibraryName>/` — no path input shown to user

The `add_mount` backend command accepts `mount_point` but the wizard should not ask the user to type a path — that is a power-user setting available in the Settings > Mounts tab later. The wizard derives a safe default using the same pattern as `complete_sign_in`: `~/Cloud/<SiteName> - <LibraryName>/` (truncated if needed).

The JS side constructs this string and passes it as `mount_point`. If the path is already in use, `add_mount` returns an error which is shown inline.

## Risks / Trade-offs

**[Risk] `search_sites` returns an empty list for newly provisioned tenants or tenants with no followed sites** → Mitigation: show "No results" state with message "Try searching by site name" — already required by the sharepoint-browser spec. No code risk, just a UX text.

**[Risk] `step-source` is shown both after first-run sign-in (OneDrive already added) and after "Add Mount" from settings (authenticated, mounts exist)** → Mitigation: The OneDrive button always attempts to add an additional OneDrive mount. If that is semantically wrong for a given org, the org build can remove the step-source OneDrive option via packaged defaults — this is outside this change's scope.

**[Risk] Mount point collision if two SharePoint libraries share the same display name** → Mitigation: `add_mount` backend validates and returns an error; wizard shows it inline. The user can remove one mount from Settings and retry.

## Migration Plan

Primarily frontend HTML/JS edits. One new Rust command (`open_wizard`) is added to `commands.rs` and registered in `invoke_handler!`. No database migrations, no config schema changes, no breaking changes to existing commands.

Rollout: ship updated `wizard.html` and `settings.html` with next release. No special deployment steps.

Rollback: revert the two HTML files. No data is at risk since `add_mount` only writes to the user config TOML.

## Open Questions

- Should the OneDrive button in `step-source` be hidden when OneDrive is already mounted (i.e., always after first-run sign-in)? Currently kept visible for the "add second OneDrive" edge case. Recommend leaving it visible for now and revisiting in a follow-up UX pass.
- Wizard window size: currently 800×600 (from `open_or_focus_window`). The new `step-sharepoint` needs enough vertical space for a search input + ~5 result rows. 480px max-width container with scroll handles this. No window size change needed.
