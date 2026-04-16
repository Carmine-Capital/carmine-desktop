import { defineConfig } from 'vite';
import { resolve } from 'node:path';
import solid from 'vite-plugin-solid';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: {
    port: 5174,
    strictPort: true,
    host: host ?? false,
    hmr: host
      ? { protocol: 'ws', host, port: 5175 }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**', '**/target/**'],
    },
  },
  build: {
    target: 'esnext',
    minify: 'esbuild',
    sourcemap: false,
    emptyOutDir: true,
    rollupOptions: {
      input: {
        // Settings window — served at `/`, referenced by Tauri's default URL.
        index: resolve(__dirname, 'index.html'),
        // First-run wizard window — opened from Rust via
        // WebviewUrl::App("wizard.html"), so the file must land at dist/wizard.html.
        wizard: resolve(__dirname, 'wizard.html'),
      },
    },
  },
});
