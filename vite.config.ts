/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri expects a fixed dev-server port and must not clear the terminal.
const host = process.env.TAURI_DEV_HOST;

import pkg from './package.json' with { type: 'json' };

export default defineConfig({
  define: { __APP_VERSION__: JSON.stringify(pkg.version) },
  plugins: [svelte()],
  resolve: { alias: { $lib: new URL('./src/lib', import.meta.url).pathname } },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
    // The engine (desktop: started by `tauri dev` on 1430; or a dev server via
    // TESSERA_DEV_ENGINE) serves the API, media and derived files.
    proxy: Object.fromEntries(
      ['/api', '/media', '/d', '/remote', '/ca.crt', '/healthz'].map((p) => [
        p,
        { target: process.env.TESSERA_DEV_ENGINE ?? 'http://127.0.0.1:1430', changeOrigin: true, ws: false },
      ]),
    ),
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  build: {
    // WebKitGTK 2.40+ / Safari 16+ level features are fine.
    target: 'safari16',
    minify: !process.env.TAURI_ENV_DEBUG,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    chunkSizeWarningLimit: 2000,
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
