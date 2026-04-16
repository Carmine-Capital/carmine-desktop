import {
  Match,
  Switch,
  createEffect,
  createMemo,
  createSignal,
  on,
  onMount,
  type JSX,
} from 'solid-js';

import { api } from './ipc';
import { onNavigateToPanel } from './eventBus';
import { bootstrapSettings, settings } from './store/settings';
import {
  attachActivityEvents,
  bootstrapActivity,
} from './store/activity';
import { attachAuthEvents, ingestDashboardAuth } from './store/auth';
import { attachDriveEvents, bootstrapDrives } from './store/drives';
import { attachErrorEvents, bootstrapErrors } from './store/errors';
import { attachPinEvents, bootstrapPins } from './store/pins';
import { showStatus, formatError, StatusBar } from './components/StatusBar';
import { ToastHost } from './components/ToastHost';
import { ConfirmModal } from './components/ConfirmModal';
import { confirm } from './store/confirm';
import { pushToast } from './store/toasts';
import {
  AboutIcon,
  DashboardIcon,
  DrivesIcon,
  GearIcon,
  OfflineIcon,
} from './components/Icons';
import { Dashboard } from './panels/Dashboard';
import { General } from './panels/General';
import { Mounts } from './panels/Mounts';
import { Offline } from './panels/Offline';
import { About } from './panels/About';
import type { PanelId } from './bindings';

interface NavTab {
  id: PanelId;
  label: string;
  icon: (p: { size?: number }) => JSX.Element;
}

const TABS: NavTab[] = [
  { id: 'dashboard', label: 'Tableau de bord', icon: DashboardIcon },
  { id: 'general', label: 'Général', icon: GearIcon },
  { id: 'mounts', label: 'Lecteurs', icon: DrivesIcon },
  { id: 'offline', label: 'Hors-ligne', icon: OfflineIcon },
  { id: 'about', label: 'À propos', icon: AboutIcon },
];

function initialPanel(): PanelId {
  const raw = new URLSearchParams(window.location.search).get('panel');
  if (raw && TABS.some((t) => t.id === raw)) return raw as PanelId;
  return 'dashboard';
}

export const App = (): JSX.Element => {
  const [active, setActive] = createSignal<PanelId>(initialPanel());
  const [signingOut, setSigningOut] = createSignal(false);
  let panelRef: HTMLElement | undefined;

  // Focus the panel section after every user-driven tab switch so screen
  // readers announce the new panel and keyboard users land in-context.
  // `defer: true` skips the initial mount so we don't steal focus from the
  // window's natural start.
  createEffect(
    on(
      active,
      () => {
        panelRef?.focus({ preventScroll: true });
      },
      { defer: true },
    ),
  );

  onMount(() => {
    document.title = 'Paramètres Carmine';
    // Live streams: attach first so no push event between bootstrap fetches
    // and the first render is missed.
    attachDriveEvents();
    attachAuthEvents();
    attachActivityEvents();
    attachErrorEvents();
    attachPinEvents();
    // One-shot bootstraps — each store re-renders itself from events thereafter.
    void bootstrapSettings().catch((e) => showStatus(formatError(e), 'error'));
    void bootstrapDrives()
      .then(ingestDashboardAuth)
      .catch((e) => showStatus(formatError(e), 'error'));
    void bootstrapActivity().catch((e) => showStatus(formatError(e), 'error'));
    void bootstrapErrors().catch((e) => showStatus(formatError(e), 'error'));
    void bootstrapPins().catch((e) => showStatus(formatError(e), 'error'));
    void onNavigateToPanel((p) => {
      if (TABS.some((t) => t.id === p)) setActive(p as PanelId);
    });
  });

  const accountLabel = createMemo(() => settings.accountDisplay ?? 'Non connecté');

  const signOut = async () => {
    const ok = await confirm({
      title: 'Se déconnecter ?',
      message:
        'Vous devrez vous reconnecter pour accéder à vos fichiers. Tous les lecteurs seront démontés.',
      confirmLabel: 'Se déconnecter',
      danger: true,
    });
    if (!ok) return;
    setSigningOut(true);
    try {
      await api.signOut();
      pushToast({ kind: 'success', title: 'Déconnecté' });
    } catch (e) {
      pushToast({ kind: 'error', title: 'Échec de la déconnexion', message: formatError(e) });
    } finally {
      setSigningOut(false);
    }
  };

  return (
    <div class="app-layout">
      <aside class="sidebar">
        <div class="sidebar-header">
          <div class="sidebar-logo">C</div>
          <span class="sidebar-title">Carmine</span>
        </div>
        <nav class="sidebar-nav" role="tablist">
          {TABS.map((tab) => (
            <button
              type="button"
              role="tab"
              class="nav-item"
              classList={{ active: active() === tab.id }}
              id={`tab-${tab.id}`}
              aria-selected={active() === tab.id}
              onClick={() => setActive(tab.id)}
            >
              <tab.icon size={16} />
              {tab.label}
            </button>
          ))}
        </nav>
        <div class="sidebar-footer">
          <p class="account-email">{accountLabel()}</p>
          <button
            type="button"
            class="btn-danger"
            disabled={signingOut()}
            aria-busy={signingOut()}
            onClick={signOut}
          >
            {signingOut() ? 'Déconnexion en cours…' : 'Déconnexion'}
          </button>
        </div>
      </aside>

      <main class="main-content">
        <section
          class="panel active"
          aria-labelledby={`tab-${active()}`}
          tabindex={-1}
          ref={panelRef}
        >
          <Switch>
            <Match when={active() === 'dashboard'}>
              <Dashboard />
            </Match>
            <Match when={active() === 'general'}>
              <General />
            </Match>
            <Match when={active() === 'mounts'}>
              <Mounts />
            </Match>
            <Match when={active() === 'offline'}>
              <Offline />
            </Match>
            <Match when={active() === 'about'}>
              <About />
            </Match>
          </Switch>
        </section>
      </main>

      <StatusBar />
      <ToastHost />
      <ConfirmModal />
    </div>
  );
};
