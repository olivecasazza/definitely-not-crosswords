import { test, expect } from "@playwright/test";

// Authenticated golden path — the product story, and the source of the demo
// video. Needs a staging test account (E2E_EMAIL / E2E_PASSWORD); skipped if
// absent so the canary still runs without creds. Login only (no signup), so it
// doesn't create throwaway users each run.

const EMAIL = process.env.E2E_EMAIL;
const PASSWORD = process.env.E2E_PASSWORD;

test.skip(!EMAIL || !PASSWORD, "E2E_EMAIL / E2E_PASSWORD not set");

// A couple of short holds ON THE MONEY SHOTS keep the recording watchable.
// These are for video pacing only — never used for synchronization (assertions
// below auto-wait), so they don't make the test flaky.
const beat = (page: import("@playwright/test").Page) => page.waitForTimeout(1200);

test("sign in and open a game", async ({ page }) => {
  await page.goto("/");
  await beat(page);

  // Sign in.
  await page.goto("/auth/login");
  await page.getByRole("textbox").first().fill(EMAIL!);
  await page.locator('input[type="password"]').fill(PASSWORD!);
  await page.getByRole("button", { name: /^sign in/i }).click();

  // Landed as an authenticated user (left the login page).
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });
  await beat(page);

  // Go to the games list and open the first playable game.
  await page.goto("/games");
  await expect(page.getByText(/available|active|completed/i).first()).toBeVisible();
  await beat(page);

  // Open a game (a card/link into /game/:id) and confirm the board renders.
  const gameLink = page.locator('a[href^="/game/"]').first();
  await expect(gameLink).toBeVisible({ timeout: 20_000 });
  await gameLink.click();
  await expect(page).toHaveURL(/\/game\//, { timeout: 20_000 });
  // The crossword board should be on screen (inputs = the grid cells).
  await expect(page.locator("input").first()).toBeVisible({ timeout: 20_000 });
  await beat(page);
});
