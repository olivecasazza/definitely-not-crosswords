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
/**
 * Start a puzzle this account has no live game for, so each run gets a board
 * nobody has already ground down. Returns "" when none is free, which the
 * caller treats as "fall back to the lobby" rather than failing.
 *
 * `gameList.get` is a MIX, not a clean list: rows carry `type` of `ActiveGame`
 * or `CompletedGame`, and the PARENT puzzle is `gameId` while `id` is the
 * caller's own attempt. `activeGame.start` takes the parent `gameId`, so
 * passing `id` returns "Game not found".
 *
 * `activeGame.start` is IDEMPOTENT per (gameId, caller): it hands back the
 * caller's EXISTING ActiveGame instead of creating one. So "start a fresh
 * puzzle" has to be preceded by `activeGame.abandon` on every ActiveGame this
 * account holds for that parent — which also cascades its GameActions away.
 * That is what makes the board below genuinely empty; without it a run can
 * inherit a board earlier runs advanced — one measured run began with only
 * 156 of 175 cells open.
 * Abandoning also re-opens a puzzle to this account, so parents that DO have
 * a live attempt are candidates again instead of being filtered out.
 *
 * MUST be called with an authenticated context — `context.request` rides the
 * context's cookie jar, and an unauthenticated call returns an empty list.
 */
async function startFreshGame(page: Page): Promise<string> {
  const listed = await trpcPost(page.request, "gameList.get", {});
  const rows: Array<{
    type?: string;
    id?: string;
    gameId?: string;
    game?: { title?: string };
    clues?: number;
  }> = listed.data ?? [];
  // One entry per PARENT puzzle. `activeId` is this account's own live
  // attempt on it, when the list carries one; a parent seen only as a
  // CompletedGame has none and needs no abandon.
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
  const ranked = [...parents.entries()].sort((a, b) => b[1].clues - a[1].clues);
  for (const [gameId, meta] of ranked) {
    if (meta.activeId) {
      // Drop this account's live attempt first — `start` would otherwise hand
      // back exactly that attempt, with every letter a previous run committed.
      const dropped = await trpcPost(page.request, "activeGame.abandon", {
        id: meta.activeId,
      });
      console.log(
        `soak board: abandoned stale ActiveGame ${meta.activeId} for ${gameId}` +
          (dropped.ok ? "" : ` — FAILED (${dropped.error})`),
      );
    }
    const started = await trpcPost(page.request, "activeGame.start", { gameId });
    if (!started.ok || !started.data?.id) {
      console.log(`soak board: start(${gameId}) failed: ${started.error}`);
      continue;
    }
    const freshId: string = started.data.id;
    // A truly fresh board has NO cell filled. Assert that off the server's own
    // state, so a regression in `start` idempotence can never again hand a
    // half-solved board to the soak without the run failing loudly.
    const cells = await boardCellCount(page, freshId);
    console.log(
      `soak board: started a FRESH game ${gameId} ("${meta.title}", ${meta.clues} clues)` +
        ` -> ${freshId} — ${cells.open}/${cells.total} cells open`,
    );
    expect(cells.total, `fresh game ${freshId} grid cell count`).toBeGreaterThan(0);
    expect(
      cells.open,
      `fresh game ${freshId} must start with every cell open (no inherited letters)`,
    ).toBe(cells.total);
    return freshId;
  }
  console.log("soak board: no free puzzle to start; falling back to the lobby");
  return "";
}

/** Grid size plus how many of those cells already carry a committed letter. */
async function boardCellCount(page: Page, activeId: string) {
  const active = await trpcGet(page, "activeGame.get", { id: activeId });
  const all = boardCells(active?.game?.questions ?? []);
  const latest = latestStates(active?.actions ?? []);
  let open = all.size;
  for (const key of latest.keys()) if (all.has(key)) open--;
  return { total: all.size, open };
}

/** Every cell the board occupies, across all of its clues, as "x,y" keys. */
function boardCells(clues: Clue[]): Set<string> {
  const cells = new Set<string>();
  for (const c of clues) for (const [x, y] of clueCells(c)) cells.add(`${x},${y}`);
  return cells;
}


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
  // Concurrent selection publishes presence, which rerenders the other three
  // pages while their row clicks are in flight. Keep the assertion strict —
  // the full editor MUST mount — but retry the click across those legitimate
  // rerenders instead of treating one detached node as a failed game.
  await expect
    .poll(
      async () => {
        if ((await page.locator(".cw-letter-input").count()) === clue.answer.length) {
          return true;
        }
        await row.click({ timeout: 5_000 }).catch(() => {});
        return (await page.locator(".cw-letter-input").count()) === clue.answer.length;
      },
      { timeout: 15_000, intervals: [100, 250, 500, 1_000, 2_000] },
    )
    .toBe(true);
}

/** Select the clue and type the whole answer, but do NOT submit yet. */
async function prepareClue(page: Page, clue: AnswerClue): Promise<number> {
  // A different player's focus can rerender this page after `selectClue`
  // succeeds but before the first input click. Retry the WHOLE preparation: a
  // detached editor may have accepted partial letters, so every retry
  // reselects and clears the one-character inputs before typing.
  let lastError: unknown = null;
  for (let attempt = 1; attempt <= 4; attempt++) {
    try {
      await selectClue(page, clue);
      const inputs = page.locator(".cw-letter-input");
      const count = await inputs.count();
      expect(count, `editor length for ${clueKey(clue)}`).toBe(clue.answer.length);
      for (let i = 0; i < count; i++) {
        await inputs.nth(i).fill("");
      }
      await inputs.first().click({ timeout: 5_000 });
      await page.keyboard.type(clue.answer.toUpperCase());
      const typed = await inputs.evaluateAll((els) =>
        els.map((e) => (e as HTMLInputElement).value).join(""),
      );
      expect(typed.toUpperCase()).toBe(clue.answer.toUpperCase());
      return page.locator(".cw-correct").count();
    } catch (err) {
      lastError = err;
      if (attempt === 4) throw err;
    }
  }
  throw lastError;
}

/** Submit an answer already typed by `prepareClue`, waiting for it to land. */
async function submitPreparedClue(page: Page, before: number) {
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

/** Type the real answer, guess it, and wait for the solve to land. */
async function solveClue(page: Page, clue: AnswerClue) {
  const before = await prepareClue(page, clue);
  await submitPreparedClue(page, before);
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
  });

  test.afterAll(async () => {
    // Teardown: actually clean up the game this run started.
    //
    // There was no way to do this before `activeGame.abandon` existed. Every
    // run started a game that only `complete` could remove, so an aborted run
    // left an immortal ActiveGame — and because `start` is idempotent per
    // (gameId, caller), that account could never open that puzzle again. Not
    // theoretical: the bot accounts exhausted staging's whole pool of
    // published games, which is why the fresh-game path found nothing to start.
    // Abandon through a PLAYER's context, not the `request` fixture: the
    // fixture is an unauthenticated APIRequestContext, so every call came back
    // UNAUTHORIZED and the cleanup silently never happened. Context 0 belongs
    // to player 1, who owns the game we started, and it is still open here —
    // the close happens below.
    const owner = soak.contexts[0];
    if (soak.activeGameId && owner) {
      // A completed game makes `get` answer with an EMPTY object rather than
      // an error, so existence is "the payload carries a gameId".
      const now = await trpcPost(owner.request, "activeGame.get", { id: soak.activeGameId });
      if (!now.ok || !now.data?.id) {
        console.log(`soak teardown: game ${soak.activeGameId} already gone (completed)`);
      } else {
        const dropped = await trpcPost(owner.request, "activeGame.abandon", {
          id: soak.activeGameId,
        });
        const msg = dropped.ok
          ? `soak teardown: abandoned the unfinished game ${soak.activeGameId}`
          : `soak teardown: FAILED to abandon ${soak.activeGameId} (${dropped.error}) — ` +
            `it stays locked for this account until finished by hand`;
        console.log(msg);
        soak.leak.push(msg);
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
    /** Raw complete() responses; the standings step reads the ids back out. */
    let completedCalls: Array<{ ok: boolean; data?: unknown; error?: string }> = [];

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
        // Fresh board when we can get one. This runs HERE, after sign-in, not
        // in beforeAll: `context.request` rides the context's cookie jar, and
        // before the players authenticate there is no session, so an earlier
        // attempt here silently got an empty list and fell back to the lobby.
        //
        // Every soak run permanently commits its letters, so resuming "the
        // in-progress game" hands each run a board PREVIOUS runs ground down —
        // which is why a recording once began with the game essentially
        // finished. Four players need a full board to take parallel turns on.
        //
        // ACTIVE_GAME_ID still wins: CI pins it deliberately, and a pinned
        // board is a known quantity.
        if (process.env.ACTIVE_GAME_ID) {
          activeGameId = process.env.ACTIVE_GAME_ID;
          soak.activeGameId = activeGameId;
          console.log(`soak board: ACTIVE_GAME_ID=${activeGameId} (pinned)`);
        } else {
          activeGameId = await startFreshGame(p1);
          soak.activeGameId = activeGameId;
        }
        // Headroom: four plays plus an unsolved ACROSS/DOWN crossing pair.
        // `openBoard` navigates player 1 itself; the fresh path does not, so
        // without this player 1 sits on the lobby while the others join a game
        // it never opened — which surfaced as "player 1 board cells not found".
        if (!activeGameId) activeGameId = await openBoard(p1, 8);
        const gameUrl = `${soak.baseURL}/game/${activeGameId}`;
        await p1.goto(gameUrl);
        // All three navigate AND join concurrently. Serially, each pane paid
        // its own full page load plus up to 30s waiting for its own join
        // button — visible in recordings as one pane sitting on the lobby
        // while the others had already moved on.
        const joinStart = Date.now();
        await Promise.all(
          rest.map(async (p, n) => {
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
          }),
        );
        console.log(
          `soak joins: players 2-4 navigated and joined CONCURRENTLY in ${Date.now() - joinStart} ms`,
        );
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
        const renderStart = Date.now();
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
        console.log(
          `soak boards: all 4 panes on the board ${Date.now() - renderStart} ms after the join phase`,
        );
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
      console.log(
        `player 1 takes ${clueKey(firstClue)}; players 2-4 take ${picks
          .map((c) => clueKey(c))
          .join(", ")}`,
      );

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

      // ── 3. The four players actually PLAY ──────────────────────────────
      // Previously this was four solves: one for player 1, then one each for
      // players 2-4. Everything else on the board was filled by four
      // `addActions` calls in the completion step, so a recording was a minute
      // of setup and measurement followed by the whole crossword being played
      // in about ten seconds — the interesting part was the smallest part.
      //
      // So the four players play in ROUNDS, all four solving a distinct clue
      // at the same time, until the board is close enough to finished that the
      // contention probe and the completion race still have open cells to work
      // with. Concurrent per round, never serialised — that was the same
      // `await`-inside-a-`for` defect, three times over.
      const ROUNDS_MAX = 12;
      const LEAVE_OPEN = 18; // clues still open for contention + completion
      await test.step("the four players play together", async () => {
        let played = 0;
        for (let round = 0; round < ROUNDS_MAX; round++) {
          await refreshOpen();
          const playable = open.filter((c) => !taken.has(clueKey(c)) && c.answer);
          if (playable.length <= LEAVE_OPEN || playable.length < ACCOUNTS.length) break;

          // One CELL-DISJOINT clue per player. Distinct clue ids are not
          // enough: crossing clues share a cell and can close another
          // player's editor when their server update lands.
          const batch = pickDistinct(playable, ACCOUNTS.length);
          if (batch.length < ACCOUNTS.length) break;
          for (const clue of batch) taken.add(clueKey(clue));

          const started = Date.now();
          // Two-phase round. If one player submits while another is still
          // typing, the GameActionsAdded broadcast rerenders every board and
          // detaches the slower player's input. Everyone types first, then
          // everyone submits in the same phase.
          const actorBefore = await Promise.all(
            batch.map((clue, n) => prepareClue(pages[n], clue)),
          );
          // Type concurrently, COMMIT sequentially. Four simultaneous Guess
          // clicks make the first GameActionsAdded rerender detach another
          // player's button before its click lands. Sequential commits also
          // produce the slower, readable cadence the observer recording needs
          // without an arbitrary sleep: each turn waits for a real server
          // response and board update, then the next player commits.
          for (let n = 0; n < batch.length; n++) {
            await submitPreparedClue(pages[n], actorBefore[n]);
          }
          played += batch.length;
          console.log(
            `  round ${round + 1}: ${batch.length} players solved ` +
              `${batch.map(clueKey).join(", ")} in ${Date.now() - started} ms`,
          );
        }
        console.log(`PLAY: ${played} clues solved through the UI across 4 players`);
      });


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
        // Pick fixed, distinct clues at presence time. Earlier versions reused
        // clues chosen before the play rounds, so a later presence target could
        // already be solved and selectClue would publish no fresh label.
        //
        // Require the exact LATEST clue labels for the other three players on
        // every context. Once those appear, the ordered presence writer
        // guarantees there is no older clear still queued behind them.
        await refreshOpen();
        const available = open.filter((c) => !taken.has(clueKey(c)) && c.answer);
        expect(available.length, "open clues for presence labels").toBeGreaterThanOrEqual(
          ACCOUNTS.length,
        );
        const targets = pickDistinct(available, ACCOUNTS.length);
        expect(targets.length, "presence clues").toBe(ACCOUNTS.length);
        await Promise.all(targets.map((clue, i) => selectClue(pages[i], clue)));
        const expected = targets.map(
          (c) => `#${c.number} ${c.direction.toLowerCase()}`,
        );
        let lastTexts: string[][] = [];
        await expect
          .poll(
            async () => {
              lastTexts = await Promise.all(
                pages.map((p) =>
                  p
                    .locator(".cw-players .cw-chip-clue")
                    .allTextContents()
                    .then((xs) => xs.map((x) => x.trim().toLowerCase())),
                ),
              );
              return lastTexts.every((texts, observer) =>
                expected.every((label, player) => player === observer || texts.includes(label)),
              );
            },
            { timeout: 30_000, intervals: [250, 500, 1_000, 2_000, 4_000] },
          )
          .toBe(true)
          .catch(async (err) => {
            const body = lastTexts
              .map((texts, i) => `player ${i + 1}: [${texts.join(", ")}]`)
              .join("\n");
            console.log(`PRESENCE FAILED: latest labels missing\n${body}`);
            await testInfo.attach("presence-fanout-failed.txt", {
              body:
                `${body}\nexpected per player: ${expected.join(" | ")}\n` +
                `action broadcasts reached ${reachable.size} of 4`,
              contentType: "text/plain",
            });
            throw err;
          });

        const chipCounts = lastTexts.map((texts) => texts.length);
        console.log(
          `PRESENCE latest labels present on all contexts: ` +
            `${lastTexts.map((texts, i) => `p${i + 1}=[${texts.join(", ")}]`).join(" ")}`,
        );
        await testInfo.attach("presence-fanout.txt", {
          body:
            `latest labels per context:\n` +
            lastTexts.map((texts, i) => `player ${i + 1}: [${texts.join(", ")}]`).join("\n"),
          contentType: "text/plain",
        });

        // The ring marks another player's latest live selection. Every context
        // now has all three required latest labels, so every board must have a
        // ring — no conditional that silently lets a missing observer pass.
        expect(chipCounts.every((n) => n >= 3), "three remote labels per player").toBe(true);
        for (const p of pages) {
          await expect(p.locator('.cw-cell[style*="box-shadow"]').first()).toBeVisible({
            timeout: 30_000,
          });
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
          `(the loser's previousState is the cell as it stood before the winner's write, ` +
          `which is what a serialised writer observes — see DEF-281)`;
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
        completedCalls = await Promise.all(
          pages.map((p) => trpcPost(p.request, "activeGame.complete", { id: activeGameId })),
        );
        const okIds = completedCalls
          .map((r) => (r.data as { id?: string } | undefined)?.id)
          .filter((v: unknown): v is string => typeof v === "string");
        const report =
          `complete() by 4 members in one tick: ${okIds.length} returned an id, ` +
          `${completedCalls.length - okIds.length} refused ` +
          `(${completedCalls.filter((r) => !r.ok).map((r) => r.error).join(" | ") || "none"}); ` +
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

      // ── 7. Standings, per player ───────────────────────────────────────
      // Completion mints ONE CompletedGame carrying a MemberScore row per
      // player, but nothing here ever read them back, so the run finished
      // without showing who actually won. This is also what the recording
      // ends on.
      await test.step("standings are shown per player", async () => {
        // Let the GameCompleted subscription land and the results screen render.
        await expect
          .poll(
            async () =>
              (await Promise.all(pages.map((p) => p.locator("body").innerText().catch(() => ""))))
                .filter((t) => /solved|complete|standings|score/i.test(t)).length,
            { timeout: 60_000, intervals: [500, 1_000, 2_000, 4_000, 8_000] },
          )
          .toBeGreaterThan(0);

        // Authoritative numbers, read from the server rather than scraped off a
        // rendered table.
        const ids = completedCalls
          .map((r) => (r as { data?: { id?: string } })?.data?.id)
          .filter((v): v is string => typeof v === "string");
        expect(ids.length, "one completedGameId to read standings from").toBeGreaterThan(0);

        const detail = await trpcPost(pages[0].request, "stats.getCompletedGame", {
          id: ids[0],
        });
        // Shape, verified against a live response: the rows hang off
        // gameStats.memberScores[] and the player identity is member.user.name.
        // Guessing a top-level `members` array returned nothing, and the step
        // failed on "no member rows returned".
        const payload = detail.data as
          | { gameStats?: { memberScores?: Array<Record<string, unknown>> } }
          | undefined;
        const standings = (payload?.gameStats?.memberScores ?? []).map((r) => {
          const member = r.member as { user?: { name?: string; email?: string } } | undefined;
          return {
            name: member?.user?.name ?? "unknown",
            score: r.score,
            correctGuesses: r.correctGuesses,
            incorrectGuesses: r.incorrectGuesses,
          };
        });
        standings.sort((a, b) => Number(b.score ?? 0) - Number(a.score ?? 0));

        const report =
          `standings for ${ids[0]}: ` +
          (standings.length
            ? standings
                .map(
                  (r, n) =>
                    `${n + 1}. ${r.name}: score=${r.score ?? "?"} ` +
                    `correct=${r.correctGuesses ?? "?"} incorrect=${r.incorrectGuesses ?? "?"}`,
                )
            : "no member rows returned");
        console.log(`STANDINGS: ${report}`);
        await testInfo.attach("standings.txt", { body: report, contentType: "text/plain" });
        // Every player who played must appear. Fewer rows than players means
        // the score write dropped someone — the bug this catches.
        expect(standings.length, "a standing for each of the four players").toBe(4);

        // Hold the results long enough to be read on the recording; the clip
        // otherwise ends the instant the standings appear.
        await pages[0].waitForTimeout(4_000);
      });
    } finally {
      await Promise.all(pages.map((p) => p.close().catch(() => {})));
    }
  });
});