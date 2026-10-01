# e2e — Playwright canary + demo video

Deterministic end-to-end tests against staging that double as a marketing demo
recording. Semantic locators (`getByRole`/`getByText`/stable class hooks) keep
this low-maintenance: styling/layout refactors don't break it; only a real
change to the user-facing flow does — which is exactly when the canary should
page you.

- `tests/smoke.spec.ts` — unauthenticated canary (no creds; safe nightly).
- `tests/boot-failure.spec.ts` — boot-shell canary, all unauthenticated. A
  blocked bundle load must be a recoverable card, never a blank page, with
  exactly one automatic retry; and with scripting **disabled** the `<noscript>`
  card must be the only card on screen rather than a permanent false "Loading"
  (the `javaScriptEnabled: false` branch, DEF-171).
- `tests/pro-checkout.spec.ts` — billing canary: signs in, asserts the priced
  `Upgrade to Pro — $10/year` CTA is **visible**, clicks it, and asserts the
  browser lands on a real `lemonsqueezy.com` checkout. This is the only thing
  that proves the `LEMONSQUEEZY_*` key is *valid* rather than merely *present*
  (`/api/config`'s `proCheckout` flag only proves presence). It creates a
  checkout and **stops there** — never complete it: staging runs the ~90%-off
  beta code, so a finished order is a real charge that also fires the live
  `/api/webhooks/lemonsqueezy` handler. Skipped unless `E2E_EMAIL` /
  `E2E_PASSWORD` are set.
- `tests/demo.spec.ts` — authenticated premium tour; its 1080p recording is the
  demo video. Skipped unless `E2E_EMAIL` / `E2E_PASSWORD` are set.
- `tests/helpers.ts` — human-ish interaction helpers (jittered dwells, waypoint
  mouse movement, per-keystroke cadence, idle drift) so the recording looks
  hand-driven. Pacing only — sync always comes from web-first assertions.

## What the demo covers (also the feature-completeness smoke test)

1. **Sign-in** through the UI with natural typing.
2. **Lobby** — Available/Active/Completed panels render.
3. **Gameplay** — opens a game (continue first, else start), solves a clue for
   real (public board state from tRPC; generated answers from `/api/grids/:id`).
4. **Co-op** — copies the invite link, then a second player joins the same
   game from an emulated iPhone (its own recording; CI composites it
   picture-in-picture into the published demo.mp4): roster chips, per-player
   presence ring on the board, and the partner's correct letters landing live
   on the recorded page. Uses `E2E_EMAIL_2` / `E2E_PASSWORD_2` when set;
   otherwise falls back to the primary account (live transport only —
   presence hides same-user echoes).
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
# billing canary (creates a Lemon Squeezy checkout, never completes it):
E2E_EMAIL=... E2E_PASSWORD=... npm run pro-checkout
# authenticated demo (records video under test-results/):
E2E_EMAIL=... E2E_PASSWORD=... E2E_EMAIL_2=... E2E_PASSWORD_2=... npm run demo
npm run report
```

## CI (`.github/workflows/e2e-canary.yml`)

- **Nightly** (cron) — canary against staging; Discord alert on failure
  (`DISCORD_WEBHOOK` secret).
- **On release** — records + publishes `demo.mp4` to the GitHub release.
- Runs in the official Playwright container (browsers preinstalled).

To enable the authenticated demo, the billing canary, and a fuller canary, add
repo secrets `E2E_EMAIL` / `E2E_PASSWORD` (a dedicated staging test account) and
optionally `E2E_EMAIL_2` / `E2E_PASSWORD_2` (a second account for the co-op
chapter). The billing canary and the demo both need that account to be a
**Free** subscriber — a Pro account has no upgrade control to click.

## Multiplayer soak and load (four players)

- `tests/multiplayer-soak.spec.ts` — four authenticated contexts in ONE spec
  (not four workers: separate processes cannot observe each other's screens),
  sharing one `activeGameId`. Needs four DISTINCT accounts
  (`E2E_EMAIL`…`E2E_PASSWORD_4`): `publishPresence` deliberately hides
  same-user echoes, so reusing one account yields four boards with no presence
  and a green run that proves nothing. Self-skips if a pair is unset.
- `scripts/multiplayer-bot-admin.sh` (repo root) provisions the four accounts via
  `user.signup` — no admin credential — picks the in-progress game with the MOST
  open cells, and joins players 2–4. Read-only by default; `--apply` to mutate;
  refuses production without `--allow-production`.
- `tests/` runs nightly; the protocol-level k6 load test is
  `nix run ./client#crossword-load -- run multiplayer.js`.

### DEF-274 (fixed upstream — this suite now guards it)

Broadcast fan-out was pod-local: `EventBus` was an in-process
`tokio::broadcast` under `replicaCount: 2`, so an event reached only sockets
co-located with the publishing pod. That broke `demo.spec.ts` chapter 4 for 23
consecutive canary runs. It is fixed — events now relay through a Postgres
outbox (`fix(server): fan app events out across pods via a Postgres outbox`,
#204), and measured on staging 0.1.83 a single `addActions` reaches 4/4 sockets
(24/24 over six publishes).

This suite asserts **all four** contexts receive every solve, precisely so that
regression is caught here. An earlier version asserted "at least one peer" to
route around the defect while it was live; that form would now pass even if the
outbox broke again, so it was reverted after the fix landed. If you ever see
this spec fail on a `did NOT reach` line, that is DEF-274 returning.

### Still open: crossing-cell writes race

`add_actions` reads its pre-write cell snapshot before inserting, with no lock.
`crossing_conflict_rate` measures 0.42–0.66 against a `rate<0.05` threshold, so
the k6 load job stays red until the write path serialises. The soak observes
and reports it (last-write-wins, stale `previousState`) rather than asserting on
it, since the outcome is the documented behaviour today.

## Self-heal bundles (`HEAL.md`)

`heal-reporter.ts` is wired into `playwright.config.ts` behind `E2E_HEAL=1`. On
a failure it writes a redacted `heal-bundle.{json,md}` next to the test's own
output; on a green run it writes nothing, which is what keeps the canary
deterministic. See `HEAL.md`. The remaining future phase is the cheap LLM pass
that consumes a bundle and posts a "test needs updating" suggestion — the
deterministic half, which produces the input, is done.
