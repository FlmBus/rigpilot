import { defineConfig } from "vitest/config";

// Standalone config: the geometry/export helpers under src/lib are plain TS, so the
// tests run without the SvelteKit plugin (and without a browser environment).
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
