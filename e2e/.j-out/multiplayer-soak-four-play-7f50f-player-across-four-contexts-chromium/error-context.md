# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: multiplayer-soak.spec.ts >> four-player multiplayer soak >> live multiplayer across four contexts
- Location: tests/multiplayer-soak.spec.ts:450:7

# Error details

```
Error: player 1 board cells

expect(locator).toBeVisible() failed

Locator: locator('.cw-letter').first()
Expected: visible
Timeout: 30000ms
Error: element(s) not found

Call log:
  - player 1 board cells with timeout 30000ms
  - waiting for locator('.cw-letter').first()

```

```yaml
- text: "STAGING (beta) — Pro is $1 here, but this is a test environment: expect occasional data loss and unexpected changes. You're a beta tester. 🎈"
- link "Report a bug →":
  - /url: https://github.com/olivecasazza/definitely-not-crosswords/issues/new?labels=staging&title=%5Bstaging%5D+&body=%2A%2AEnvironment%3A%2A%2A+staging+%28reported+from+the+app%29%0A%0A%2A%2AWhat+happened%3F%2A%2A%0A%0A%2A%2ASteps+to+reproduce%3A%2A%2A%0A
- button "Dismiss": ✕
- banner:
  - link "definitely-not-crosswords":
    - /url: /
    - img
    - text: definitely-not-crosswords
  - navigation:
    - link "Games":
      - /url: /games
    - link "Stats":
      - /url: /stats
    - button "☀"
    - link "Riley Calloway":
      - /url: /profile
      - img
      - text: Riley Calloway
    - link "Sign out":
      - /url: /api/auth/signout
- main:
  - toolbar "Play Now":
    - button "tiling / floating"
    - button "minimize"
    - button "maximize / restore"
    - text: Play Now
  - text: TODAY'S PUZZLE SOLVED
  - link "Animals 42 clues View →":
    - /url: /game/a0c1e06b-3884-4737-8ac7-73dabb2381ca/new
  - text: START SOMETHING FRESH
  - status: No new puzzles right now.
  - link "All games →":
    - /url: /games
  - button "Resize panel"
  - toolbar "Pulse":
    - button "tiling / floating"
    - button "minimize"
    - button "maximize / restore"
    - text: Pulse
  - text: YOUR PULSE
  - img
  - text: "Global Rank #1 of 33 players Career Score 4972 Accuracy 93% Games Played 5"
  - link "Full stats →":
    - /url: /stats
  - button "Resize panel"
  - toolbar "How to Play":
    - button "tiling / floating"
    - button "minimize"
    - button "maximize / restore"
    - text: How to Play
  - text: HOW TO PLAY Hi, Riley Calloway
  - list:
    - listitem:
      - text: "1"
      - paragraph: Pick a clue
      - paragraph: Click a clue in the Clues panel, or click any cell on the board. The word you're on highlights across the grid.
    - listitem:
      - text: "2"
      - paragraph: Type the answer
      - paragraph: The clue you pick opens up in the Clues list with a box per letter. Typing advances automatically; Esc clears the selection.
    - listitem:
      - text: "3"
      - paragraph: Guess to lock it in
      - paragraph: Hit Guess. Correct words stay on the board and score; wrong ones clear so someone else can try.
    - listitem:
      - text: "4"
      - paragraph: Solve together
      - paragraph: Share the invite link. Everyone sees the grid live, and a coloured border shows which clue each player is on.
  - button "Resize panel"
  - text: "dock: — nothing minimized —"
- contentinfo:
  - text: © definitely-not-crosswords·v0.1.87
  - navigation:
    - link "GitHub":
      - /url: https://github.com/olivecasazza/definitely-not-crosswords
```

# Test source

```ts
  430 |       } else {
  431 |         const dropped = await trpcPost(request, "activeGame.abandon", { id: soak.activeGameId });
  432 |         const msg = dropped.ok
  433 |           ? `soak teardown: abandoned the unfinished game ${soak.activeGameId}`
  434 |           : `soak teardown: FAILED to abandon ${soak.activeGameId} (${dropped.error}) — ` +
  435 |             `it stays locked for this account until finished by hand`;
  436 |         console.log(msg);
  437 |         soak.leak.push(msg);
  438 |       }
  439 |     }
  440 | 
  441 |     // Close in a finally so a mid-test throw still releases all four contexts.
  442 |     try {
  443 |       await Promise.all(soak.contexts.map((c) => c.close()));
  444 |     } catch (err) {
  445 |       console.error("soak cleanup: closing contexts failed", err);
  446 |     }
  447 |     soak.contexts = [];
  448 |   });
  449 | 
  450 |   test("live multiplayer across four contexts", async ({}, testInfo) => {
  451 |     const pages: Page[] = [];
  452 |     let activeGameId = "";
  453 |     /** Clues already played or claimed, so no two players race the same one. */
  454 |     const taken = new Set<string>();
  455 | 
  456 |     try {
  457 |       // ── Setup: four signed-in contexts in one shared game ────────────────
  458 |       await test.step("sign in all four players", async () => {
  459 |         for (const ctx of soak.contexts) pages.push(await ctx.newPage());
  460 |         await Promise.all(
  461 |           pages.map((p, i) => signInDirect(p, ACCOUNTS[i].email!, ACCOUNTS[i].password!)),
  462 |         );
  463 |       });
  464 | 
  465 |       await test.step("open the shared game and join the other three", async () => {
  466 |         const [p1, ...rest] = pages;
  467 |         // Fresh board when we can get one. This runs HERE, after sign-in, not
  468 |         // in beforeAll: `context.request` rides the context's cookie jar, and
  469 |         // before the players authenticate there is no session, so an earlier
  470 |         // attempt here silently got an empty list and fell back to the lobby.
  471 |         //
  472 |         // Every soak run permanently commits its letters, so resuming "the
  473 |         // in-progress game" hands each run a board PREVIOUS runs ground down —
  474 |         // which is why a recording once began with the game essentially
  475 |         // finished. Four players need a full board to take parallel turns on.
  476 |         //
  477 |         // ACTIVE_GAME_ID still wins: CI pins it deliberately, and a pinned
  478 |         // board is a known quantity.
  479 |         if (process.env.ACTIVE_GAME_ID) {
  480 |           activeGameId = process.env.ACTIVE_GAME_ID;
  481 |           soak.activeGameId = activeGameId;
  482 |           console.log(`soak board: ACTIVE_GAME_ID=${activeGameId} (pinned)`);
  483 |         } else {
  484 |           activeGameId = await startFreshGame(p1);
  485 |           soak.activeGameId = activeGameId;
  486 |         }
  487 |         // Headroom: four plays plus an unsolved ACROSS/DOWN crossing pair.
  488 |         if (!activeGameId) activeGameId = await openBoard(p1, 8);
  489 |         const gameUrl = `${soak.baseURL}/game/${activeGameId}`;
  490 |         for (const [n, p] of rest.entries()) {
  491 |           await p.goto(gameUrl);
  492 |           // "Join game" is optional — members of this active game don't see it.
  493 |           const join = p.getByRole("button", { name: /^join game$/i });
  494 |           const joined = await join
  495 |             .waitFor({ state: "visible", timeout: 30_000 })
  496 |             .then(() => true)
  497 |             .catch(() => false);
  498 |           if (joined) {
  499 |             await join.click();
  500 |             console.log(`clicked "Join game" for player ${n + 2}`);
  501 |           } else {
  502 |             console.log(`no "Join game" button for player ${n + 2} — already a member`);
  503 |           }
  504 |         }
  505 |         // The members query has to land before addActions will accept anyone.
  506 |         await expect
  507 |           .poll(
  508 |             async () => {
  509 |               const m = await trpcGet(p1, "activeGame.get", { id: activeGameId });
  510 |               return (m?.gameMembers ?? []).length;
  511 |             },
  512 |             { timeout: 30_000 },
  513 |           )
  514 |           .toBeGreaterThanOrEqual(4);
  515 |       });
  516 | 
  517 |       let open: AnswerClue[] = [];
  518 |       const refreshOpen = async () => {
  519 |         const data = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
  520 |         open = await loadOpenClues(pages[0], data);
  521 |       };
  522 |       await refreshOpen();
  523 | 
  524 |       // ── 1. All four boards render ───────────────────────────────────────
  525 |       await test.step("all four boards render", async () => {
  526 |         for (const [i, p] of pages.entries()) {
  527 |           await expect(
  528 |             p.locator(".cw-letter").first(),
  529 |             `${ACCOUNTS[i].label} board cells`,
> 530 |           ).toBeVisible({ timeout: 30_000 });
      |             ^ Error: player 1 board cells
  531 |           await expect(
  532 |             p.locator(".cw-clue-row").first(),
  533 |             `${ACCOUNTS[i].label} clue list`,
  534 |           ).toBeVisible({ timeout: 30_000 });
  535 |         }
  536 |       });
  537 | 
  538 |       // Pick the four clues up front: player 1's, plus one distinct clue each
  539 |       // for players 2–4, preferring clues that share no cell with each other so
  540 |       // no two players ever race the same cell.
  541 |       const pickDistinct = (candidates: AnswerClue[], n: number) => {
  542 |         const picks: AnswerClue[] = [];
  543 |         for (const c of candidates) {
  544 |           if (picks.length >= n) break;
  545 |           const pickedCells = new Set(picks.flatMap(clueCells).map(([x, y]) => `${x},${y}`));
  546 |           if (clueCells(c).every(([x, y]) => !pickedCells.has(`${x},${y}`))) picks.push(c);
  547 |         }
  548 |         for (const c of candidates) {
  549 |           if (picks.length >= n) break;
  550 |           if (!picks.includes(c)) picks.push(c);
  551 |         }
  552 |         return picks;
  553 |       };
  554 |       expect(open.length, "at least four open clues to play").toBeGreaterThanOrEqual(4);
  555 |       const firstClue = open[0];
  556 |       taken.add(clueKey(firstClue));
  557 |       const picks = pickDistinct(
  558 |         open.filter((c) => !taken.has(clueKey(c))),
  559 |         3,
  560 |       );
  561 |       expect(picks.length, "enough distinct open clues for players 2–4").toBe(3);
  562 |       // A SECOND clue per player, used only to alternate selections during the
  563 |       // presence step. Selecting the same clue twice is a client-side no-op and
  564 |       // emits no new publishPresence, so a retry needs a real change to retry.
  565 |       // These are never solved — the presence step only selects them.
  566 |       const alternates = pickDistinct(
  567 |         open.filter((c) => !taken.has(clueKey(c)) && !picks.includes(c)),
  568 |         3,
  569 |       );
  570 |       const altClue =
  571 |         open.find((c) => c !== firstClue && !picks.includes(c)) ?? firstClue;
  572 |       console.log(
  573 |         `player 1 takes ${clueKey(firstClue)}; players 2-4 take ${picks
  574 |           .map((c) => clueKey(c))
  575 |           .join(", ")}`,
  576 |       );
  577 | 
  578 |       /**
  579 |        * How many of the four contexts receive a server broadcast.
  580 |        *
  581 |        * Measured first because it is the load-bearing property of the whole
  582 |        * suite: a broadcast that reaches only some subscribers means co-op play
  583 |        * silently desynchronises, and no single-browser test can see it.
  584 |        *
  585 |        * This used to return 1-3 of 4 — events were an in-process
  586 |        * tokio::broadcast and only reached sockets on the publishing pod
  587 |        * (DEF-274). They now relay through a Postgres outbox, so the expected
  588 |        * value is 4 of 4. The measurement stays rather than becoming a bare
  589 |        * assertion: it reports the number in the log and attachment, so a
  590 |        * regression says WHICH peers went dark instead of just "failed".
  591 |        */
  592 |       let reachable = new Set<number>();
  593 |       await test.step("measure broadcast fan-out", async () => {
  594 |         await refreshOpen();
  595 |         expect(open.length, "an open clue to measure fan-out with").toBeGreaterThan(0);
  596 |         const probe = open[0];
  597 |         const [x, y] = clueCells(probe)[0];
  598 |         // Observe the ONE cell the probe writes: its rendered aria-label is the
  599 |         // precise signal, where a whole-board count could stay flat.
  600 |         const cellLabel = (i: number) =>
  601 |           pages[i]
  602 |             .locator(`.cw-cell[data-x="${x}"][data-y="${y}"]`)
  603 |             .first()
  604 |             .getAttribute("aria-label")
  605 |             .catch(() => null);
  606 |         // Snapshot BEFORE publishing — the broadcast can land in the same tick
  607 |         // the write returns, and a baseline read afterwards never differs.
  608 |         const beforeLabels = await Promise.all(pages.map((_, i) => cellLabel(i)));
  609 | 
  610 |         // The letter must DIFFER from what the cell holds, or the rendered
  611 |         // verdict is unchanged and the probe sees nothing.
  612 |         const truth = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
  613 |         const stored = latestStates(truth?.actions ?? []).get(`${x},${y}`)?.state ?? "";
  614 |         const wrongLetter =
  615 |           "ABDEFGHIJKLMNOPRSTUVW".split("").find(
  616 |             (c) => c !== stored.toUpperCase() && c !== probe.answer[0].toUpperCase(),
  617 |           ) ?? "Q";
  618 |         const published = await trpcPost(pages[1].request, "activeGame.addActions", {
  619 |           id: activeGameId,
  620 |           actions: [{ cordX: x, cordY: y, state: wrongLetter }],
  621 |         });
  622 |         expect(published.ok, `fan-out probe write: ${published.error}`).toBe(true);
  623 |         taken.add(clueKey(probe));
  624 | 
  625 |         await expect
  626 |           .poll(
  627 |             async () =>
  628 |               (await Promise.all(pages.map((_, i) => cellLabel(i)))).join("|") !==
  629 |               beforeLabels.join("|"),
  630 |             { timeout: 30_000 },
```