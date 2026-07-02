# e2e — Playwright canary + demo video

Deterministic end-to-end tests against staging that double as a marketing demo
recording. Semantic locators (`getByRole`/`getByText`/`getByLabel`) keep this
low-maintenance: styling/layout refactors don't break it; only a real change to
the user-facing flow does — which is exactly when the canary should page you.

- `tests/smoke.spec.ts` — unauthenticated canary (no creds; safe nightly).
- `tests/demo.spec.ts` — authenticated golden path; its 1080p recording is the
  demo video. Skipped unless `E2E_EMAIL` / `E2E_PASSWORD` are set.

## Run locally

```bash
cd e2e
npm ci
npx playwright install chromium         # NixOS: browsers need FHS libs — run in
                                        # the mcr.microsoft.com/playwright container
E2E_BASE_URL=https://crosswords-staging.casazza.io npm run canary
# authenticated demo (records video under test-results/):
E2E_EMAIL=... E2E_PASSWORD=... npm run demo
npm run report
```

## CI (`.github/workflows/e2e-canary.yml`)

- **Nightly** (cron) — canary against staging; Discord alert on failure
  (`DISCORD_WEBHOOK` secret).
- **On release** — records + publishes `demo.mp4` to the GitHub release.
- Runs in the official Playwright container (browsers preinstalled).

To enable the authenticated demo + a fuller canary, add repo secrets
`E2E_EMAIL` / `E2E_PASSWORD` (a dedicated staging test account).

## Follow-up: AI self-heal (lower maintenance still)

Semantic locators already survive most UI change. A future phase can add a cheap
LLM fallback that, when a locator genuinely breaks, recovers the step and posts a
"test needs updating" suggestion — cutting maintenance further without making the
canary non-deterministic (AI only runs on a real break, never on the happy path).
