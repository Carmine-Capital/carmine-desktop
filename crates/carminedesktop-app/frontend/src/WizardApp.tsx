import {
  For,
  Match,
  Show,
  Switch,
  createEffect,
  createSignal,
  on,
  onCleanup,
  onMount,
  type JSX,
} from 'solid-js';
import { createStore, produce } from 'solid-js/store';

import { api } from './ipc';
import { onAuthComplete, onAuthError } from './eventBus';
import { autoAnimateList } from './components/autoAnimate';
import { Skeleton } from './components/Skeleton';
import { showStatus, formatError, StatusBar } from './components/StatusBar';
import { ToastHost } from './components/ToastHost';
import { ConfirmModal } from './components/ConfirmModal';
import { sanitizePath } from './utils/format';
import type { DriveInfo, MountInfo, PrimarySiteInfo } from './bindings';

type StepId = 'welcome' | 'libraries' | 'success';

interface StepDef {
  id: StepId;
  label: string;
  title: string;
  icon: () => JSX.Element;
}

const WELCOME_STEP: StepDef = {
  id: 'welcome',
  label: 'Connexion',
  title: 'Configuration Carmine',
  icon: LinkIcon,
};
const LIBRARIES_STEP: StepDef = {
  id: 'libraries',
  label: 'Bibliothèques',
  title: 'Sélection des bibliothèques — Carmine',
  icon: BoxIcon,
};
const SUCCESS_STEP: StepDef = {
  id: 'success',
  label: 'Terminé',
  title: 'Terminé — Carmine',
  icon: CheckIcon,
};
const STEPS: StepDef[] = [WELCOME_STEP, LIBRARIES_STEP, SUCCESS_STEP];

// Matches `auth.oauth::AUTH_TIMEOUT_SECS`.  Keep in sync if the Rust side
// changes — hard-coded in both places on purpose, no event piped back.
const AUTH_TIMEOUT_SECS = 120;

type UnlistenFn = () => void;

interface Selection {
  onedrive: boolean;
  libraries: Set<string>;
}

export const WizardApp = (): JSX.Element => {
  const [step, setStep] = createSignal<StepId>('welcome');
  const [signingIn, setSigningIn] = createSignal(false);
  const [authUrl, setAuthUrl] = createSignal('');
  const [authError, setAuthError] = createSignal<string | null>(null);
  const [copyLabel, setCopyLabel] = createSignal('Copier');
  const [countdown, setCountdown] = createSignal<number | null>(null);

  const [defaultMountRoot, setDefaultMountRoot] = createSignal('~/Cloud');
  const [driveInfo, setDriveInfo] = createSignal<DriveInfo | null>(null);
  const [primarySite, setPrimarySite] = createSignal<PrimarySiteInfo | null>(null);
  const [libraries, setLibraries] = createSignal<DriveInfo[]>([]);
  const [librariesLoading, setLibrariesLoading] = createSignal(false);
  const [librariesError, setLibrariesError] = createSignal<string | null>(null);

  const [selection, setSelection] = createStore<Selection>({
    onedrive: true,
    libraries: new Set<string>(),
  });

  const [finalizing, setFinalizing] = createSignal(false);
  const [finalMounts, setFinalMounts] = createSignal<MountInfo[]>([]);
  const [switchingAccount, setSwitchingAccount] = createSignal(false);

  // DOM refs used for focus transfer after step changes so screen readers
  // announce the new step (matches the accessibility polish in App.tsx).
  let welcomeHeadingRef: HTMLHeadingElement | undefined;
  let librariesHeadingRef: HTMLHeadingElement | undefined;
  let successHeadingRef: HTMLHeadingElement | undefined;

  const authListeners: UnlistenFn[] = [];
  let countdownTimer: ReturnType<typeof setInterval> | null = null;
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  const cleanupAuthListeners = (): void => {
    while (authListeners.length) {
      const fn = authListeners.pop();
      try {
        fn?.();
      } catch {
        /* ignore */
      }
    }
  };

  const stopCountdown = (): void => {
    if (countdownTimer) {
      clearInterval(countdownTimer);
      countdownTimer = null;
    }
    setCountdown(null);
  };

  const startCountdown = (): void => {
    stopCountdown();
    setCountdown(AUTH_TIMEOUT_SECS);
    countdownTimer = setInterval(() => {
      const next = (countdown() ?? 0) - 1;
      if (next <= 0) {
        stopCountdown();
        return;
      }
      setCountdown(next);
    }, 1000);
  };

  onCleanup(() => {
    stopCountdown();
    cleanupAuthListeners();
    if (copyTimer) clearTimeout(copyTimer);
  });

  // Focus transfer: after step changes, drop focus on the new step's heading
  // so keyboard users land in context and AT announces the new panel.
  // `defer: true` so we don't fight the browser's default focus on mount.
  createEffect(
    on(
      step,
      (current) => {
        const target =
          current === 'welcome'
            ? welcomeHeadingRef
            : current === 'libraries'
              ? librariesHeadingRef
              : successHeadingRef;
        target?.focus({ preventScroll: true });
        const def = STEPS.find((s) => s.id === current);
        if (def) document.title = def.title;
      },
      { defer: true },
    ),
  );

  onMount(() => {
    document.title = WELCOME_STEP.title;
    void api
      .getDefaultMountRoot()
      .then((root) => setDefaultMountRoot(root.replace(/[/\\]+$/, '')))
      .catch(() => {
        /* keep fallback */
      });
  });

  const currentStepIndex = (): number => STEPS.findIndex((s) => s.id === step());

  const goTo = (next: StepId): void => {
    setStep(next);
  };

  const startSignIn = async (): Promise<void> => {
    if (signingIn()) return;
    setSigningIn(true);
    setAuthError(null);
    setAuthUrl('');
    startCountdown();

    try {
      authListeners.push(
        await onAuthComplete(() => {
          if (!signingIn()) return;
          setSigningIn(false);
          stopCountdown();
          cleanupAuthListeners();
          void onSignInComplete();
        }),
      );
      authListeners.push(
        await onAuthError((payload) => {
          if (!signingIn()) return;
          setSigningIn(false);
          stopCountdown();
          cleanupAuthListeners();
          setAuthError(`Échec de connexion : ${payload || 'erreur inconnue'}`);
        }),
      );

      const url = await api.startSignIn();
      setAuthUrl(url);
    } catch (e) {
      setSigningIn(false);
      stopCountdown();
      cleanupAuthListeners();
      showStatus(formatError(e), 'error');
    }
  };

  const cancelSignIn = async (): Promise<void> => {
    setSigningIn(false);
    stopCountdown();
    cleanupAuthListeners();
    setAuthUrl('');
    setAuthError(null);
    try {
      await api.cancelSignIn();
    } catch {
      /* best effort */
    }
  };

  const copyAuthUrl = async (): Promise<void> => {
    const url = authUrl();
    if (!url) return;
    try {
      await navigator.clipboard.writeText(url);
      setCopyLabel('Copié !');
      if (copyTimer) clearTimeout(copyTimer);
      copyTimer = setTimeout(() => setCopyLabel('Copier'), 2000);
    } catch {
      showStatus("Impossible de copier l'URL", 'error');
    }
  };

  const onSignInComplete = async (): Promise<void> => {
    goTo('libraries');
    await loadLibraries();
  };

  const loadLibraries = async (): Promise<void> => {
    setLibrariesLoading(true);
    setLibrariesError(null);
    const [driveResult, librariesResult, siteInfoResult] = await Promise.allSettled([
      api.getDriveInfo(),
      api.listPrimarySiteLibraries(),
      api.getPrimarySiteInfo(),
    ]);
    setLibrariesLoading(false);

    if (siteInfoResult.status === 'fulfilled') {
      setPrimarySite(siteInfoResult.value);
    }
    if (driveResult.status === 'fulfilled') {
      setDriveInfo(driveResult.value);
    }
    if (librariesResult.status === 'fulfilled') {
      setLibraries(librariesResult.value);
    }

    // Both sides dead: surface a blocking error so the user knows to retry.
    // A single side failing is non-fatal: we keep whichever loaded and just
    // nudge via a toast.
    if (driveResult.status === 'rejected' && librariesResult.status === 'rejected') {
      setLibrariesError('Impossible de charger les données du compte. Veuillez vous reconnecter.');
    } else if (driveResult.status === 'rejected') {
      showStatus('OneDrive indisponible — Bibliothèques SharePoint chargées', 'info');
    } else if (librariesResult.status === 'rejected') {
      showStatus('Bibliothèques SharePoint indisponibles — OneDrive chargé', 'info');
    }
  };

  const toggleLibrary = (driveId: string): void => {
    setSelection(
      produce((s) => {
        if (s.libraries.has(driveId)) {
          s.libraries.delete(driveId);
        } else {
          s.libraries.add(driveId);
        }
      }),
    );
  };

  const hasSelection = (): boolean => {
    const oneDrive = selection.onedrive && driveInfo() !== null;
    return oneDrive || selection.libraries.size > 0;
  };

  const getStarted = async (): Promise<void> => {
    setLibrariesError(null);
    setFinalizing(true);

    const onedriveChecked = selection.onedrive && driveInfo() !== null;
    const mountRoot = defaultMountRoot();
    const errors: string[] = [];
    let totalMounted = 0;

    if (onedriveChecked) {
      const di = driveInfo()!;
      try {
        await api.addMount({
          mountType: 'drive',
          driveId: di.id,
          mountPoint: `${mountRoot}/OneDrive`,
        });
        totalMounted += 1;
      } catch (e) {
        errors.push(`OneDrive : ${formatError(e)}`);
      }
    }

    const site = primarySite();
    for (const lib of libraries()) {
      if (!selection.libraries.has(lib.id)) continue;
      const mountPoint = `${mountRoot}/${sanitizePath(lib.name)}/`;
      try {
        await api.addMount({
          mountType: 'sharepoint',
          mountPoint,
          driveId: lib.id,
          siteId: site?.site_id ?? null,
          siteName: site?.site_name ?? null,
          libraryName: lib.name,
        });
        totalMounted += 1;
      } catch (e) {
        errors.push(`${lib.name} : ${formatError(e)}`);
      }
    }

    if (errors.length > 0 && totalMounted === 0) {
      setLibrariesError(`Échec de l'ajout des lecteurs : ${errors.join('; ')}`);
      showStatus('Échec de la configuration', 'error');
      setFinalizing(false);
      return;
    }

    try {
      await api.completeWizard();
    } catch (e) {
      showStatus(formatError(e), 'error');
      setFinalizing(false);
      return;
    }

    try {
      const mounts = await api.listMounts();
      setFinalMounts(mounts);
    } catch {
      /* leave empty */
    }

    setFinalizing(false);
    goTo('success');
  };

  const switchAccount = async (): Promise<void> => {
    setSwitchingAccount(true);
    try {
      await api.signOut();
    } catch {
      /* best effort */
    }
    setSwitchingAccount(false);
    setDriveInfo(null);
    setLibraries([]);
    setPrimarySite(null);
    setSelection(
      produce((s) => {
        s.onedrive = true;
        s.libraries = new Set<string>();
      }),
    );
    setAuthUrl('');
    setAuthError(null);
    goTo('welcome');
  };

  const closeWizard = async (): Promise<void> => {
    type WindowModule = { getCurrentWindow?: () => { close: () => Promise<void> } };
    const w = window as unknown as { __TAURI__?: { window?: WindowModule } };
    const getCurrent = w.__TAURI__?.window?.getCurrentWindow;
    try {
      await getCurrent?.().close();
    } catch {
      /* window may already be closing */
    }
  };

  return (
    <div class="app-layout">
      <aside class="sidebar">
        <div class="sidebar-header">
          <div class="sidebar-logo">C</div>
          <span class="sidebar-title">Carmine</span>
        </div>
        <div class="section-heading">Configuration</div>
        <div class="sidebar-nav">
          <For each={STEPS}>
            {(def, i) => (
              <div
                class="nav-item"
                classList={{
                  active: step() === def.id,
                  done: i() < currentStepIndex(),
                }}
              >
                <def.icon />
                {def.label}
              </div>
            )}
          </For>
        </div>
      </aside>

      <main class="main-content">
        <Switch>
          <Match when={step() === 'welcome'}>
            <section
              class="step step-centered active"
              aria-labelledby="wizard-welcome-title"
            >
              <h1
                id="wizard-welcome-title"
                ref={welcomeHeadingRef}
                tabindex={-1}
              >
                Bienvenue sur Carmine
              </h1>
              <p class="step-sub">
                Accédez à vos documents OneDrive et SharePoint directement depuis votre explorateur
                de fichiers, avec une synchronisation ultra-rapide.
              </p>

              <Show when={!signingIn() && !authError()}>
                <button
                  type="button"
                  class="btn-primary"
                  onClick={startSignIn}
                  aria-busy={signingIn()}
                >
                  Se connecter avec Microsoft
                </button>
              </Show>

              {/* Wrapper stays mounted on error (signingIn flips back to false
                  but authError is set) so the failure is visible until the
                  user cancels or retries. */}
              <Show when={signingIn() || authError()}>
                <div class="auth-waiting">
                  <Show when={signingIn()}>
                    <div class="auth-status-row">
                      <div class="spinner" />
                      <CountdownLabel remaining={countdown()} />
                    </div>
                    <div class="url-row">
                      <input
                        class="url-input"
                        type="text"
                        readonly
                        value={authUrl()}
                        aria-label="URL d'authentification"
                      />
                      <button
                        type="button"
                        class="btn-ghost btn-sm"
                        onClick={copyAuthUrl}
                        disabled={!authUrl()}
                      >
                        {copyLabel()}
                      </button>
                    </div>
                    <p class="hint">
                      Le navigateur ne s'est pas ouvert ? Copiez le lien manuellement.
                    </p>
                  </Show>
                  <Show when={authError()}>
                    <div class="error-msg" role="alert">
                      {authError()}
                    </div>
                  </Show>
                  <button
                    type="button"
                    class="btn-link cancel-link"
                    onClick={cancelSignIn}
                  >
                    Annuler
                  </button>
                </div>
              </Show>
            </section>
          </Match>

          <Match when={step() === 'libraries'}>
            <section
              class="step step-scroll active"
              aria-labelledby="wizard-libraries-title"
            >
              <h1
                id="wizard-libraries-title"
                ref={librariesHeadingRef}
                tabindex={-1}
                class="section-heading"
                style={{ 'margin-top': '0' }}
              >
                Sélectionnez vos lecteurs
              </h1>

              <Show when={librariesLoading()}>
                <Skeleton variant="row" label="Chargement des bibliothèques…" />
              </Show>

              <Show when={!librariesLoading() && driveInfo()}>
                {(di) => (
                  <div class="setting-row">
                    <div class="setting-label">
                      <div class="label-text">{di().name || 'OneDrive'}</div>
                      <div class="label-sub">{defaultMountRoot()}/OneDrive</div>
                    </div>
                    <div class="setting-control">
                      <label class="toggle-switch">
                        <input
                          type="checkbox"
                          checked={selection.onedrive}
                          onChange={(e) =>
                            setSelection('onedrive', e.currentTarget.checked)
                          }
                        />
                        <span class="toggle-track" />
                      </label>
                    </div>
                  </div>
                )}
              </Show>

              <Show when={!librariesLoading() && libraries().length > 0}>
                <div class="section-heading">Sites SharePoint</div>
                <div class="sp-list" ref={autoAnimateList}>
                  <For each={libraries()}>
                    {(lib) => (
                      <button
                        type="button"
                        class="lib-row"
                        classList={{ selected: selection.libraries.has(lib.id) }}
                        onClick={() => toggleLibrary(lib.id)}
                        aria-pressed={selection.libraries.has(lib.id)}
                      >
                        <div class="lib-check">✓</div>
                        <div class="lib-info">
                          <div class="lib-name">{lib.name}</div>
                        </div>
                      </button>
                    )}
                  </For>
                </div>
              </Show>

              <Show
                when={
                  !librariesLoading() &&
                  libraries().length === 0 &&
                  !librariesError()
                }
              >
                <p class="hint">
                  Aucune bibliothèque SharePoint trouvée pour votre organisation.
                </p>
              </Show>

              <Show when={librariesError()}>
                <div class="error-msg" role="alert">
                  {librariesError()}
                </div>
              </Show>

              <div class="wizard-actions">
                <button
                  type="button"
                  class="btn-primary"
                  disabled={!hasSelection() || finalizing()}
                  aria-busy={finalizing()}
                  onClick={getStarted}
                >
                  {finalizing() ? 'Configuration…' : 'Continuer'}
                </button>
                <button
                  type="button"
                  class="btn-ghost"
                  disabled={switchingAccount()}
                  aria-busy={switchingAccount()}
                  onClick={switchAccount}
                >
                  {switchingAccount() ? 'Déconnexion…' : 'Changer de compte'}
                </button>
              </div>
            </section>
          </Match>

          <Match when={step() === 'success'}>
            <section
              class="step step-centered active"
              aria-labelledby="wizard-success-title"
            >
              <h1
                id="wizard-success-title"
                ref={successHeadingRef}
                tabindex={-1}
              >
                Configuration terminée
              </h1>
              <p class="step-sub">
                Vos lecteurs sont maintenant montés. Vous les retrouverez dans votre barre latérale
                Windows.
              </p>
              <ul class="done-mount-list" ref={autoAnimateList}>
                <For each={finalMounts()}>
                  {(mount) => (
                    <li class="mount-item">
                      {mount.name} → {mount.mount_point}
                    </li>
                  )}
                </For>
              </ul>
              <button type="button" class="btn-primary" onClick={closeWizard}>
                Accéder à mes fichiers
              </button>
              <p class="hint">Carmine restera actif dans votre barre des tâches.</p>
            </section>
          </Match>
        </Switch>
      </main>

      <StatusBar />
      <ToastHost />
      <ConfirmModal />
    </div>
  );
};

const CountdownLabel = (props: { remaining: number | null }): JSX.Element => (
  <Show when={props.remaining !== null} fallback={<span class="auth-countdown" />}>
    <span
      class="auth-countdown"
      classList={{ warning: (props.remaining ?? Infinity) <= 30 }}
      aria-live="polite"
    >
      Temps restant : {props.remaining}s
    </span>
  </Show>
);

function LinkIcon(): JSX.Element {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4" />
      <polyline points="10 17 15 12 10 7" />
      <line x1="15" y1="12" x2="3" y2="12" />
    </svg>
  );
}

function BoxIcon(): JSX.Element {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="m7.5 4.27 9 5.15" />
      <path d="M21 8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16Z" />
      <path d="m3.3 7 8.7 5 8.7-5" />
      <path d="M12 22V12" />
    </svg>
  );
}

function CheckIcon(): JSX.Element {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2.5"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  );
}
