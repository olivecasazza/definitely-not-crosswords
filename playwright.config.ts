import { defineConfig, devices } from "@playwright/test"

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "html",
  use: {
    trace: "on-first-retry",
    browserName: "chromium",
    launchOptions: {
      executablePath: "/home/olive/.nix-profile/bin/google-chrome-stable",
    },
    nuxt: {
      rootDir: __dirname,
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
      use: {
        ...devices["Desktop Chromium"],
      },
      dependencies: ["setup"],
    },
  ],
})
