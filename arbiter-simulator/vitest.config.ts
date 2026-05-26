import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config";

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      include: ["src/**/*.test.ts"],
      pool: "forks",
    },
    optimizeDeps: {
      include: ["policy-core-wasm"],
    },
    ssr: {
      noExternal: ["policy-core-wasm"],
    },
  }),
);
