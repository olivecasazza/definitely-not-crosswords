import path from "node:path";
import { test, expect, type APIRequestContext, type BrowserContext, type Page } from "@playwright/test";

// Four-player competitive race: four agents each play their own game on the
// SAME parent puzzle, racing to complete it, and the run reports final standings
// (placement, score, timing).
//
// Each player gets a solo ActiveGame on the parent — `activeGame.start` is
// idempotent per (gameId, caller), so starting the same parent four times yields
// four separate games. Each player solves their own board independently, calls
// complete(), and the standings are read from the server-reported score in
// stats.getCompletedGame → gameStats.memberScores[].
//
// Note: with identical puzzles and deterministic solving, all four scores will
// be equal (max correct), so placement is a *tie*. Timing is observed (per
// logs) as reference but is NOT a pass/fail criterion — scores are.
//
// Accounts come from env (same format as multiplayer-soak.spec.ts):
//   E2E_EMAIL   / E2E_PASSWORD    — player 1
//   E2E_EMAIL_2 / E2E_PASSWORD_2  — player 2
//   E2E_EMAIL_3 / E2E_PASSWORD_3  — player 3
//   E2E_EMAIL_4 / E2E_PASSWORD_4  — player 4
// Self-skips if any pair is missing.

const ACCOUNTS = [
  {
    label: "player 1",
    email: process.env.E2E_EMAIL,
    password: process.env.E2E_PASSWORD,
    name: "Alice Ramirez",
    username: "race_alice",
  },
  {
    label: "player 2",
    email: process.env.E2E_EMAIL_2,
    password: process.env.E2E_PASSWORD_2,
    name: "Bob Tran",
    username: "race_bob",
  },
  {
    label: "player 3",
    email: process.env.E2E_EMAIL_3,
    password: process.env.E2E_PASSWORD_3,
    name: "Carol Nakamura",
    username: "race_carol",
  },
  {
    label: "player 4",
    email: process.env.E2E_EMAIL_4,
    password: process.env.E2E_PASSWORD_4,
    name: "David Song",
    username: "race_david",
  },
] as const;

const missing = ACCOUNTS.map((acct, i) => {
  if (acct.email && acct.password) return null;
  const suffix = i === 0 ? "" : `_${i + 1}`;
  return `E2E_EMAIL${suffix} / E2E_PASSWORD${suffix}`;
}).filter((s): s is string => s !== null);

test.skip(
  missing.length > 0,
  `multiplayer race needs four accounts; missing ${missing.join(", ")}`,
);

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

/** Sign in straight through the login form. */
async function signInDirect(page: Page, email: string, password: string) {
  await page.goto("/auth/login");
  await page.locator('input[type="email"]').fill(email);
  await page.locator('input[type="password"]').fill(password);
  await page.getByRole("button", { name: /^sign in/i }).click();
  await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 30_000 });
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

/** All clues with their answers attached, minus the ones already solved. */
async function loadOpenClues(page: Page, active: {
  gameId?: string;
  game?: { questions?: Clue[] };
  actions?: ClueAction[];
}): Promise<AnswerClue[]> {
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
 * observable is the actor's own `.cw-correct` count.
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

/**
 * Start a fresh game on the given parent gameId for this account, abandoning
 * any existing attempt on that parent first.
 */
async function startFreshGameOnParent(
  page: Page,
  parentGameId: string,
): Promise<string | null> {
  const listed = await trpcPost(page.request, "gameList.get", {});
  const rows: Array<{
    type?: string;
    id?: string;
    gameId?: string;
    game?: { title?: string };
    clues?: number;
  }> = listed.data ?? [];

  // Find this account's existing attempt on the parent, if any.
  const existing = rows.find(
    (r) => r.gameId === parentGameId && r.type === "ActiveGame" && r.id,
  );
  if (existing?.id) {
    const dropped = await trpcPost(page.request, "activeGame.abandon", {
      id: existing.id,
    });
    console.log(
      `race: abandoned existing ActiveGame ${existing.id} for parent ${parentGameId}` +
        (dropped.ok ? "" : ` — FAILED (${dropped.error})`),
    );
  }

  const started = await trpcPost(page.request, "activeGame.start", { gameId: parentGameId });
  if (!started.ok || !started.data?.id) {
    console.log(`race: start(${parentGameId}) failed: ${started.error}`);
    return null;
  }

  const freshId: string = started.data.id;
  console.log(`race: started fresh game on parent ${parentGameId} → ${freshId}`);
  return freshId;
}

/**
 * Pick the best parent gameId from player 1's library (most open clues).
 * Returns empty string if none found.
 */
async function pickParentGameId(page: Page): Promise<string> {
  const listed = await trpcPost(page.request, "gameList.get", {});
  const rows: Array<{
    type?: string;
    id?: string;
    gameId?: string;
    game?: { title?: string };
    clues?: number;
  }> = listed.data ?? [];

  const parents = new Map<string, { activeId: string; title: string; clues: number }>();
  for (const r of rows) {
    if (!r.gameId) continue;
    const held = parents.get(r.gameId);
    if (!held) {
      parents.set(r.gameId, {
        activeId: r.type === "ActiveGame" && r.id ? r.id : "",
        title: r.game?.title ?? "?",
        clues: r.clues ?? 0,
      });
    } else if (!held.activeId && r.type === "ActiveGame" && r.id) {
      held.activeId = r.id;
    }
  }

  if (parents.size === 0) return "";

  const ranked = [...parents.entries()].sort((a, b) => b[1].clues - a[1].clues);
  const [gameId] = ranked[0];
  return gameId;
}

type RaceState = {
  contexts: BrowserContext[];
  baseURL: string;
  parentGameId: string;
  activeGameIds: string[];
  completedGameIds: string[];
  leak: string[];
};

const race: RaceState = {
  contexts: [],
  baseURL: "",
  parentGameId: "",
  activeGameIds: [],
  completedGameIds: [],
  leak: [],
};

test.describe("four-player competitive race", () => {
  test.beforeAll(async ({ browser, request }, testInfo) => {
    for (const acct of ACCOUNTS) await provision(request, acct);

    race.baseURL = process.env.E2E_BASE_URL ?? "https://crosswords-staging.casazza.io";
    try {
      for (const acct of ACCOUNTS) {
        const ctx = await browser.newContext({ baseURL: race.baseURL });
        race.contexts.push(ctx);
      }
    } catch (err) {
      await Promise.all(race.contexts.map((c) => c.close()));
      throw err;
    }
  });

  test.afterAll(async () => {
    // Teardown: abandon each player's game, including on throw.
    for (const [i, activeId] of race.activeGameIds.entries()) {
      if (!activeId || !race.contexts[i]) continue;
      const dropped = await trpcPost(race.contexts[i]!.request, "activeGame.abandon", {
        id: activeId,
      });
      const msg = dropped.ok
        ? `race teardown: abandoned ${activeId} (player ${i + 1})`
        : `race teardown: FAILED to abandon ${activeId} (${dropped.error}) — ` +
          `it stays locked for player ${i + 1} until finished by hand`;
      console.log(msg);
      if (!dropped.ok) race.leak.push(msg);
    }

    try {
      await Promise.all(race.contexts.map((c) => c.close()));
    } catch (err) {
      console.error("race cleanup: closing contexts failed", err);
    }
    race.contexts = [];
  });

  test("four players race on the same puzzle", async ({}, testInfo) => {
    const pages: Page[] = [];
    const raceStart = Date.now();
    const playerStartTimes: number[] = [];
    const playerCompleteTimes: number[] = [];
    const playerCompleteResponses: Array<{ ok: boolean; data?: any; error?: string }> = [];

    try {
      // ── Setup: sign in all four ────────────────────────────────
      console.log(`race: signing in all four players at T+${Date.now() - raceStart}ms`);
      for (const [i, ctx] of race.contexts.entries()) {
        pages.push(await ctx.newPage());
        await signInDirect(pages[i]!, ACCOUNTS[i].email!, ACCOUNTS[i].password!);
      }

      // ── Pick parent and start four separate games ────────────────
      console.log(`race: picking parent puzzle at T+${Date.now() - raceStart}ms`);
      race.parentGameId = await pickParentGameId(pages[0]!);
      expect(race.parentGameId, "pick a parent puzzle").not.toBe("");

      console.log(`race: starting fresh games on parent ${race.parentGameId} at T+${Date.now() - raceStart}ms`);
      for (const [i, page] of pages.entries()) {
        playerStartTimes.push(Date.now());
        const activeId = await startFreshGameOnParent(page, race.parentGameId!);
        expect(activeId, `player ${i + 1} starts a game`).not.toBeNull();
        race.activeGameIds.push(activeId!);
        console.log(`race: player ${i + 1} started at T+${Date.now() - raceStart}ms`);
      }

      // ── Each player fills their board with addActions batches ────
      console.log(`race: loading puzzles and filling boards at T+${Date.now() - raceStart}ms`);
      for (const [i, page] of pages.entries()) {
        const activeId = race.activeGameIds[i]!;
        const data = await trpcGet(page, "activeGame.get", { id: activeId });
        expect(data?.gameId).toBeTruthy();

        const answers = await answersFor(page, data.gameId);
        const all: AnswerClue[] = (data?.game?.questions ?? [])
          .map((c: Clue) => ({ ...c, answer: answers.get(clueKey(c)) }))
          .filter((c: Clue & { answer?: string }): c is AnswerClue => Boolean(c.answer));

        // Batch all cells for this player.
        const wanted: Array<{ cordX: number; cordY: number; state: string }> = [];
        const cellKeys = new Set<string>();
        for (const clue of all) {
          for (const [i, [x, y]] of clueCells(clue).entries()) {
            const key = `${x},${y}`;
            if (!cellKeys.has(key)) {
              cellKeys.add(key);
              wanted.push({ cordX: x, cordY: y, state: clue.answer[i]! });
            }
          }
        }

        console.log(`race: player ${i + 1} filling ${wanted.length} cells at T+${Date.now() - raceStart}ms`);
        const filled = await trpcPost(page.request, "activeGame.addActions", {
          id: activeId,
          actions: wanted,
        });
        expect(filled.ok, `player ${i + 1} fill batch accepted`).toBe(true);
        expect(filled.data?.solved, `player ${i + 1} board solved`).toBe(true);
      }

      // ── All four call complete() concurrently ────────────────────
      console.log(`race: calling complete() for all four at T+${Date.now() - raceStart}ms`);
      playerCompleteResponses.push(
        ...(await Promise.all(
          pages.map((p, i) => {
            playerCompleteTimes[i] = Date.now();
            return trpcPost(p!.request, "activeGame.complete", { id: race.activeGameIds[i]! });
          }),
        )),
      );

      const completedIds = playerCompleteResponses
        .map((r) => (r.data as { id?: string } | undefined)?.id)
        .filter((v): v is string => typeof v === "string");

      const completed = new Set(completedIds);
      console.log(
        `race: completion: ${completedIds.length} succeeded, ` +
          `${completed.size} distinct completedGameId(s)`,
      );
      // NOT "exactly one". Each racer plays their OWN ActiveGame, so four
      // separate boards are completed and four CompletedGames are minted —
      // one per game. The soak's exactly-once rule is per GAME, so here it
      // means: every racer who finished got exactly one id, and no racer
      // minted a second for the same board. All four racing to one shared id
      // would mean they had collapsed onto a single game.
      expect(completed.size, "one CompletedGame per racer, no duplicate").toBe(completedIds.length);
      expect(new Set(completedIds).size, "each racer completed their own board").toBe(4);

      race.completedGameIds = [...completed];

      // ── Read standings from the server ─────────────────────────────
      // Read EVERY completed game, not just one. Each racer finished their own
      // board, so each CompletedGame carries exactly one MemberScore — its
      // owner. Reading a single game therefore yielded a single row and the
      // standings collapsed to 1 of 4.
      console.log(`race: reading standings at T+${Date.now() - raceStart}ms`);
      const standings: Array<{
        idx: number;
        name: string;
        score: unknown;
        correctGuesses: unknown;
        incorrectGuesses: unknown;
        wallClockMs: number;
        solveTimeMs: number;
      }> = [];
      for (const gameIdx of completedIds.keys()) {
        const detail = await trpcPost(pages[0]!.request, "stats.getCompletedGame", {
          id: completedIds[gameIdx],
        });
        expect(detail.ok, `getCompletedGame succeeds for game ${gameIdx + 1}`).toBe(true);
        const payload = detail.data as
          | { gameStats?: { memberScores?: Array<Record<string, unknown>> } }
          | undefined;
        const rows = payload?.gameStats?.memberScores ?? [];
        expect(
          rows.length,
          `game ${gameIdx + 1} carries exactly its owner's MemberScore`,
        ).toBe(1);
        const r = rows[0]!;
        const member = r.member as { user?: { name?: string; email?: string } } | undefined;
        standings.push({
          idx: gameIdx,
          name: member?.user?.name ?? "unknown",
          score: r.score,
          correctGuesses: r.correctGuesses,
          incorrectGuesses: r.incorrectGuesses,
          wallClockMs: playerCompleteTimes[gameIdx] - raceStart,
          solveTimeMs: playerCompleteTimes[gameIdx] - playerStartTimes[gameIdx],
        });
      }
      standings.sort((a, b) => Number(b.score ?? 0) - Number(a.score ?? 0));

      // ── Report findings ────────────────────────────────────────────
      const report = standings
        .map(
          (r, placement) =>
            `${placement + 1}. ${r.name}: score=${r.score ?? "?"} ` +
            `correct=${r.correctGuesses ?? "?"} incorrect=${r.incorrectGuesses ?? "?"} ` +
            `(wall-clock ${r.wallClockMs}ms, solve-time ${r.solveTimeMs}ms)`,
        )
        .join("\n");
      const summary =
        `RACE STANDINGS:\n${report}\n` +
        `\nNOTE: All scores equal (max) → all four tie. ` +
        `Placement is consistent with server-reported scores (all tied). ` +
        `Wall-clock timing is observed for reference but is non-deterministic ` +
        `(browser scheduling, network, server latency); it is not a pass/fail ` +
        `criterion. The deterministic invariant is: four independent ActiveGames → ` +
        `one CompletedGame → four MemberScore rows with identical max scores.`;
      console.log(summary);
      await testInfo.attach("race-standings.txt", {
        body: summary,
        contentType: "text/plain",
      });

      // ── Assertions ─────────────────────────────────────────────────
      // ONE standing per CompletedGame, not four. Each racer played their own
      // game, so each completed game carries a single MemberScore — its owner.
      // Four players therefore means four games, four ids and four standings
      // read ACROSS the games, not four rows inside one.
      expect(standings.length, "one standing per completed game").toBe(completedIds.length);
      expect(
        new Set(standings.map((s) => s.name)).size,
        "each racer has their own standing",
      ).toBe(4);
      expect(
        standings.every((s) => s.score === standings[0]!.score),
        "all scores equal (deterministic tie)",
      ).toBe(true);
      // Was "exactly one CompletedGame shared by all four" — soak semantics.
      // In a race each player owns a separate board, so the correct invariant
      // is the opposite: every racer completed their OWN game, and no two
      // share one (which would mean they had collapsed onto a single board
      // rather than racing).
      expect(completed.size, "each racer completed their OWN board").toBe(
        completedIds.length,
      );
      expect(new Set(completedIds).size, "four distinct completed games").toBe(4);
    } finally {
      await Promise.all(pages.map((p) => p.close().catch(() => {})));
    }
  });
});
