import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Tauri serves the app from this dev server and expects a fixed port, so
// failing loudly beats silently moving to another one.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    // Match the web view Tauri ships with rather than the oldest browser
    // anyone still uses.
    target: 'esnext',
    outDir: 'dist',
    emptyOutDir: true,
  },
});
