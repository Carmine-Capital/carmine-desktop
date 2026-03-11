## Context

`open-in-sharepoint` already introduces the core deep-link flow (`cloudmount://open-online?path=...`) and a Linux Nautilus script. KDE users primarily interact through Dolphin, which uses Service Menus (`.desktop` metadata + command invocation) instead of Nautilus script hooks. The backend path resolution and URL opening logic already exists; this change focuses on KDE-native entry points and predictable argument handling.

## Goals / Non-Goals

**Goals:**
- Provide a native Dolphin right-click action labeled "Open in SharePoint".
- Reuse the existing deep-link handler instead of adding new Tauri commands or IPC surfaces.
- Support paths with spaces/unicode and multi-selection without breaking existing Linux behavior.
- Document Plasma 5/6 installation paths and troubleshooting expectations.

**Non-Goals:**
- Replacing or removing the Nautilus integration.
- Implementing a KDE-specific binary plugin or KIO extension.
- Changing SharePoint URL resolution logic in Rust core/app crates.
- Guaranteeing dynamic menu visibility only for CloudMount mounts in v1.

## Decisions

### 1. Dolphin integration via Service Menu, not plugin code

**Decision:** Use a `.desktop` Service Menu file for Dolphin that invokes a shell helper with selected files.

**Rationale:** Service Menus are the standard lightweight extension mechanism on KDE and avoid binary plugin complexity.

**Alternative considered:** Build a native KDE plugin/KIO extension. Rejected for v1 due to packaging complexity, ABI coupling, and larger maintenance burden.

### 2. Keep deep-link as the single invocation contract

**Decision:** The helper script percent-encodes each selected absolute path and calls `xdg-open "cloudmount://open-online?path=<encoded>"`.

**Rationale:** This reuses existing validation, mount checks, notifications, and Office/browser selection behavior from the current `open_online` flow.

**Alternative considered:** Call a new CLI endpoint directly. Rejected to avoid duplicating path resolution behavior and introducing new public execution paths.

### 3. Multi-selection is processed item-by-item

**Decision:** For multiple selected files, invoke one deep-link call per file in deterministic order.

**Rationale:** The existing deep-link contract accepts one `path` parameter. Item-by-item handling preserves compatibility and keeps failure isolation (one bad file does not block others).

**Alternative considered:** Add batch deep-link payload support. Deferred to avoid extending protocol format and parser complexity.

### 4. Linux integrations remain parallel, not unified

**Decision:** Maintain separate Nautilus and Dolphin integration assets while sharing encoding/open logic patterns.

**Rationale:** GNOME and KDE expose different extension surfaces; forcing a single integration artifact would degrade UX and increase fragile environment detection.

**Alternative considered:** A distro-agnostic generic script only. Rejected because it would not appear natively in Dolphin context menus.

### 5. Contract pinning to `open-online` deep link

**Decision:** KDE integration is strictly bound to `cloudmount://open-online?path=<encoded>` and does not introduce alternative actions.

**Rationale:** This keeps KDE entrypoints aligned with existing path validation, URL resolution, notifications, and Office/browser fallback behavior.

**Alternative considered:** A KDE-specific deep-link action or direct CLI endpoint. Rejected to avoid contract fragmentation and duplicated behavior.

## Risks / Trade-offs

- **[Risk] Service menu install path differences across distros/Plasma versions** -> Mitigation: document supported locations and include validation/troubleshooting guidance.
- **[Risk] Multiple windows/tabs opening for large multi-selection** -> Mitigation: define expected behavior and consider future batching if user feedback requires it.
- **[Risk] Menu entry appears for non-CloudMount files** -> Mitigation: rely on existing deep-link-side validation and user-visible error notification.
- **[Risk] Path quoting/encoding edge cases** -> Mitigation: require percent-encoding of full absolute path and add explicit test cases/examples for spaces and unicode.

## Migration Plan

1. Ship Service Menu and helper script alongside existing Linux assets.
2. Keep Nautilus script unchanged for backward compatibility.
3. Update docs/install notes to include KDE instructions.
4. Validate on a KDE environment that selecting the Dolphin action routes through deep-link and opens expected target.

Rollback: remove the KDE Service Menu file and helper script from packaging/docs; existing open-in-sharepoint behavior remains intact.

## Open Questions

- Should packaging install KDE assets automatically, or remain manual setup for first iteration?
- Do we want to gate the menu display to specific MIME groups (office + common docs) or keep all-files coverage?
- Is there a preferred UX for multi-selection bursts (open all immediately vs. soft limit with warning)?
