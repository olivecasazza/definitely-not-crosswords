import { test, expect, type Page, type WebSocketRoute } from "@playwright/test";

// Connection state for the co-op socket — DEF-175, deltas DEF-188.
//
// A dropped socket used to be indistinguishable from a live one: the play
// screen's three subscriptions each held a bare cancel handle, so a failed
// open, a failed start frame and a dead stream all ended the task in silence.
// The board stopped updating, the player's own letters stopped reaching the
// other players, and nothing said so.
//
// Covers acceptance criteria 1 (pill appears on socket death), 2 (pill absent
// while live — and, as of DEF-188, absent from FIRST PAINT), 3 (reconnect
// reconciles the board without a reload) and 5 (`navigator.onLine`
// short-circuits to offline). DEF-188 adds the contrast assertions its design
// review could only compute by hand: the stale chip/ring dimming and the
// grid's focus ring are now measured here, in both themes, so the review's
// table is a permanent test rather than a one-off.
//
// Needs a staging test account, like the other authenticated specs:
//   E2E_EMAIL / E2E_PASSWORD     — the observer (the socket we kill)
//   E2E_EMAIL_2 / E2E_PASSWORD_2 — the player who types while the observer is
//     disconnected, and the only source of a remote presence ring; the two
//     assertions that need one self-skip without it.
//
// Self-skips without the primary creds so the unauthenticated canary still
// runs (`pnpm canary`).

const EMAIL = process.env.E2E_EMAIL;
const PASSWORD = process.env.E2E_PASSWORD;
const EMAIL2 = process.env.E2E_EMAIL_2;
const PASSWORD2 = process.env.E2E_PASSWORD_2;

test.skip(!EMAIL || !PASSWORD, "E2E_EMAIL / E2E_PASSWORD not set");

/** The co-op subscription endpoint. Blocked/closed by the tests below. */
const WS_GLOB = "**/api/trpc-ws";

const pill = (page: Page) => page.locator(".cw-conn-pill");

/**
 * The computed value of a design token, read by painting it.
 *
 * `getPropertyValue` on a custom property returns its own text
 * (`var(--pastel-yellow)`), not the resolved colour, so resolve it the way the
 * browser does: put it on an element and read the computed background. This
 * keeps the assertion pinned to the *token* — a palette change breaks the
 * design review, not an unrelated canary run.
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

/**
 * WCAG 2.x relative luminance of a computed `rgb()` / `rgba()` colour.
 *
 * Computed styles are the only place the resolved value exists: `getPropertyValue`
 * on a custom property hands back its own text, and the `color-mix()` staleness
 * DIM1 introduced has no text form at all.
 */
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
 * Flip `.light-mode` on `<html>` — the same class the header toggle sets — run
 * `body`, then put the page back the way it was found.
 *
 * Contrast is a per-theme property of the token cascade, and three of the four
 * regressions DEF-188 fixes only exist in ONE of the two themes, so a
 * single-theme assertion would have passed all three of them.
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

/**
 * Whether a status pill has EVER existed in this document, observed from the
 * first paint.
 *
 * DEF-188 A5: "the pill is absent once the roster has settled" is a much weaker
 * claim than "the pill was never on screen", which is what D4 is about — a
 * `Connecting` badge flashed on every single cold load and a DOM query that
 * starts after sign-in walks straight past it. `addInitScript` is the only hook
 * that runs before the app's own scripts, and a MutationObserver is the only
 * thing that can see a subtree that appears and disappears again. Re-runs on
 * every navigation, so the flag describes the play-screen document.
 */
async function watchForPill(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const seen = { pill: false };
    (window as unknown as { __pillSeen: typeof seen }).__pillSeen = seen;
    const mark = () => {
      if (!seen.pill && document.querySelector(".cw-conn-pill")) seen.pill = true;
    };
    const root = document.documentElement;
    if (root) new MutationObserver(mark).observe(root, { childList: true, subtree: true });
    mark();
  });
}

async function signIn(page: Page, email: string, password: string) {
  await page.goto("/auth/login");
  await page.locator('input[type="email"]').fill(email);
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole("button", { name: /^sign in/i }).click();
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });
}

/**
 * Get onto a play screen with a live co-op socket.
 *
 * Prefers resuming an in-progress game (straight to the board); falls back to
 * starting a NEW one, which is slow server-side. Copied from demo.spec.ts so
 * both specs agree on what "a playable game" looks like on staging.
 */
async function openPlayScreen(page: Page): Promise<boolean> {
  await signIn(page, EMAIL!, PASSWORD!);
  await page.goto("/games");
  await expect(page.getByText("Library").first()).toBeVisible();

  const card = (label: string) =>
    page
      .locator('div[style*="cursor: pointer"]')
      .and(page.locator(`[aria-label*="${label}"]`))
      .first();
  const active = card("— IN PROGRESS");
  const unstarted = card("— NEW");
  if (await active.count()) {
    await active.click();
    await expect(page).toHaveURL(/\/game\/[^/]+$/, { timeout: 60_000 });
  } else if (await unstarted.count()) {
    await unstarted.click();
    const start = page.getByRole("button", {
      name: /^(start game|continue game)$/i,
    });
    await expect(start).toBeVisible({ timeout: 20_000 });
    await start.click();
    // Fresh starts generate server-side; the /new briefing page matches the
    // play-URL pattern, so gate on LEAVING /new.
    await expect(page).not.toHaveURL(/\/game\/[^/]+\/new$/, { timeout: 150_000 });
  } else {
    return false;
  }
  // The board is the last gate: only once letters render is the page really up.
  await expect(page.locator(".cw-letter").first()).toBeVisible({ timeout: 60_000 });
  return true;
}

/**
 * Take manual control of every co-op socket on the page.
 *
 * `routeWebSocket` must be installed before the socket is opened, so the
 * handler captures each `WebSocketRoute` as it connects. `kill` flips the
 * handler's behaviour for future sockets; `drop()` closes the ones already
 * open, which is what actually simulates a dropped connection.
 */
async function trapSockets(page: Page) {
  const state = { kill: false, live: [] as WebSocketRoute[] };
  await page.routeWebSocket(WS_GLOB, (ws) => {
    state.live.push(ws);
    if (state.kill) ws.close();
    else ws.connectToServer();
  });
  return {
    /** How many sockets have ever been opened. */
    opened: () => state.live.length,
    /** Close every live socket, the way a dropped network would. */
    drop: async () => {
      for (const ws of state.live) await ws.close();
      state.live = [];
    },
    /** Start refusing new sockets too (blocks a reconnect from succeeding). */
    block: () => {
      state.kill = true;
    },
    /** Let sockets through again. */
    unblock: () => {
      state.kill = false;
    },
  };
}

test("the status pill is absent while the socket is live (DEF-175 c2)", async ({ page }) => {
  // Armed BEFORE the first navigation, so the "never on screen" half of the
  // assertion covers the whole cold load rather than starting after sign-in.
  await watchForPill(page);
  if (!(await openPlayScreen(page))) {
    test.skip(true, "no playable game available on staging");
  }
  // Criterion 2 is an assertion of ABSENCE. "I didn't see the pill" is not a
  // test, and a badge that is right 100% of the time is the noise this design
  // deliberately avoids — so it must be checked, not eyeballed.
  await expect(pill(page)).toHaveCount(0);
  // ...and it must still be absent after the roster has had time to render and
  // the socket has demonstrably come up.
  await page.waitForTimeout(2_000);
  await expect(pill(page)).toHaveCount(0);
  // DEF-188 A5 / D4. The first paint is the window that matters: `Connecting`
  // is only ever a subscription's first-ever open, so mapping it to the warning
  // variant put an amber badge on every single game load — and, since the pill
  // is a live region, an announced "Connecting…" every single time. Both were
  // invisible to the two assertions above, which start after sign-in.
  expect(
    await page.evaluate(
      () => (window as unknown as { __pillSeen: { pill: boolean } }).__pillSeen.pill,
    ),
    "a .cw-conn-pill existed at some point in this document",
  ).toBe(false);
});

test("a dropped socket raises the pill within a second (DEF-175 c1)", async ({ page }) => {
  const sockets = await trapSockets(page);
  if (!(await openPlayScreen(page))) {
    test.skip(true, "no playable game available on staging");
  }
  expect(sockets.opened()).toBeGreaterThan(0);
  await expect(pill(page)).toHaveCount(0);

  await sockets.drop();

  // Criterion 1: in the DOM within 1s, reading "Reconnecting…", filled with
  // --color-warning.
  await expect(pill(page)).toBeVisible({ timeout: 1_000 });
  await expect(pill(page)).toHaveAttribute("data-conn-state", "warn");
  await expect(pill(page)).toHaveText(/Reconnecting/);
  expect(await pill(page).evaluate((el) => getComputedStyle(el).backgroundColor)).toBe(
    await tokenColor(page, "--color-warning"),
  );
});

test("navigator.onLine parks the pill at offline without waiting (DEF-175 c5)", async ({
  page,
}) => {
  if (!(await openPlayScreen(page))) {
    test.skip(true, "no playable game available on staging");
  }
  await expect(pill(page)).toHaveCount(0);

  // The browser already knows the network is gone, so the state has to flip
  // without waiting out a socket timeout or the retry backoff.
  await page.context().setOffline(true);
  const errored = page.locator('.cw-conn-pill[data-conn-state="error"]');
  await expect(errored).toBeVisible({ timeout: 1_000 });
  await expect(errored).toHaveText(/your letters aren't being shared/i);
  await expect(errored.getByRole("button", { name: /retry now/i })).toBeVisible();

  // Criterion 4's tail: "Retry now" resets the attempt counter and re-enters
  // the ladder. With the context still offline it lands back in offline, which
  // is the honest answer — the retry happened, the network is still down.
  await errored.getByRole("button", { name: /retry now/i }).click();
  await expect(errored).toBeVisible();

  await page.context().setOffline(false);
  // `online` unparks the ladder immediately; the pill clears once the socket is
  // back up and the board has been reconciled.
  await expect(pill(page)).toHaveCount(0, { timeout: 30_000 });
});

test("a reconnect reconciles the board without a reload (DEF-175 c3)", async ({ page }) => {
  if (!EMAIL2 || !PASSWORD2) {
    test.skip(true, "E2E_EMAIL_2 / E2E_PASSWORD_2 not set");
  }
  const sockets = await trapSockets(page);
  if (!(await openPlayScreen(page))) {
    test.skip(true, "no playable game available on staging");
  }
  await expect(pill(page)).toHaveCount(0);

  // Cut the observer off entirely: drop what is open AND refuse reconnects, so
  // every action below happens while they are provably disconnected.
  sockets.block();
  await sockets.drop();
  await expect(pill(page)).toBeVisible({ timeout: 2_000 });

  // A second player types into the same game. The observer must NOT see it
  // yet — that is the whole failure this issue is about.
  const other = await page.context().browser()!.newContext();
  const p2 = await other.newPage();
  const url = page.url();
  await signIn(p2, EMAIL2, PASSWORD2);
  await p2.goto(url);
  await expect(p2.locator(".cw-letter").first()).toBeVisible({ timeout: 60_000 });

  const row = p2.locator(".cw-clue-row").first();
  await row.click();
  const input = p2.locator(".cw-letter-input").first();
  await expect(input).toBeVisible({ timeout: 20_000 });
  // Any single letter, submitted: a remote action the observer's `actions`
  // list is missing. Value correctness is irrelevant to this test.
  await input.fill("S");
  await p2.getByRole("button", { name: /^guess$/i }).click();

  // Let the pill climb its backoff ladder while the socket cannot come back.
  await expect(page.locator('.cw-conn-pill[data-conn-state="error"]')).toBeVisible({
    timeout: 60_000,
  });

  // Let the observer back in. The reconcile path re-fetches `activeGame.get`
  // after the re-subscribe lands, so the letter arrives with no reload.
  sockets.unblock();
  await expect(pill(page)).toHaveCount(0, { timeout: 60_000 });
  await expect(page.locator(".cw-char").filter({ hasText: /s/i }).first()).toBeVisible({
    timeout: 30_000,
  });
  // Still the same document — a full-page reload would also have shown the
  // letter, so pin that the SPA never navigated away.
  await expect(page).toHaveURL(url);
  await other.close();
});

// ── DEF-188: the review's contrast table, as a test ──────────────────────────
//
// DEF-181's review could not open a browser in its container, so it computed
// the token cascade out of the two stylesheets and hand-measured WCAG from the
// result. That is a good method and a bad home: a table in a comment goes stale
// the next time a token moves. These assertions run the same arithmetic against
// values the BROWSER resolved, in both themes, on the real DOM — so a palette
// change now fails here instead of quietly invalidating a review comment.
//
// Two regressions are being pinned, both of which the hand maths found and both
// of which were invisible to a single-theme check:
//
//   A1  `.cw-chip-stale { opacity: .55 }` composited the player's NAME down to
//       3.89:1 on --bg-card in light mode. The roster is how you find out who is
//       in the game with you, and an outage is exactly when you need to read it.
//   A2  `.cw-ring-stale { opacity: .55 }` is applied to the CELL, so it took a
//       remote player's already-confirmed LETTER down to 3.94:1 on
//       --bg-cell-letter in light mode. The position is last-known; the letter
//       is not.
//   A3  The grid's focus ring was `--fill-ink` alone, which is 1.10:1 on a plain
//       .cw-letter in DARK mode — and the roving cell is a plain .cw-letter
//       until a clue is picked, so that was the common case, not the edge one.
//
// Text on a chip is `--fs-xs` (12px) at normal weight, so AA is 4.5:1. WCAG
// 1.4.11 asks 3:1 of a focus indicator. Both numbers are asserted as they are,
// not rounded toward the fix.

type AXNode = {
  nodeId: string;
  role?: { value?: string };
  name?: { value?: string };
  childIds?: string[];
};

/**
 * The whole accessibility tree, via CDP.
 *
 * DEF-215: the question these assertions answer is not "is the text in the
 * DOM" but "does a screen reader land on it". Only the AX tree answers that,
 * so the tests read the AX tree rather than inspecting markup — a node removed
 * from the tree by `display: none` still passes every DOM query.
 */
async function fullAXTree(page: Page): Promise<AXNode[]> {
  const session = await page.context().newCDPSession(page);
  try {
    const { nodes } = (await session.send("Accessibility.getFullAXTree")) as { nodes: AXNode[] };
    return nodes;
  } finally {
    await session.detach();
  }
}

/** Whether `predicate` holds anywhere at or under `node`. */
function hasSelfOrDescendant(
  nodesById: Map<string, AXNode>,
  node: AXNode,
  predicate: (node: AXNode) => boolean,
): boolean {
  if (predicate(node)) return true;
  for (const childId of node.childIds ?? []) {
    const child = nodesById.get(childId);
    if (child && hasSelfOrDescendant(nodesById, child, predicate)) return true;
  }
  return false;
}

/** A stale chip, with the name's own computed colour and the surface behind it. */
async function firstChipBorderBottomStyle(page: Page) {
  return page.locator(".cw-chip").first().evaluate((chip) => getComputedStyle(chip).borderBottomStyle);
}

async function staleChip(page: Page) {
  return page.evaluate(() => {
    const chip = document.querySelector<HTMLElement>(".cw-chip-stale");
    if (!chip) throw new Error("no .cw-chip-stale in the roster bar");
    const cs = getComputedStyle(chip);
    const hidden = chip.querySelector<HTMLElement>(".cw-sr-only");
    return {
      text: (chip.textContent ?? "").trim(),
      // DEF-215 A15: the visible text is the name, the tags and the clue ref.
      // The hidden last-known span must not appear here, so it is subtracted
      // back out of textContent and compared against the live chip.
      visibleText: [...chip.childNodes]
        .filter((node) => !(node instanceof HTMLElement && node.classList.contains("cw-sr-only")))
        .map((node) => node.textContent ?? "")
        .join("")
        .trim(),
      height: chip.getBoundingClientRect().height,
      // `display: none` would also pass a textContent check — the whole point of
      // .cw-sr-only is that it stays in the AX tree — so the hiding mechanism
      // itself is asserted.
      hiddenPosition: hidden ? getComputedStyle(hidden).position : "",
      hiddenDisplay: hidden ? getComputedStyle(hidden).display : "",
      colour: cs.color,
      background: cs.backgroundColor,
      borderBottomStyle: cs.borderBottomStyle,
      // The bug WAS an opacity. Assert it stays gone rather than trusting the
      // comment next to it.
      opacity: cs.opacity,
    };
  });
}

/** The ring colours on a stale cell, and the letter's colour inside it. */
async function staleRingCell(page: Page) {
  return page.evaluate(() => {
    const cell = document.querySelector<HTMLElement>(".cw-cell.cw-ring-stale");
    if (!cell) throw new Error("no .cw-cell.cw-ring-stale on the board");
    const cs = getComputedStyle(cell);
    return {
      letter: cell.querySelector<HTMLElement>(".cw-char")?.textContent ?? "",
      colour: cs.color,
      background: cs.backgroundColor,
      opacity: cs.opacity,
      ring: cs.boxShadow,
      insetBands: cs.boxShadow.match(/inset/g)?.length ?? 0,
      aria: cell.getAttribute("aria-label") ?? "",
    };
  });
}

/**
 * The roving cell's focus ring, read off the `::after`.
 *
 * DEF-188 D2 puts the ring on a pseudo-element precisely so it cannot collide
 * with the remote presence ring, which is an inline `box-shadow` on the cell
 * itself. That also means `getComputedStyle(cell).boxShadow` reads the WRONG
 * ring, so the pseudo-element has to be named — which is the test, in a sense:
 * it fails if anyone moves the ring back onto the element.
 */
async function focusRing(page: Page) {
  return page.evaluate(() => {
    const cell = document.querySelector<HTMLElement>('.cw-cell[tabindex="0"]');
    if (!cell) throw new Error("no roving cell (tabindex=0) on the board");
    const after = getComputedStyle(cell, "::after");
    return {
      cellClass: cell.getAttribute("class") ?? "",
      background: getComputedStyle(cell).backgroundColor,
      // Box-shadows serialise as "<colour> <offsets> inset", comma-joined.
      bands: [...after.boxShadow.matchAll(/rgba?\([^)]+\)/g)].map((m) => m[0]),
    };
  });
}

/**
 * Tab until the roving cell really has focus.
 *
 * `element.focus()` does NOT arm `:focus-visible` on a `div[tabindex]` — the
 * UA treats a script-driven focus as a pointer focus — so the ring under test
 * would never exist and the assertion would pass vacuously. Real Tab presses
 * are the only honest way to get there, and walking the page's own focus order
 * is what a keyboard player actually does.
 */
async function tabIntoBoard(page: Page): Promise<boolean> {
  for (let press = 0; press < 60; press++) {
    const onCell = await page.evaluate(
      () => document.activeElement?.getAttribute("class")?.includes("cw-cell") ?? false,
    );
    if (onCell) return true;
    await page.keyboard.press("Tab");
  }
  return false;
}

test("a stale roster chip keeps its name above AA and announces last-known state (DEF-188 A1, DEF-215 A13/A15)", async ({
  page,
}) => {
  const sockets = await trapSockets(page);
  if (!(await openPlayScreen(page))) {
    test.skip(true, "no playable game available on staging");
  }
  // Snapshot the live chip BEFORE going stale: A15 is a before/after
  // comparison, so the baseline has to be the unhidden rendering.
  const liveChip = await page.locator(".cw-chip").first().evaluate((chip) => ({
    visibleText: (chip.textContent ?? "").trim(),
    height: chip.getBoundingClientRect().height,
  }));
  await inEachTheme(page, async (theme) => {
    expect(await firstChipBorderBottomStyle(page), `live chip underline in ${theme}`).toBe("solid");
  });

  // Staleness is global, not per-player: one dropped socket marks every chip.
  sockets.block();
  await sockets.drop();
  await expect(page.locator(".cw-chip-stale").first()).toBeVisible({ timeout: 10_000 });

  // DEF-215 A13: the words have to reach the AX tree. The chip's `title` is
  // already in there, but as a `description` on a nameless `generic`, which no
  // screen reader announces — so the assertion is on a real StaticText.
  const axTree = await fullAXTree(page);
  const nodesById = new Map(axTree.map((node) => [node.nodeId, node]));
  const isLastKnownText = (node: AXNode) =>
    node.role?.value === "StaticText" && /last known position/i.test(node.name?.value ?? "");
  expect(
    axTree.some(isLastKnownText),
    "no StaticText node exposes the stale chip's last-known text",
  ).toBe(true);
  expect(
    axTree.some((node) => hasSelfOrDescendant(nodesById, node, isLastKnownText)),
    "the stale-chip StaticText is not reachable under any AX parent",
  ).toBe(true);

  await inEachTheme(page, async (theme) => {
    const chip = await staleChip(page);
    expect(chip.text, `no name in the stale chip (${theme})`).not.toBe("");
    // A15: the hidden span must be invisible to layout in BOTH directions —
    // same words on screen, same box. A chip that grew would reflow the roster
    // at exactly the moment a player is reading it during an outage.
    expect(chip.visibleText, `visible chip text changed (${theme})`).toBe(liveChip.visibleText);
    expect(chip.height, `hidden stale text changed chip height (${theme})`).toBe(liveChip.height);
    // Clipped, not display:none — display:none would pass every text assertion
    // above while deleting the node from the tree they exist to protect.
    expect(chip.hiddenPosition, `hidden stale text is not clipped (${theme})`).toBe("absolute");
    expect(chip.hiddenDisplay, `hidden stale text uses display:none (${theme})`).not.toBe("none");
    expect(chip.opacity, `stale chip is dimmed by opacity (${theme})`).toBe("1");
    expect(chip.borderBottomStyle, `stale chip underline in ${theme}`).toBe("dashed");
    expect(
      contrast(chip.colour, chip.background),
      `stale chip name in ${theme}: ${ratio(contrast(chip.colour, chip.background))}`,
    ).toBeGreaterThanOrEqual(4.5);
  });
});

test("a stale presence ring never dims the letter it surrounds and announces last-known state (DEF-188 A2, DEF-215 A14/A16)", async ({
  page,
}) => {
  if (!EMAIL2 || !PASSWORD2) {
    test.skip(true, "E2E_EMAIL_2 / E2E_PASSWORD_2 not set — a remote ring needs a second player");
  }
  const sockets = await trapSockets(page);
  if (!(await openPlayScreen(page))) {
    test.skip(true, "no playable game available on staging");
  }
  // Put a real remote selection on the board: a second player opens the same
  // game and picks a clue, so their presence projects a ring.
  const other = await page.context().browser()!.newContext();
  const p2 = await other.newPage();
  const url = page.url();
  await signIn(p2, EMAIL2!, PASSWORD2!);
  await p2.goto(url);
  await expect(p2.locator(".cw-letter").first()).toBeVisible({ timeout: 60_000 });
  await p2.locator(".cw-clue-row").first().click();
  await expect(page.locator('[title*="is working here"]').first()).toBeVisible({
    timeout: 30_000,
  });
  await inEachTheme(page, async (theme) => {
    const liveBands = await page
      .locator('[title*="is working here"]')
      .first()
      .evaluate((cell) => getComputedStyle(cell).boxShadow.match(/inset/g)?.length ?? 0);
    expect(liveBands, `live ring inset bands in ${theme}`).toBe(1);
  });

  // Now go offline, which is what makes a ring last-known rather than live.
  sockets.block();
  await sockets.drop();
  await expect(page.locator(".cw-cell.cw-ring-stale").first()).toBeVisible({ timeout: 10_000 });

  await inEachTheme(page, async (theme) => {
    const cell = await staleRingCell(page);
    expect(cell.opacity, `stale ring cell is dimmed by opacity (${theme})`).toBe("1");
    expect(cell.letter, `stale ring cell has no letter (${theme})`).not.toBe("");
    // DEF-215 A14: the accessible name has to say the position is last-known,
    // AND still answer "is this letter right?" — appending presence must not
    // have displaced the state suffix that `title` could never override.
    expect(cell.aria, `stale ring cell omits last-known state (${theme})`).toMatch(
      /last known/i,
    );
    expect(cell.aria, `stale ring cell dropped answer state (${theme})`).toMatch(
      /correct|incorrect|in progress/i,
    );
    // A16: unchanged ring geometry. The live/stale inset-band counts are the
    // DEF-200 A10 assertion, kept here so this change cannot quietly restyle
    // the ring it only ever meant to describe.
    expect(cell.insetBands, `stale ring inset bands in ${theme}: ${cell.ring}`).toBe(2);
    expect(
      contrast(cell.colour, cell.background),
      `letter on a stale ring in ${theme}: ${ratio(contrast(cell.colour, cell.background))}`,
    ).toBeGreaterThanOrEqual(4.5);
  });
  await other.close();
});

test("the roving cell's focus ring clears 3:1 before any clue is picked (DEF-188 A3)", async ({
  page,
}) => {
  if (!(await openPlayScreen(page))) {
    test.skip(true, "no playable game available on staging");
  }
  // Before a clue is picked there is no derived cursor, so the roving tabindex
  // parks on the first non-block cell in row-major order — a bare .cw-letter.
  // That is the case the old --fill-ink ring was invisible in.
  expect(
    await tabIntoBoard(page),
    "Tab never reached the crossword board (roving cell stayed unfocused)",
  ).toBe(true);

  await inEachTheme(page, async (theme) => {
    const ring = await focusRing(page);
    expect(
      ring.cellClass,
      `roving cell is not a plain .cw-letter in ${theme}, so this is no longer the D2 case`,
    ).toContain("cw-letter");
    expect(ring.cellClass, `roving cell is already the cursor in ${theme}`).not.toContain(
      "cw-focused",
    );
    // Two tones, or the ring cannot clear 3:1 against both cell fills: no
    // single token does (--fill-ink is 1.10:1 on a dark letter cell,
    // --text-primary is 1.10:1 on the dark-mode cursor fill).
    expect(ring.bands.length, `focus ring bands in ${theme}: ${ring.bands.length}`).toBe(2);
    // The requirement is on the INDICATOR: one of its two bands has to clear
    // 3:1 against whatever this cell is filled with. Asserting each band
    // separately would be asserting a stricter thing than WCAG asks and would
    // fail the design on a technicality.
    const best = Math.max(...ring.bands.map((b) => contrast(b, ring.background)));
    expect(
      best,
      `focus ring in ${theme} on ${ring.background}: ${ratio(best)} (bands ${ring.bands.join(", ")})`,
    ).toBeGreaterThanOrEqual(3);
  });
});
