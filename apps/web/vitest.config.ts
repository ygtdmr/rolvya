import { defineConfig } from "vitest/config";
export default defineConfig({
  define: { __DEMO__: "true", __BUILD_ID__: '"test"' },
  test: {
    environment: "jsdom",
    environmentOptions: { jsdom: { url: "https://control.invalid/" } },
    setupFiles: ["./src/testing/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    pool: "threads",
    maxWorkers: 2,
  },
});
