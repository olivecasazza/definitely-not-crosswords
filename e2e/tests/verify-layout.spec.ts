import { test, expect, devices, type Page } from "@playwright/test";

// Geometry verification for the UX surface — objective bounding-box assertions
// instead of eyeballing recordings. Covers the regressions that kept slipping
// through screenshot review: board grid clipping its panel, the "Ready to
// solve?" empty state ballooning, home panels overflowing the viewport, and
// remote guesses landing all-at-once instead of staggered.
//
// The home tests are unauthenticated (public pages). The board tests need the
// staging e2e account (E2E_EMAIL/E2E_PASSWORD) and self-skip without it, like
// demo.spec.ts. One account suffices: the spectator page watches anonymously
// (activeGame.get is public by design).

const EMAIL = process.env.E2E_EMAIL;
const PASSWORD = process.env.E2E_PASSWORD;

type Rect = { x: number; y: number; width: number; height: number };

async function bbox(page: Page, selector: string): Promise<Rect | null> {
  const loc = page.locator(selector).first();
  if (!(await loc.count())) return null;
  return loc.boundingBox();
}

/** r fully inside container, with `tol` px of slack for borders/rounding. */
function expectInside(r: Rect, container: Rect, tol = 2, what = "rect") {
  expect(
    r.x >= container.x - tol &&
      r.y >= container.y - tol &&
      r.x + r.width <= container.x + container.width + tol &&
      r.y + r.height <= container.y + container.height + tol,
    `${what} ${JSON.stringify(r)} must fit inside ${JSON.stringify(container)}`,
  ).toBe(true);
}

/**
 * The grid's TRACKS fit its own box. `boundingBox()` can't see this: the board
 * element stays the size CSS gave it while its `1fr` tracks blow past it and
 * `.cw-board-area { overflow: hidden }` silently clips the last columns/rows —
 * exactly the "board cut off" regression. scrollWidth/Height is the only probe
 * that catches it.
 */
async function expectGridNotClipped(page: Page, label: string) {
  const m = await page.evaluate(() => {
    const b = document.querySelector(".cw-board") as HTMLElement | null;
    if (!b) return null;
    return { sw: b.scrollWidth, sh: b.scrollHeight, cw: b.clientWidth, ch: b.clientHeight };
  });
  expect(m, `${label}: .cw-board rendered`).not.toBeNull();
  expect(
    m!.sw <= m!.cw + 1 && m!.sh <= m!.ch + 1,
    `${label}: grid tracks overflow the board box (content ${m!.sw}x${m!.sh} vs box ${m!.cw}x${m!.ch}) — columns/rows are being clipped`,
  ).toBe(true);
}

async function expectPageFitsViewport(page: Page, label: string) {
  const overflow = await page.evaluate(() => ({
    scrollW: document.documentElement.scrollWidth,
    innerW: window.innerWidth,
    scrollH: document.documentElement.scrollHeight,
    innerH: window.innerHeight,
  }));
  expect(
    overflow.scrollW <= overflow.innerW + 1,
    `${label}: horizontal overflow (scrollWidth ${overflow.scrollW} > innerWidth ${overflow.innerW})`,
  ).toBe(true);
}

/** Sign in via the login form directly (no nav dependency). */
async function signIn(page: Page, email: string, password: string) {
  await page.goto("/auth/login");
  await page.locator('input[type="email"]').fill(email);
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole("button", { name: /^sign in/i }).click();
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });
}

/** Land on a playable board: resume ACTIVE, else start an UNSTARTED game. */
async function openGame(page: Page): Promise<string | null> {
  await page.goto("/games");
  await expect(page.getByText("Available").first()).toBeVisible();
  const card = (label: string) =>
    page
      .locator('div[style*="cursor: pointer"]')
      .filter({ hasText: label })
      .first();
  const active = card("ACTIVE");
  const unstarted = card("UNSTARTED");
  // The lobby loads async — wait for either card before deciding there's no
  // playable data (an instant count() races the query and false-skips).
  try {
    await active.or(unstarted).first().waitFor({ state: "visible", timeout: 20_000 });
  } catch {
    return null;
  }
  if (await active.count()) {
    await active.click();
  } else if (await unstarted.count()) {
    await unstarted.click();
    const start = page.getByRole("button", {
      name: /^(start game|continue game)$/i,
    });
    await expect(start).toBeVisible({ timeout: 20_000 });
    await start.click();
  } else {
    return null;
  }
  await expect(page).toHaveURL(/\/game\/[^/]+(\/new)?$/, { timeout: 60_000 });
  await expect(page.locator(".cw-letter").first()).toBeVisible();
  return page.url();
}

test("home panels fit a desktop viewport", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#main")).not.toBeEmpty();
  await expectPageFitsViewport(page, "home/desktop");
  // Both panels fully on-screen.
  const viewport = page.viewportSize()!;
  const screen: Rect = { x: 0, y: 0, width: viewport.width, height: viewport.height };
  for (const title of ["Welcome", "Get Started"]) {
    const panel = page.locator(".panel", { hasText: title }).first();
    await expect(panel).toBeVisible();
    const r = (await panel.boundingBox())!;
    expectInside(r, screen, 2, `home panel "${title}"`);
  }
});

test("home panels fit a phone viewport", async ({ browser }) => {
  const ctx = await browser.newContext({ ...devices["iPhone 13"] });
  const page = await ctx.newPage();
  try {
    await page.goto("/");
    await expect(page.locator("#main")).not.toBeEmpty();
    await expectPageFitsViewport(page, "home/phone");
    const cta = page.getByRole("link", { name: /sign in|create account/i }).first();
    await expect(cta).toBeVisible();
    const r = (await cta.boundingBox())!;
    expect(r.width).toBeGreaterThan(0);
    expect(
      r.x + r.width <= page.viewportSize()!.width + 1,
      "phone CTA clipped on the right",
    ).toBe(true);
  } finally {
    await ctx.close();
  }
});

test.describe("game board (needs e2e account)", () => {
  test.skip(!EMAIL || !PASSWORD, "E2E_EMAIL / E2E_PASSWORD not set");

  test("board grid fits its panel with no clipping and no zoom hacks", async ({
    page,
  }) => {
    await signIn(page, EMAIL!, PASSWORD!);
    const url = await openGame(page);
    test.skip(!url, "no playable game on staging");

    const viewport = page.viewportSize()!;
    const screen: Rect = { x: 0, y: 0, width: viewport.width, height: viewport.height };
    const area = await bbox(page, ".cw-board-area");
    const board = await bbox(page, ".cw-board");
    expect(area, ".cw-board-area rendered").not.toBeNull();
    expect(board, ".cw-board rendered").not.toBeNull();
    expectInside(area!, screen, 2, "board area");
    expectInside(board!, area!, 2, "board grid");
    await expectGridNotClipped(page, "board/desktop");

    // Not collapsed either: the fit must USE the area (the failure mode where
    // aspect-ratio + fr tracks shrink-wraps the grid to its content).
    expect(
      board!.width >= area!.width * 0.5 || board!.height >= area!.height * 0.5,
      `board ${JSON.stringify(board)} suspiciously small inside ${JSON.stringify(area)}`,
    ).toBe(true);

    // Cells are square (the grid ratio math holds per-cell, not just overall).
    const cell = await bbox(page, ".cw-letter");
    expect(cell, "at least one letter cell").not.toBeNull();
    expect(
      Math.abs(cell!.width - cell!.height) <= 2,
      `cell not square: ${JSON.stringify(cell)}`,
    ).toBe(true);

    // No residual zoom hack on the board (the old fitBoardToViewport glue).
    const zoom = await page.evaluate(
      () => (document.querySelector(".cw-board") as HTMLElement)?.style.zoom ?? "",
    );
    expect(zoom, ".cw-board must fit naturally, no JS zoom").toBe("");

    // Empty state: the "Ready to solve?" card is bounded inside its panel.
    const emptyWrap = await bbox(page, ".cw-clue-empty");
    const card2 = await bbox(page, ".cw-empty-card");
    expect(emptyWrap, "clue empty state rendered").not.toBeNull();
    expect(card2, "empty-state card rendered").not.toBeNull();
    expectInside(card2!, emptyWrap!, 2, "empty-state card");
    expect(
      card2!.width <= emptyWrap!.width * 0.95,
      `empty-state card eats its panel: ${JSON.stringify(card2)} in ${JSON.stringify(emptyWrap)}`,
    ).toBe(true);
  });

  test("phone board fits too", async ({ browser }) => {
    const ctx = await browser.newContext({ ...devices["iPhone 13"] });
    const page = await ctx.newPage();
    try {
      await signIn(page, EMAIL!, PASSWORD!);
      const url = await openGame(page);
      test.skip(!url, "no playable game on staging");
      const viewport = page.viewportSize()!;
      const screen: Rect = { x: 0, y: 0, width: viewport.width, height: viewport.height };
      await expectPageFitsViewport(page, "board/phone");
      const area = await bbox(page, ".cw-board-area");
      const board = await bbox(page, ".cw-board");
      expect(area).not.toBeNull();
      expect(board).not.toBeNull();
      expectInside(area!, screen, 2, "phone board area");
      expectInside(board!, area!, 2, "phone board grid");
      await expectGridNotClipped(page, "board/phone");
      // ...and the stacked mobile panel actually gives the board room. It used
      // to collapse to panel-kit's 180px floor (cells ~2px) because
      // `container-type: size` hid the board's height from the auto-sized panel.
      expect(
        board!.width >= Math.min(area!.width, viewport.width * 0.8),
        `phone board collapsed: ${JSON.stringify(board)} in area ${JSON.stringify(area)}`,
      ).toBe(true);
    } finally {
      await ctx.close();
    }
  });

  test("remote letters land staggered, not all-at-once", async ({ browser }) => {
    const ctxA = await browser.newContext();
    const a = await ctxA.newPage();
    try {
      await signIn(a, EMAIL!, PASSWORD!);
      const url = await openGame(a);
      test.skip(!url, "no playable game on staging");

      // Anonymous spectator — activeGame.get is public, and the subscription
      // fires for watchers too. This is the remote viewer's perspective.
      const ctxB = await browser.newContext();
      const b = await ctxB.newPage();
      try {
        await b.goto(url!);
        await expect(b.locator(".cw-letter").first()).toBeVisible();

        // Pull answers through A's session to play a real, correct word.
        const activeId = a.url().split("/game/")[1];
        const res = await a.request.get(
          `/api/trpc/activeGame.get?batch=1&input=${encodeURIComponent(
            JSON.stringify({ "0": { id: activeId } }),
          )}`,
        );
        const data = (await res.json())[0]?.result?.data;
        const clues = (data?.game?.questions ?? []) as {
          number: number;
          answer: string;
          questionText: string;
          direction: "ACROSS" | "DOWN";
          rootX: number;
          rootY: number;
        }[];
        const actions = (data?.actions ?? []) as {
          cordX: number;
          cordY: number;
        }[];
        // Prefer a long clue whose cells are mostly UNFILLED — the stagger is
        // only observable on letters that newly appear on the spectator board.
        const touched = new Set(actions.map((x) => `${x.cordX},${x.cordY}`));
        const unfilled = (c: (typeof clues)[number]) =>
          Array.from({ length: c.answer.length }, (_, i) => {
            const x = c.direction === "ACROSS" ? c.rootX + i : c.rootX;
            const y = c.direction === "ACROSS" ? c.rootY : c.rootY + i;
            return touched.has(`${x},${y}`) ? 0 : 1;
          }).reduce((s, n) => s + n, 0);
        const target = clues
          .filter((c) => c.answer.length >= 4)
          .sort((p, q) => unfilled(q) - unfilled(p))[0];
        test.skip(
          !target || unfilled(target) < 3,
          "no mostly-unfilled 4+ letter clue available for the stagger probe",
        );

        // Baseline filled-cell count on the spectator board.
        const filled = () =>
          b.locator(".cw-letter .cw-char").evaluateAll(
            (els) => els.filter((e) => (e.textContent ?? "").trim() !== "").length,
          );
        const baseline = await filled();

        // A selects the clue and types the answer.
        const tab = a.getByRole("button", {
          name: target!.direction === "ACROSS" ? /^across$/i : /^down$/i,
        });
        if (!((await tab.getAttribute("class")) ?? "").includes("cw-tab-active")) {
          await tab.click();
        }
        await a
          .locator(".cw-clue-row", {
            has: a.locator(".cw-clue-badge", { hasText: String(target!.number) }),
            hasText: target!.questionText.slice(0, 20),
          })
          .first()
          .click();
        const inputs = a.locator(".cw-letter-input");
        await expect(inputs).toHaveCount(target!.answer.length);
        await inputs.first().click();
        await a.keyboard.type(target!.answer, { delay: 40 });

        // Watch the spectator board while A submits: sample the filled count
        // every 25ms through the guess and the stagger window.
        const samples: { t: number; n: number }[] = [];
        const t0 = Date.now();
        const watcher = (async () => {
          while (Date.now() - t0 < 4000) {
            samples.push({ t: Date.now() - t0, n: await filled() });
            await b.waitForTimeout(25);
          }
        })();
        await a.getByRole("button", { name: /^guess$/i }).click();
        await watcher;

        // Letters must arrive (the guess went out over the wire)...
        const arrived = samples.filter((s) => s.n > baseline);
        expect(
          arrived.length,
          "spectator never saw the remote letters land",
        ).toBeGreaterThan(0);
        // ...in at least two visibly distinct steps...
        const steps = arrived.filter((s, i) => i === 0 || s.n > arrived[i - 1].n);
        expect(
          steps.length,
          `letters landed in one flash (samples: ${JSON.stringify(samples.filter((s) => s.n > baseline).slice(0, 12))})`,
        ).toBeGreaterThanOrEqual(2);
        // ...spread over time — the stagger is 90ms/letter; require a
        // conservative fraction of that so CI jitter can't flake it.
        const spread = arrived[arrived.length - 1].t - arrived[0].t;
        expect(
          spread,
          `stagger window too tight: ${spread}ms across ${steps.length} steps`,
        ).toBeGreaterThanOrEqual(90);
      } finally {
        await ctxB.close();
      }
    } finally {
      await ctxA.close();
    }
  });
});
