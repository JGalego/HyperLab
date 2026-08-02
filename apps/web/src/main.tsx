/** Where the playground starts. */

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import '../../desktop/src/theme/neo-classic.css';

import { App } from './App';

const root = document.getElementById('root');
if (!root) {
  throw new Error('the page is missing its root element');
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
