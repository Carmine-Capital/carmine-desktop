// Typed wrappers around Tauri's listen() — one subscription per topic, lifted
// into app state via the stores.  Callers should not call listen() directly.

import type {
  ActivityEntry,
  AuthStateEvent,
  DashboardError,
  DriveOnlineEvent,
  DriveStatusEvent,
  DriveUploadProgressEvent,
  PinHealthEvent,
  PinRemovedEvent,
} from './bindings';

type UnlistenFn = () => void;
type EventHandler<T> = (payload: T) => void;
type ListenFn = <T>(
  event: string,
  handler: (event: { payload: T }) => void,
) => Promise<UnlistenFn>;

function tauri(): { listen: ListenFn } {
  const w = window as unknown as { __TAURI__?: { event?: { listen?: ListenFn } } };
  const fn = w.__TAURI__?.event?.listen;
  if (!fn) throw new Error('Tauri bridge non disponible (window.__TAURI__.event.listen manquant)');
  return { listen: fn };
}

function subscribe<T>(event: string, handler: EventHandler<T>): Promise<UnlistenFn> {
  return tauri().listen<T>(event, (e) => handler(e.payload));
}

export function onPinHealth(handler: EventHandler<PinHealthEvent>): Promise<UnlistenFn> {
  return subscribe<PinHealthEvent>('pin:health', handler);
}

export function onPinRemoved(handler: EventHandler<PinRemovedEvent>): Promise<UnlistenFn> {
  return subscribe<PinRemovedEvent>('pin:removed', handler);
}

export function onNavigateToPanel(handler: EventHandler<string>): Promise<UnlistenFn> {
  return subscribe<string>('navigate-to-panel', handler);
}

export function onDriveStatus(handler: EventHandler<DriveStatusEvent>): Promise<UnlistenFn> {
  return subscribe<DriveStatusEvent>('drive:status', handler);
}

export function onDriveOnline(handler: EventHandler<DriveOnlineEvent>): Promise<UnlistenFn> {
  return subscribe<DriveOnlineEvent>('drive:online', handler);
}

export function onDriveUploadProgress(
  handler: EventHandler<DriveUploadProgressEvent>,
): Promise<UnlistenFn> {
  return subscribe<DriveUploadProgressEvent>('drive:upload-progress', handler);
}

export function onAuthState(handler: EventHandler<AuthStateEvent>): Promise<UnlistenFn> {
  return subscribe<AuthStateEvent>('auth:state', handler);
}

export function onActivityAppend(handler: EventHandler<ActivityEntry>): Promise<UnlistenFn> {
  return subscribe<ActivityEntry>('activity:append', handler);
}

export function onErrorAppend(handler: EventHandler<DashboardError>): Promise<UnlistenFn> {
  return subscribe<DashboardError>('error:append', handler);
}

// Wizard-only auth events.  Emitted by `start_sign_in` once the PKCE flow
// resolves (commands.rs:87-119); the wizard listens for both to close the
// countdown modal and either advance to step 2 or surface the failure.
export function onAuthComplete(handler: EventHandler<void>): Promise<UnlistenFn> {
  return subscribe<void>('auth-complete', handler);
}

export function onAuthError(handler: EventHandler<string>): Promise<UnlistenFn> {
  return subscribe<string>('auth-error', handler);
}
