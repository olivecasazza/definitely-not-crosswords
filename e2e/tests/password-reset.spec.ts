import { test, expect } from "@playwright/test";

// Regression guard for the reset form reloading the page and dropping the
// ?token=. The form's only defence was e.prevent_default() inside the onsubmit
// handler, which lost the race against the browser's native submit: the reload
// stripped the token (the inputs carry no `name`), the component remounted into
// email-request mode, and the in-flight user.resetPassword POST was cancelled.
// The reset silently didn't take and the next sign-in failed with "Invalid
// email or password" — intermittently, since it was a race.
//
// Nothing else covered this: the canary is unauthenticated and the demo spec
// signs in with an already-valid password, so the reset route shipped
// unexercised.
//
// No credentials and no inbox needed — the reload strips the URL whether or not
// the token is real, so an unknown-but-well-formed token exercises the same
// path. The server replies "Invalid or expired reset link.", which the page
// renders as the "Link Expired" card. Falling back to the email-request form
// instead means the token was lost, i.e. the bug is back.

test("submitting the reset form does not reload the page or drop the token", async ({ page }) => {
  const token = `reset_${"0".repeat(32)}`;

  // Count documents, not window state: a reload gets a fresh `window`, so a
  // counter on it can never exceed 1. sessionStorage survives same-origin
  // navigation, so this actually distinguishes "reloaded" from "didn't".
  await page.addInitScript(() => {
    sessionStorage.setItem("__docs", String(Number(sessionStorage.getItem("__docs") || "0") + 1));
  });

  await page.goto(`/auth/reset-password?token=${token}`);

  const pw = page.locator("#new-password");
  await expect(pw).toBeVisible();

  const secret = "NotARealPassword123";
  await pw.fill(secret);
  await page.locator("#confirm-password").fill(secret);
  await page.getByRole("button", { name: /set new password/i }).click();

  // Expected: the token is rejected server-side and we get the "Link Expired"
  // card. The regression instead reloaded into the email-request form.
  await expect(page.getByText(/link expired/i)).toBeVisible({ timeout: 20_000 });
  await expect(page.getByRole("button", { name: /send reset link/i })).toHaveCount(0);

  // The query string and the document count are the direct evidence. The
  // regression navigated to "/auth/reset-password?" — a bare `?` is the
  // signature of a native GET submit with no named inputs.
  await expect(page).toHaveURL(/token=/);
  expect(await page.evaluate(() => sessionStorage.getItem("__docs"))).toBe("1");
});
