import { test, expect, devices, type Page } from "@playwright/test";
import path from "node:path";
import { dwell, humanClick, humanType, humanTypeLetters, wander, rand } from "./helpers";

// Authenticated product tour — the source of the demo video and a feature-
// completeness smoke test of the premium surface. Needs staging test accounts:
//   E2E_EMAIL / E2E_PASSWORD     — the primary (recorded) player
//   E2E_EMAIL_2 / E2E_PASSWORD_2 — optional second player for the co-op chapter;
//     falls back to the primary account (still exercises the live transport,
//     but presence chips/rings only render for a *different* user)
// The spec self-skips without the primary creds so the canary still runs.
//
// Chapters: sign-in → lobby → co-op gameplay (join, presence, live actions)
// → puzzle completion → stats (leaderboard/career/compare/teams) → profile
// → subscription. Hard assertions gate the feature chrome; data-dependent
// moments (a playable game existing, a small-enough puzzle) degrade
// gracefully so the run still produces a clean video.

const EMAIL = process.env.E2E_EMAIL;
const PASSWORD = process.env.E2E_PASSWORD;
const EMAIL2 = process.env.E2E_EMAIL_2;
const PASSWORD2 = process.env.E2E_PASSWORD_2;

test.skip(!EMAIL || !PASSWORD, "E2E_EMAIL / E2E_PASSWORD not set");

// The full tour (with human pacing) runs several minutes.
test.setTimeout(480_000);

/** Smallest puzzle we'll solve end-to-end on camera. */
const COMPLETION_MAX_CLUES = 12;

type Clue = {
  number: number;
  answer: string;
  questionText: string;
  direction: "ACROSS" | "DOWN";
  rootX: number;
  rootY: number;
};

type ClueAction = {
  cordX: number;
  cordY: number;
  actionType: string;
  state: string;
};

/** A clue is solved when every cell's latest action is its correct letter. */
const solvedClues = (clues: Clue[], actions: ClueAction[]) => {
  const latest = new Map<string, ClueAction>();
  for (const a of actions) latest.set(`${a.cordX},${a.cordY}`, a); // ASC → last wins
  return new Set(
    clues
      .filter((c) =>
        Array.from({ length: c.answer.length }, (_, i) => i).every((i) => {
          const x = c.direction === "ACROSS" ? c.rootX + i : c.rootX;
          const y = c.direction === "ACROSS" ? c.rootY : c.rootY + i;
          const a = latest.get(`${x},${y}`);
          return (
            a?.actionType === "correctGuess" &&
            a.state.toUpperCase() === c.answer[i].toUpperCase()
          );
        }),
      )
      .map((c) => `${c.number}${c.direction}`),
  );
};

/** Batched tRPC query, sharing the page's session cookie. */
async function trpcGet(page: Page, proc: string, input: unknown) {
  const url = `/api/trpc/${proc}?batch=1&input=${encodeURIComponent(
    JSON.stringify({ "0": input ?? null }),
  )}`;
  const res = await page.request.get(url);
  const body = await res.json();
  if (body?.[0]?.error) throw new Error(JSON.stringify(body[0].error));
  return body[0]?.result?.data;
}

/** Sign in through the UI with human cadence. */
async function signIn(page: Page, email: string, password: string) {
  await humanClick(page, page.getByRole("link", { name: /^sign in$/i }).first());
  await expect(page).toHaveURL(/\/auth\/login/);
  await humanType(page, page.locator('input[type="email"]'), email);
  await humanType(page, page.locator('input[type="password"]'), password);
  await dwell(page, 400, 900);
  await humanClick(page, page.getByRole("button", { name: /^sign in/i }));
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });
}

/**
 * Sign in via the login page directly — the mobile nav can tuck the "Sign in"
 * link behind a menu, so the phone player goes straight to the form.
 */
async function signInDirect(page: Page, email: string, password: string) {
  await page.goto("/auth/login");
  await humanType(page, page.locator('input[type="email"]'), email);
  await humanType(page, page.locator('input[type="password"]'), password);
  await dwell(page, 400, 900);
  await humanClick(page, page.getByRole("button", { name: /^sign in/i }));
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 20_000 });
}

/**
 * Keep the phone's view pinned to its board panel so the spectator cam shows
 * the PC's live updates. On the 390×844 stacked mobile layout the Active Clue
 * panel a player just edited sits above the board; without scrolling back, the
 * board drifts out of frame and partner updates happen off-camera.
 */
async function keepPhoneOnBoard(p2: Page) {
  const board = p2.locator(".cw-board-area");
  if (await board.count()) await board.scrollIntoViewIfNeeded();
}

/**
 * Zoom the board down when its grid overflows the visible board area — the
 * CSS caps width but tall grids can still clip under `.cw-board-area`'s
 * overflow:hidden, pushing live letter updates off-camera. No-op when the
 * board already fits.
 */
async function fitBoardToViewport(page: Page) {
  await page.evaluate(() => {
    const area = document.querySelector<HTMLElement>(".cw-board-area");
    const board = document.querySelector<HTMLElement>(".cw-board");
    if (!area || !board) return;
    board.style.zoom = ""; // measure unzoomed
    const visible = area.getBoundingClientRect();
    const grid = board.getBoundingClientRect();
    const factor = Math.min(
      visible.width / grid.width,
      visible.height / grid.height,
      1,
    );
    if (factor < 1) board.style.zoom = String(Math.floor(factor * 100) / 100);
  });
}

/** Select a clue from the list and guess it correctly, at reading speed. */
async function solveClue(page: Page, clue: Clue) {
  // The clue list is filtered by direction tabs — make sure ours is showing.
  const tab = page.getByRole("button", {
    name: clue.direction === "ACROSS" ? /^across$/i : /^down$/i,
  });
  if (!((await tab.getAttribute("class")) ?? "").includes("cw-tab-active")) {
    await humanClick(page, tab);
  }
  const row = page
    .locator(".cw-clue-row", {
      has: page.locator(".cw-clue-badge", { hasText: String(clue.number) }),
      hasText: clue.questionText.slice(0, 20),
    })
    .first();
  await humanClick(page, row);
  const inputs = page.locator(".cw-letter-input");
  await expect(inputs).toHaveCount(clue.answer.length);
  await humanTypeLetters(page, clue.answer);
  // Read the boxes back before committing — if the auto-advance raced a slow
  // frame, fail here with a clear diff instead of a mystery wrong guess.
  const typed = await inputs.evaluateAll((els) =>
    els.map((e) => (e as HTMLInputElement).value).join(""),
  );
  expect(typed.toUpperCase()).toBe(clue.answer.toUpperCase());
  await dwell(page, 300, 800); // a beat to "read it back"
  const guess = page.getByRole("button", { name: /^guess$/i });
  await humanClick(page, guess);
  // Correct guesses clear the entry row — that's the scoring-path assertion.
  // Give the mutation a beat, then retry once if a re-render ate the click.
  await page.waitForTimeout(1500);
  if (await inputs.count()) {
    await humanClick(page, guess);
  }
  await expect(inputs).toHaveCount(0);
}

test("authenticated product tour", async ({ page, browser }, testInfo) => {
  // Player two gets their own phone recording — CI composites it
  // picture-in-picture into the published demo.mp4. Created at test start so
  // the phone video spans the full tour and shares its timeline.
  const ctx2 = await browser.newContext({
    ...devices["iPhone 13"],
    baseURL:
      process.env.E2E_BASE_URL ?? "https://crosswords-staging.casazza.io",
    recordVideo: { dir: path.join(testInfo.outputDir, "phone") },
  });
  const p2 = await ctx2.newPage();
  // Land on the home page immediately so the PiP isn't blank during chapter 1.
  await p2.goto("/");
  try {
    // ── Chapter 1: landing + sign-in ───────────────────────────────────────
    await test.step("Landing and sign-in", async () => {
      await page.goto("/");
      await expect(page.locator("#main")).not.toBeEmpty();
      await wander(page);
      await dwell(page);
      await signIn(page, EMAIL!, PASSWORD!);
      await dwell(page, 2000, 3000); // landed — let the viewer register it
    });

    // ── Player two signs in on their phone (the PiP in the video) ──────────
    await test.step("Player two on phone", async () => {
      await expect(p2.locator("#main")).not.toBeEmpty();
      await signInDirect(p2, (EMAIL2 ?? EMAIL)!, (PASSWORD2 ?? PASSWORD)!);
      await p2.goto("/games");
      await expect(p2.getByText("Available").first()).toBeVisible();
      await dwell(p2);
    });

  // ── Chapter 2: the lobby ─────────────────────────────────────────────────
  let clues: Clue[] = [];
  await test.step("Games lobby", async () => {
    await humanClick(page, page.locator('header a.navlink[href="/games"]'));
    await expect(page).toHaveURL(/\/games/, { timeout: 15_000 });
    // Feature gate: the three lobby panels render (even when empty).
    await expect(page.getByText("Available").first()).toBeVisible();
    await expect(page.getByText("Completed").first()).toBeVisible();
    await wander(page);
    await dwell(page, 2600, 3600);

    // Game rows are clickable cards (onclick handlers, not links). Prefer
    // resuming an ACTIVE game (straight to the board); otherwise start an
    // UNSTARTED one (via the pre-game card). Fresh starts can be slow.
    const card = (label: string) =>
      page
        .locator('div[style*="cursor: pointer"]')
        .filter({ hasText: label })
        .first();
    const active = card("ACTIVE");
    const unstarted = card("UNSTARTED");
    if (await active.count()) {
      await humanClick(page, active);
    } else if (await unstarted.count()) {
      await humanClick(page, unstarted);
      const start = page.getByRole("button", {
        name: /^(start game|continue game)$/i,
      });
      await expect(start).toBeVisible({ timeout: 20_000 });
      await dwell(page);
      await humanClick(page, start);
    } else {
      return; // no playable data on staging — later chapters degrade
    }
    await expect(page).toHaveURL(/\/game\/[^/]+$/, { timeout: 60_000 });

    // Feature gate: board + clue list actually rendered.
    await expect(page.locator(".cw-letter").first()).toBeVisible();
    await expect(page.locator(".cw-clue-row").first()).toBeVisible();
    await fitBoardToViewport(page);
    await dwell(page, 2400, 3400);

    // Pull the answers through the API (the client gets them too) so we can
    // play real, correct words on camera. Resumed games come back with some
    // clues already solved — play only what's still open.
    const activeId = page.url().split("/game/")[1];
    const data = await trpcGet(page, "activeGame.get", { id: activeId });
    const all = (data?.game?.questions ?? []) as Clue[];
    const solved = solvedClues(all, (data?.actions ?? []) as ClueAction[]);
    clues = all.filter((c) => !solved.has(`${c.number}${c.direction}`));
  });
  const completing = clues.length > 0 && clues.length <= COMPLETION_MAX_CLUES;
  const soloClues = completing ? clues : clues.slice(0, 3);
  const playedClues = new Set<string>();
  const clueKey = (c: Clue) => `${c.number}${c.direction}`;

  // ── Chapter 3: solve the first clue (never the last one — that's the
  //    finale, so a tiny puzzle doesn't complete mid-chapter) ──────────────
  if (soloClues.length >= 2) {
    await test.step("Solve a clue", async () => {
      await solveClue(page, soloClues[0]);
      playedClues.add(clueKey(soloClues[0]));
      await expect(page.locator(".cw-correct").first()).toBeVisible();
      await dwell(page, 2600, 3600); // let the correct letters sink in
    });
  }

  // ── Chapter 4: co-op — invite, join, presence, live letters ─────────────
  // Skipped when player two could finish the puzzle (a ≤2-clue game): their
  // final guess would complete it mid-chapter and yank the recorded page to
  // the results screen early.
  const partnerCouldFinish = completing && clues.length <= 2;
  if (soloClues.length > 1 && playedClues.size > 0 && !partnerCouldFinish) {
    await test.step("Co-op join + presence", async () => {
      // The invite affordance itself is part of the tour.
      const invite = page.locator(".cw-invite-btn");
      await expect(invite).toBeVisible();
      await humanClick(page, invite);
      await expect(page.getByText(/link copied/i)).toBeVisible();
      await dwell(page);

      const gameUrl = page.url();
      const secondAccount = Boolean(EMAIL2 && PASSWORD2);
      await fitBoardToViewport(page);
      // The phone player follows the invite link onto the same game.
      await p2.goto(gameUrl);
      await expect(p2.locator(".cw-letter").first()).toBeVisible();
      await keepPhoneOnBoard(p2);

        if (secondAccount) {
          // A different user gets the join prompt — unless a previous attempt
          // already joined them (retries/re-runs stay green). Wait for either:
          // the button, or their chip already being on their roster.
          const join = p2.getByRole("button", { name: /^join game$/i });
          const p2Chips = p2.locator(".cw-players .cw-chip");
          await expect(async () => {
            expect((await join.isVisible()) || (await p2Chips.count()) >= 2).toBeTruthy();
          }).toPass({ timeout: 25_000 });
          if (await join.isVisible()) {
            await join.click();
            await expect(join).not.toBeVisible();
          }
          // The roster on the *recorded* page lights up with the second chip.
          await expect(page.locator(".cw-players .cw-chip")).toHaveCount(2, {
            timeout: 20_000,
          });
          await dwell(page, 2600, 3600);
        }

        // Player two picks a different clue and works it — while we watch the
        // presence ring + roster badge light up on the recorded page.
        const partner = soloClues[1];
        const correctBefore = await page.locator(".cw-correct").count();
        const solving = solveClue(p2, partner);
        if (secondAccount) {
          await expect(
            page.locator('.cw-cell[style*="box-shadow"]').first(),
          ).toBeVisible({ timeout: 20_000 });
          await expect(
            page.locator(".cw-players .cw-chip-clue").first(),
          ).toBeVisible();
        }
        await solving;
        playedClues.add(clueKey(partner));
        await keepPhoneOnBoard(p2);

        // Player two's correct letters must land on the recorded board.
        await expect
          .poll(async () => page.locator(".cw-correct").count(), {
            timeout: 20_000,
          })
          .toBeGreaterThan(correctBefore);
        await dwell(page, 3200, 4200); // watch the partner's letters land live
    });
  }

  // ── Chapter 5: finish the puzzle (small games) ───────────────────────────
  if (completing) {
    await test.step("Complete the puzzle", async () => {
      for (const clue of clues) {
        if (playedClues.has(clueKey(clue))) continue;
        await solveClue(page, clue);
        await keepPhoneOnBoard(p2);
      }
      // The last correct guess completes the game and lands on the results.
      await expect(page).toHaveURL(/\/game\/[^/]+\/completed/, {
        timeout: 30_000,
      });
      await expect(
        page.getByRole("heading", { name: /crossword solved/i }),
      ).toBeVisible();
      await expect(page.getByText(/match standings/i)).toBeVisible();
      await wander(page);
      await dwell(page, 4200, 5500); // the results money shot
    });
  }

  // ── Chapter 6: stats — leaderboard, career, compare, teams ───────────────
  await test.step("Stats suite", async () => {
    await humanClick(page, page.locator('header a.navlink[href="/stats"]'));
    await expect(page).toHaveURL(/\/stats/, { timeout: 15_000 });

    // Leaderboard — podium/table (or its empty state) renders.
    await expect(page.getByText("Leaderboard").first()).toBeVisible();
    await dwell(page, 2600, 3600);

    // Career — the panel always renders; stat cards only once the account has
    // games (post-completion it does, and the numbers are fresh on camera).
    const career = page.getByText("Career").first();
    await career.scrollIntoViewIfNeeded();
    await expect(career).toBeVisible();
    if (await page.getByText("Global Rank").count()) {
      await expect(page.getByText("Global Rank")).toBeVisible();
      await dwell(page, 2600, 3600);
    }

    // Compare — pick an opponent and let the head-to-head render. The player
    // list is scoped to teammates (the e2e users share a seeded team, and the
    // co-op completion above gives them a real match record).
    const picker = page.locator("select.app-input");
    await picker.scrollIntoViewIfNeeded();
    await expect(picker).toBeVisible();
    const opponentCount = await picker.locator("option").count();
    if (opponentCount > 1) {
      await picker.selectOption({ index: 1 });
      await expect(page.getByText(/co-op match record/i)).toBeVisible({
        timeout: 15_000,
      });
      await dwell(page, 3200, 4400);
    }

    // Teams — the collaboration hub (create form + leaderboard).
    const teams = page.getByText(/create a team/i);
    await teams.scrollIntoViewIfNeeded();
    await expect(teams).toBeVisible();
    await wander(page);
    await dwell(page, 2400, 3400);
  });

  // ── Chapter 7: profile + the premium pitch ───────────────────────────────
  await test.step("Profile and subscription", async () => {
    await humanClick(page, page.locator('header a.navlink[href="/profile"]'));
    await expect(page).toHaveURL(/\/profile/, { timeout: 15_000 });
    await expect(page.getByText(/profile settings/i)).toBeVisible();
    await expect(page.getByLabel(/display name/i)).toBeVisible();
    await dwell(page);

    // Subscription panel: plan row always renders; Pro accounts show the
    // active chip, free accounts the quota + upgrade CTA.
    await expect(page.getByText(/current plan/i)).toBeVisible();
    const proActive = page.getByText(/^active$/i);
    const upgrade = page.getByRole("button", { name: /upgrade to pro/i });
    await expect(proActive.or(upgrade).first()).toBeVisible();
    await wander(page);
    await dwell(page, 2800, 3800); // end on the premium pitch
  });
  } finally {
    await ctx2.close();
  }
});
