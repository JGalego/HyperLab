/** Where the interface starts. */

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import './theme/neo-classic.css';

const root = document.getElementById('root');
if (!root) {
  throw new Error('the page is missing its root element');
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
