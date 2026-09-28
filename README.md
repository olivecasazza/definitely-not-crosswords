# definitely-not-crosswords

Cooperative, real-time crosswords. A Rust app end to end: a [Dioxus](https://dioxuslabs.com/)
WebAssembly frontend (built on [panel-kit](https://github.com/olivecasazza/panel-kit)),
an [Axum](https://github.com/tokio-rs/axum) backend speaking the tRPC wire format,
and an ONNX-powered crossword generator — all built reproducibly with Nix + crane.

## Layout

The Rust workspace lives in [`client/`](client/):

| Crate | What it is |
|-------|------------|
| `web` (`crossword-web`) | Dioxus → WebAssembly frontend |
| `core` (`crossword-core`) | shared rpc/wire types |
| `desktop` (`crossword-desktop`) | Tauri v2 shell wrapping the wasm frontend |
| `backend/server` (`crossword-server`) | Axum server: tRPC routers, next-auth-compatible auth, WebSocket subscriptions, and the ONNX crossword generator (`ort` + `tokenizers`) |
| `backend/{db,auth,events}` | shared types, JWE session auth, event bus |
| `backend/tools` (`crossword-tools`) | `migrate` / `seed` / `seed_admin` binaries (sqlx) |

Repo root holds the deployment flake, the k8s `charts/`, infra (`scratch-nixlab/`,
`secrets/`), and the generator's runtime data manifest (`data/crossword/`).

## Develop

The app toolchain (cargo, [dx](https://dioxuslabs.com/learn/0.6/CLI/), tauri, the
GTK/WebKit + onnxruntime deps) lives in the `client` dev shell:

```bash
nix develop ./client

# frontend (hot-reload dev server)
cd client && dx serve -p crossword-web

# backend (serves /api and, with WEB_DIST set, the wasm bundle single-origin)
DATABASE_URL=… NEXTAUTH_SECRET=… cargo run -p crossword-server
```

The root dev shell (`nix develop`) carries `psql`, `sops`, and `age` for database
and secrets work.

## Database

Migrations and seeding are sqlx-based (no Prisma). From `client/`:

```bash
DATABASE_URL=…  cargo run -p crossword-tools --bin migrate       # apply migrations
DATABASE_URL=…  cargo run -p crossword-tools --bin seed          # WordNet dictionary
ADMIN_USERS_JSON='[{"email":"you@example.com","role":"ADMIN"}]' \
DATABASE_URL=…  cargo run -p crossword-tools --bin seed_admin    # admin users
```

`docker-compose.yaml` provides a local Postgres for development.

## Plans & pricing

| | Free | Pro |
|---|---|---|
| Price | $0 | **$10.00 USD / year** (recurring) |
| Solving puzzles | unlimited | unlimited |
| Generator runs | 5 / calendar month | unlimited |
| Team size | 4 | 10 |

The quota and team caps are enforced by `FREE_LIMIT` (`routers/subscription.rs`) and
`FREE_MAX_SIZE` / `PRO_MAX_SIZE` (`routers/team.rs`). **The price itself is not
configured here** — Lemon Squeezy is the source of truth (store `390247`, variant
`1718877`, one annual subscription; no one-time SKUs). The literals in `pages/home.rs`
(front door), `components/brand.rs` (login + signup panels), and this table are copies,
so change the LS variant and all three.
Staging runs the same variant with a 90%-off beta code, hence its `$1` banner.

The Pro **purchase** control is a separate thing from the price, and it is gated at
runtime: `GET /api/config` returns `features.proCheckout`, derived from the same
`billing.lemonSqueezy.enabled` chart flag that gates the `LEMONSQUEEZY_*` injection.
When it is `false` no surface renders a purchase control (`components/brand.rs`,
`pages/home.rs`, `pages/game_new.rs`, `components/pro_upgrade.rs`) and the copy says
Pro is opening soon — do not hardcode the price copy as if it were always buyable.

## Build & deploy

Everything builds with Nix:

```bash
nix build ./client#crossword-server   # the Axum binary
nix build ./client#crossword-web      # the wasm bundle
nix build ./client#crossword-desktop  # the Tauri desktop binary
nix build .#dockerImage               # deployable OCI image (server + bundle + assets + tools)
```

The generator's embedding model and WordNet dictionary are fetched and hash-verified
by the flake (`.#assets`) — there is no separate asset-download step. The deploy image
serves the frontend and API on one origin and carries `migrate`/`seed` for init jobs;
it ships to the registry via CI and is reconciled onto the cluster by Flux.

### Cutting a release

`release-plz` owns the version. It opens a `chore: release vX.Y.Z` PR against `main`;
once that merges, a later `release-plz` run sees `client/Cargo.toml` ahead of the newest
`v*` tag and cuts the tag, the GitHub release, the release notes, and the changelog
section. Pushing `v*` then builds the image and `deploy-production` bumps the pinned
production tag in nixlab.

**Dispatch `release-plz` by hand after the release PR merges.** The `auto-merge` job
merges with `secrets.GITHUB_TOKEN`, and GitHub creates no workflow run for events
triggered by the repository token — so the merge of the release PR emits no `push`
event and the tag is never cut. This silently swallowed v0.1.49: `client/Cargo.toml`
was already `0.1.49` with no `v0.1.49` tag anywhere. #109 fixed the same class of gap
for `ci.yaml` (build); `release-plz.yml` was left with it. Until that is closed, the
release day sequence is:

```bash
git checkout main && git pull
gh workflow run release-plz.yml --ref main   # cuts the tag + release
gh run watch $(gh run list -w release-plz.yml -L 1 --json databaseId -q '.[0].databaseId')
```

`release-plz` runs its `release-pr` step first, so expect it to *offer* a bump for the
next version; the tag it cuts is the one matching `client/Cargo.toml`. Close that
follow-up PR once the release is verified rather than letting it ride.

### Verifying a release

A green deploy run is not a health signal — verify the bytes the site serves:

```bash
scripts/verify-release-copy.sh                              # production, vs client/Cargo.toml
scripts/verify-release-copy.sh https://crosswords-staging.casazza.io 0.1.49
```

It asserts `/api/config` reports the expected version, that the approved copy is present
in the served `.wasm`, and that superseded copy is gone. Note that the copy lives in
`crossword-web_bg.wasm`, not in the `crossword-web.js` that `index.html` loads — the
`.js` is only the wasm-bindgen glue and contains no Rust string literals.
