import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import wasm from "vite-plugin-wasm";

export default defineConfig({
  base: process.env.BASE_PATH || "/",
  plugins: [wasm(), svelte()],
  server: {
    port: 5199,
    host: "127.0.0.1",
    fs: {
      allow: [".."],
    },
  },
  optimizeDeps: {
    exclude: ["policy-core-wasm", "arbiter-core-wasm"],
  },
  build: {
    target: "esnext",
  },
});
