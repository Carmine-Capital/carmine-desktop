# Phase 3: Dashboard UI - Research

**Researched:** 2026-03-18
**Domain:** Vanilla JS frontend — Tauri WebView dashboard panel (no framework, no build step)
**Confidence:** HIGH

## Summary

Phase 3 is a purely frontend phase. The entire data layer was built in Phase 2: four Tauri commands (`get_dashboard_status`, `get_recent_errors`, `get_activity_feed`, `get_cache_stats`) and a real-time event bus (`obs-event` via `app.emit()`). The frontend already has an established state management pattern (`state` + `setState(patch)` + `render()`), sidebar navigation (`data-panel`/`panel-{name}`), event delegation (`data-action`), and a complete design token system in CSS custom properties.

The work is: add one new sidebar nav item (Dashboard, before General), one new panel div, one new `renderDashboard()` function, CSS styles for cards/bars/lists, and `listen()` subscriptions for real-time updates. No new Rust code is needed except possibly enhancing the Offline panel's `renderOfflinePins()` to show health badges.

**Primary recommendation:** Follow the existing `settings.js` patterns exactly. Add dashboard state fields, a `renderDashboard()` function, extend `init()` with `Promise.all()` for the 4 new commands, and wire `listen('obs-event', ...)` for incremental updates. All CSS uses existing design tokens plus one new `--warning` token.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- **Dashboard is the default landing panel** -- first tab in sidebar nav, active on window open. Existing tabs (General, Mounts, Offline, About) shift down in the nav order.
- **New sidebar nav item** with a dashboard icon, added before the existing General tab.
- **No sub-tabs** -- single scrollable column within the dashboard panel.
- **Stacked sections in a single scrollable column**, top to bottom: Auth degradation banner (conditional), DRIVES section (compact cards side by side), Upload queue summary line, RECENT ACTIVITY section, ERRORS section (with count badge), CACHE & OFFLINE section (usage bar + pin summary).
- **Full-width warning banner** at the very top of the dashboard panel for auth degradation (DASH-04). Visible only when `auth_degraded` is true. Includes actionable "Sign In" link/button.
- **Compact cards side by side** for drive status (flex-wrap row, 2-3 cards in ~500px). Each card: drive name, online/offline status dot, sync status text, last synced time.
- **Semantic status dots**: Green (`#22c55e`, `--success`) = Online up to date; Amber (`#f59e0b`, new `--warning` token) = Syncing/warning; Red (`#ef4444`, `--danger`) = Error; Gray (`--text-muted`) = Offline.
- **Upload queue summary line below drive cards**: "3 uploading, 2 queued". Expandable disclosure reveals per-file writeback queue. Collapsed by default. Hidden when empty.
- **Activity feed**: Separate section, latest 10 entries by default, "Show more" link expands to full buffer (up to 500). Each entry: type tag, file name (truncated), relative timestamp.
- **Error log**: Separate ERRORS section with count badge in heading. Each error: file name, error type, relative timestamp, actionable hint. Conflicts appear with amber left border, distinct from hard errors (red).
- **Cache usage**: Visual progress bar with text "2.1 GB / 5 GB". Color thresholds: 0-70% green, 70-90% amber, 90-100% red.
- **Pin health summary on dashboard**: "3 pins . 2 Downloaded, 1 Partial" compact one-liner.
- **Pin health detail on Offline panel** (enhanced): each pin gets health badge + file count alongside existing TTL display.
- **Pin health badge colors**: Downloaded=green, Partial=amber, Stale=red. Subtle background tint badges.
- **Minimal empty states**: Cache bar always visible (even at 0). "No offline pins" in muted text.
- **Silent auto-refresh** via Tauri `listen()` events from Phase 2's event bus. No manual refresh, no spinner.
- **Initial data load** via `Promise.all()` of Phase 2's `invoke()` commands on panel activation (same as existing `init()` pattern).

### Claude's Discretion
- Dashboard icon SVG design for sidebar nav item
- Exact card dimensions and spacing between drive cards
- CSS transition/animation details for value updates
- "Show more" interaction micro-pattern (inline expand vs. modal)
- Activity entry row layout details (icon placement, text truncation strategy)
- How the disclosure triangle for writeback queue is implemented
- Section heading icon choices (if any)
- Whether the Offline panel pin health enhancement needs a separate data fetch or reuses `get_cache_stats`

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope.

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DASH-01 | Dashboard view showing sync state, activity, errors, cache usage, offline status at a glance | Dashboard panel with stacked sections; data from `get_dashboard_status`, `get_recent_errors`, `get_activity_feed`, `get_cache_stats` |
| DASH-02 | Per-drive sync status indicator ("Up to date" / "Syncing N files" / "Error") | `DriveStatus.syncState` + `DriveStatus.uploadQueue` from `get_dashboard_status`; drive cards with status text |
| DASH-03 | Online/offline status indicator per drive | `DriveStatus.online` boolean from `get_dashboard_status`; semantic status dot (green/gray) |
| DASH-04 | Auth degraded banner when token refresh failing | `DashboardStatus.authDegraded` boolean; conditional full-width banner with "Sign In" action |
| DASH-05 | Last synced timestamp per drive updating in real time | `DriveStatus.lastSynced` ISO 8601 string; relative time formatting + real-time ObsEvent updates |
| ACT-01 | Upload queue count ("3 uploading, 2 queued") | `DriveStatus.uploadQueue.inFlight` + `queueDepth` aggregated across drives; summary line |
| ACT-02 | Recent errors with actionable detail | `DashboardError` entries from `get_recent_errors`: fileName, errorType, timestamp, actionHint |
| ACT-03 | Conflict notifications surfaced in UI | Conflicts are `DashboardError` entries with `errorType: "conflict"`; amber left border in errors section |
| ACT-04 | Recent activity feed (synced, uploaded, deleted, conflicts) | `ActivityEntry` list from `get_activity_feed`: filePath, activityType, timestamp |
| ACT-05 | Writeback queue detail (file names, not just count) | `CacheStatsResponse.writebackQueue` from `get_cache_stats`: per-file WritebackEntry with fileName |
| COFF-01 | Cache disk usage display ("2.1 GB / 5 GB") | `CacheStatsResponse.diskUsedBytes` + `diskMaxBytes`; visual progress bar with color thresholds |
| COFF-02 | Offline pin health status (Downloaded/Partial/Stale) | `CacheStatsResponse.pinnedItems` with PinHealthInfo: status, totalFiles, cachedFiles; dashboard summary + Offline panel detail |

</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Vanilla JS | ES2020+ | All dashboard logic | Project convention: no framework, no build step, Tauri WebView |
| CSS Custom Properties | N/A | Design tokens, theming | Already established in `styles.css` with full token system |
| Tauri IPC | v2 (bundled) | `invoke()` for commands, `listen()` for events | Already used by `settings.js`; `window.__TAURI__.core.invoke()` and `window.__TAURI__.event.listen()` |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| None | N/A | N/A | Zero new dependencies -- project decision. All capabilities exist in the platform. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Vanilla JS DOM | Lit, Preact, Svelte | Project explicitly chose vanilla JS with no build step; adding a framework would be a fundamental architectural change |
| CSS Custom Props | Tailwind, CSS modules | Existing design system is already well-structured with tokens; adding tooling adds build complexity |

**Installation:**
```bash
# No installation needed -- zero new dependencies
```

## Architecture Patterns

### Existing File Structure (extend, don't add)
```
crates/carminedesktop-app/dist/
  settings.html      # Add dashboard nav item + panel div
  settings.js        # Add dashboard state, render, init, listen
  styles.css         # Add --warning token, card/bar/list styles
  ui.js              # No changes needed (showStatus, formatError reused)
```

### Pattern 1: State + Render Cycle (existing, extend)
**What:** Centralized state object, `setState(patch)` triggers full re-render via `render()` which calls per-section render functions.
**When to use:** Every data update (initial load, real-time event, user interaction).
**Example:**
```javascript
// Source: settings.js (existing pattern)
const state = {
  // ... existing fields ...
  dashboardStatus: null,  // DashboardStatus from get_dashboard_status
  recentActivity: [],     // ActivityEntry[] from get_activity_feed
  recentErrors: [],       // DashboardError[] from get_recent_errors
  cacheStats: null,       // CacheStatsResponse from get_cache_stats
  activityExpanded: false, // Whether "Show more" is active
  writebackExpanded: false, // Whether upload queue disclosure is open
};

function render() {
  renderNav();
  renderSettings();
  renderMounts();
  renderHandlers();
  renderOfflinePins();
  renderDashboard();  // NEW
}
```

### Pattern 2: Event Delegation for Dynamic Content (existing, extend)
**What:** Single click listener on `.main-content` with `data-action` attributes on interactive elements.
**When to use:** Dashboard actions like "Show more", expand writeback queue, "Sign In" from auth banner.
**Example:**
```javascript
// Source: settings.js (existing delegation pattern)
document.querySelector('.main-content').addEventListener('click', async (e) => {
  const target = e.target.closest('[data-action]');
  if (!target) return;
  const action = target.dataset.action;
  // existing actions...
  if (action === 'toggle-activity-expanded') { /* ... */ }
  else if (action === 'toggle-writeback-expanded') { /* ... */ }
  else if (action === 'dashboard-sign-in') { /* ... */ }
});
```

### Pattern 3: Real-time Event Subscription (existing, extend)
**What:** `listen('event-name', callback)` subscribes to Tauri backend events. Callback updates state incrementally.
**When to use:** Dashboard real-time updates from `obs-event`.
**Example:**
```javascript
// Source: settings.js (existing listen pattern)
listen('obs-event', (event) => {
  const payload = event.payload;
  switch (payload.type) {
    case 'syncStateChanged':
      // Update specific drive's sync state in dashboardStatus
      break;
    case 'onlineStateChanged':
      // Update specific drive's online status
      break;
    case 'authStateChanged':
      // Update auth_degraded banner visibility
      break;
    case 'error':
      // Prepend to recentErrors array
      break;
    case 'activity':
      // Prepend to recentActivity array
      break;
  }
  render();
});
```

### Pattern 4: Parallel Data Fetch on Init (existing, extend)
**What:** `Promise.all()` fetches all data in parallel during init.
**When to use:** Dashboard panel activation and initial page load.
**Example:**
```javascript
// Source: settings.js init() (existing pattern)
const [settings, mounts, handlers, offlinePins, dashboardStatus, recentErrors, recentActivity, cacheStats] = await Promise.all([
  invoke('get_settings'),
  invoke('list_mounts'),
  invoke('get_file_handlers'),
  invoke('list_offline_pins'),
  invoke('get_dashboard_status'),     // NEW
  invoke('get_recent_errors'),        // NEW
  invoke('get_activity_feed'),        // NEW
  invoke('get_cache_stats'),          // NEW
]);
```

### Anti-Patterns to Avoid
- **Separate JS file for dashboard:** The existing codebase uses a single `settings.js` for all panels. Creating `dashboard.js` would break the shared state model. Keep it in `settings.js`.
- **Inline event handlers:** CSP `script-src 'self'` blocks `onclick="..."`. Always use `addEventListener` or event delegation.
- **Polling for updates:** Never use `setInterval` to poll commands. Use `listen('obs-event', ...)` for push-based updates from the backend event bus.
- **Full re-fetch on every event:** For real-time events, update state incrementally (patch the specific drive/error/activity). Only re-fetch full data on panel switch or init.
- **innerHTML with user data:** Activity file paths and error messages come from the backend. Use `textContent` for user-supplied strings, `createElement` for DOM construction (existing pattern in `renderMounts`, `renderOfflinePins`).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Relative time formatting | Custom date parser | Simple `formatRelativeTime(isoString)` helper with Math.floor division | ISO 8601 strings from backend; only need "2m ago", "1h ago", "3d ago" precision |
| Byte formatting | Manual division chains | `formatBytes(bytes)` helper (KB/MB/GB with 1 decimal) | Used for cache bar display and pin file counts |
| File path truncation | Complex path splitting | `truncatePath(fullPath, maxLen)` that shows `.../{folder}/{file}` | Activity entries have full remote paths; display needs last 2 segments |
| Progress bar with thresholds | CSS gradient tricks | Simple div-in-div with `style.width` percentage + class swap for color | Three color states (green/amber/red) based on percentage thresholds |

**Key insight:** Every UI component in this phase is a rendering function that reads from `state` and builds DOM elements. No complex state machines, no async flows beyond init fetch and event subscription.

## Common Pitfalls

### Pitfall 1: Default Panel Not Set to Dashboard
**What goes wrong:** `activePanel` defaults to `'general'` in the state initializer, so the dashboard isn't the landing panel.
**Why it happens:** The existing code has `activePanel: 'general'` hardcoded.
**How to avoid:** Change the initial state to `activePanel: 'dashboard'` AND ensure the HTML has `class="nav-item active"` on the dashboard nav button (not general) AND `class="panel active"` on `panel-dashboard` (not `panel-general`).
**Warning signs:** Opening settings window shows General panel instead of Dashboard.

### Pitfall 2: CSP Violation from Inline Handlers
**What goes wrong:** Browser console shows CSP error, interactive elements don't work.
**Why it happens:** Using `onclick="..."` or `<a href="javascript:...">` in HTML. CSP is `script-src 'self'`.
**How to avoid:** All interactivity through `addEventListener` in JS files only. Use `data-action` attributes for delegation.
**Warning signs:** Console error: "Refused to execute inline event handler because it violates... Content Security Policy".

### Pitfall 3: Race Condition on Real-time Updates
**What goes wrong:** An `obs-event` arrives during initial `Promise.all()` fetch, state gets overwritten.
**Why it happens:** `listen()` is set up after or during `init()` and the callback calls `setState` which triggers `render()` while initial data is still loading.
**How to avoid:** Set up `listen()` AFTER the initial `Promise.all()` resolves and `setState` is called. Or guard the listener to merge incrementally into existing state rather than overwrite.
**Warning signs:** Dashboard briefly shows data then goes blank, or shows stale data after receiving events.

### Pitfall 4: Render Performance with Full Re-render
**What goes wrong:** Dashboard flickers or feels sluggish because every `setState` triggers `render()` which rebuilds all panels.
**Why it happens:** The existing pattern re-renders ALL panels on every state change (Nav, Settings, Mounts, Handlers, OfflinePins + now Dashboard).
**How to avoid:** Keep `renderDashboard()` cheap: use DOM diffing only where needed (update text content rather than rebuilding entire lists). For high-frequency events, batch updates or debounce render.
**Warning signs:** Noticeable flicker when events fire every 1-2 seconds.

### Pitfall 5: Stale Relative Timestamps
**What goes wrong:** "Last synced: 2m ago" never updates unless new data arrives.
**Why it happens:** Relative timestamps are computed once at render time from the ISO 8601 string. Time passes but render isn't triggered.
**How to avoid:** Set a 30-60 second `setInterval` that re-renders the dashboard panel to update relative times. Or use a lightweight timer that only updates timestamp elements.
**Warning signs:** "Last synced: 1m ago" stays frozen for 10 minutes.

### Pitfall 6: Activity/Error Array Growing Unbounded in Client State
**What goes wrong:** Real-time events keep prepending to `state.recentActivity` and `state.recentErrors` arrays without bound.
**Why it happens:** The `listen('obs-event')` handler adds entries but never trims.
**How to avoid:** Cap client-side arrays at the same limits as backend buffers (500 for activity, 100 for errors). When a new event arrives, prepend and `slice(0, limit)`.
**Warning signs:** Memory usage grows over long sessions; rendering slows down.

### Pitfall 7: ObsEvent Payload Shape Mismatch
**What goes wrong:** Frontend reads `event.payload.syncState` but the actual field is `event.payload.state`.
**Why it happens:** Rust serde `rename_all = "camelCase"` applies to struct fields but the `#[serde(tag = "type")]` enum uses variant names as the discriminator. The `ObsEvent::SyncStateChanged` variant has a field called `state` (not `syncState`), and `drive_id` is renamed to `driveId` by per-field `#[serde(rename)]`.
**How to avoid:** Reference the exact Rust struct definitions in `types.rs`. The JSON shapes are:
- `{ type: "syncStateChanged", driveId: "...", state: "syncing"|"up_to_date"|"error" }`
- `{ type: "onlineStateChanged", driveId: "...", online: true|false }`
- `{ type: "authStateChanged", degraded: true|false }`
- `{ type: "error", driveId: null|"...", fileName: null|"...", remotePath: null|"...", errorType: "...", message: "...", actionHint: null|"...", timestamp: "..." }`
- `{ type: "activity", driveId: "...", filePath: "...", activityType: "uploaded"|"synced"|"deleted"|"conflict", timestamp: "..." }`
**Warning signs:** Dashboard doesn't update on events; console shows undefined field access.

## Code Examples

### Dashboard JSON Response Shapes (from Phase 2 Rust types)

#### `invoke('get_dashboard_status')` returns:
```json
{
  "drives": [
    {
      "driveId": "b!abc...",
      "name": "OneDrive",
      "mountPoint": "/home/user/Cloud/OneDrive",
      "online": true,
      "lastSynced": "2026-03-18T14:30:00Z",
      "syncState": "up_to_date",
      "uploadQueue": {
        "queueDepth": 0,
        "inFlight": 0,
        "failedCount": 0,
        "totalUploaded": 42,
        "totalFailed": 1
      }
    }
  ],
  "authenticated": true,
  "authDegraded": false
}
```

#### `invoke('get_recent_errors')` returns:
```json
[
  {
    "driveId": "b!abc...",
    "fileName": "report.xlsx",
    "remotePath": "/Documents/Reports/report.xlsx",
    "errorType": "conflict",
    "message": "Conflict detected: report.xlsx renamed to report (conflict 2026-03-18).xlsx",
    "actionHint": "Both versions kept. Review the conflict copy.",
    "timestamp": "2026-03-18T14:25:00Z"
  }
]
```

#### `invoke('get_activity_feed')` returns:
```json
[
  {
    "driveId": "b!abc...",
    "filePath": "/Documents/Reports/Q4.xlsx",
    "activityType": "synced",
    "timestamp": "2026-03-18T14:30:00Z"
  }
]
```

#### `invoke('get_cache_stats')` returns:
```json
{
  "diskUsedBytes": 2254857830,
  "diskMaxBytes": 5368709120,
  "memoryEntryCount": 1234,
  "pinnedItems": [
    {
      "driveId": "b!abc...",
      "itemId": "01ABC...",
      "folderName": "Reports",
      "status": "downloaded",
      "totalFiles": 52,
      "cachedFiles": 52,
      "pinnedAt": "2026-03-17T10:00:00Z",
      "expiresAt": "2026-03-24T10:00:00Z"
    }
  ],
  "writebackQueue": [
    {
      "driveId": "b!abc...",
      "itemId": "01DEF...",
      "fileName": "draft.docx"
    }
  ]
}
```

### Relative Time Formatting Helper
```javascript
// Source: Common pattern for dashboard time displays
function formatRelativeTime(isoString) {
  if (!isoString) return 'Never';
  const now = Date.now();
  const then = new Date(isoString).getTime();
  const diffSec = Math.floor((now - then) / 1000);
  if (diffSec < 60) return 'Just now';
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return diffMin + 'm ago';
  const diffHr = Math.floor(diffMin / 60);
  if (diffHr < 24) return diffHr + 'h ago';
  const diffDay = Math.floor(diffHr / 24);
  return diffDay + 'd ago';
}
```

### Byte Formatting Helper
```javascript
// Source: Common pattern for file size display
function formatBytes(bytes) {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = bytes / Math.pow(1024, i);
  return (i === 0 ? val : val.toFixed(1)) + ' ' + units[i];
}
```

### Sidebar Navigation Extension
```html
<!-- Source: settings.html existing nav pattern -->
<!-- Dashboard nav item inserted BEFORE General -->
<button class="nav-item active" data-panel="dashboard" role="tab" tabindex="0"
        aria-selected="true" aria-controls="panel-dashboard" id="tab-dashboard">
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor"
       stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
    <!-- Dashboard/grid icon (Claude's discretion) -->
    <rect x="3" y="3" width="7" height="7" rx="1"/>
    <rect x="14" y="3" width="7" height="7" rx="1"/>
    <rect x="3" y="14" width="7" height="7" rx="1"/>
    <rect x="14" y="14" width="7" height="7" rx="1"/>
  </svg>
  Dashboard
</button>
<!-- General changes to: remove 'active' class, data-panel="general", tabindex="-1", aria-selected="false" -->
```

### Drive Status Card DOM Construction
```javascript
// Source: Follows renderMounts() pattern from settings.js
function createDriveCard(drive) {
  const card = document.createElement('div');
  card.className = 'drive-card';

  // Status dot
  const dot = document.createElement('span');
  dot.className = 'status-dot';
  if (!drive.online) dot.classList.add('offline');
  else if (drive.syncState === 'error') dot.classList.add('error');
  else if (drive.syncState === 'syncing') dot.classList.add('syncing');
  else dot.classList.add('ok');

  // Drive name
  const name = document.createElement('div');
  name.className = 'drive-card-name';
  name.textContent = drive.name;

  // Sync status text
  const status = document.createElement('div');
  status.className = 'drive-card-status';
  status.textContent = formatSyncStatus(drive);

  // Last synced
  const lastSync = document.createElement('div');
  lastSync.className = 'drive-card-last-sync';
  lastSync.textContent = 'Last: ' + formatRelativeTime(drive.lastSynced);

  card.appendChild(dot);
  card.appendChild(name);
  card.appendChild(status);
  card.appendChild(lastSync);
  return card;
}
```

### CSS Token Extension
```css
/* Source: styles.css existing token system */
:root {
  /* ... existing tokens ... */
  --warning: #f59e0b;  /* NEW: Amber for syncing/partial/conflict states */
}
```

### Real-time Event Handling
```javascript
// Source: Extends existing listen('refresh-settings') pattern
listen('obs-event', (event) => {
  const p = event.payload;
  const ds = state.dashboardStatus;
  if (!ds) return;

  switch (p.type) {
    case 'syncStateChanged': {
      const drive = ds.drives.find(d => d.driveId === p.driveId);
      if (drive) drive.syncState = p.state;
      break;
    }
    case 'onlineStateChanged': {
      const drive = ds.drives.find(d => d.driveId === p.driveId);
      if (drive) drive.online = p.online;
      break;
    }
    case 'authStateChanged': {
      ds.authDegraded = p.degraded;
      break;
    }
    case 'error': {
      state.recentErrors.unshift({
        driveId: p.driveId,
        fileName: p.fileName,
        remotePath: p.remotePath,
        errorType: p.errorType,
        message: p.message,
        actionHint: p.actionHint,
        timestamp: p.timestamp,
      });
      if (state.recentErrors.length > 100) state.recentErrors.length = 100;
      break;
    }
    case 'activity': {
      state.recentActivity.unshift({
        driveId: p.driveId,
        filePath: p.filePath,
        activityType: p.activityType,
        timestamp: p.timestamp,
      });
      if (state.recentActivity.length > 500) state.recentActivity.length = 500;
      break;
    }
  }
  render();
});
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No dashboard | Phase 2 data layer + Phase 3 UI | This milestone | Users gain full observability into sync state |
| `refresh-settings` event only | `obs-event` broadcast with typed variants | Phase 2 | Fine-grained real-time updates instead of full refresh |
| No offline health | PinStore.health() with recursive CTE | Phase 2 | Pin health computed on demand, no background overhead |

**Existing code that doesn't need changes:**
- `ui.js` -- `showStatus()` and `formatError()` remain as-is
- `commands.rs` -- All 4 dashboard commands already registered
- `observability.rs` -- Event bridge already running
- `main.rs` -- `AppState`, `invoke_handler!`, event bus all ready

## Open Questions

1. **Offline panel pin health data source**
   - What we know: `get_cache_stats` returns `pinnedItems` with health. `list_offline_pins` returns basic pin info without health.
   - What's unclear: Should the Offline panel's enhanced display call `get_cache_stats` (which also fetches disk usage, writeback, etc.) or should there be a lighter endpoint?
   - Recommendation: Reuse `get_cache_stats` -- the payload is small (a few KB at most). Store `cacheStats.pinnedItems` in state and read it from `renderOfflinePins()`. Avoids adding new Rust commands.

2. **Render debouncing for high-frequency events**
   - What we know: During a large delta sync, 150+ activity events can fire in quick succession. Each triggers `render()`.
   - What's unclear: Whether 150 rapid DOM rebuilds will cause visible jank in the WebView.
   - Recommendation: Add a simple `requestAnimationFrame` debounce around `render()` calls from the event listener. This batches multiple events into a single frame.

3. **Timestamp refresh interval**
   - What we know: Relative timestamps ("2m ago") become stale if no events trigger re-render.
   - What's unclear: What refresh interval balances accuracy with performance.
   - Recommendation: 60-second `setInterval` that only updates timestamp text nodes (not full re-render). Stops when dashboard panel is not active.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Manual browser console verification (WebView) |
| Config file | None -- frontend has no automated test framework |
| Quick run command | `make build` (verifies Rust compiles; frontend is static assets) |
| Full suite command | `make clippy && make build` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DASH-01 | Dashboard panel visible on window open | manual-only | Visual: open settings window, verify dashboard is default panel | N/A |
| DASH-02 | Per-drive sync status text | manual-only | Console: `invoke('get_dashboard_status')` then verify cards show correct text | N/A |
| DASH-03 | Online/offline status dot per drive | manual-only | Toggle network, verify dot changes color | N/A |
| DASH-04 | Auth degraded banner | manual-only | Console: verify banner appears when `authDegraded: true` | N/A |
| DASH-05 | Last synced timestamp updates | manual-only | Trigger sync, verify timestamp changes | N/A |
| ACT-01 | Upload queue count | manual-only | Write file to mount, verify "1 uploading" appears | N/A |
| ACT-02 | Error entries with detail | manual-only | Trigger conflict/error, verify error row with actionable hint | N/A |
| ACT-03 | Conflict in error section with amber border | manual-only | Trigger conflict, verify amber-bordered entry in errors | N/A |
| ACT-04 | Activity feed with entries | manual-only | Trigger sync, verify activity entries appear | N/A |
| ACT-05 | Writeback queue file names | manual-only | Expand upload queue disclosure, verify file names listed | N/A |
| COFF-01 | Cache usage bar | manual-only | Verify bar shows correct usage; cache some files and re-check | N/A |
| COFF-02 | Pin health badges | manual-only | Pin folder, verify health badge on dashboard + Offline panel | N/A |

**Justification for manual-only:** This phase is pure frontend UI in a Tauri WebView. There is no automated test framework for the frontend (vanilla JS, no Jest/Vitest). The project uses integration tests for Rust crates only. All verification is done visually and via browser console.

### Sampling Rate
- **Per task commit:** `make build` (verifies static assets are valid and Rust compiles)
- **Per wave merge:** `make clippy && make build` + manual visual verification
- **Phase gate:** Full manual walkthrough of all 12 requirements in running app

### Wave 0 Gaps
None -- no test infrastructure needed for a manual-only frontend verification phase. The build system (`make build`) already validates that HTML/CSS/JS assets are bundled correctly.

## Sources

### Primary (HIGH confidence)
- `crates/carminedesktop-app/dist/settings.html` -- Existing sidebar nav structure, panel pattern, CSP meta tag
- `crates/carminedesktop-app/dist/settings.js` -- State management pattern, init(), render(), event delegation, listen()
- `crates/carminedesktop-app/dist/styles.css` -- Complete design token system, component styles, layout
- `crates/carminedesktop-app/dist/ui.js` -- showStatus(), formatError() utilities
- `crates/carminedesktop-app/src/commands.rs` -- All 4 dashboard Tauri commands with exact signatures
- `crates/carminedesktop-core/src/types.rs` -- ObsEvent enum, DashboardStatus, DashboardError, ActivityEntry, CacheStatsResponse, PinHealthInfo, WritebackEntry, UploadQueueInfo struct definitions with serde annotations
- `crates/carminedesktop-app/src/observability.rs` -- Event bridge (emit + ring buffer routing)
- `crates/carminedesktop-app/src/main.rs` -- AppState struct, invoke_handler registration, obs_tx broadcast channel
- `.planning/phases/03-dashboard-ui/03-CONTEXT.md` -- All locked decisions
- `.planning/phases/02-observability-infrastructure/02-CONTEXT.md` -- Data layer decisions

### Secondary (MEDIUM confidence)
- None -- all sources are primary project code

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- No new libraries; extending existing vanilla JS + CSS with well-understood patterns
- Architecture: HIGH -- All patterns already established in settings.js; just adding one more panel
- Pitfalls: HIGH -- Derived from direct code analysis of existing patterns and Phase 2 data shapes

**Research date:** 2026-03-18
**Valid until:** 2026-04-18 (stable -- no external dependencies, all code is project-internal)
