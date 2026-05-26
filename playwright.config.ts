import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    baseURL: 'http://localhost:3011',
    trace: 'on-first-retry',
    browserName: 'chromium',
    launchOptions: {
      executablePath: '/home/olive/.nix-profile/bin/google-chrome-stable',
    },
    nuxt: {
      rootDir: __dirname,
    },
  },
})
