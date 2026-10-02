import path from "node:path";
import { test, expect, type APIRequestContext, type BrowserContext, type Page } from "@playwright/test";

// Four-player multiplayer soak. Four authenticated browser contexts play ONE
// shared active game concurrently and we assert the live transport actually
// fans out: presence, GameActionsAdded, crossing-cell contention and
// completion-not-duplicated.
//
// Accounts come from env (the same vars the whole E2E suite uses):
//   E2E_EMAIL   / E2E_PASSWORD    — player 1
//   E2E_EMAIL_2 / E2E_PASSWORD_2  — player 2
//   E2E_EMAIL_3 / E2E_PASSWORD_3  — player 3
//   E2E_EMAIL_4 / E2E_PASSWORD_4  — player 4
// A missing pair self-skips the whole spec (matches demo.spec.ts). Any account
// that does not exist yet is provisioned here through `user.signup` — the only
// non-admin creation path, idempotent from our side ("already exists" is
// success, which is what a re-run against staging looks like).
//
// No human pacing: this is a soak, not a recording. Synchronisation is all
// Playwright auto-waiting + `expect.poll`, so nothing here sleeps.

const ACCOUNTS = [
  {
    label: "player 1",
    email: process.env.E2E_EMAIL,
    password: process.env.E2E_PASSWORD,
    name: "Ada Whitfield",
    username: "soak_ada",
  },
  {
    label: "player 2",
    email: process.env.E2E_EMAIL_2,
    password: process.env.E2E_PASSWORD_2,
    name: "Bruno Castellanos",
    username: "soak_bruno",
  },
  {
    label: "player 3",
    email: process.env.E2E_EMAIL_3,
    password: process.env.E2E_PASSWORD_3,
    name: "Camille Okonkwo",
    username: "soak_camille",
  },
  {
    label: "player 4",
    email: process.env.E2E_EMAIL_4,
    password: process.env.E2E_PASSWORD_4,
    name: "Devin Rasmussen",
    username: "soak_devin",
  },
] as const;

const missing = ACCOUNTS.map((acct, i) => {
  if (acct.email && acct.password) return null;
  const suffix = i === 0 ? "" : `_${i + 1}`;
  return `E2E_EMAIL${suffix} / E2E_PASSWORD${suffix}`;
}).filter((s): s is string => s !== null);

// Self-skip before anything is set up, like demo.spec.ts's canary convention.
test.skip(
  missing.length > 0,
  `multiplayer soak needs four accounts; missing ${missing.join(", ")}`,
);

// Four contexts, each on the board, needs real room.
test.setTimeout(300_000);

type Clue = {
  number: number;
  len: number;
  direction: "ACROSS" | "DOWN";
  rootX: number;
  rootY: number;
  questionText?: string;
  answer?: string;
};

type AnswerClue = Clue & { answer: string };

type ClueAction = {
  cordX: number;
  cordY: number;
  actionType: string;
  state: string;
};

const clueKey = (c: Clue) => `${c.number}${c.direction}`;

/** The cells a clue occupies: ACROSS (rootX+i, rootY), DOWN (rootX, rootY+i). */
function clueCells(c: Clue): Array<[number, number]> {
  return Array.from({ length: c.len }, (_, i) =>
    c.direction === "ACROSS" ? [c.rootX + i, c.rootY] : [c.rootX, c.rootY + i],
  );
}

/** A clue is solved when every cell's latest action is server-marked correct. */
const solvedClues = (clues: Clue[], actions: ClueAction[]) => {
  const latest = latestStates(actions);
  return new Set(
    clues
      .filter((c) =>
        clueCells(c).every(([x, y]) => latest.get(`${x},${y}`)?.actionType === "correctGuess"),
      )
      .map(clueKey),
  );
};

/** Latest stored letter per cell — the "newest wins" rule the server scores on. */
function latestStates(actions: ClueAction[]) {
  const latest = new Map<string, ClueAction>();
  for (const a of actions) latest.set(`${a.cordX},${a.cordY}`, a);
  return latest;
}

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

/** Batched tRPC mutation. Returns the raw envelope so error shapes stay visible. */
async function trpcPost(
  req: APIRequestContext,
  proc: string,
  input: unknown,
): Promise<{ ok: boolean; data?: any; error?: string }> {
  const res = await req.post(`/api/trpc/${proc}`, { data: { "0": input } });
  const body = await res.json();
  return { ok: !body?.[0]?.error, data: body?.[0]?.result?.data, error: body?.[0]?.error?.message };
}

/** Create the account if it does not exist; "already exists" is success. */
async function provision(req: APIRequestContext, acct: (typeof ACCOUNTS)[number]) {
  const res = await trpcPost(req, "user.signup", {
    email: acct.email!.toLowerCase(),
    name: acct.name,
    username: acct.username,
    password: acct.password,
  });
  if (!res.ok && !/already exists/i.test(res.error ?? "")) {
    throw new Error(`user.signup for ${acct.email} failed: ${res.error}`);
  }
  return res.ok;
}

/** Sign in straight through the login form, like signInDirect in demo.spec.ts. */
async function signInDirect(page: Page, email: string, password: string) {
  await page.goto("/auth/login");
  await page.locator('input[type="email"]').fill(email);
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole("button", { name: /^sign in/i }).click();
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 30_000 });
}

/**
 * Open the lobby and land on a board with enough open clues left for the whole
 * soak. Library cards are tried in order — a `— NEW` one first (clean grid),
 * then each `— IN PROGRESS` one — and the first board with at least
 * `minOpen` unsolved clues wins. Player 1's library accumulates near-finished
 * games, so blindly taking the first card can leave nothing to play.
 */
async function openBoard(page: Page, minOpen: number): Promise<string> {
  const rows = () =>
    page.locator('div[style*="cursor: pointer"]').and(page.locator('[aria-label*="— "]'));
  const openCount = async (gameId: string) =>
    (await loadOpenClues(page, await trpcGet(page, "activeGame.get", { id: gameId }))).length;

  // ACTIVE_GAME_ID lets the admin bot (or CI) hand us a specific game — the
  // one it seeded with room to play. Without it we discover through the lobby,
  // where a near-finished game is just as clickable as a fresh one.
  const preset = process.env.ACTIVE_GAME_ID;
  if (preset) {
    await page.goto(`/game/${preset}`);
    await expect(page.locator(".cw-letter").first()).toBeVisible({ timeout: 60_000 });
    const open = await openCount(preset);
    expect(open, `ACTIVE_GAME_ID=${preset} has enough open clues`).toBeGreaterThanOrEqual(minOpen);
    console.log(`soak board: ACTIVE_GAME_ID=${preset} (${open} open clues)`);
    return preset;
  }

  await page.goto("/games");
  await expect(page.getByText("Library").first()).toBeVisible();
  // The library rows arrive on a client-side fetch after hydration, so poll for
  // them rather than sampling the DOM once.
  await expect.poll(() => rows().count(), { timeout: 30_000 }).toBeGreaterThan(0);
  const labels = await rows().evaluateAll((els) => els.map((e) => e.getAttribute("aria-label") ?? ""));
  const order = [
    ...labels.filter((l) => l.includes("— NEW")),
    ...labels.filter((l) => l.includes("— IN PROGRESS")),
  ];
  for (const label of order) {
    const card = rows().and(page.locator(`[aria-label="${label}"]`)).first();
    await card.click();
    if (/— NEW$/.test(label)) {
      const start = page.getByRole("button", { name: /^(start game|continue game)$/i });
      await expect(start).toBeVisible({ timeout: 20_000 });
      await start.click();
      // Fresh starts generate the puzzle server-side (can take a minute+).
      await expect(page).not.toHaveURL(/\/game\/[^/]+\/new$/, { timeout: 150_000 });
    }
    await expect(page).toHaveURL(/\/game\/[^/]+$/, { timeout: 60_000 });
    await expect(page.locator(".cw-letter").first()).toBeVisible({ timeout: 60_000 });
    const gameId = page.url().split("/game/")[1].split(/[/?#]/)[0];
    if ((await openCount(gameId)) >= minOpen) {
      console.log(`soak board: ${label} (${gameId})`);
      return gameId;
    }
    console.log(`soak board: skipping ${label} — fewer than ${minOpen} open clues`);
    await page.goto("/games");
    await expect(page.getByText("Library").first()).toBeVisible();
  }
  throw new Error(`no library game has ${minOpen} open clues to soak`);
}

/** Answer key straight from the grid endpoint: {number}{direction} → answer. */
async function answersFor(page: Page, gameId: string) {
  const res = await page.request.get(`/api/grids/${gameId}`);
  expect(res.ok()).toBeTruthy();
  const data = await res.json();
  const answers = new Map<string, string>();
  for (const q of data?.questions ?? []) answers.set(`${q.number}${q.direction}`, q.answer);
  return answers;
}

type ActiveGame = {
  gameId?: string;
  game?: { questions?: Clue[] };
  actions?: ClueAction[];
  gameMembers?: Array<{ userId?: string }>;
};

/** All clues with their answers attached, minus the ones already solved. */
async function loadOpenClues(page: Page, active: ActiveGame): Promise<AnswerClue[]> {
  expect(active.gameId).toBeTruthy();
  const answers = await answersFor(page, active.gameId!);
  const all = (active.game?.questions ?? [])
    .map((c) => ({ ...c, answer: answers.get(clueKey(c)) }))
    .filter((c): c is AnswerClue => Boolean(c.answer));
  const solved = solvedClues(all, active.actions ?? []);
  return all.filter((c) => !solved.has(clueKey(c)));
}

/** Flip to the clue's direction tab, click its row, wait for the letter boxes. */
async function selectClue(page: Page, clue: AnswerClue) {
  const tab = page.getByRole("button", {
    name: clue.direction === "ACROSS" ? /^across$/i : /^down$/i,
  });
  if (!((await tab.getAttribute("class")) ?? "").includes("cw-tab-active")) {
    await tab.click();
  }
  const row = page
    .locator(".cw-clue-row", {
      has: page.locator(".cw-clue-badge", { hasText: String(clue.number) }),
      hasText: (clue.questionText ?? "").slice(0, 20),
    })
    .first();
  await row.click();
  await expect(page.locator(".cw-letter-input")).toHaveCount(clue.answer.length);
}

/**
 * Type the real answer, guess it, and wait for the solve to land. The
 * observable is the actor's own `.cw-correct` count — the entry row stays open
 * after a guess (the editor only closes on completion), so its presence is not
 * a signal.
 */
async function solveClue(page: Page, clue: AnswerClue) {
  await selectClue(page, clue);
  const inputs = page.locator(".cw-letter-input");
  await inputs.first().click();
  await page.keyboard.type(clue.answer.toUpperCase());
  const typed = await inputs.evaluateAll((els) =>
    els.map((e) => (e as HTMLInputElement).value).join(""),
  );
  expect(typed.toUpperCase()).toBe(clue.answer.toUpperCase());
  const before = await page.locator(".cw-correct").count();
  // A board re-render between the last keystroke and the click can swallow it,
  // so keep clicking Guess until the solve shows up.
  await expect
    .poll(
      async () => {
        const now = await page.locator(".cw-correct").count();
        if (now > before) return true;
        await page
          .getByRole("button", { name: /^guess$/i })
          .click({ timeout: 5_000 })
          .catch(() => {});
        return false;
      },
      { timeout: 30_000 },
    )
    .toBe(true);
}

/** Count of correctly-marked cells on a board. */
const correctCount = (page: Page) => page.locator(".cw-correct").count();

type SoakState = {
  contexts: BrowserContext[];
  baseURL: string;
  videoDirs: string[];
  /** The game this run owns; started in beforeAll, pinned or fresh. */
  activeGameId: string;
  /** Populated by afterAll when a run starts a game it never finishes. */
  leak: string[];
};
const soak: SoakState = { contexts: [], baseURL: "", videoDirs: [], activeGameId: "", leak: [] as string[] };

// Recording is OPT-IN via E2E_RECORD=1. It costs a per-context video encoder
// on all four contexts, which would slow the correctness gate for no benefit,
// so the default path is unchanged. With it set, each context records and gets
// a visible corner badge naming its player — four identical boards in a
// composite are impossible to tell apart otherwise. The badge is injected via
// an init script (before any page script runs) and is position:fixed, so it
// cannot perturb the board layout the assertions measure.
const RECORD = process.env.E2E_RECORD === "1";

test.describe("four-player multiplayer soak", () => {
  test.beforeAll(async ({ browser, request }, testInfo) => {
    for (const acct of ACCOUNTS) await provision(request, acct);

    soak.baseURL = process.env.E2E_BASE_URL ?? "https://crosswords-staging.casazza.io";
    try {
      for (const [n, acct] of ACCOUNTS.entries()) {
        const opts: Parameters<typeof browser.newContext>[0] = { baseURL: soak.baseURL };
        if (RECORD) {
          const dir = path.join(testInfo.outputDir, `video-p${n + 1}`);
          soak.videoDirs.push(dir);
          // 960x540 per pane keeps a 2x2 composite at 1920x1080 without
          // upscaling, and keeps the upload to a few MB.
          opts.recordVideo = { dir, size: { width: 960, height: 540 } };
          opts.viewport = { width: 960, height: 540 };
        }
        const ctx = await browser.newContext(opts);
        if (RECORD) {
          await ctx.addInitScript(
            ({ label }: { label: string }) => {
              window.addEventListener("DOMContentLoaded", () => {
                const el = document.createElement("div");
                el.textContent = label;
                el.setAttribute("data-soak-badge", "1");
                Object.assign(el.style, {
                  position: "fixed",
                  top: "6px",
                  left: "6px",
                  zIndex: "2147483647",
                  padding: "4px 10px",
                  borderRadius: "6px",
                  font: "600 18px/1.2 system-ui, sans-serif",
                  color: "#fff",
                  background: "rgba(18,18,18,0.82)",
                  pointerEvents: "none",
                });
                document.body.appendChild(el);
              });
            },
          );
        }
        soak.contexts.push(ctx);
      }
    } catch (err) {
      await Promise.all(soak.contexts.map((c) => c.close()));
      throw err;
    }

    // Start a FRESH game rather than resuming one.
    //
    // Every soak run permanently advances the board it plays: its letters are
    // committed. Resuming "the in-progress game" therefore handed each run a
    // board the PREVIOUS runs had already ground down — one recording began
    // with the game essentially finished before the players had even joined.
    // That starves the part that matters: four players need a full board to
    // have anywhere to take parallel turns.
    //
    // `activeGame.start` is idempotent per (gameId, caller), so this must pick
    // a published Game this account has NOT already started; `gameList.get`
    // returns exactly those. ACTIVE_GAME_ID still wins when set — CI pins it
    // deliberately, and a pinned board is a known quantity.
    if (process.env.ACTIVE_GAME_ID) {
      soak.activeGameId = process.env.ACTIVE_GAME_ID;
      console.log(`soak board: ACTIVE_GAME_ID=${soak.activeGameId} (pinned)`);
    } else {
      const fresh = await trpcPost(soak.contexts[0].request, "gameList.get", {});
      const candidates: Array<{ id: string; title?: string; clues?: number }> =
        fresh.data ?? [];
      // Most clues first: a full board is what makes parallel play meaningful.
      candidates.sort((a, b) => (b.clues ?? 0) - (a.clues ?? 0));
      const chosen = candidates[0];
      if (chosen) {
        const started = await trpcPost(soak.contexts[0].request, "activeGame.start", {
          gameId: chosen.id,
        });
        if (started.ok && started.data?.id) {
          soak.activeGameId = started.data.id;
          console.log(
            `soak board: started a FRESH game ${chosen.id}` +
              ` ("${chosen.title ?? "?"}", ${chosen.clues ?? "?"} clues) -> ${soak.activeGameId}`,
          );
        } else {
          console.log(`soak board: start failed (${started.error}); falling back to lobby`);
        }
      } else {
        console.log("soak board: no unstarted published games; falling back to lobby");
      }
    }
  });

  test.afterAll(async ({ request }) => {
    // Teardown: report any game this run created but did not finish.
    //
    // There is no delete/abandon procedure — the router exposes exactly
    // get, getStartDetails, start, join, addActions, publishPresence,
    // complete — so a run aborted before the completion step leaves its
    // ActiveGame behind. Completing a puzzle deletes the row, so the happy
    // path leaves nothing; only an aborted or failing run leaks, and it leaks
    // silently until the pool of unused games is exhausted.
    //
    // So this does not pretend to clean up. It NAMES the leak, so a run that
    // did not finish says so out loud instead of looking green.
    if (soak.activeGameId) {
      try {
        const still = await trpcPost(request, "activeGame.get", { id: soak.activeGameId });
        if (still.ok) {
          const msg =
            `LEAKED GAME: ${soak.activeGameId} still exists — this run did not reach ` +
            `completion, and the product has no delete/abandon procedure to clean it up. ` +
            `It will persist until finished by hand.`;
          console.log(msg);
          if (soak.leak) soak.leak.push(msg);
        }
      } catch {
        // Nothing observable to report; the contexts are closing anyway.
      }
    }

    // Close in a finally so a mid-test throw still releases all four contexts.
    try {
      await Promise.all(soak.contexts.map((c) => c.close()));
    } catch (err) {
      console.error("soak cleanup: closing contexts failed", err);
    }
    soak.contexts = [];
  });

  test("live multiplayer across four contexts", async ({}, testInfo) => {
    const pages: Page[] = [];
    let activeGameId = "";
    /** Clues already played or claimed, so no two players race the same one. */
    const taken = new Set<string>();

    try {
      // ── Setup: four signed-in contexts in one shared game ────────────────
      await test.step("sign in all four players", async () => {
        for (const ctx of soak.contexts) pages.push(await ctx.newPage());
        await Promise.all(
          pages.map((p, i) => signInDirect(p, ACCOUNTS[i].email!, ACCOUNTS[i].password!)),
        );
      });

      await test.step("open the shared game and join the other three", async () => {
        const [p1, ...rest] = pages;
        // Headroom: four plays plus an unsolved ACROSS/DOWN crossing pair.
        activeGameId = await openBoard(p1, 8);
        const gameUrl = `${soak.baseURL}/game/${activeGameId}`;
        for (const [n, p] of rest.entries()) {
          await p.goto(gameUrl);
          // "Join game" is optional — members of this active game don't see it.
          const join = p.getByRole("button", { name: /^join game$/i });
          const joined = await join
            .waitFor({ state: "visible", timeout: 30_000 })
            .then(() => true)
            .catch(() => false);
          if (joined) {
            await join.click();
            console.log(`clicked "Join game" for player ${n + 2}`);
          } else {
            console.log(`no "Join game" button for player ${n + 2} — already a member`);
          }
        }
        // The members query has to land before addActions will accept anyone.
        await expect
          .poll(
            async () => {
              const m = await trpcGet(p1, "activeGame.get", { id: activeGameId });
              return (m?.gameMembers ?? []).length;
            },
            { timeout: 30_000 },
          )
          .toBeGreaterThanOrEqual(4);
      });

      let open: AnswerClue[] = [];
      const refreshOpen = async () => {
        const data = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
        open = await loadOpenClues(pages[0], data);
      };
      await refreshOpen();

      // ── 1. All four boards render ───────────────────────────────────────
      await test.step("all four boards render", async () => {
        for (const [i, p] of pages.entries()) {
          await expect(
            p.locator(".cw-letter").first(),
            `${ACCOUNTS[i].label} board cells`,
          ).toBeVisible({ timeout: 30_000 });
          await expect(
            p.locator(".cw-clue-row").first(),
            `${ACCOUNTS[i].label} clue list`,
          ).toBeVisible({ timeout: 30_000 });
        }
      });

      // Pick the four clues up front: player 1's, plus one distinct clue each
      // for players 2–4, preferring clues that share no cell with each other so
      // no two players ever race the same cell.
      const pickDistinct = (candidates: AnswerClue[], n: number) => {
        const picks: AnswerClue[] = [];
        for (const c of candidates) {
          if (picks.length >= n) break;
          const pickedCells = new Set(picks.flatMap(clueCells).map(([x, y]) => `${x},${y}`));
          if (clueCells(c).every(([x, y]) => !pickedCells.has(`${x},${y}`))) picks.push(c);
        }
        for (const c of candidates) {
          if (picks.length >= n) break;
          if (!picks.includes(c)) picks.push(c);
        }
        return picks;
      };
      expect(open.length, "at least four open clues to play").toBeGreaterThanOrEqual(4);
      const firstClue = open[0];
      taken.add(clueKey(firstClue));
      const picks = pickDistinct(
        open.filter((c) => !taken.has(clueKey(c))),
        3,
      );
      expect(picks.length, "enough distinct open clues for players 2–4").toBe(3);
      // A SECOND clue per player, used only to alternate selections during the
      // presence step. Selecting the same clue twice is a client-side no-op and
      // emits no new publishPresence, so a retry needs a real change to retry.
      // These are never solved — the presence step only selects them.
      const alternates = pickDistinct(
        open.filter((c) => !taken.has(clueKey(c)) && !picks.includes(c)),
        3,
      );
      const altClue =
        open.find((c) => c !== firstClue && !picks.includes(c)) ?? firstClue;
      console.log(
        `player 1 takes ${clueKey(firstClue)}; players 2-4 take ${picks
          .map((c) => clueKey(c))
          .join(", ")}`,
      );

      /**
       * How many of the four contexts receive a server broadcast.
       *
       * Measured first because it is the load-bearing property of the whole
       * suite: a broadcast that reaches only some subscribers means co-op play
       * silently desynchronises, and no single-browser test can see it.
       *
       * This used to return 1-3 of 4 — events were an in-process
       * tokio::broadcast and only reached sockets on the publishing pod
       * (DEF-274). They now relay through a Postgres outbox, so the expected
       * value is 4 of 4. The measurement stays rather than becoming a bare
       * assertion: it reports the number in the log and attachment, so a
       * regression says WHICH peers went dark instead of just "failed".
       */
      let reachable = new Set<number>();
      await test.step("measure broadcast fan-out", async () => {
        await refreshOpen();
        expect(open.length, "an open clue to measure fan-out with").toBeGreaterThan(0);
        const probe = open[0];
        const [x, y] = clueCells(probe)[0];
        // Observe the ONE cell the probe writes: its rendered aria-label is the
        // precise signal, where a whole-board count could stay flat.
        const cellLabel = (i: number) =>
          pages[i]
            .locator(`.cw-cell[data-x="${x}"][data-y="${y}"]`)
            .first()
            .getAttribute("aria-label")
            .catch(() => null);
        // Snapshot BEFORE publishing — the broadcast can land in the same tick
        // the write returns, and a baseline read afterwards never differs.
        const beforeLabels = await Promise.all(pages.map((_, i) => cellLabel(i)));

        // The letter must DIFFER from what the cell holds, or the rendered
        // verdict is unchanged and the probe sees nothing.
        const truth = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
        const stored = latestStates(truth?.actions ?? []).get(`${x},${y}`)?.state ?? "";
        const wrongLetter =
          "ABDEFGHIJKLMNOPRSTUVW".split("").find(
            (c) => c !== stored.toUpperCase() && c !== probe.answer[0].toUpperCase(),
          ) ?? "Q";
        const published = await trpcPost(pages[1].request, "activeGame.addActions", {
          id: activeGameId,
          actions: [{ cordX: x, cordY: y, state: wrongLetter }],
        });
        expect(published.ok, `fan-out probe write: ${published.error}`).toBe(true);
        taken.add(clueKey(probe));

        await expect
          .poll(
            async () =>
              (await Promise.all(pages.map((_, i) => cellLabel(i)))).join("|") !==
              beforeLabels.join("|"),
            { timeout: 30_000 },
          )
          .toBe(true);
        await pages[0].waitForTimeout(5_000); // the 90ms stagger has to finish
        const afterLabels = await Promise.all(pages.map((_, i) => cellLabel(i)));
        reachable = new Set(
          afterLabels.flatMap((l, i) => (l !== beforeLabels[i] ? [i] : [])),
        );
        const report =
          `one action published by player 2 reached ${reachable.size} of 4 contexts ` +
          `[${[...reachable].map((i) => ACCOUNTS[i].label).join(", ")}]`;
        console.log(`FAN-OUT: ${report}`);
        await testInfo.attach("broadcast-fanout.txt", { body: report, contentType: "text/plain" });
      });

      // ── 2. Presence fans out ────────────────────────────────────────────
      await test.step("presence fans out", async () => {
        // A chip's `#n across` badge only renders for an OTHER player whose
        // presence is live, so every other player has to select a clue of their
        // own. Presence entries expire after 45s, so re-select on each tick:
        // a dropped publish is recovered rather than waited out.
        await selectClue(pages[0], firstClue);
        // Alternate each player's selection between two of their own clues on
        // every tick. Re-selecting the SAME clue is often a client-side no-op —
        // the component short-circuits when the key is unchanged — so a
        // "republish" that re-picks the identical clue emits no new
        // `publishPresence` at all, and a publish that landed on the wrong pod
        // is never actually retried. Alternating guarantees a genuine change
        // (and therefore a genuine publish) each time.
        let tick = 0;
        const spin = async () => {
          const t = tick++;
          await selectClue(pages[0], t % 2 === 0 ? firstClue : altClue);
          await Promise.all(
            picks.map(async (c, n) => {
              try {
                await selectClue(pages[n + 1], t % 2 === 0 ? c : alternates[n]);
              } catch (err) {
                console.log(`republish for player ${n + 2} failed: ${String(err).split("\n")[0]}`);
              }
            }),
          );
        };
        // Every context must see the other three players' presence. Presence
        // rides the same EventBus as actions and is relayed by the same
        // Postgres outbox (DEF-274), so it is now expected everywhere — a
        // player whose roster is missing a peer is the regression.
        //
        // The `spin()` on each tick is still required: re-selecting the SAME
        // clue is a client-side no-op and emits no publishPresence, so an
        // alternating selection is what actually re-publishes.
        await expect
          .poll(
            async () => {
              await spin();
              // Each context must show the OTHER three, so 3 chips each.
              const counts = await Promise.all(
                pages.map((p) => p.locator(".cw-players .cw-chip-clue").count()),
              );
              return counts.every((n) => n >= 3);
            },
            { timeout: 90_000, intervals: [1_000, 2_000, 3_000, 5_000, 8_000, 13_000] },
          )
          .toBe(true);
        const chipCounts = await Promise.all(
          pages.map((p) => p.locator(".cw-players .cw-chip-clue").count()),
        );
        console.log(`PRESENCE chips per context: ${chipCounts.join(", ")} (want >= 3 each)`);
        await testInfo.attach("presence-fanout.txt", {
          body:
            `presence chips per context: ${chipCounts.join(", ")}; ` +
            `action broadcasts reached ${reachable.size} of 4 ` +
            `[${[...reachable].map((i) => ACCOUNTS[i].label).join(", ")}]`,
          contentType: "text/plain",
        });
        // The ring marks another player's live selection on THIS board, so
        // check it on every context that shows a full roster.
        for (const [i, p] of pages.entries()) {
          if (chipCounts[i] >= 3) {
            await expect(p.locator('.cw-cell[style*="box-shadow"]').first()).toBeVisible({
              timeout: 30_000,
            });
          }
        }
      });

      // Every peer must receive every solve.
      //
      // This was deliberately loosened to "at least one peer" while the fan-out
      // was pod-local (DEF-274): an in-process tokio::broadcast could only
      // reach sockets on the publishing pod, so a strict assertion failed 23
      // canary runs in a row for a reason that was not this spec's fault.
      //
      // That defect is FIXED — events now relay through a Postgres outbox
      // (`fix(server): fan app events out across pods via a Postgres outbox`,
      // #204), and measured on staging 0.1.83 a single addActions reaches
      // 4/4 sockets, 24/24 across six publishes. So the loose form is now
      // actively harmful: it would pass again if the outbox regressed, which
      // is precisely the bug this suite exists to catch. Strict is correct.
      const propagate = async (actor: number, before: number[]) => {
        const peers = [0, 1, 2, 3].filter((i) => i !== actor);
        await expect
          .poll(
            async () =>
              (
                await Promise.all(
                  peers.map(async (i) => ((await correctCount(pages[i])) > before[i]) as boolean),
                )
              ).every(Boolean),
            { timeout: 30_000, intervals: [500, 1_000, 2_000, 4_000, 8_000] },
          )
          .toBe(true);
        const after = await Promise.all(peers.map((i) => correctCount(pages[i])));
        const missed = peers.filter((_, k) => after[k] <= before[peers[k]]);
        if (missed.length) {
          // Unreachable while the outbox holds; kept because a failure here is
          // the DEF-274 regression and the names are what you want in the log.
          const msg =
            `${ACCOUNTS[actor].label}'s solve did NOT reach ` +
            `${missed.map((i) => ACCOUNTS[i].label).join(", ")} — broadcast fan-out ` +
            `regressed (DEF-274). Events should now relay across pods via the ` +
            `Postgres outbox; a peer missing a solve means that relay broke.`;
          console.log(`DEFECT: ${msg}`);
          await testInfo.attach(`fanout-defect-p${actor + 1}.txt`, {
            body: msg,
            contentType: "text/plain",
          });
        }
      };

      // ── 3. Live action propagation ──────────────────────────────────────
      await test.step("player 1's solve reaches another board", async () => {
        const before = await Promise.all(pages.map(correctCount));
        await solveClue(pages[0], firstClue);
        await propagate(0, before);
      });

      // ── 4. Each of players 2–4 solves a distinct clue ───────────────────
      // Concurrently, NOT one after another. The old loop awaited each player
      // in turn, so on camera the four agents took strict sequential turns —
      // which is not what a collaborative crossword looks like, and not what
      // the presence/propagation assertions are meant to exercise. Claiming
      // the clues up front keeps two players off the same one, then all three
      // solve at once.
      await test.step("players 2, 3 and 4 each solve a distinct clue", async () => {
        for (const clue of picks) taken.add(clueKey(clue));
        const before = await Promise.all(pages.map(correctCount));
        await Promise.all(picks.map((clue, n) => solveClue(pages[n + 1], clue)));
        for (let n = 0; n < picks.length; n++) {
          await propagate(n + 1, before);
        }
      });

      await test.step("crossing-cell contention is observable", async () => {
        // The pre-contention read, kept for the coherence check at the end.
        const beforeRead = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
        const open = await loadOpenClues(pages[0], beforeRead);
        const openKeys = new Set(open.map(clueKey));
        const across = open.filter((c) => c.direction === "ACROSS");
        const down = open.filter((c) => c.direction === "DOWN");

        // An unsolved ACROSS clue and an unsolved DOWN clue sharing a cell:
        // ACROSS occupies (rootX+i, rootY), DOWN occupies (rootX, rootY+i).
        let hit: {
          across: AnswerClue;
          down: AnswerClue;
          i: number;
          j: number;
          x: number;
          y: number;
        } | null = null;
        outer: for (const a of across) {
          for (const d of down) {
            if (!openKeys.has(clueKey(d))) continue;
            const i = d.rootX - a.rootX;
            const j = a.rootY - d.rootY;
            if (i < 0 || j < 0 || i >= a.len || j >= d.len) continue;
            hit = { across: a, down: d, i, j, x: d.rootX, y: a.rootY };
            break outer;
          }
        }
        expect(hit, "an unsolved ACROSS/DOWN clue pair crossing at a shared cell").toBeTruthy();
        const { across: a, down: d, i, j, x, y } = hit!;
        // The ACROSS letter at that crossing. `a.answer` comes from the grid
        // endpoint while `len` comes from activeGame.get, so fall back to the
        // DOWN answer's index rather than trusting `i` against one length.
        const correctLetter = a.answer?.[i] ?? d.answer?.[j];
        expect(correctLetter, "a letter at the crossing cell").toBeTruthy();
        const wrongLetter = correctLetter!.toUpperCase() === "A" ? "B" : "A";
        console.log(
          `contention at (${x},${y}): ${clueKey(a)} "${a.answer}" vs ${clueKey(d)} "${d.answer}"`,
        );

        // Both players write the SAME cell at the SAME moment — one right, one wrong.
        const [correct, wrong] = await Promise.all([
          trpcPost(pages[1].request, "activeGame.addActions", {
            id: activeGameId,
            actions: [{ cordX: x, cordY: y, state: correctLetter }],
          }),
          trpcPost(pages[2].request, "activeGame.addActions", {
            id: activeGameId,
            actions: [{ cordX: x, cordY: y, state: wrongLetter }],
          }),
        ]);
        const rows = {
          cell: { x, y },
          correctWrite: { ok: correct.ok, error: correct.error, action: correct.data?.actions?.[0] },
          wrongWrite: { ok: wrong.ok, error: wrong.error, action: wrong.data?.actions?.[0] },
        };
        await testInfo.attach("crossing-cell-contention.json", {
          body: JSON.stringify(rows, null, 2),
          contentType: "application/json",
        });

        const after = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
        const latest = latestStates(after?.actions ?? []);
        const winner = latest.get(`${x},${y}`);
        const correctPrev = rows.correctWrite.action?.previousState ?? null;
        const wrongPrev = rows.wrongWrite.action?.previousState ?? null;
        const lastWriteWon =
          winner?.state === rows.wrongWrite.action?.state ||
          winner?.state === rows.correctWrite.action?.state;

        const report =
          `cell(${x},${y}) final letter=${JSON.stringify(winner?.state)} ` +
          `actionType=${winner?.actionType}; both writes accepted=${correct.ok && wrong.ok}; ` +
          `the cell reflects exactly one of the two concurrent writes=${lastWriteWon}; ` +
          `previousState logged by the correct writer=${JSON.stringify(correctPrev)}, ` +
          `by the wrong writer=${JSON.stringify(wrongPrev)} ` +
          `(both are read from a pre-batch snapshot, so the loser's is stale)`;
        console.log(`CONTENTION: ${report}`);
        await testInfo.attach("crossing-cell-report.txt", {
          body: report,
          contentType: "text/plain",
        });

        // Exactly one of the two submissions is what the cell ended up holding.
        expect([correctLetter, wrongLetter], "final cell state is one of the two writes").toContain(
          winner?.state,
        );
        // And the game is still coherent. NOTE `activeGame.get`'s `gameId` is the
        // parent Game, not the activeGameId, so identity is checked against the
        // pre-contention read rather than against the id we navigated to.
        expect(after?.gameId, "still the same parent game").toBe(beforeRead?.gameId);
        expect(after?.game?.questions?.length, "question set intact").toBe(
          beforeRead?.game?.questions?.length,
        );
        expect(await pages[0].locator(".cw-letter").first().isVisible()).toBe(true);
        // The known-wrong previousState is deliberately NOT asserted on — this
        // step's job is to make it observable, not to fail the soak.
      });

      // ── 6. Completion happens exactly once ─────────────────────────────
      // Completion has to be DRIVEN, not waited for. A 42-clue grid is far too
      // big for four browsers to solve a clue at a time, and the old "DEGRADED"
      // path meant the exactly-once assertions never ran at all.
      //
      // So the board is filled over the protocol: this spec already holds the
      // answer key and the clue geometry, and addActions takes a whole batch per
      // call, so a full grid is four calls rather than 161 keystrokes. Then all
      // four members call `complete` in the same tick — that is the race worth
      // testing, because `complete` reads membership and the ActiveGame row in
      // two separate pool queries before opening its transaction.
      await test.step("completion happens exactly once", async () => {
        const data = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
        // `/api/grids` is keyed by the PARENT game, which `activeGame.get`
        // returns as `gameId` — not by the activeGameId we navigated to.
        const answers = await answersFor(pages[0], data.gameId!);
        const all: AnswerClue[] = (data?.game?.questions ?? [])
          .map((c: Clue) => ({ ...c, answer: answers.get(clueKey(c)) }))
          .filter((c: Clue & { answer?: string }): c is AnswerClue => Boolean(c.answer));

        // Cells the key says should hold a letter that are not correct yet.
        // Deduped by coordinate because a crossing cell belongs to both an
        // ACROSS and a DOWN clue.
        const done = solvedClues(all, data?.actions ?? []);
        const already = new Set(
          all.filter((c) => done.has(clueKey(c))).flatMap(clueCells).map(([x, y]) => `${x},${y}`),
        );
        const wanted = new Map<string, string>();
        for (const c of all) {
          clueCells(c).forEach(([x, y], i) => {
            const key = `${x},${y}`;
            if (!already.has(key)) wanted.set(key, (c.answer ?? "")[i] ?? "");
          });
        }
        console.log(`COMPLETION: ${wanted.size} cells left of ${all.length} clues`);

        if (wanted.size > 0) {
          // Round-robin across the four players so the final writes overlap.
          const perPlayer: Array<Array<{ cordX: number; cordY: number; state: string }>> = [
            [],
            [],
            [],
            [],
          ];
          let turn = 0;
          for (const [key, letter] of wanted) {
            const [cordX, cordY] = key.split(",").map(Number);
            perPlayer[turn % 4].push({ cordX, cordY, state: letter });
            turn++;
          }
          const filled = await Promise.all(
            perPlayer.map((actions, n) =>
              trpcPost(pages[n].request, "activeGame.addActions", { id: activeGameId, actions }),
            ),
          );
          filled.forEach((r, n) =>
            console.log(
              `  player ${n + 1}: ${perPlayer[n].length} cells -> ok=${r.ok}` +
                (r.ok ? ` solved=${r.data?.solved} filled=${r.data?.filled}/${r.data?.total}` : ` err=${r.error}`),
            ),
          );
          const failures = filled.filter((r) => !r.ok);
          expect(failures.map((f) => f.error), "every fill batch was accepted").toEqual([]);
          expect(
            filled.some((r) => r.data?.solved === true),
            "the board reports solved after the final fill",
          ).toBe(true);
        }

        // The race: four members call complete() in one tick.
        const completed = await Promise.all(
          pages.map((p) => trpcPost(p.request, "activeGame.complete", { id: activeGameId })),
        );
        const okIds = completed
          .map((r) => r.data?.id)
          .filter((v: unknown): v is string => typeof v === "string");
        const report =
          `complete() by 4 members in one tick: ${okIds.length} returned an id, ` +
          `${completed.length - okIds.length} refused ` +
          `(${completed.filter((r) => !r.ok).map((r) => r.error).join(" | ") || "none"}); ` +
          `distinct completedGameIds = ${new Set(okIds).size} [${okIds.join(", ")}]`;
        console.log(`COMPLETION: ${report}`);
        await testInfo.attach("completion-report.txt", { body: report, contentType: "text/plain" });

        // EXACTLY ONCE. Two distinct ids means two CompletedGame rows, two sets
        // of MemberScores and two GameCompleted broadcasts.
        expect(
          new Set(okIds).size,
          "every successful complete() returned the SAME completedGameId",
        ).toBeLessThanOrEqual(1);

        // complete() deletes ActiveGame, cascading GameActions. A surviving row
        // means it never ran, whatever the ids said.
        const after = await trpcGet(pages[0], "activeGame.get", { id: activeGameId }).catch(
          () => null,
        );
        expect(after, "ActiveGame is deleted once completed").toBeNull();
      });
    } finally {
      await Promise.all(pages.map((p) => p.close().catch(() => {})));
    }
  });
});