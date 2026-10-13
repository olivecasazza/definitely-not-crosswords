# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: multiplayer-soak.spec.ts >> four-player multiplayer soak >> live multiplayer across four contexts
- Location: tests/multiplayer-soak.spec.ts:379:7

# Error details

```
Error: every successful complete() returned the SAME completedGameId

expect(received).toBeLessThanOrEqual(expected)

Expected: <= 1
Received:    2
```

# Test source

```ts
  775 |         // parent Game, not the activeGameId, so identity is checked against the
  776 |         // pre-contention read rather than against the id we navigated to.
  777 |         expect(after?.gameId, "still the same parent game").toBe(beforeRead?.gameId);
  778 |         expect(after?.game?.questions?.length, "question set intact").toBe(
  779 |           beforeRead?.game?.questions?.length,
  780 |         );
  781 |         expect(await pages[0].locator(".cw-letter").first().isVisible()).toBe(true);
  782 |         // The known-wrong previousState is deliberately NOT asserted on — this
  783 |         // step's job is to make it observable, not to fail the soak.
  784 |       });
  785 | 
  786 |       // ── 6. Completion happens exactly once ─────────────────────────────
  787 |       // Completion has to be DRIVEN, not waited for. A 42-clue grid is far too
  788 |       // big for four browsers to solve a clue at a time, and the old "DEGRADED"
  789 |       // path meant the exactly-once assertions never ran at all.
  790 |       //
  791 |       // So the board is filled over the protocol: this spec already holds the
  792 |       // answer key and the clue geometry, and addActions takes a whole batch per
  793 |       // call, so a full grid is four calls rather than 161 keystrokes. Then all
  794 |       // four members call `complete` in the same tick — that is the race worth
  795 |       // testing, because `complete` reads membership and the ActiveGame row in
  796 |       // two separate pool queries before opening its transaction.
  797 |       await test.step("completion happens exactly once", async () => {
  798 |         const data = await trpcGet(pages[0], "activeGame.get", { id: activeGameId });
  799 |         // `/api/grids` is keyed by the PARENT game, which `activeGame.get`
  800 |         // returns as `gameId` — not by the activeGameId we navigated to.
  801 |         const answers = await answersFor(pages[0], data.gameId!);
  802 |         const all: AnswerClue[] = (data?.game?.questions ?? [])
  803 |           .map((c: Clue) => ({ ...c, answer: answers.get(clueKey(c)) }))
  804 |           .filter((c: Clue & { answer?: string }): c is AnswerClue => Boolean(c.answer));
  805 | 
  806 |         // Cells the key says should hold a letter that are not correct yet.
  807 |         // Deduped by coordinate because a crossing cell belongs to both an
  808 |         // ACROSS and a DOWN clue.
  809 |         const done = solvedClues(all, data?.actions ?? []);
  810 |         const already = new Set(
  811 |           all.filter((c) => done.has(clueKey(c))).flatMap(clueCells).map(([x, y]) => `${x},${y}`),
  812 |         );
  813 |         const wanted = new Map<string, string>();
  814 |         for (const c of all) {
  815 |           clueCells(c).forEach(([x, y], i) => {
  816 |             const key = `${x},${y}`;
  817 |             if (!already.has(key)) wanted.set(key, (c.answer ?? "")[i] ?? "");
  818 |           });
  819 |         }
  820 |         console.log(`COMPLETION: ${wanted.size} cells left of ${all.length} clues`);
  821 | 
  822 |         if (wanted.size > 0) {
  823 |           // Round-robin across the four players so the final writes overlap.
  824 |           const perPlayer: Array<Array<{ cordX: number; cordY: number; state: string }>> = [
  825 |             [],
  826 |             [],
  827 |             [],
  828 |             [],
  829 |           ];
  830 |           let turn = 0;
  831 |           for (const [key, letter] of wanted) {
  832 |             const [cordX, cordY] = key.split(",").map(Number);
  833 |             perPlayer[turn % 4].push({ cordX, cordY, state: letter });
  834 |             turn++;
  835 |           }
  836 |           const filled = await Promise.all(
  837 |             perPlayer.map((actions, n) =>
  838 |               trpcPost(pages[n].request, "activeGame.addActions", { id: activeGameId, actions }),
  839 |             ),
  840 |           );
  841 |           filled.forEach((r, n) =>
  842 |             console.log(
  843 |               `  player ${n + 1}: ${perPlayer[n].length} cells -> ok=${r.ok}` +
  844 |                 (r.ok ? ` solved=${r.data?.solved} filled=${r.data?.filled}/${r.data?.total}` : ` err=${r.error}`),
  845 |             ),
  846 |           );
  847 |           const failures = filled.filter((r) => !r.ok);
  848 |           expect(failures.map((f) => f.error), "every fill batch was accepted").toEqual([]);
  849 |           expect(
  850 |             filled.some((r) => r.data?.solved === true),
  851 |             "the board reports solved after the final fill",
  852 |           ).toBe(true);
  853 |         }
  854 | 
  855 |         // The race: four members call complete() in one tick.
  856 |         const completed = await Promise.all(
  857 |           pages.map((p) => trpcPost(p.request, "activeGame.complete", { id: activeGameId })),
  858 |         );
  859 |         const okIds = completed
  860 |           .map((r) => r.data?.id)
  861 |           .filter((v: unknown): v is string => typeof v === "string");
  862 |         const report =
  863 |           `complete() by 4 members in one tick: ${okIds.length} returned an id, ` +
  864 |           `${completed.length - okIds.length} refused ` +
  865 |           `(${completed.filter((r) => !r.ok).map((r) => r.error).join(" | ") || "none"}); ` +
  866 |           `distinct completedGameIds = ${new Set(okIds).size} [${okIds.join(", ")}]`;
  867 |         console.log(`COMPLETION: ${report}`);
  868 |         await testInfo.attach("completion-report.txt", { body: report, contentType: "text/plain" });
  869 | 
  870 |         // EXACTLY ONCE. Two distinct ids means two CompletedGame rows, two sets
  871 |         // of MemberScores and two GameCompleted broadcasts.
  872 |         expect(
  873 |           new Set(okIds).size,
  874 |           "every successful complete() returned the SAME completedGameId",
> 875 |         ).toBeLessThanOrEqual(1);
      |           ^ Error: every successful complete() returned the SAME completedGameId
  876 | 
  877 |         // complete() deletes ActiveGame, cascading GameActions. A surviving row
  878 |         // means it never ran, whatever the ids said.
  879 |         const after = await trpcGet(pages[0], "activeGame.get", { id: activeGameId }).catch(
  880 |           () => null,
  881 |         );
  882 |         expect(after, "ActiveGame is deleted once completed").toBeNull();
  883 |       });
  884 |     } finally {
  885 |       await Promise.all(pages.map((p) => p.close().catch(() => {})));
  886 |     }
  887 |   });
  888 | });
```