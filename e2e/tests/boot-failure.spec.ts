import { test, expect } from "@playwright/test";

// DEF-148 (spec DEF-146 §10 AC 1-3 and 12): a failed bundle load must be a card
// the user can act on, never a white page, and the shell must make exactly ONE
// automatic retry — not a loop against the edge. DEF-171 covers AC 4, the
// scripting-disabled state, at the bottom of this file.
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

  // DEF-183 D3: in the loading state the error line and the action row are already
  // in the DOM and already OCCUPY SPACE — that reservation is what keeps the card
  // one fixed height across loading → slow → failed (CLS 0.056 → 0). They are held
  // out of the accessibility tree by aria-hidden, not by `hidden`/`display:none`,
  // which would collapse the row and move the card.
  const reserved = await page.locator("#boot-err").boundingBox();
  expect(reserved?.height ?? 0).toBeGreaterThan(0);
  await expect(page.locator("#boot-err")).toBeHidden();
  await expect(page.locator("#boot-actions")).toBeHidden();

  // AC 1: the failure state, with both recovery actions and the reason line.
  const retry = page.getByRole("button", { name: /^retry$/i });
  await expect(
    page.getByRole("heading", { name: /couldn't load the app/i }),
  ).toBeVisible();
  await expect(retry).toBeVisible();
  await expect(page.getByRole("button", { name: /^reload$/i })).toBeVisible();
  // The reason line is what makes this reportable rather than a shrug.
  await expect(page.locator("#boot-err")).not.toBeEmpty();
  // Same rows, now shown: still present, now painted, and no longer aria-hidden.
  await expect(page.locator("#boot-err")).toBeVisible();
  await expect(page.locator("#boot-actions")).toBeVisible();
  await expect(page.locator("#boot-actions")).toHaveAttribute("aria-hidden", "false");
  // DEF-183 D2: the live region announces POLITELY, through aria-live — an explicit
  // aria-live beats the implicit value of `role`, so the old status->alert swap was
  // inert. Assert the attribute that decides the announcement, not the one that
  // used to be swapped; the card is a status in every state.
  await expect(page.locator("#boot")).toHaveAttribute("aria-live", "polite");
  await expect(page.locator("#boot")).toHaveAttribute("role", "status");
  // The app mount root is untouched: this is a card over an empty page, not the app
  // half-rendered underneath it.
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

// DEF-171 (spec DEF-146 §10 AC 4): the fourth state of the shell is `noscript`,
// and it was unreachable. With scripting disabled the `<script type="module">`
// never runs, so `boot()` is never called, `fail()` never runs, and no timer is
// ever armed — #boot keeps its initial markup forever and the user is left on a
// card that says "Loading / Fetching the app…" indefinitely. That is the exact
// false state the spec exists to prevent, and it is worse than a simple overlap:
// #boot is `position:fixed;inset:0;z-index:400` with no background of its own,
// so it paints above the in-flow noscript card and is what the user reads.
//
// The fix is one `<noscript><style>` in <head>. `<noscript>` in <head>` is
// parsed as MARKUP only when scripting is disabled and as raw text when it is
// enabled, so the rule can only ever apply on the no-JS path and is inert for
// every normal user. `!important` beats the normal `display:flex` regardless of
// source order.
//
// This is the assertion DEF-171's author could not make: no browser was
// available in that container, so the claim was derived from the CSS plus a
// jsdom parse of both scripting branches. `javaScriptEnabled: false` is the
// real-browser branch.
test.describe("with scripting disabled", () => {
  // A context option, not a page option: it has to be decided before the
  // context is created, so it goes on the describe, not on the test.
  test.use({ javaScriptEnabled: false });

  test("the noscript card is the only card on screen, not a false Loading", async ({
    page,
  }) => {
    // No bundle was ever requested: with scripting off there is no loader to
    // fetch one, so a request here would mean the shell is doing work it cannot
    // finish. Registered before the first navigation so the reload is covered.
    const glue: string[] = [];
    page.on("request", (req) => {
      if (GLUE.test(req.url())) glue.push(req.url());
    });

    await page.goto("/");

    // The false state, gone. #boot is display:none via the noscript rule, so it
    // is not rendered at all — not merely empty, not merely off-screen. The
    // element is still in the DOM (the rule hides it, it does not remove it), so
    // assert the rendering, not the existence.
    await expect(page.locator("#boot")).toBeHidden();
    await expect(page.locator("#boot")).toHaveCount(1);
    // The specific words the user must never be left reading. These are still in
    // the DOM inside #boot — getByText matches hidden elements, so assert that
    // they are not rendered rather than that they are absent.
    await expect(page.locator("#boot-title")).toHaveText("Loading");
    await expect(page.locator("#boot-title")).toBeHidden();
    await expect(page.locator("#boot-body")).toBeHidden();

    // The noscript card is what is read instead, and it is the ONLY card on
    // screen. There are two .boot-card elements in the DOM — one in #boot, one
    // in <noscript> — so the count has to be of RENDERED cards: `:visible`, not a
    // bare count, which would pass or fail on DOM presence and say nothing about
    // what the user sees. This single assertion is what separates the two states.
    const cards = page.locator(".boot-card:visible");
    await expect(cards).toHaveCount(1);
    // And it is specifically the noscript one, not a surviving boot card.
    await expect(page.locator("#boot .boot-card")).toBeHidden();
    await expect(page.locator("noscript .boot-card")).toBeVisible();
    await expect(
      page.getByRole("heading", { name: /javascript is required/i }),
    ).toBeVisible();
    await expect(
      page.getByText(/needs JavaScript enabled to run/i),
    ).toBeVisible();

    // It is the most prominent element on screen, not a footnote. DEF-183 moved
    // the noscript card to a fixed full-viewport overlay, so the thing the user
    // sees is centred and covers the whole page. Assert the geometry, not just
    // the visibility: a card that is visible but 4rem from the top is the
    // DEF-171 defect, and visibility alone cannot tell the two apart.
    //
    // The overlay is the full-viewport box; the card inside it is centred by
    // flexbox, so its centre is the viewport centre. Assert both, and assert the
    // card is NOT the boot card's position (the boot card is also centred, which
    // is why the count above is the assertion that actually separates them).
    const overlay = page.locator(".boot-noscript");
    await expect(overlay).toBeVisible();
    const obox = await overlay.boundingBox();
    const viewport = page.viewportSize()!;
    expect(obox).not.toBeNull();
    expect(Math.abs(obox!.width - viewport.width)).toBeLessThanOrEqual(2);
    expect(Math.abs(obox!.height - viewport.height)).toBeLessThanOrEqual(2);

    const cbox = await cards.boundingBox();
    expect(cbox).not.toBeNull();
    const cx = cbox!.x + cbox!.width / 2;
    const cy = cbox!.y + cbox!.height / 2;
    expect(Math.abs(cx - viewport.width / 2)).toBeLessThanOrEqual(2);
    expect(Math.abs(cy - viewport.height / 2)).toBeLessThanOrEqual(2);

    // The app mount root is untouched: this is a card over an empty page.
    await expect(page.locator("#main")).toBeEmpty();
    await expect(page.locator(".app-shell")).toHaveCount(0);

    await page.reload();
    expect(glue).toHaveLength(0);
  });
});

// Deliberately a SIBLING of the describe above, not a test inside it.
// `test.use({ javaScriptEnabled: false })` is scoped to the describe and is
// fixed when the context is created, so a test placed inside it cannot get a
// scripting-enabled page from the `page` fixture. This is the converse half of
// DEF-171 AC 4 and the reason the fix is safe: the same document served to a
// normal user must still boot. If the <noscript><style> ever leaked into the JS
// path, every user would get the no-JS card and the app would never mount — a
// far worse failure than the one being fixed.
test("the noscript rule is inert for a scripting user", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#boot")).toBeVisible();
  await expect(page.locator(".boot-card")).toHaveCount(1);
  await expect(
    page.getByRole("heading", { name: /javascript is required/i }),
  ).toHaveCount(0);
  // And the app still boots from the same document, which is the stronger half
  // of the claim: #boot being visible would not survive a leak of the rule.
  await expect(page.locator(".app-shell")).toBeVisible();
  await expect(page.locator("#boot")).toHaveCount(0);
});
