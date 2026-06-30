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
