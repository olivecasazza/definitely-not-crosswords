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

  // Play a game — the heart of the demo. Available games are clickable cards
  // (onclick handlers, not <a> links) tagged "UNSTARTED".
  const card = page.getByText("UNSTARTED").first();
  if (await card.count()) {
    await card.click(); // -> /game/:id/new
    // Start the game.
    const start = page.getByRole("button", { name: /start game/i });
    await expect(start).toBeVisible({ timeout: 20_000 });
    await beat(page);
    await start.click();
    // Now on the board (/game/:id, no /new).
    await expect(page).toHaveURL(/\/game\/[^/]+$/, { timeout: 20_000 });
    await beat(page, 1800);

    // Pick a clue and type an answer, letter by letter, so the recording shows
    // the crossword being filled in. (We don't know the solution — this is a
    // demo of playing, not a solve.)
    const clue = page.locator(".cw-clue-row").first();
    if (await clue.count()) {
      await clue.click();
      await beat(page);
      const slots = page.locator(".cw-letter-input");
      const n = await slots.count();
      if (n > 0) {
        await slots.first().click();
        // Auto-advance moves focus to the next cell as each letter lands.
        await page.keyboard.type("CROSSWORDS".slice(0, n), { delay: 170 });
        await beat(page);
        const guess = page.getByRole("button", { name: /^guess$/i });
        if (await guess.count()) {
          await guess.click();
          await beat(page, 2000);
        }
      }
    }
  }

  // Round out the tour: the leaderboard.
  const stats = page.getByRole("link", { name: /^stats$/i }).first();
  if (await stats.count()) {
    await stats.click();
    await beat(page, 1800);
  }
});
