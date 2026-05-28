import { defineConfig, devices } from "@playwright/test"

const isCiReporter = process.env.PW_REPORT_FORMAT === "ci"

const launchOptions = {
  ...(process.env.CI
    ? {}
    : {
        executablePath: "/home/olive/.nix-profile/bin/google-chrome-stable",
      }),
}

const reporters = isCiReporter
  ? [
      ["html", { open: "never" }],
      ["junit", { outputFile: "test-results/junit-results.xml" }],
    ]
  : "html"

export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: 1,
  reporter: reporters,
  use: {
    trace: "on-first-retry",
    browserName: "chromium",
    launchOptions,
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
