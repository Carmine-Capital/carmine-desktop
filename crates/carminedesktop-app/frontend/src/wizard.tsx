/* @refresh reload */
import { render } from 'solid-js/web';

import { WizardApp } from './WizardApp';

const root = document.getElementById('root');
if (!root) {
  throw new Error('Élément #root introuvable dans wizard.html');
}

render(() => <WizardApp />, root);
