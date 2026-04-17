import { Show, type JSX } from 'solid-js';

import { settings } from '../../store/settings';

export const AboutSection = (): JSX.Element => (
  <>
    <div class="setting-row">
      <div class="setting-label">
        <div class="label-text">Carmine Desktop</div>
        <Show
          when={settings.appVersion}
          fallback={<p class="label-sub">Version –</p>}
        >
          <p class="label-sub">Version {settings.appVersion}</p>
        </Show>
      </div>
    </div>
    <p class="section-heading">Informations légales</p>
    <div class="setting-row">
      <div class="setting-label">
        <p class="label-sub">
          WinFsp — Windows File System Proxy, Copyright (C) Bill Zissimopoulos. Sous licence
          GPLv3.
          <br />
          <a
            href="https://github.com/winfsp/winfsp"
            target="_blank"
            rel="noopener noreferrer"
          >
            github.com/winfsp/winfsp
          </a>
        </p>
      </div>
    </div>
  </>
);
