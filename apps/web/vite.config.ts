import path from 'node:path';

const here = import.meta.dirname;

import react from '@vitejs/plugin-react';
import { defineConfig, type Plugin } from 'vite';

/**
 * The desktop's components are compiled into this app unchanged — that is
 * the point of the site — and three of them import `../api`, the module
 * that talks to the Tauri shell. This plugin swaps that one module for
 * `src/api.ts`, which exposes the same functions backed by the WebAssembly
 * runtime. Nothing else from the desktop tree is touched.
 */
function webApi(): Plugin {
  const desktopApi = path.resolve(here, '../desktop/src/api.ts');
  const browserApi = path.resolve(here, 'src/api.ts');
  return {
    name: 'hyperlab:web-api',
    enforce: 'pre',
    async resolveId(source, importer, options) {
      const resolved = await this.resolve(source, importer, {
        ...options,
        skipSelf: true,
      });
      if (resolved !== null && path.normalize(resolved.id) === desktopApi) {
        return browserApi;
      }
      return null;
    },
  };
}

/**
 * The headers that make `SharedArrayBuffer` available, which is what lets a
 * script block on a dialog the way HyperCard's did. GitHub Pages cannot send
 * them, so production uses `coi-serviceworker.js` instead; the dev server
 * sends them directly so development looks like production.
 */
const isolation = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp',
};

export default defineConfig({
  plugins: [webApi(), react()],
  resolve: {
    // The desktop's components resolve their imports against their own
    // directory, where nothing is installed; point them at this app's copy.
    alias: {
      react: path.resolve(here, 'node_modules/react'),
      'react-dom': path.resolve(here, 'node_modules/react-dom'),
    },
    dedupe: ['react', 'react-dom'],
  },
  // Served from https://<user>.github.io/<repo>/, so every URL is relative.
  base: './',
  clearScreen: false,
  server: {
    headers: isolation,
    fs: {
      // The desktop's sources and the built wasm package sit outside this
      // app's root.
      allow: [path.resolve(here, '../..')],
    },
  },
  preview: {
    headers: isolation,
  },
  worker: {
    format: 'es',
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        index: path.resolve(here, 'index.html'),
        play: path.resolve(here, 'play.html'),
      },
    },
  },
});
