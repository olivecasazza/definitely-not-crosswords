import { test, expect, type Page, type Route } from "@playwright/test";

// The play screen's pre-board states — DEF-195, spec DEF-180.
//
// /game/:id spent its whole first paint on `<div class="container"><p
// class="muted">Loading game…</p></div>`, and its failure on a bare <h1> + <p> +
// <Link>. Two things you see before you see a crossword, and the only surfaces
// in the app with no card around them: no shared vocabulary, and nothing
// announced to a screen reader.
//
// Both states now render one card. These specs cover the acceptance criteria
// that need a real browser: AC#1 (a card, not a paragraph), AC#2 (a bar, not a
// spinner), AC#3 (reduced motion needs no app CSS), AC#4/AC#6 (contrast, both
// themes), AC#5 (the error card has a recovery action), AC#7 (no resize between
// the two states) and AC#8 (announced, focus untouched).
//
// No credentials, and no dependency on a game existing: the tRPC query that
// picks the state is gated and answered here, so both states are reachable on
// staging by any run — `pnpm canary` included. Nothing is asserted about the
// staging deployment's data.

/** The one request that decides which pre-board state renders. */
const ACTIVE_GAME = "**/api/trpc/activeGame.get?*";

/**
 * A tRPC batch envelope carrying a null result — what `activeGame.get` answers
 * for a game that does not exist. The client reads it as `Ok(Null)`, which is
 * the `load_error` arm (net.rs → game_play.rs:383).
 */
const NOT_FOUND = JSON.stringify([{ result: { data: null } }]);

/**
 * Hold `activeGame.get` at the door until `release()`, then answer it.
 *
 * The loading state is a request in flight, so the only honest way to test it
 * is to not let the request finish: there is no other hook between "the app
 * mounted" and "the board is there". Holding the route is also what makes the
 * error state reachable on a run with no account — the app never learns whether
 * the id is real, because the test answers.
 */
function gateActiveGame(page: Page) {
  const held: Route[] = [];
  let released = false;
  const answer = (route: Route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: NOT_FOUND });
  page.route(ACTIVE_GAME, (route) => {
    if (released) {
      void answer(route);
      return;
    }
    held.push(route);
  });
  return {
    /** How many loads are being held. Proves the app asked before we assert. */
    held: () => held.length,
    /** Let the load finish, so the app can move to its next state. */
    release: async () => {
      released = true;
      await Promise.all(held.splice(0).map(answer));
    },
  };
}

/** Load the play screen for a game that will never arrive, and wait for the card. */
async function openGatedPlayScreen(page: Page) {
  const gate = gateActiveGame(page);
  await page.goto("/game/e2e-play-status-card");
  const card = page.locator(".gp-status-card");
  await expect(card).toBeVisible({ timeout: 30_000 });
  expect(gate.held(), "the play screen never asked for the game").toBeGreaterThan(0);
  return { gate, card };
}

/**
 * The computed value of a design token, read by painting it.
 *
 * Copied from connection-state.spec.ts: `getPropertyValue` on a custom property
 * returns its own text (`var(--pastel-yellow)`), not the resolved colour, so
 * resolve it the way the browser does. Keeping assertions pinned to the TOKEN
 * means a palette change fails the design that owns it.
 */
async function tokenColor(page: Page, token: string): Promise<string> {
  return page.evaluate((t) => {
    const probe = document.createElement("div");
    probe.style.background = `var(${t})`;
    document.body.appendChild(probe);
    const resolved = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return resolved;
  }, token);
}

/** WCAG 2.x relative luminance of a computed `rgb()` / `rgba()` colour. */
function luminance(colour: string): number {
  const m = colour.match(/rgba?\(\s*([\d.]+),\s*([\d.]+),\s*([\d.]+)/);
  if (!m) throw new Error(`not an rgb() colour: ${colour}`);
  const channel = (raw: string) => {
    const c = Number(raw) / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * channel(m[1]) + 0.7152 * channel(m[2]) + 0.0722 * channel(m[3]);
}

/** WCAG 2.x contrast ratio between two computed colours: 1:1 … 21:1. */
function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** Round for a failure message, so a reader does not do the arithmetic. */
const ratio = (n: number) => `${n.toFixed(2)}:1`;

/**
 * Flip `.light-mode` on `<html>` — the class the header toggle sets — run `body`,
 * then put the page back. Contrast is a per-theme property of the token
 * cascade, and the light palette is a separate set of values, not an inversion.
 */
async function inEachTheme(
  page: Page,
  body: (theme: "dark" | "light") => Promise<void>,
): Promise<void> {
  const wasLight = await page.evaluate(() =>
    document.documentElement.classList.contains("light-mode"),
  );
  try {
    for (const theme of ["dark", "light"] as const) {
      await page.evaluate(
        (t) => document.documentElement.classList.toggle("light-mode", t === "light"),
        theme,
      );
      await body(theme);
    }
  } finally {
    await page.evaluate(
      (l) => document.documentElement.classList.toggle("light-mode", l),
      wasLight,
    );
  }
}

test("a cold play screen is a card with an indeterminate bar, not a paragraph", async ({
  page,
}) => {
  const { card } = await openGatedPlayScreen(page);

  // AC#1 — the defect, asserted as its absence. The old markup was exactly
  // `.container > p.muted`, so this fails if the card ever regresses to it
  // rather than merely passing while both are on screen.
  await expect(page.locator(".gp-status-card")).toBeVisible();
  expect(
    await page.locator(".container p.muted").count(),
    "the bare-paragraph first paint is back",
  ).toBe(0);
  await expect(card.locator("h1.gp-status-title")).toHaveText(/loading game/i);

  // AC#2 — a bar, not a spinner. panel-kit's own contract is that a pending
  // page-level load renders a ProgressBar and never a Spinner, so the
  // indeterminate fill has to be there...
  await expect(card.locator(".pk-progress")).toHaveCount(1);
  await expect(card.locator(".pk-progress-fill.indeterminate")).toHaveCount(1);
  // ...and nothing in the card may carry a spinner/shimmer/pulse animation.
  // `pk-indet` is the one animation that belongs here; the check is on the
  // NAMES, so it does not fail the bar's own translate loop.
  const names = await page.evaluate(() => {
    const cardEl = document.querySelector<HTMLElement>(".gp-status-card");
    if (!cardEl) throw new Error("no .gp-status-card");
    const seen = new Set<string>();
    for (const el of [cardEl, ...cardEl.querySelectorAll<HTMLElement>("*")]) {
      seen.add(getComputedStyle(el).animationName);
    }
    return [...seen];
  });
  for (const name of names) {
    expect(name, "a spinner-class animation inside the loading card").not.toMatch(
      /spin|shimmer|pulse/i,
    );
  }

  // AC#8 — announced, and not focused. The card is not interactive: the
  // recovery link is in the DOM (that is what reserves its space) but hidden,
  // and `visibility: hidden` takes it out of the tab order with it.
  await expect(card).toHaveAttribute("role", "status");
  await expect(card).toHaveAttribute("aria-live", "polite");
  await expect(card).toHaveAttribute("aria-busy", "true");
  expect(
    await page.evaluate(() => document.activeElement?.tagName.toLowerCase()),
    "the loading card stole focus",
  ).toBe("body");
  await expect(card.locator(".gp-status-actions")).toHaveAttribute("aria-hidden", "true");
  expect(
    await page.locator(".gp-status-actions a:visible").count(),
    "the loading card has a focusable control in it",
  ).toBe(0);
});

test("reduced motion needs no app CSS: panel-kit's own guard already holds", async ({
  page,
}) => {
  const { card } = await openGatedPlayScreen(page);
  const fill = card.locator(".pk-progress-fill.indeterminate");
  await expect(fill).toBeVisible();

  // Motion first, so the difference below is the guard and not a browser that
  // never had it. The fill is mid-animation while it runs, so this only pins
  // that the animation EXISTS — that is the half that has to be there.
  expect(await fill.evaluate((el) => getComputedStyle(el).animationName)).toMatch(
    /^pk-indet$/,
  );

  await page.emulateMedia({ reducedMotion: "reduce" });

  // AC#3 — the whole point: the app adds no rule for this. panel-kit's
  // stylesheet already stops the animation AND holds the bar at a static 40%
  // sliver, because a full-width bar with no motion reads as "done".
  const measured = await fill.evaluate((el) => {
    const track = el.parentElement!;
    // `width: 40%` resolves against the track's CONTENT box. The track has a
    // 1px border and no padding, so clientWidth IS the content width — and
    // getComputedStyle reports computed width in px, so the honest runtime
    // form of "40%" is the fill's share of that.
    const inner = track.clientWidth;
    return {
      animationName: getComputedStyle(el).animationName,
      share: inner > 0 ? el.getBoundingClientRect().width / inner : NaN,
    };
  });
  expect(measured.animationName, "the bar is still animating under reduce").toBe("none");
  expect(measured.share, "the fill is no longer a partial sliver").toBeCloseTo(0.4, 2);

  await page.emulateMedia({ reducedMotion: null });
});

test("a failed load keeps the card, adds a way out, and does not resize it", async ({
  page,
}) => {
  const { gate, card } = await openGatedPlayScreen(page);

  // AC#7's first half: the height BEFORE the state changes. Measured, not
  // assumed, and taken from the same element the second reading comes from.
  const before = await page.evaluate(
    () => document.querySelector<HTMLElement>(".gp-status-card")!.offsetHeight,
  );
  expect(before, "the loading card has no height at all").toBeGreaterThan(0);

  await gate.release();

  // AC#5 — the failure is a card with a recovery action, not a bare h1.
  await expect(card).toBeVisible();
  await expect(card.locator("h1.gp-status-title")).toHaveText(/couldn't load/i);
  await expect(card.locator(".gp-status-detail")).toHaveText(/game not found/i);
  const back = card.getByRole("link", { name: /back to games/i });
  await expect(back).toBeVisible();
  await expect(back).toHaveAttribute("href", "/games");

  // AC#8 — the error state announces itself assertively, and still does not
  // take focus: a user mid-keyboard-navigation must not be yanked.
  await expect(card).toHaveAttribute("role", "alert");
  await expect(card).toHaveAttribute("aria-busy", "false");
  expect(
    await page.evaluate(() => document.activeElement?.tagName.toLowerCase()),
    "the error card stole focus",
  ).toBe("body");
  // One tab stop, and it is the link — the hidden loading-state bar and detail
  // must not be reachable.
  expect(
    await page.locator(".gp-status-card a").count(),
    "the error card has more than one link in it",
  ).toBe(1);

  // AC#7 — the card's height is the SAME in both states. The body slot reserves
  // three lines and the action row is always rendered, so a one-line
  // "Game not found" cannot leave the card shorter than the load it replaced.
  // DEF-183 D3 measured CLS 0.056 for exactly this growth on the boot card
  // (122.8px loading -> 294.0px failed).
  const after = await page.evaluate(
    () => document.querySelector<HTMLElement>(".gp-status-card")!.offsetHeight,
  );
  expect(after, `the card resized between states (${before}px -> ${after}px)`).toBe(before);

  // AC#9 — a 360px viewport must not scroll sideways. A network failure carries
  // a URL, so the detail has to wrap rather than widen the card. The card's own
  // width is the half of this that is not already guaranteed: `.app-shell` is
  // `overflow: hidden`, so the document's scrollWidth reads 360 whatever the
  // card does.
  await page.setViewportSize({ width: 360, height: 720 });
  const scrollWidth = await page.evaluate(
    () => document.documentElement.scrollWidth,
  );
  expect(scrollWidth, "the error card is wider than a 360px viewport").toBeLessThanOrEqual(
    360,
  );
  expect(
    await page.evaluate(
      () => document.querySelector(".gp-status-card")!.getBoundingClientRect().width,
    ),
    "the card itself overflows a 360px viewport",
  ).toBeLessThanOrEqual(360);
});

test("the error card clears AA, and its action clears 3:1, in both themes", async ({
  page,
}) => {
  const { gate, card } = await openGatedPlayScreen(page);
  await gate.release();
  await expect(card).toBeVisible();

  await inEachTheme(page, async (theme) => {
    // Pin each surface to the token the design claims, THEN measure. Two
    // reasons, and both are the reason the token is checked at all:
    //   • a palette change fails here, in the spec that claims the number;
    //   • `body` and `.app-btn` cross-fade `color`/`border-color` over .15s, so
    //     a colour read immediately after the flip is a mid-transition value.
    //     `toHaveCSS` is a web-first assertion: it polls until the settled
    //     value is there, which is a wait with a condition rather than a sleep.
    const surface = await tokenColor(page, "--bg-card");
    const primary = await tokenColor(page, "--text-primary");
    const secondary = await tokenColor(page, "--text-secondary");
    await expect(card).toHaveCSS("background-color", surface);
    await expect(card.locator(".gp-status-title")).toHaveCSS("color", primary);
    await expect(card.locator(".gp-status-detail")).toHaveCSS("color", secondary);
    await expect(card.locator(".gp-status-actions a")).toHaveCSS("color", secondary);
    // The action's boundary is the DEF-188 D1 fix: --text-secondary, not the
    // --border-app that sits at 1.19:1 on a dark card.
    await expect(card.locator(".gp-status-actions a")).toHaveCSS(
      "border-top-color",
      secondary,
    );

    // AC#4 — body text, so AA at 4.5:1. Design intent from the spec's token
    // table: title 16.12:1 dark / 16.55:1 light, detail 6.91:1 / 7.22:1.
    expect(
      contrast(primary, surface),
      `title in ${theme}: ${ratio(contrast(primary, surface))}`,
    ).toBeGreaterThanOrEqual(4.5);
    expect(
      contrast(secondary, surface),
      `detail in ${theme}: ${ratio(contrast(secondary, surface))}`,
    ).toBeGreaterThanOrEqual(4.5);
    // AC#6 — the action is a control, so WCAG 1.4.11 asks 3:1 of its boundary
    // and AA of the label inside it. Same token, 6.91:1 dark / 7.22:1 light.
    expect(
      contrast(secondary, surface),
      `action boundary in ${theme}: ${ratio(contrast(secondary, surface))}`,
    ).toBeGreaterThanOrEqual(3);

    // The card's own 1px edge stays .app-card's --border-app (1.19:1 dark). It
    // is a decorative box boundary on a non-interactive card, at 20+ call
    // sites app-wide — so pin that it is the app token, not a local one.
    expect(
      await card.evaluate((el) => getComputedStyle(el).borderTopColor),
      `card edge in ${theme}`,
    ).toBe(await tokenColor(page, "--border-app"));
  });
});
