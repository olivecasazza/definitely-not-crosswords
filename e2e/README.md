# e2e — Playwright canary + demo video

Deterministic end-to-end tests against staging that double as a marketing demo
recording. Semantic locators (`getByRole`/`getByText`/stable class hooks) keep
this low-maintenance: styling/layout refactors don't break it; only a real
change to the user-facing flow does — which is exactly when the canary should
page you.

- `tests/smoke.spec.ts` — unauthenticated canary (no creds; safe nightly).
- `tests/demo.spec.ts` — authenticated premium tour; its 1080p recording is the
  demo video. Skipped unless `E2E_EMAIL` / `E2E_PASSWORD` are set.
- `tests/helpers.ts` — human-ish interaction helpers (jittered dwells, waypoint
  mouse movement, per-keystroke cadence, idle drift) so the recording looks
  hand-driven. Pacing only — sync always comes from web-first assertions.

## What the demo covers (also the feature-completeness smoke test)

1. **Sign-in** through the UI with natural typing.
2. **Lobby** — Available/Active/Completed panels render.
3. **Gameplay** — opens a game (continue first, else start), solves a clue for
   real (answers pulled via the tRPC API with the session cookie).
4. **Co-op** — copies the invite link, then a second browser context joins the
   same game: roster chips, per-player presence ring on the board, and the
   partner's correct letters landing live on the recorded page. Uses
   `E2E_EMAIL_2` / `E2E_PASSWORD_2` when set; otherwise falls back to the
   primary account (live transport only — presence hides same-user echoes).
5. **Completion** — when the puzzle is small (≤12 clues) the tour finishes it,
   landing on "Crossword Solved!" with real standings. This writes a
   CompletedGame to the test account on purpose: it keeps the stats pages
   alive for the video and exercises the scoring path.
6. **Stats** — leaderboard, career, head-to-head compare (picks an opponent),
   teams panel.
7. **Profile + subscription** — the premium pitch (plan row, quota, upgrade
   CTA or active Pro chip).

Team creation is deliberately *not* exercised (owners can't leave teams, so
each run would accumulate junk).

## Run locally

```bash
cd e2e
npm ci
npx playwright install chromium         # NixOS: browsers need FHS libs — run in
                                        # the mcr.microsoft.com/playwright container
E2E_BASE_URL=https://crosswords-staging.casazza.io npm run canary
# authenticated demo (records video under test-results/):
E2E_EMAIL=... E2E_PASSWORD=... E2E_EMAIL_2=... E2E_PASSWORD_2=... npm run demo
npm run report
```

## CI (`.github/workflows/e2e-canary.yml`)

- **Nightly** (cron) — canary against staging; Discord alert on failure
  (`DISCORD_WEBHOOK` secret).
- **On release** — records + publishes `demo.mp4` to the GitHub release.
- Runs in the official Playwright container (browsers preinstalled).

To enable the authenticated demo + a fuller canary, add repo secrets
`E2E_EMAIL` / `E2E_PASSWORD` (a dedicated staging test account) and optionally
`E2E_EMAIL_2` / `E2E_PASSWORD_2` (a second account for the co-op chapter).

## Follow-up: AI self-heal (lower maintenance still)

Semantic locators already survive most UI change. A future phase can add a cheap
LLM fallback that, when a locator genuinely breaks, recovers the step and posts a
"test needs updating" suggestion — cutting maintenance further without making the
canary non-deterministic (AI only runs on a real break, never on the happy path).
