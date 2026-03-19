# Phase 3: Dashboard UI - Context

**Gathered:** 2026-03-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Deliver the full observability surface: drive status, activity feed, error log, cache usage, and offline pin health. Users see sync state, activity, errors, cache usage, and offline status at a glance — the app is transparent, not a black box. Dashboard is a new panel in the existing settings window (single UI surface). All data is already available from Phase 2's Tauri commands and event bus.

Requirements covered: DASH-01 through DASH-05, ACT-01 through ACT-05, COFF-01, COFF-02.

</domain>

<decisions>
## Implementation Decisions

### Dashboard placement & navigation
- **Dashboard is the default landing panel** — first tab in sidebar nav, active on window open. Existing tabs (General, Mounts, Offline, About) shift down in the nav order.
- **New sidebar nav item** with a dashboard icon, added before the existing General tab.
- **No sub-tabs** — single scrollable column within the dashboard panel.

### Dashboard layout
- **Stacked sections in a single scrollable column**, top to bottom:
  1. Auth degradation banner (conditional — only when auth is failing)
  2. DRIVES section — compact cards side by side
  3. Upload queue summary line (below drives)
  4. RECENT ACTIVITY section — scrollable log
  5. ERRORS section — with count badge
  6. CACHE & OFFLINE section — usage bar + pin summary
- Consistent with how General panel stacks setting rows — same scrollable column pattern.

### Auth degradation banner (DASH-04)
- **Full-width warning banner** at the very top of the dashboard panel, above the DRIVES section.
- Visible only when `auth_degraded` is true. Disappears when auth recovers.
- Includes actionable "Sign In" link/button.
- Auth degradation is a state, not an error (carried from Phase 2) — does NOT appear in the errors section.

### Drive status cards (DASH-02, DASH-03, DASH-05)
- **Compact cards side by side** — flex-wrap row, 2-3 cards fit in the ~500px content area.
- Each card shows: drive name, online/offline status dot, sync status text ("Up to date" / "Syncing N files" / "Error"), last synced time ("Last: 2m ago").
- **Semantic status dots:**
  - Green (`#22c55e`, existing `--success`) = Online, up to date
  - Amber (`#f59e0b`, new `--warning` token) = Syncing / warning
  - Red (`#ef4444`, existing `--danger`) = Error state
  - Gray (`--text-muted`) = Offline
- Cards use `--bg-elevated` background with subtle border, consistent with existing design system.

### Upload queue (ACT-01, ACT-05)
- **Summary line below drive cards:** "3 uploading, 2 queued" — aggregated across all drives.
- **Expandable disclosure** — clicking the summary line reveals per-file writeback queue with file names and status (uploading/queued). Collapsed by default.
- Hidden entirely when upload queue is empty.

### Activity feed (ACT-04)
- **Separate section from errors** — RECENT ACTIVITY with its own heading.
- **Show latest 10 entries by default**, with a "Show more" link that expands to the full buffer (up to 500 entries from Phase 2 ring buffer).
- Each entry: type tag (synced/uploaded/deleted/conflict), file name (truncated from full remote path), relative timestamp.
- Individual entries per file, tagged by type (carried from Phase 2).
- Most recent entries at top.

### Error log (ACT-02, ACT-03)
- **Separate ERRORS section** with count badge in heading: "ERRORS (3)".
- Each error entry: file name, error type, relative timestamp, actionable hint (from Phase 2).
- **Conflicts appear in the errors section** with amber left border/indicator, visually distinguished from hard errors (red). Matches Phase 2's dual-event design (ObsEvent::Error + ObsEvent::Activity for conflicts).
- Error detail level: file name, error type, timestamp, and actionable hint text — all visible without expansion.

### Cache & offline display (COFF-01, COFF-02)
- **Visual progress bar** for disk cache usage: horizontal bar showing used vs. max with text "2.1 GB / 5 GB".
- **Color thresholds on the bar:** 0-70% green, 70-90% amber, 90-100% red.
- **Pin health summary on dashboard:** "3 pins · 2 Downloaded, 1 Partial" — compact one-liner.
- **Pin health detail on Offline panel** (enhanced) — each pin gets a health badge (Downloaded/Partial/Stale) with file count (e.g., "47/52 files") alongside existing TTL expiry display.
- **Pin health badge colors** match semantic status dots: Downloaded=green, Partial=amber, Stale=red. Subtle background tint badges.
- **Minimal empty states:** Cache bar always visible (even at 0 / 5 GB). "No offline pins" in muted text when no pins exist.

### Real-time updates (DASH-01 success criterion 5)
- **Silent auto-refresh** via Tauri `listen()` events from Phase 2's event bus. No manual refresh needed.
- Dashboard values update in place — no spinner, no flash, just smooth value transitions.
- Initial data load via `Promise.all()` of Phase 2's `invoke()` commands on panel activation (same pattern as existing `settings.js init()`).

### Claude's Discretion
- Dashboard icon SVG design for sidebar nav item
- Exact card dimensions and spacing between drive cards
- CSS transition/animation details for value updates
- "Show more" interaction micro-pattern (inline expand vs. modal)
- Activity entry row layout details (icon placement, text truncation strategy)
- How the disclosure triangle for writeback queue is implemented
- Section heading icon choices (if any)
- Whether the Offline panel pin health enhancement needs a separate data fetch or reuses `get_cache_stats`

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 2 backend (data source for all dashboard sections)
- `crates/carminedesktop-app/src/commands.rs` — `get_dashboard_status`, `get_recent_errors`, `get_cache_stats`, `get_recent_activity` Tauri commands. These are the data sources for all dashboard sections.
- `crates/carminedesktop-app/src/main.rs` — `AppState` struct (holds observability state), `invoke_handler!` registration, Tauri `emit()` event setup. Event names and payload shapes for `listen()`.

### Frontend (extend these files)
- `crates/carminedesktop-app/dist/settings.html` — Existing sidebar nav + panels. Dashboard panel HTML goes here.
- `crates/carminedesktop-app/dist/settings.js` — State management pattern (`state` + `setState(patch)` + `render()`), `invoke()`/`listen()` usage, event delegation for dynamic content. Dashboard JS extends this.
- `crates/carminedesktop-app/dist/ui.js` — `showStatus()`, `formatError()` utilities. Shared across pages.
- `crates/carminedesktop-app/dist/styles.css` — Design tokens (colors, spacing, radii, shadows), component styles. Dashboard CSS goes here.

### Phase 2 context (data layer decisions)
- `.planning/phases/02-observability-infrastructure/02-CONTEXT.md` — Error buffer 100, activity buffer 500, return all/filter client-side, individual entries per file, tagged by type, auth degradation as state not error, pin health definitions (Downloaded/Partial/Stale), actionable hints per error type, in-memory only buffers.

### Requirements
- `.planning/REQUIREMENTS.md` — DASH-01 through DASH-05, ACT-01 through ACT-05, COFF-01, COFF-02 acceptance criteria.

### VFS event types (activity/error taxonomy)
- `crates/carminedesktop-vfs/src/core_ops.rs` — `VfsEvent` enum variants define the error types that appear in the error log.
- `crates/carminedesktop-vfs/src/sync_processor.rs` — `SyncMetrics` struct defines the upload queue metrics.

### CSP constraint
- `crates/carminedesktop-app/tauri.conf.json` — CSP policy: `script-src 'self'`. No inline event handlers — use `addEventListener` in `.js` files only.

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`settings.js` state pattern** — `state` object + `setState(patch)` + `render()` + per-section render functions. Dashboard adds `renderDashboard()` to the `render()` chain.
- **`ui.js` utilities** — `showStatus()` for feedback, `formatError()` for error messages. Dashboard reuses these.
- **Design tokens in `styles.css`** — Full token system: `--bg-base`, `--bg-surface`, `--bg-elevated`, `--border`, `--text-primary/secondary/muted`, `--accent`, `--success`, `--danger`, spacing scale, radii. New `--warning` token (#f59e0b) needed.
- **Event delegation pattern** — `document.querySelector('.main-content').addEventListener('click', ...)` with `data-action` attributes. Dashboard actions (Show more, expand writeback queue) use this same pattern.
- **`Promise.all()` for parallel data fetch** — `init()` already loads settings, mounts, handlers, pins in parallel. Dashboard adds its own commands to this.
- **`listen()` for backend events** — `listen('refresh-settings', ...)` pattern already exists. Dashboard subscribes to Phase 2's real-time events similarly.

### Established Patterns
- **Sidebar nav with panels** — `data-panel` attribute on nav buttons, `panel-{name}` IDs on panels, `renderNav()` toggles active state. Dashboard adds one more panel to this system.
- **Section heading style** — `<p class="section-heading">` for uppercase muted labels. Dashboard section headings use this.
- **Setting row pattern** — `.setting-row` with `.setting-label` + `.setting-control`. Pin health on Offline panel can reuse this.
- **Empty state text** — Muted italic text for empty lists (`.mount-empty`, `.pin-empty`). Dashboard empty states follow this.

### Integration Points
- **`settings.html` sidebar nav** — Add `<button class="nav-item" data-panel="dashboard">` before General. Add `<div class="panel" id="panel-dashboard">` in main content.
- **`settings.js` init()** — Add dashboard commands to `Promise.all()`. Add `renderDashboard()` to `render()`. Wire `listen()` for real-time events.
- **`settings.js` state** — Extend `state` object with dashboard data (`driveStatus`, `recentActivity`, `errors`, `cacheStats`).
- **`styles.css`** — Add `--warning` token, dashboard card styles, progress bar styles, activity/error row styles, health badge styles.
- **Offline panel enhancement** — Modify `renderOfflinePins()` to show health badges and file counts alongside existing TTL display.

</code_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. The dashboard should feel like a natural extension of the existing settings UI: same dark theme, same compact density, same interaction patterns. Think Linear/Raycast-style information density — clean, data-rich, not cluttered.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 03-dashboard-ui*
*Context gathered: 2026-03-18*
