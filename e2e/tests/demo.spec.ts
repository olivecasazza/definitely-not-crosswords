import { test, expect } from "@playwright/test";

// Authenticated product tour — the source of the demo video. Needs a staging
// test account (E2E_EMAIL / E2E_PASSWORD); self-skips if absent so the canary
// still runs without creds. Login only (no signup) -> no throwaway users.
// Deliberately resilient: it tours whatever's there (staging may have no games),
// so it produces a clean video every run rather than hard-failing on data.

const EMAIL = process.env.E2E_EMAIL;
const PASSWORD = process.env.E2E_PASSWORD;

test.skip(!EMAIL || !PASSWORD, "E2E_EMAIL / E2E_PASSWORD not set");

// Short holds on the money shots keep the recording watchable. Pacing only —
// never used for synchronization (assertions auto-wait), so no added flakiness.
const beat = (page: import("@playwright/test").Page, ms = 1400) =>
  page.waitForTimeout(ms);

test("authenticated product tour", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#main")).not.toBeEmpty();
  await beat(page);

  // Sign in through the UI (SPA nav — avoids the cold deep-link path).
  await page.getByRole("link", { name: /^sign in$/i }).first().click();
  await expect(page).toHaveURL(/\/auth\/login/);
  await page.locator('input[type="email"]').fill(EMAIL!);
  await page.locator('input[type="password"]').fill(PASSWORD!);
  await beat(page);
  await page.getByRole("button", { name: /^sign in/i }).click();
  // Signed in -> left the login page.
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });
  await beat(page);

  // Tour the games dashboard.
  await page.getByRole("link", { name: /^games$/i }).first().click();
  await expect(page).toHaveURL(/\/games/, { timeout: 15_000 });
  await beat(page, 1800);

  // If a playable game exists, open it and show the board (optional — staging
  // may have none, and the tour should still produce a video).
  const gameLink = page.locator('a[href^="/game/"]').first();
  if (await gameLink.count()) {
    await gameLink.click();
    await expect(page).toHaveURL(/\/game\//, { timeout: 15_000 });
    await beat(page, 2200);
  }

  // Leaderboard, then profile — round out the tour.
  const stats = page.getByRole("link", { name: /stats|leaderboard/i }).first();
  if (await stats.count()) {
    await stats.click();
    await beat(page, 1600);
  }
  const profile = page.getByRole("link", { name: /^profile$/i }).first();
  if (await profile.count()) {
    await profile.click();
    await beat(page, 1600);
  }
});
