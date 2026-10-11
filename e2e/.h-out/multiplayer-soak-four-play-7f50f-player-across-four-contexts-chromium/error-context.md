# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: multiplayer-soak.spec.ts >> four-player multiplayer soak >> live multiplayer across four contexts
- Location: tests/multiplayer-soak.spec.ts:454:7

# Error details

```
Error: no library game has 8 open clues to soak
```

# Test source

```ts
  117 |   const url = `/api/trpc/${proc}?batch=1&input=${encodeURIComponent(
  118 |     JSON.stringify({ "0": input ?? null }),
  119 |   )}`;
  120 |   const res = await page.request.get(url);
  121 |   const body = await res.json();
  122 |   if (body?.[0]?.error) throw new Error(JSON.stringify(body[0].error));
  123 |   return body[0]?.result?.data;
  124 | }
  125 | 
  126 | /** Batched tRPC mutation. Returns the raw envelope so error shapes stay visible. */
  127 | async function trpcPost(
  128 |   req: APIRequestContext,
  129 |   proc: string,
  130 |   input: unknown,
  131 | ): Promise<{ ok: boolean; data?: any; error?: string }> {
  132 |   const res = await req.post(`/api/trpc/${proc}`, { data: { "0": input } });
  133 |   const body = await res.json();
  134 |   return { ok: !body?.[0]?.error, data: body?.[0]?.result?.data, error: body?.[0]?.error?.message };
  135 | }
  136 | 
  137 | /** Create the account if it does not exist; "already exists" is success. */
  138 | async function provision(req: APIRequestContext, acct: (typeof ACCOUNTS)[number]) {
  139 |   const res = await trpcPost(req, "user.signup", {
  140 |     email: acct.email!.toLowerCase(),
  141 |     name: acct.name,
  142 |     username: acct.username,
  143 |     password: acct.password,
  144 |   });
  145 |   if (!res.ok && !/already exists/i.test(res.error ?? "")) {
  146 |     throw new Error(`user.signup for ${acct.email} failed: ${res.error}`);
  147 |   }
  148 |   return res.ok;
  149 | }
  150 | 
  151 | /** Sign in straight through the login form, like signInDirect in demo.spec.ts. */
  152 | async function signInDirect(page: Page, email: string, password: string) {
  153 |   await page.goto("/auth/login");
  154 |   await page.locator('input[type="email"]').fill(email);
  155 |   await page.locator('input[type="password"]').fill(password);
  156 |   await page.getByRole("button", { name: /^sign in/i }).click();
  157 |   await expect(page).not.toHaveURL(/\/auth\/login/, { timeout: 30_000 });
  158 | }
  159 | 
  160 | /**
  161 |  * Open the lobby and land on a board with enough open clues left for the whole
  162 |  * soak. Library cards are tried in order — a `— NEW` one first (clean grid),
  163 |  * then each `— IN PROGRESS` one — and the first board with at least
  164 |  * `minOpen` unsolved clues wins. Player 1's library accumulates near-finished
  165 |  * games, so blindly taking the first card can leave nothing to play.
  166 |  */
  167 | async function openBoard(page: Page, minOpen: number): Promise<string> {
  168 |   const rows = () =>
  169 |     page.locator('div[style*="cursor: pointer"]').and(page.locator('[aria-label*="— "]'));
  170 |   const openCount = async (gameId: string) =>
  171 |     (await loadOpenClues(page, await trpcGet(page, "activeGame.get", { id: gameId }))).length;
  172 | 
  173 |   // ACTIVE_GAME_ID lets the admin bot (or CI) hand us a specific game — the
  174 |   // one it seeded with room to play. Without it we discover through the lobby,
  175 |   // where a near-finished game is just as clickable as a fresh one.
  176 |   const preset = process.env.ACTIVE_GAME_ID;
  177 |   if (preset) {
  178 |     await page.goto(`/game/${preset}`);
  179 |     await expect(page.locator(".cw-letter").first()).toBeVisible({ timeout: 60_000 });
  180 |     const open = await openCount(preset);
  181 |     expect(open, `ACTIVE_GAME_ID=${preset} has enough open clues`).toBeGreaterThanOrEqual(minOpen);
  182 |     console.log(`soak board: ACTIVE_GAME_ID=${preset} (${open} open clues)`);
  183 |     return preset;
  184 |   }
  185 | 
  186 |   await page.goto("/games");
  187 |   await expect(page.getByText("Library").first()).toBeVisible();
  188 |   // The library rows arrive on a client-side fetch after hydration, so poll for
  189 |   // them rather than sampling the DOM once.
  190 |   await expect.poll(() => rows().count(), { timeout: 30_000 }).toBeGreaterThan(0);
  191 |   const labels = await rows().evaluateAll((els) => els.map((e) => e.getAttribute("aria-label") ?? ""));
  192 |   const order = [
  193 |     ...labels.filter((l) => l.includes("— NEW")),
  194 |     ...labels.filter((l) => l.includes("— IN PROGRESS")),
  195 |   ];
  196 |   for (const label of order) {
  197 |     const card = rows().and(page.locator(`[aria-label="${label}"]`)).first();
  198 |     await card.click();
  199 |     if (/— NEW$/.test(label)) {
  200 |       const start = page.getByRole("button", { name: /^(start game|continue game)$/i });
  201 |       await expect(start).toBeVisible({ timeout: 20_000 });
  202 |       await start.click();
  203 |       // Fresh starts generate the puzzle server-side (can take a minute+).
  204 |       await expect(page).not.toHaveURL(/\/game\/[^/]+\/new$/, { timeout: 150_000 });
  205 |     }
  206 |     await expect(page).toHaveURL(/\/game\/[^/]+$/, { timeout: 60_000 });
  207 |     await expect(page.locator(".cw-letter").first()).toBeVisible({ timeout: 60_000 });
  208 |     const gameId = page.url().split("/game/")[1].split(/[/?#]/)[0];
  209 |     if ((await openCount(gameId)) >= minOpen) {
  210 |       console.log(`soak board: ${label} (${gameId})`);
  211 |       return gameId;
  212 |     }
  213 |     console.log(`soak board: skipping ${label} — fewer than ${minOpen} open clues`);
  214 |     await page.goto("/games");
  215 |     await expect(page.getByText("Library").first()).toBeVisible();
  216 |   }
> 217 |   throw new Error(`no library game has ${minOpen} open clues to soak`);
      |         ^ Error: no library game has 8 open clues to soak
  218 | }
  219 | 
  220 | /** Answer key straight from the grid endpoint: {number}{direction} → answer. */
  221 | async function answersFor(page: Page, gameId: string) {
  222 |   const res = await page.request.get(`/api/grids/${gameId}`);
  223 |   expect(res.ok()).toBeTruthy();
  224 |   const data = await res.json();
  225 |   const answers = new Map<string, string>();
  226 |   for (const q of data?.questions ?? []) answers.set(`${q.number}${q.direction}`, q.answer);
  227 |   return answers;
  228 | }
  229 | 
  230 | type ActiveGame = {
  231 |   gameId?: string;
  232 |   game?: { questions?: Clue[] };
  233 |   actions?: ClueAction[];
  234 |   gameMembers?: Array<{ userId?: string }>;
  235 | };
  236 | 
  237 | /** All clues with their answers attached, minus the ones already solved. */
  238 | async function loadOpenClues(page: Page, active: ActiveGame): Promise<AnswerClue[]> {
  239 |   expect(active.gameId).toBeTruthy();
  240 |   const answers = await answersFor(page, active.gameId!);
  241 |   const all = (active.game?.questions ?? [])
  242 |     .map((c) => ({ ...c, answer: answers.get(clueKey(c)) }))
  243 |     .filter((c): c is AnswerClue => Boolean(c.answer));
  244 |   const solved = solvedClues(all, active.actions ?? []);
  245 |   return all.filter((c) => !solved.has(clueKey(c)));
  246 | }
  247 | 
  248 | /** Flip to the clue's direction tab, click its row, wait for the letter boxes. */
  249 | async function selectClue(page: Page, clue: AnswerClue) {
  250 |   const tab = page.getByRole("button", {
  251 |     name: clue.direction === "ACROSS" ? /^across$/i : /^down$/i,
  252 |   });
  253 |   if (!((await tab.getAttribute("class")) ?? "").includes("cw-tab-active")) {
  254 |     await tab.click();
  255 |   }
  256 |   const row = page
  257 |     .locator(".cw-clue-row", {
  258 |       has: page.locator(".cw-clue-badge", { hasText: String(clue.number) }),
  259 |       hasText: (clue.questionText ?? "").slice(0, 20),
  260 |     })
  261 |     .first();
  262 |   await row.click();
  263 |   await expect(page.locator(".cw-letter-input")).toHaveCount(clue.answer.length);
  264 | }
  265 | 
  266 | /**
  267 |  * Type the real answer, guess it, and wait for the solve to land. The
  268 |  * observable is the actor's own `.cw-correct` count — the entry row stays open
  269 |  * after a guess (the editor only closes on completion), so its presence is not
  270 |  * a signal.
  271 |  */
  272 | async function solveClue(page: Page, clue: AnswerClue) {
  273 |   await selectClue(page, clue);
  274 |   const inputs = page.locator(".cw-letter-input");
  275 |   await inputs.first().click();
  276 |   await page.keyboard.type(clue.answer.toUpperCase());
  277 |   const typed = await inputs.evaluateAll((els) =>
  278 |     els.map((e) => (e as HTMLInputElement).value).join(""),
  279 |   );
  280 |   expect(typed.toUpperCase()).toBe(clue.answer.toUpperCase());
  281 |   const before = await page.locator(".cw-correct").count();
  282 |   // A board re-render between the last keystroke and the click can swallow it,
  283 |   // so keep clicking Guess until the solve shows up.
  284 |   await expect
  285 |     .poll(
  286 |       async () => {
  287 |         const now = await page.locator(".cw-correct").count();
  288 |         if (now > before) return true;
  289 |         await page
  290 |           .getByRole("button", { name: /^guess$/i })
  291 |           .click({ timeout: 5_000 })
  292 |           .catch(() => {});
  293 |         return false;
  294 |       },
  295 |       { timeout: 30_000 },
  296 |     )
  297 |     .toBe(true);
  298 | }
  299 | 
  300 | /** Count of correctly-marked cells on a board. */
  301 | const correctCount = (page: Page) => page.locator(".cw-correct").count();
  302 | 
  303 | type SoakState = {
  304 |   contexts: BrowserContext[];
  305 |   baseURL: string;
  306 |   videoDirs: string[];
  307 |   /** The game this run owns; started in beforeAll, pinned or fresh. */
  308 |   activeGameId: string;
  309 |   /** Populated by afterAll when a run starts a game it never finishes. */
  310 |   leak: string[];
  311 | };
  312 | const soak: SoakState = { contexts: [], baseURL: "", videoDirs: [], activeGameId: "", leak: [] as string[] };
  313 | 
  314 | // Recording is OPT-IN via E2E_RECORD=1. It costs a per-context video encoder
  315 | // on all four contexts, which would slow the correctness gate for no benefit,
  316 | // so the default path is unchanged. With it set, each context records and gets
  317 | // a visible corner badge naming its player — four identical boards in a
```