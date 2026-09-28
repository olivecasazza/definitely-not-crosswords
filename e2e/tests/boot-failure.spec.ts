import { test, expect } from "@playwright/test";

// DEF-148 (spec DEF-146 §10 AC 1-3 and 12): a failed bundle load must be a card
// the user can act on, never a white page, and the shell must make exactly ONE
// automatic retry — not a loop against the edge.
//
// These run against the deployed shell with no credentials and write no data.
// Everything is asserted through the document the shell serves, so they stay
// valid across app refactors.

/**
 * The top-level glue only. Its own `./snippets/…` imports must NOT match: the
 * shell puts the `?r=` cache-buster on this one URL and nowhere else, because the
 * bundle path is content-addressed (client/flake.nix). A log that counted every
 * /_assets/ request could not tell "one retry" from "one retry plus every
 * snippet refetched with a query".
 */
const GLUE = /\/_assets\/[^/]+\/crossword-web\.js(\?|$)/;

test("a blocked bundle load shows a recoverable card, not a white page", async ({
  page,
}) => {
  const glue: string[] = [];
  page.on("request", (req) => {
    if (GLUE.test(req.url())) glue.push(req.url());
  });
  await page.route("**/_assets/**", (route) => route.abort());

  await page.goto("/");

  // AC 1: the failure state, with both recovery actions and the reason line.
  const retry = page.getByRole("button", { name: /^retry$/i });
  await expect(
    page.getByRole("heading", { name: /couldn't load the app/i }),
  ).toBeVisible();
  await expect(retry).toBeVisible();
  await expect(page.getByRole("button", { name: /^reload$/i })).toBeVisible();
  // The reason line is what makes this reportable rather than a shrug.
  await expect(page.locator("#boot-err")).not.toBeEmpty();
  // Still a live region, promoted to an alert for the failure, and the app mount
  // root is untouched: this is a card over an empty page, not the app
  // half-rendered underneath it.
  await expect(page.locator("#boot")).toHaveAttribute("role", "alert");
  await expect(page.locator("#main")).toBeEmpty();
  await expect(page.locator(".app-shell")).toHaveCount(0);

  // AC 2: exactly two glue requests, the second carrying the cache-buster.
  // Polled because the retry is deliberately delayed 1.5s (spec §4); the card
  // above is only shown once that retry has already failed, so by here the
  // count is settled and the two plain expects below are the real assertions.
  await expect.poll(() => glue.length, { timeout: 30_000 }).toBe(2);
  expect(glue[0]).not.toContain("?r=");
  expect(glue[1]).toContain("?r=");
  // Nothing further: the card owns the clock now. A retry loop would have added
  // a third request by the time the card is up.
  expect(glue).toHaveLength(2);

  // AC 7: focus lands on the one control that can fix this.
  await expect(retry).toBeFocused();
});

test("Retry boots the app and removes the card", async ({ page }) => {
  // Fail the first attempt and its automatic retry, then heal the network and
  // press the button — the exact sequence from spec AC 3.
  let blocked = true;
  await page.route("**/_assets/**", (route) =>
    blocked ? route.abort() : route.continue(),
  );

  await page.goto("/");
  const retry = page.getByRole("button", { name: /^retry$/i });
  await expect(retry).toBeVisible();

  blocked = false;
  await retry.click();

  // AC 3: the app boots, #boot is gone, and .app-shell is what is left.
  await expect(page.locator(".app-shell")).toBeVisible();
  await expect(page.locator("#boot")).toHaveCount(0);
});
