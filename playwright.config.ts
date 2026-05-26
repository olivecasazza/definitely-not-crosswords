import { defineConfig, devices } from "@playwright/test"

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "html",
  use: {
    baseURL: "http://localhost:3000",
    trace: "on-first-retry",
    browserName: "chromium",
    launchOptions: {
      executablePath: "/home/olive/.nix-profile/bin/google-chrome-stable",
    },
    // Configure the storageState path globally
    storageState: "playwright/.auth/user.json",
  },
  projects: [
    // Setup project to authenticate
    {
      name: "setup",
      testMatch: /auth\.setup\.ts/,
      use: {
        // Clear storage state for the setup project so it starts unauthenticated
        storageState: { cookies: [], origins: [] },
      },
    },
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
      dependencies: ["setup"],
    },
  ],
  webServer: {
    command: "node node_modules/nuxt/bin/nuxt.mjs dev",
    url: "http://localhost:3000",
    reuseExistingServer: !process.env.CI,
    stdout: "ignore",
    stderr: "pipe",
    timeout: 120000,
  },
})
