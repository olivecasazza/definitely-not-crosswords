import { test, expect } from "@playwright/test";

// Regression guard for the fresh-start crash (nav.push from outside the
// Dioxus runtime → RuntimeError → poisoned router). The demo/layout specs
// prefer RESUMING an active game, so the start-a-new-game path shipped
// unexercised — this spec drives it explicitly and fails on any wasm panic.

const EMAIL = process.env.E2E_EMAIL;
const PASSWORD = process.env.E2E_PASSWORD;
test.skip(!EMAIL || !PASSWORD, "E2E_EMAIL / E2E_PASSWORD not set");

test("starting a new game does not panic", async ({ page }) => {
  const panics: string[] = [];
  page.on("pageerror", (e) => panics.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error" && /panicked at/.test(m.text())) panics.push(m.text());
  });

  await page.goto("/auth/login");
  await page.locator('input[type="email"]').fill(EMAIL!);
  await page.locator('input[type="password"]').fill(PASSWORD!);
  await page.getByRole("button", { name: /^sign in/i }).click();
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });

  await page.goto("/games");
  await expect(page.getByText("Library").first()).toBeVisible();
  const fresh = page
    .locator('div[style*="cursor: pointer"]')
    .and(page.locator('[aria-label*="— NEW"]'))
    .first();
  try {
    await fresh.waitFor({ state: "visible", timeout: 20_000 });
  } catch {
    test.skip(true, "no unstarted game available on staging");
  }
  await fresh.click();

  // Pre-game brief → Start. Fresh generation can be slow server-side.
  const start = page.getByRole("button", { name: /^(start game|continue game)$/i });
  await expect(start).toBeVisible({ timeout: 20_000 });
  await start.click();
  await expect(page).not.toHaveURL(/\/game\/[^/]+\/new$/, { timeout: 150_000 });
  await expect(page.locator(".cw-letter").first()).toBeVisible({ timeout: 30_000 });

  expect(panics, `wasm panics during fresh start:\n${panics.join("\n")}`).toEqual([]);
});
