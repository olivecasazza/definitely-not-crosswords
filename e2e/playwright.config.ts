import { defineConfig, devices } from "@playwright/test";

// One Playwright project drives both jobs:
//   • smoke.spec.ts — the always-on canary (unauthenticated; no creds needed)
//   • demo.spec.ts  — the authenticated golden path (records the demo video;
//                     skipped unless E2E_EMAIL/E2E_PASSWORD are set)
//
// Target defaults to staging; override with E2E_BASE_URL.
const baseURL = process.env.E2E_BASE_URL ?? "https://crosswords-staging.casazza.io";

export default defineConfig({
  testDir: "./tests",
  // A canary must be trustworthy: no arbitrary sleeps anywhere — rely on
  // Playwright's auto-waiting + web-first assertions (handles WASM hydration).
  timeout: 60_000,
  expect: { timeout: 15_000 },
  // One retry absorbs transient network/cold-start flake without hiding real
  // regressions (a real break fails both attempts).
  retries: 1,
  reporter: [["list"], ["html", { open: "never" }]],
  use: {
    baseURL,
    // 1080p so the recording doubles as a shareable demo clip. `size` forces the
    // video to full res (Playwright otherwise downscales the recording to ~800px).
    viewport: { width: 1920, height: 1080 },
    video: { mode: "on", size: { width: 1920, height: 1080 } },
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    actionTimeout: 15_000,
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // The device descriptor resets the viewport to 1280x720 — re-assert
        // 1080p so the page fills the 1920x1080 recording canvas (otherwise
        // the video gets grey gutters on the right and bottom).
        viewport: { width: 1920, height: 1080 },
      },
    },
  ],
});
