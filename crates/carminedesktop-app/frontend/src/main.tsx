/* @refresh reload */
import { render } from 'solid-js/web';

import { App } from './App';

const root = document.getElementById('root');
if (!root) {
  throw new Error('Élément #root introuvable dans index.html');
}

render(() => <App />, root);
