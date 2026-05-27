process.env.AUTH_ORIGIN = 'http://localhost:3000';
process.env.NUXT_AUTH_ORIGIN = 'http://localhost:3000';

const { defineConfig } = require('@playwright/test');

module.exports = defineConfig({
  testDir: './tests/e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',
  use: {
    trace: 'on-first-retry',
    browserName: 'chromium',
    launchOptions: {
      executablePath: '/home/olive/.nix-profile/bin/google-chrome-stable',
    },
    nuxt: {
      rootDir: __dirname,
    },
  },
});
