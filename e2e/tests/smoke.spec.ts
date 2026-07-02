import { test, expect } from "@playwright/test";

// Unauthenticated canary — the always-on health check. No credentials required,
// no data written, so it's safe to run nightly. Locators are semantic
// (role/label/text), so a styling or layout refactor won't break it — only a
// real change to the user-facing flow will, which is exactly when you want to know.

test("home page loads and the WASM app hydrates", async ({ page }) => {
  await page.goto("/");
  await expect(page).toHaveTitle(/crosswords/i);
  // The app mounts into #main; hydration is done once real content appears.
  await expect(page.locator("#main")).not.toBeEmpty();
  // A primary call-to-action should be reachable from the landing page.
  await expect(
    page.getByRole("link", { name: /get started|sign in|play/i }).first(),
  ).toBeVisible();
});

test("login page renders the credentials form", async ({ page }) => {
  await page.goto("/auth/login");
  await expect(page.getByText(/email address/i)).toBeVisible();
  await expect(page.locator('input[type="password"]')).toBeVisible();
  await expect(page.getByRole("button", { name: /^sign in/i })).toBeVisible();
  // Path to sign-up is present.
  await expect(page.getByRole("link", { name: /sign up/i })).toBeVisible();
});

test("signup page renders the registration form", async ({ page }) => {
  await page.goto("/auth/signup");
  await expect(page.getByText(/full name/i)).toBeVisible();
  await expect(page.getByText(/email address/i)).toBeVisible();
  await expect(page.locator('input[type="password"]')).toBeVisible();
  await expect(page.getByRole("button", { name: /create account/i })).toBeVisible();
});

test("api health is green", async ({ request }) => {
  const res = await request.get("/api/healthz");
  expect(res.status()).toBe(200);
});
