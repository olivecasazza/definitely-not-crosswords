import { test, expect, type Page, type WebSocketRoute } from "@playwright/test";

// Connection state for the co-op socket — DEF-175.
//
// A dropped socket used to be indistinguishable from a live one: the play
// screen's three subscriptions each held a bare cancel handle, so a failed
// open, a failed start frame and a dead stream all ended the task in silence.
// The board stopped updating, the player's own letters stopped reaching the
// other players, and nothing said so.
//
// Covers acceptance criteria 1 (pill appears on socket death), 2 (pill absent
// while live), 3 (reconnect reconciles the board without a reload) and 5
// (`navigator.onLine` short-circuits to offline). 4/6/7/8 are CSS or timing
// claims that are cheaper to assert by hand in a browser than to drive through
// a staging network.
//
// Needs a staging test account, like the other authenticated specs:
//   E2E_EMAIL / E2E_PASSWORD     — the observer (the socket we kill)
//   E2E_EMAIL_2 / E2E_PASSWORD_2 — the player who types while the observer is
//     disconnected; only criterion 3 needs them, so that test self-skips.
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
