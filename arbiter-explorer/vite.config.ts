import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5199,
    host: '127.0.0.1',
    fs: {
      allow: ['..'],
    },
  },
  optimizeDeps: {
    exclude: ['arbiter-wasm'],
  },
  build: {
    target: 'esnext',
  },
});
