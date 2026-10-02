# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: multiplayer-soak.spec.ts >> four-player multiplayer soak >> live multiplayer across four contexts
- Location: tests/multiplayer-soak.spec.ts:458:7

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
  - text: JUMP BACK IN
  - link "History 4 players · 3m ago Resume →":
    - /url: /game/3c765929-7407-4f4a-9f8c-b509d738a1d8
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
  438 |           id: soak.activeGameId,
  439 |         });
  440 |         const msg = dropped.ok
  441 |           ? `soak teardown: abandoned the unfinished game ${soak.activeGameId}`
  442 |           : `soak teardown: FAILED to abandon ${soak.activeGameId} (${dropped.error}) — ` +
  443 |             `it stays locked for this account until finished by hand`;
  444 |         console.log(msg);
  445 |         soak.leak.push(msg);
  446 |       }
  447 |     }
  448 | 
  449 |     // Close in a finally so a mid-test throw still releases all four contexts.
  450 |     try {
  451 |       await Promise.all(soak.contexts.map((c) => c.close()));
  452 |     } catch (err) {
  453 |       console.error("soak cleanup: closing contexts failed", err);
  454 |     }
  455 |     soak.contexts = [];
  456 |   });
  457 | 
  458 |   test("live multiplayer across four contexts", async ({}, testInfo) => {
  459 |     const pages: Page[] = [];
  460 |     let activeGameId = "";
  461 |     /** Clues already played or claimed, so no two players race the same one. */
  462 |     const taken = new Set<string>();
  463 | 
  464 |     try {
  465 |       // ── Setup: four signed-in contexts in one shared game ────────────────
  466 |       await test.step("sign in all four players", async () => {
  467 |         for (const ctx of soak.contexts) pages.push(await ctx.newPage());
  468 |         await Promise.all(
  469 |           pages.map((p, i) => signInDirect(p, ACCOUNTS[i].email!, ACCOUNTS[i].password!)),
  470 |         );
  471 |       });
  472 | 
  473 |       await test.step("open the shared game and join the other three", async () => {
  474 |         const [p1, ...rest] = pages;
  475 |         // Fresh board when we can get one. This runs HERE, after sign-in, not
  476 |         // in beforeAll: `context.request` rides the context's cookie jar, and
  477 |         // before the players authenticate there is no session, so an earlier
  478 |         // attempt here silently got an empty list and fell back to the lobby.
  479 |         //
  480 |         // Every soak run permanently commits its letters, so resuming "the
  481 |         // in-progress game" hands each run a board PREVIOUS runs ground down —
  482 |         // which is why a recording once began with the game essentially
  483 |         // finished. Four players need a full board to take parallel turns on.
  484 |         //
  485 |         // ACTIVE_GAME_ID still wins: CI pins it deliberately, and a pinned
  486 |         // board is a known quantity.
  487 |         if (process.env.ACTIVE_GAME_ID) {
  488 |           activeGameId = process.env.ACTIVE_GAME_ID;
  489 |           soak.activeGameId = activeGameId;
  490 |           console.log(`soak board: ACTIVE_GAME_ID=${activeGameId} (pinned)`);
  491 |         } else {
  492 |           activeGameId = await startFreshGame(p1);
  493 |           soak.activeGameId = activeGameId;
  494 |         }
  495 |         // Headroom: four plays plus an unsolved ACROSS/DOWN crossing pair.
  496 |         if (!activeGameId) activeGameId = await openBoard(p1, 8);
  497 |         const gameUrl = `${soak.baseURL}/game/${activeGameId}`;
  498 |         for (const [n, p] of rest.entries()) {
  499 |           await p.goto(gameUrl);
  500 |           // "Join game" is optional — members of this active game don't see it.
  501 |           const join = p.getByRole("button", { name: /^join game$/i });
  502 |           const joined = await join
  503 |             .waitFor({ state: "visible", timeout: 30_000 })
  504 |             .then(() => true)
  505 |             .catch(() => false);
  506 |           if (joined) {
  507 |             await join.click();
  508 |             console.log(`clicked "Join game" for player ${n + 2}`);
  509 |           } else {
  510 |             console.log(`no "Join game" button for player ${n + 2} — already a member`);
  511 |           }
  512 |         }
  513 |         // The members query has to land before addActions will accept anyone.
  514 |         await expect
  515 |           .poll(
  516 |             async () => {
  517 |               const m = await trpcGet(p1, "activeGame.get", { id: activeGameId });
  518 |               return (m?.gameMembers ?? []).length;
  519 |             },
  520 |             { timeout: 30_000 },
  521 |           )
  522 |           .toBeGreaterThanOrEqual(4);
  523 |       });
  524 | 
  525 |       let open: AnswerClue[] = [];
  526 |       const refreshOpen = async () => {
  527 |         const data = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
  528 |         open = await loadOpenClues(pages[0], data);
  529 |       };
  530 |       await refreshOpen();
  531 | 
  532 |       // ── 1. All four boards render ───────────────────────────────────────
  533 |       await test.step("all four boards render", async () => {
  534 |         for (const [i, p] of pages.entries()) {
  535 |           await expect(
  536 |             p.locator(".cw-letter").first(),
  537 |             `${ACCOUNTS[i].label} board cells`,
> 538 |           ).toBeVisible({ timeout: 30_000 });
      |             ^ Error: player 1 board cells
  539 |           await expect(
  540 |             p.locator(".cw-clue-row").first(),
  541 |             `${ACCOUNTS[i].label} clue list`,
  542 |           ).toBeVisible({ timeout: 30_000 });
  543 |         }
  544 |       });
  545 | 
  546 |       // Pick the four clues up front: player 1's, plus one distinct clue each
  547 |       // for players 2–4, preferring clues that share no cell with each other so
  548 |       // no two players ever race the same cell.
  549 |       const pickDistinct = (candidates: AnswerClue[], n: number) => {
  550 |         const picks: AnswerClue[] = [];
  551 |         for (const c of candidates) {
  552 |           if (picks.length >= n) break;
  553 |           const pickedCells = new Set(picks.flatMap(clueCells).map(([x, y]) => `${x},${y}`));
  554 |           if (clueCells(c).every(([x, y]) => !pickedCells.has(`${x},${y}`))) picks.push(c);
  555 |         }
  556 |         for (const c of candidates) {
  557 |           if (picks.length >= n) break;
  558 |           if (!picks.includes(c)) picks.push(c);
  559 |         }
  560 |         return picks;
  561 |       };
  562 |       expect(open.length, "at least four open clues to play").toBeGreaterThanOrEqual(4);
  563 |       const firstClue = open[0];
  564 |       taken.add(clueKey(firstClue));
  565 |       const picks = pickDistinct(
  566 |         open.filter((c) => !taken.has(clueKey(c))),
  567 |         3,
  568 |       );
  569 |       expect(picks.length, "enough distinct open clues for players 2–4").toBe(3);
  570 |       // A SECOND clue per player, used only to alternate selections during the
  571 |       // presence step. Selecting the same clue twice is a client-side no-op and
  572 |       // emits no new publishPresence, so a retry needs a real change to retry.
  573 |       // These are never solved — the presence step only selects them.
  574 |       const alternates = pickDistinct(
  575 |         open.filter((c) => !taken.has(clueKey(c)) && !picks.includes(c)),
  576 |         3,
  577 |       );
  578 |       const altClue =
  579 |         open.find((c) => c !== firstClue && !picks.includes(c)) ?? firstClue;
  580 |       console.log(
  581 |         `player 1 takes ${clueKey(firstClue)}; players 2-4 take ${picks
  582 |           .map((c) => clueKey(c))
  583 |           .join(", ")}`,
  584 |       );
  585 | 
  586 |       /**
  587 |        * How many of the four contexts receive a server broadcast.
  588 |        *
  589 |        * Measured first because it is the load-bearing property of the whole
  590 |        * suite: a broadcast that reaches only some subscribers means co-op play
  591 |        * silently desynchronises, and no single-browser test can see it.
  592 |        *
  593 |        * This used to return 1-3 of 4 — events were an in-process
  594 |        * tokio::broadcast and only reached sockets on the publishing pod
  595 |        * (DEF-274). They now relay through a Postgres outbox, so the expected
  596 |        * value is 4 of 4. The measurement stays rather than becoming a bare
  597 |        * assertion: it reports the number in the log and attachment, so a
  598 |        * regression says WHICH peers went dark instead of just "failed".
  599 |        */
  600 |       let reachable = new Set<number>();
  601 |       await test.step("measure broadcast fan-out", async () => {
  602 |         await refreshOpen();
  603 |         expect(open.length, "an open clue to measure fan-out with").toBeGreaterThan(0);
  604 |         const probe = open[0];
  605 |         const [x, y] = clueCells(probe)[0];
  606 |         // Observe the ONE cell the probe writes: its rendered aria-label is the
  607 |         // precise signal, where a whole-board count could stay flat.
  608 |         const cellLabel = (i: number) =>
  609 |           pages[i]
  610 |             .locator(`.cw-cell[data-x="${x}"][data-y="${y}"]`)
  611 |             .first()
  612 |             .getAttribute("aria-label")
  613 |             .catch(() => null);
  614 |         // Snapshot BEFORE publishing — the broadcast can land in the same tick
  615 |         // the write returns, and a baseline read afterwards never differs.
  616 |         const beforeLabels = await Promise.all(pages.map((_, i) => cellLabel(i)));
  617 | 
  618 |         // The letter must DIFFER from what the cell holds, or the rendered
  619 |         // verdict is unchanged and the probe sees nothing.
  620 |         const truth = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
  621 |         const stored = latestStates(truth?.actions ?? []).get(`${x},${y}`)?.state ?? "";
  622 |         const wrongLetter =
  623 |           "ABDEFGHIJKLMNOPRSTUVW".split("").find(
  624 |             (c) => c !== stored.toUpperCase() && c !== probe.answer[0].toUpperCase(),
  625 |           ) ?? "Q";
  626 |         const published = await trpcPost(pages[1].request, "activeGame.addActions", {
  627 |           id: activeGameId,
  628 |           actions: [{ cordX: x, cordY: y, state: wrongLetter }],
  629 |         });
  630 |         expect(published.ok, `fan-out probe write: ${published.error}`).toBe(true);
  631 |         taken.add(clueKey(probe));
  632 | 
  633 |         await expect
  634 |           .poll(
  635 |             async () =>
  636 |               (await Promise.all(pages.map((_, i) => cellLabel(i)))).join("|") !==
  637 |               beforeLabels.join("|"),
  638 |             { timeout: 30_000 },
```