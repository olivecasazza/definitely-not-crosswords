import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: 'http://127.0.0.1:3017',
    trace: 'on-first-retry',
    browserName: 'chromium',
    launchOptions: {
      executablePath: '/home/olive/.nix-profile/bin/google-chrome-stable',
    },
  },
  webServer: {
    command: 'nix develop --command node node_modules/nuxt/bin/nuxt.mjs dev --port 3017 --host 127.0.0.1',
    url: 'http://127.0.0.1:3017',
    reuseExistingServer: false,
    stdout: 'ignore',
    stderr: 'pipe',
    timeout: 120000,
  },
})
