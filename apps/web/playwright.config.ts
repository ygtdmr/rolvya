import { defineConfig, devices } from "@playwright/test";
const engines = process.env.ROLVYA_BROWSER_ENGINES?.split(",") ?? ["chromium"];
export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  timeout: 45000,
  expect: { timeout: 8000 },
  reporter: [
    ["list"],
    ["json", { outputFile: ".artifacts/browser-results.json" }],
  ],
  outputDir: "test-results",
  use: {
    baseURL: "http://127.0.0.1:4174",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: engines.map((name) => ({
    name,
    use: {
      ...devices[
        name === "webkit"
          ? "Desktop Safari"
          : name === "firefox"
            ? "Desktop Firefox"
            : "Desktop Chrome"
      ],
      ...(name === "chromium" && process.env.ROLVYA_CHROMIUM
        ? { launchOptions: { executablePath: process.env.ROLVYA_CHROMIUM } }
        : {}),
    },
  })),
  webServer: [
    {
      command: "npm run preview",
      url: "http://127.0.0.1:4173",
      reuseExistingServer: !process.env.CI,
    },
    {
      command: "npm run preview:demo",
      url: "http://127.0.0.1:4174",
      reuseExistingServer: !process.env.CI,
    },
  ],
});
