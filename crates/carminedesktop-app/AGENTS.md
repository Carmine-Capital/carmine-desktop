# carminedesktop-app

Tauri 2 desktop shell + Solid.js/Vite/TypeScript UI.  Two WebViews: settings/dashboard (`frontend/index.html`) and first-run wizard (`frontend/wizard.html`).  Real-time UI is driven by per-subject Tauri events emitted from Rust and consumed by per-topic Solid signals.

## STRUCTURE

- `src/` — Tauri backend: `commands.rs` (every `#[tauri::command]`), `main.rs` (setup, mount lifecycle, per-drive tasks), `observability.rs` (event bridge + ring buffers), `pin_events.rs` (pin aggregator), `notify.rs`, `shell_integration.rs`.
- `frontend/` — Vite project, built by `npm run build` via Tauri's `beforeBuildCommand`.  Output lands in `frontend/dist/` (git-ignored) and is served as `frontendDist` by `tauri.conf.json`.
- `capabilities/`, `gen/schemas/`, `windows/hooks.nsh`, `icons/` — Tauri ACL, generated IPC schemas, NSIS installer hooks, tray icons.

## FRONTEND LAYOUT

- `src/panels/{Dashboard,General,Mounts,Offline,About}.tsx` — one per settings tab.
- `src/components/` — presentational pieces (`PinCard`, `DriveCard`, `StatusBar`, `Skeleton`, `UploadBanner`, `AuthBanner`, `CacheSection`, `ActivityFeed`, `ErrorFeed`, `MountCard`, `DriveList`, `Icons`, `autoAnimate`).
- `src/store/` — one file per domain (`pins`, `drives`, `activity`, `errors`, `auth`, `cache`, `mounts`, `settings`).  Bootstrap via `@tanstack/solid-query`, updates via the event bus.
- `src/eventBus.ts` — typed wrappers around `tauri::event::listen()`, one `on<Topic>()` per topic.  Nothing else is allowed to call `listen()` directly.
- `src/ipc.ts` — typed wrappers around `invoke()`; prefer the `api.*` helpers (one per `#[tauri::command]`), fall back to the bare `invoke<T>` export only for ad-hoc calls.
- `src/bindings.ts` — hand-maintained TS mirror of every serde struct crossing the IPC or event boundary (camelCase).  Keep in sync with `core::types` and command return shapes.
- `src/App.tsx` + `src/main.tsx` — settings shell.  `src/WizardApp.tsx` + `src/wizard.tsx` — wizard shell.  Two entry points, one `public/styles.css`, no shared Solid tree.

## REALTIME TOPICS

Emitted from Rust, subscribed via `eventBus.ts`, routed to stores:

| Topic | Payload (`core::types`) | Producer |
|---|---|---|
| `pin:health` | `PinHealthEvent` | `pin_events::spawn_aggregator` — 250 ms debounce; emits only when the `(folder_name, mount_name, status, total_files, cached_files, pinned_at, expires_at)` tuple differs from the last snapshot. |
| `pin:removed` | `PinRemovedEvent` | same aggregator — pins that dropped out of a refreshed drive. |
| `drive:upload-progress` | `DriveUploadProgressEvent` | `main.rs::spawn_upload_progress_emitter`, one task per drive, reads the `watch<SyncMetrics>` from `carminedesktop-vfs::sync_processor`, 250 ms debounce, skips byte-equal snapshots. |
| `drive:online` | `DriveOnlineEvent` | `observability::spawn_event_bridge` fan-out of `ObsEvent::OnlineStateChanged`. |
| `drive:status` | `DriveStatusEvent` | same bridge — `ObsEvent::SyncStateChanged`. |
| `activity:append` | `ActivityEntry` | same bridge + append to `ActivityBuffer` ring. |
| `error:append` | `DashboardError` | same bridge + append to `ErrorAccumulator` ring. |
| `auth:state` | `AuthStateEvent` | same bridge — `ObsEvent::AuthStateChanged`. |
| `auth-complete` / `auth-error` | `()` / `String` | `commands::start_sign_in` — wizard-only. |
| `navigate-to-panel` | `String` | tray menu (`shell_integration.rs`). |

`ObsEvent` (`core::types`) is a Rust-internal `tokio::sync::broadcast` enum.  It is **not** emitted to the frontend; the legacy multiplex topic `obs-event` was removed in phase 7.

## CONVENTIONS

- **Flicker-free push model** — every store loads once via `createQuery`, then subscribes to its topic in `eventBus.ts`.  No `setInterval` polling anywhere in the frontend.
- **Emit only on change** — every Rust producer diffs against the last snapshot before calling `app.emit(...)`.  A no-op re-emit re-runs CSS transitions and breaks the zero-flicker contract (see `pin_events.rs::Snapshot::differs_from_event` and the `SyncMetrics` equality check in `spawn_upload_progress_emitter`).
- **One topic → one store → one signal** — Solid re-renders only the DOM nodes bound to the signal that changed.  Keep stores narrow.
- **Bindings are hand-written** — `frontend/src/bindings.ts` mirrors `core::types` and `commands.rs` return shapes.  Any struct edit on the Rust side requires an edit here; there is no codegen (tauri-specta deferred).
- **Typed IPC only** — add a new `#[tauri::command]` in `commands.rs`, register it in the `invoke_handler!` at the bottom of `main.rs`, then add a typed helper to `ipc.ts::api`.  Callers import from `./ipc.ts`, never from `@tauri-apps/api/core`.
- **User feedback on mutations** — every `invoke<void>` / `invoke<bool>` sits in `try { ... } catch (e) { showStatus(formatError(e), 'error') }` with a matching success `showStatus(..., 'success')`.  `showStatus` and `formatError` live in `components/StatusBar.tsx`.
- **List animations** — use `use:autoAnimate` from `components/autoAnimate.ts` on `pin-list`, `drive-cards`, `activity-list`, `error-list`.
- **Pin dirty signal** — triggering a re-aggregation of pin health from Rust goes through `state.pin_tx.send(PinDirty::...)`; do not call `pin_store.health()` on command paths.

## ANTI-PATTERNS

- Do NOT add `setInterval` to refresh pins / drives / activity / errors — subscribe to the matching topic.
- Do NOT call `listen()` outside `eventBus.ts`; every subscription must be typed there.
- Do NOT emit a granular topic without a "did it change?" check on the producer — the flicker-free contract depends on it.
- Do NOT add a new topic without updating the table above, `core::types`, `eventBus.ts` and `bindings.ts` together.
- Do NOT recreate `obs-event` or any other multiplexed topic — granular per-subject only.
- Do NOT import `invoke` from `@tauri-apps/api/core` — go through `ipc.ts` so arguments and return types stay typed.
- Do NOT add inline `onclick="..."` in `frontend/*.html` — CSP `script-src 'self'` blocks it; wire events via Solid's JSX event props.
- Do NOT edit `frontend/dist/` — it is the Vite build output, regenerated on every build.
- Do NOT hold `AppState` locks across `await` inside an aggregator flush — snapshot the fields you need, drop the lock, then await.  `pin_events::flush` is the reference pattern.
