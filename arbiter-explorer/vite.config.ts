import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import wasm from "vite-plugin-wasm";

export default defineConfig({
  plugins: [wasm(), svelte()],
  server: {
    port: 5199,
    host: "127.0.0.1",
    fs: {
      allow: [".."],
    },
  },
  optimizeDeps: {
    exclude: ["arbiter-wasm"],
  },
  build: {
    target: "esnext",
  },
});
