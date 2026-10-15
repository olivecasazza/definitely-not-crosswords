# HEAL — deterministic "test needs updating" bundles

## What this is

`heal-reporter.ts` implements the first half of the "Follow-up: AI self-heal"
section of [README.md](./README.md):

> Semantic locators already survive most UI change. A future phase can add a
> cheap LLM fallback that, when a locator genuinely breaks, recovers the step and
> posts a "test needs updating" suggestion — cutting maintenance further without
> making the canary non-deterministic (AI only runs on a real break, never on the
> happy path).

When a test fails, the reporter writes a **heal bundle** next to that test's own
artifacts: `heal-bundle.json` (machine-readable) and `heal-bundle.md` (the same
payload for a human). It captures the test title/file/project, the error message
and the first 40 stack lines, the failing line and the Playwright call that
failed, duration, retry attempt, the test's attachments, and the spec source
±20 lines around the break — so a reader sees the *intent* of the step, not just
the failure.

There is **no model call anywhere in the reporter**. CI must never need an API
key to run the canary. The LLM pass is a separate, opt-in consumer of the bundle.

## Wiring it in

`e2e/playwright.config.ts` already carries the entry:

```ts
reporter: [
  ["list"],
  ["html", { open: "never" }],
  // Explicit relative path: Playwright resolves a bare "heal-reporter" as a
  // package name and fails with MODULE_NOT_FOUND, since it is a local file.
  ...(process.env.E2E_HEAL ? [["./heal-reporter.ts", {}]] : []),
],
```

The `E2E_HEAL` gate is what keeps the default reporter list untouched: with the
var unset the entry contributes nothing and the run behaves exactly as it does
today. Both paths are verified — the default run is unchanged, and `E2E_HEAL=1`
loads the reporter and writes nothing on a green suite.

To try it without the config at all:

```bash
E2E_HEAL=1 npx playwright test smoke.spec.ts --reporter=list,./heal-reporter.ts
```


## Using a bundle on a real break

1. Run the failing spec headed, with a trace: `--headed --trace on`.
2. Open `<output>/<test-id>-<project>/heal-bundle.md`. It names the spec and
   line, shows the failing call, and carries a deterministic `suggestedNextStep`.
3. `npx playwright show-trace trace.zip` in the same directory to see which step
   timed out or resolved to the wrong node.
4. Fix the **locator** (prefer `getByRole` / `getByLabel` / `getByText`) at the
   cited `file:line`. Do not relax the assertion to make it green.
5. If the UI genuinely regressed, that is a bug report — not something to heal.

## How an LLM pass would consume it later

A future job reads `heal-bundle.json` (schema is versioned via `"version": 1`)
and produces a *suggestion* — a proposed replacement selector plus the `file:line`
to patch — posted as a PR comment or issue. Two rules it must keep:

- It runs **only** when a bundle exists. No bundle, no model call, no cost, no
  non-determinism in the canary.
- Its output is a suggestion for a human to accept, never an auto-merge.

## Redaction guarantee (and its limits)

Every string written to a bundle — error, stack, spec source, attachments — passes
through `redact()` first, and there is no code path that writes a raw bundle.
Redacted patterns:

- `next-auth.*session-token=…` and other session/cookie-ish `name=value` pairs
- the entire value of a `Cookie:` / `Set-Cookie:` / `Authorization:` header
- credentials embedded in a URL (`https://user:pass@host`)
- anything matching `SOME_SECRET_VAR=value` (`SECRET`, `TOKEN`, `PASSWORD`,
  `API_KEY`, `PRIVATE_KEY`, `CREDENTIAL`, `COOKIE`, `SESSION`, …)
- the literal values of known secret env vars, if they appear bare

**This is not a DLP.** It is a known-pattern scrub, and it has known limits:

- An arbitrary credential with no recognisable shape (a raw bearer token printed
  on its own line, a password embedded in prose) will pass through.
- Secrets held only in the *spec file's own source* are visible to anyone who can
  already read the spec — redaction cannot fix a leaked secret committed to git.
- It says nothing about the test's other artifacts (`trace.zip`, videos), which
  are captured independently by Playwright's own config. A trace can contain
  request headers. If you publish traces, check them separately.

The reason it is mandatory at all: a heal bundle gets attached to CI artifacts
and pasted into issues, and a leaked `next-auth.session-token` is a **live
credential** that anyone reading the issue could replay.