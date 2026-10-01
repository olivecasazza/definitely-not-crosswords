# Multiplayer load test

A protocol-level load test for the **multiplayer API surface** — no browser, no
Dioxus bundle. It drives exactly the calls a four-player co-op game makes:
`activeGame.join`, `activeGame.addActions`, `activeGame.publishPresence`,
`activeGame.get`, `GET /api/grids/<gameId>`, and the `/api/trpc-ws` push
channel.

Everything is plain ES modules run by `k6` itself — no npm, no lockfile, no
build step.

## Running it

Through Nix (recommended — pins k6 and ships this directory into the store):

```sh
nix run ./client#crossword-load -- version
nix run ./client#crossword-load -- run multiplayer.js
```

The wrapper resolves any `*.js` argument against the packaged `load/` directory
and passes every other argument straight through to `k6`, so the usual k6 flags
work:

```sh
nix run ./client#crossword-load -- run multiplayer.js --vus 16 --summary-export ./result.json
```

Against a live host with credentials and a longer soak:

```sh
export E2E_BASE_URL=https://crosswords-staging.casazza.io
export E2E_EMAIL=... E2E_PASSWORD=...
export E2E_EMAIL_2=... E2E_PASSWORD_2=...
export E2E_EMAIL_3=... E2E_PASSWORD_3=...
export E2E_EMAIL_4=... E2E_PASSWORD_4=...
nix run ./client#crossword-load -- run multiplayer.js --env SOAK_DURATION=10m
```

Raw k6, if you have it:

```sh
k6 run load/multiplayer.js
```

**The four accounts must already exist, or `setup()` will create them.**
`user.signup` is the only non-admin provisioning path; "already exists" is
treated as success, so `setup()` is idempotent and safe to re-run.

## What each scenario proves

| Scenario | VUs | What it exercises | What a failure means |
| --- | --- | --- | --- |
| `join` | `JOIN_VUS` (4) | `activeGame.join` — membership gate and idempotent re-join | Every write scenario is measuring `FORBIDDEN`, not the write path |
| `actions` | `VUS` (4) | `activeGame.addActions` with REAL letters from the answer key, four writers over one shared board | The board write path is slow, failing, or losing concurrent writes |
| `presence` | `PRESENCE_VUS` (2) | `activeGame.publishPresence` at a human-ish rate (one clue every `PRESENCE_THINK` seconds) | The ephemeral broadcast path backs up or starts rejecting |
| `read` | `READ_VUS` (2) | `activeGame.get` plus `GET /api/grids/<gameId>` | Reads degrade before writes do — the common "everything gets slow at once" signal |
| `subscriptions` | `WS_VUS` (2) | `/api/trpc-ws` upgrade + `activeGame.onAddActions` / `activeGame.onPresence` fan-out | Push is dead or silent while HTTP still looks fine |

VU *N* always drives account *N* (`(__VU - 1) % 4`). With the default 4 VUs that
is one writer per bot account, which is what makes four VUs contend for the same
crossing cells.

`setup()` joins every account before any scenario starts, so the `actions` and
`presence` scenarios never measure the membership gate instead of their own
write path.

## The crossing-conflict metric

This is the one that is not a stock k6 counter.

`setup()` walks every answer into concrete cells and marks a cell as **crossing**
when more than one answer passes through it — the intersections. `CROSSING_SHARE`
(0.8) of sampled `addActions` batches are drawn from those crossings, so the
default run is dominated by concurrent writes to shared cells.

Each VU keeps its own record of the letter it last submitted to each cell it has
touched. Every `addActions` response reports, per action, the cell state the
server read *before* applying the write (`previousState`). When that does not
equal what this VU last put there, another player got in first — or the server
read a stale snapshot.

`crossing_conflict_rate` is the share of `addActions` **responses** (one sample
each) in which at least one such cell came back wrong.

* `0` — every writer saw its own last letter. Serialisation holds.
* `> 0` — writers are interleaving on shared cells. Whether that is a lost-write
  bug or just last-write-wins semantics is the thing to read the server code for;
  the metric measures that the interleaving happened, not that it is wrong.

Threshold: `rate<0.05`, override with `CROSSING_CONFLICT_MAX`.

### Letters on crossing cells are rotated per account

If every VU submitted the true answer letter, all four would write the *same*
character to the same cell, `previousState` would always equal what the caller
last wrote, and the metric would be structurally incapable of ever firing —
which is exactly what the first live run showed: 0.00% over 111 batches. So
crossing-cell writes rotate the letter by account index (`A`→`B`→`C`→`D`), which
makes each writer's letter distinct and the interleaving observable. Non-crossing
cells still submit the real answer letter, so the bulk of the load is realistic.

### Measured baseline

A 4-VU / 12-second smoke against `crosswords-staging` on 2026-10-01, re-measured
after the DEF-274 Postgres-outbox fix shipped (staging 0.1.83):
259 requests, 412 checks, 0 failures, 100 WS broadcast frames, and
`crossing_conflict_rate` = **0.40**, which crosses the `0.05` threshold.

This is the current server behaviour under four concurrent writers, not a
defect in the test: `addActions` reads the pre-write cell snapshot without
serialising the write, so concurrent writers interleave on shared cells and the
loser's logged `previousState` is stale. Read the number as "interleaving
happens this often at this concurrency", and re-baseline the threshold once the
write path locks.

Note what the outbox fix changed here: WS frames now arrive for every subscriber
(100 in this run), where the pod-local bus delivered them to only one replica.
The crossing race is a separate write-path defect and was NOT fixed by it.


## Cookies and WebSockets

k6 has no cookie jar. Every request — and the WebSocket upgrade — sets
`Cookie: next-auth.session-token=<value>` explicitly from the cookie array
`setup()` returned.

The WebSocket scenario works because `ws.connect(url, params)` accepts
`params.headers`, which are sent on the **upgrade request**. The server
authenticates the upgrade from that header
(`client/backend/server/src/main.rs`, `trpc_ws`), so the socket is a real
authenticated subscription rather than an anonymous one that silently receives
nothing. Every `data` frame that arrives is counted into
`definitely_not_crosswords_load_broadcast_received_total`; if the socket were
unauthenticated, that counter would simply stay at zero. A live smoke run
against staging received 151 frames, so it does not.

## Emitted metrics

`handleSummary()` writes three things: a human text summary to stdout, a
machine-readable `load-summary.json` (`SUMMARY_JSON` to rename), and Prometheus
text exposition format to stdout so a CI step can scrape and push it.

All names carry the `definitely_not_crosswords_load_` prefix. Every series is
labelled `base_url` and `active_game_id`.

```
definitely_not_crosswords_load_http_requests_total            counter
definitely_not_crosswords_load_http_requests_failed_ratio      gauge
definitely_not_crosswords_load_http_duration_p95_seconds       gauge
definitely_not_crosswords_load_http_duration_p99_seconds       gauge
definitely_not_crosswords_load_crossing_conflict_rate          gauge
definitely_not_crosswords_load_crossing_conflicts_total        counter
definitely_not_crosswords_load_trpc_error_rate                 gauge
definitely_not_crosswords_load_ws_error_ratio                  gauge
definitely_not_crosswords_load_broadcast_received_total        counter
definitely_not_crosswords_load_checks_ratio                    gauge
definitely_not_crosswords_load_iterations_total                counter
definitely_not_crosswords_load_vus_max                         gauge
```

Durations are in **seconds**.

There is deliberately **no `proc` label**. k6 2.0.0 does not expose tag
sub-metrics through `handleSummary` — verified empirically: `data.metrics`
contains only un-tagged entries, so a per-proc breakdown cannot be built from
there. Emitting one would mean inventing a number, so there is one total
instead. The `proc` tags are still applied to every request (they show up in
k6's own HTML/JSON output); they just cannot reach this exposition.


## Environment variables

| Variable | Default | Meaning |
| --- | --- | --- |
| `E2E_BASE_URL` | `https://crosswords-staging.casazza.io` | Host under test |
| `E2E_EMAIL` / `E2E_PASSWORD` | — | Bot account 1 (required) |
| `E2E_EMAIL_2` / `E2E_PASSWORD_2` | — | Bot account 2 (required) |
| `E2E_EMAIL_3` / `E2E_PASSWORD_3` | — | Bot account 3 (required) |
| `E2E_EMAIL_4` / `E2E_PASSWORD_4` | — | Bot account 4 (required) |
| `SOAK_DURATION` | `30s` | Duration of `actions`, `presence`, `read`, `subscriptions` |
| `VUS` | `4` | Concurrent `addActions` writers |
| `JOIN_VUS` | `4` | Concurrent joiners |
| `JOIN_DURATION` | `15s` | Join scenario duration |
| `PRESENCE_VUS` | `2` | Concurrent presence publishers |
| `READ_VUS` | `2` | Concurrent readers |
| `WS_VUS` | `2` | Concurrent subscription sockets |
| `WS_HOLD_MS` | `10000` | How long each VU holds its socket open |
| `THINK_TIME` | `1` | Seconds between `join`/`actions` iterations |
| `PRESENCE_THINK` | `3` | Seconds between presence publishes (human-ish) |
| `READ_THINK` | `2` | Seconds between read iterations |
| `BATCH_SIZE` | `5` | Letters submitted per `addActions` call |
| `CROSSING_SHARE` | `0.8` | Share of batches forced onto crossing cells |
| `FAILED_RATE_MAX` | `0.01` | `http_req_failed` threshold |
| `CROSSING_CONFLICT_MAX` | `0.05` | `crossing_conflict_rate` threshold |
| `WS_ERROR_RATE_MAX` | `0.05` | `crossword_ws_error_rate` threshold |
| `SUMMARY_JSON` | `load-summary.json` | Machine-readable summary path |

The defaults are a smoke test: 4 VUs for 30 seconds. Raise `SOAK_DURATION` and
`VUS` for anything you intend to act on.

## CI story

**This must not be a `checks` entry.** It needs a live host, four real accounts
with real passwords, and it writes to a shared staging database — none of which
`nix flake check` can give it. The Nix package is only a *packaging* of k6 plus
this directory; running it is a separate, explicitly invoked step.

The shape that fits the repo rule (no `nix build` in GitHub Actions, Nix
verification stays in `client/flake.nix`) is a scheduled workflow that:

1. decrypts `secrets.yaml` with sops to get `E2E_EMAIL*` / `E2E_PASSWORD*`;
2. installs k6 from the binary release cache — **not** via `nix build`;
3. runs `k6 run multiplayer.js --env SOAK_DURATION=5m`;
4. scrapes the Prometheus text from stdout and pushes it to the pushgateway;
5. reads `load-summary.json` and fails the job on `crossing_conflict_rate` or
   `http_req_failed` above threshold.

If that workflow is ever written, it is a separate file outside `checks` and
outside this directory's Nix packaging.