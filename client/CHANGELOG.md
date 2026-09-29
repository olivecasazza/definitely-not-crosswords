# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.66](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.65...v0.1.66) - 2026-09-29

### Fixed

- *(ci)* give the deploy readiness probe a budget that outlasts a real rollout (#175)

### Other

- changelog for v0.1.65 [skip ci]


## [0.1.65](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.64...v0.1.65) - 2026-09-29

### Fixed

- *(server)* score the grid from the answer key, not client-declared rows (#173)

### Other

- changelog for v0.1.64 [skip ci]


## [0.1.64](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.63...v0.1.64) - 2026-09-29

### Other

- changelog for v0.1.63 [skip ci]


## [0.1.63](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.62...v0.1.63) - 2026-09-29

### Other

- changelog for v0.1.62 [skip ci]
- *(tooling)* add scripts/runner-toolchain for containers with no nix dev shell (#170)


## [0.1.62](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.61...v0.1.62) - 2026-09-29

### Fixed

- *(design)* move the body sans off Montserrat to Bricolage Grotesque

### Other

- changelog for v0.1.61 [skip ci]


## [0.1.61](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.60...v0.1.61) - 2026-09-29

### Other

- changelog for v0.1.60 [skip ci]


## [0.1.60](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.59...v0.1.60) - 2026-09-29

### Other

- changelog for v0.1.59 [skip ci]
- *(server)* guard the bundle path against a second answer (#165)


## [0.1.59](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.58...v0.1.59) - 2026-09-29

### Fixed

- *(perf)* animate the progress bars on the compositor, not layout

### Other

- changelog for v0.1.58 [skip ci]


## [0.1.58](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.57...v0.1.58) - 2026-09-29

### Fixed

- *(design)* drop the toast's 3px side-tab accent

### Other

- changelog for v0.1.57 [skip ci]


## [0.1.57](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.56...v0.1.57) - 2026-09-28

### Other

- changelog for v0.1.56 [skip ci]


## [0.1.56](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.55...v0.1.56) - 2026-09-28

### Other

- changelog for v0.1.55 [skip ci]


## [0.1.55](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.54...v0.1.55) - 2026-09-28

### Fixed

- *(tools)* a --withdraw that cannot dirty a tracked file or predate the consent [DEF-228] (#158)

### Other

- changelog for v0.1.54 [skip ci]


## [0.1.54](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.53...v0.1.54) - 2026-09-28

### Fixed

- *(tools)* an empty or truncated roster must not exit 0 [DEF-120] (#157)

### Other

- changelog for v0.1.53 [skip ci]


## [0.1.53](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.52...v0.1.53) - 2026-09-28

### Fixed

- *(web)* let the wrong-guess red compose with focus and selection (#155)

### Other

- changelog for v0.1.52 [skip ci]


## [0.1.52](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.51...v0.1.52) - 2026-09-28

### Fixed

- *(ci)* the canary probes a URL that never existed, and the deploy probe bails mid-rollout [DEF-103] (#153)

### Other

- changelog for v0.1.51 [skip ci]


## [0.1.51](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.50...v0.1.51) - 2026-09-28

### Added

- *(web)* connection state for the co-op socket [DEF-175] (#131)
- *(server)* surface the mail delivery mode on /api/config (DEF-201) (#136)
- *(web)* ship an og:image and upgrade the card to summary_large_image (DEF-190) (#137)
- *(tools)* a dry-runnable closed-alpha invite send [DEF-120] (#148)

### Fixed

- *(web)* make a failed bundle load a card, not a blank page [DEF-148] (#130)
- *(web)* give the served shell crawlable metadata (DEF-164) (#132)
- *(web)* the DEF-182 review deltas — a11y, CLS, noscript theming [DEF-183] (#133)
- *(web)* the DEF-181 review deltas — stale chip/ring 3.89:1, focus ring 1.10:1 [DEF-188] (#134)
- *(web,chart)* name the plan in the indexed description [DEF-202] (#135)
- *(web)* inherit crossword clue number ink [DEF-207] (#138)
- *(server)* answer file paths with a real 404, not the SPA shell [DEF-191] (#139)
- *(web)* the play screen's pre-board states are a card, not a paragraph [DEF-195] (#140)
- *(web)* a rel=canonical is per-URL, so name the host being served [DEF-196] (#141)
- *(web)* make stale presence state non-colour [DEF-200] (#143)
- *(scripts)* let release copy check assert pro checkout flag (#144)
- *(server)* expose per-deploy build identity [DEF-198] (#145)
- *(ci)* make the Rust test suite a real gate, not an advisory job [DEF-133] (#146)
- *(tools)* make the DEF-120 card's own answer executable [DEF-120] (#149)
- *(ci)* dispatch release-plz after a release PR auto-merge [DEF-131] (#150)
- *(ci)* dispatch release-plz on every auto-merge, not just release PRs [DEF-131] (#151)


## [0.1.50](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.49...v0.1.50) - 2026-09-28

First release to reach **production** since v0.1.49. Production was 23 commits behind
`main` because `deploy-production.yml` only fires on a `v*` tag push, so every fix below
existed on main and staging only. The headline change is the front door: production used
to stamp `immutable, max-age=31536000` on its own `/_assets` 404s, so a transient miss
became a year-long pinned edge object and the app bundle rendered blank.

### Fixed

- *(server)* never mark `/_assets` errors immutable; canary checks the served bundle (#122)
- *(web)* gate the Pro CTA on whether checkout can actually start (#123)
- *(web)* guard every animated surface behind `prefers-reduced-motion` (#124)
- *(server)* guard production indexing until the Pro price is announced (#120)
- *(ops)* make the Pro price announcement settable from the Helm chart (#150) (#121)
- *(web)* correct the board keyboard layer to the DEF-142 spec (#118)
- *(web)* give the crossword grid's row wrappers `role="row"` (#119)

### Added

- *(e2e)* canary a real Pro click through to a Lemon Squeezy checkout (#126)
- *(e2e)* assert the landed checkout is store 390247 / variant 1718877 (#127)
- *(e2e)* probe the production served artifact, not just staging (#125)

## [0.1.49](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.48...v0.1.49) - 2026-09-27

### Added

- *(chart)* support topology spread and a PodDisruptionBudget

### Fixed

- *(game,test,docs)* direction badge on the inline editor, stale specs and copy
- *(ci)* fail deploy runs when the endpoint is not serving 200 (#107)
- *(web)* bound the tiled workspace to the band under the header (#108)
- *(ci)* build main after a GITHUB_TOKEN auto-merge (#109)
- *(ci)* deploy staging from the dispatched build, not only from pushes (#110)
- *(server)* scope stats.getUserStats to the session, gate stats.getCompletedGame (#111)
- *(web)* cooperative-led home copy, priced Pro CTA, drop the LAUNCH50 field (#112)
- *(server)* keep staging unindexed with robots.txt + X-Robots-Tag (#113)

### Other

- changelog for v0.1.48 [skip ci]
- move the build and migration gates to buildbot checks (#105)


## [0.1.48](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.47...v0.1.48) - 2026-09-24

### Fixed

- *(ci)* anchor nixlab tag rewrites on imagepolicy markers (#102)

### Other

- changelog for v0.1.47 [skip ci]
- evaluate checks on x86_64-linux only (#104)


## [0.1.47](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.46...v0.1.47) - 2026-09-17

### Added

- *(chart)* platform health, resource efficiency and delivery dashboards (#98)
- *(game)* merge the Active Clue panel into the clue list (#99)
- migrate web workspaces to panel-kit v1

### Fixed

- *(ci)* trigger deploy-production on tag push, not main push [DEF-69] (#100)
- panel-kit 1.1.1 — restore auto-fit tiling and click-to-raise

### Other

- changelog for v0.1.46 [skip ci]
- advance panel-kit pins to e216fa6 (lock refresh after fork push)
- refresh panel-kit lock to e216fa6
- refresh root lock client/panel-kit to e216fa6


## [0.1.46](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.45...v0.1.46) - 2026-08-24

### Added

- *(k8s)* add deploy-production GitHub Actions workflow (#90)
- *(server)* add job audit log and per-job visibility ACLs [DEF-72] (#83)
- *(server)* job:create tRPC procedure + structured RBAC envelope [DEF-70] (#81)

### Fixed

- *(ci)* rewrite git@ SSH URLs to HTTPS before nix build in deploy-staging (#84)
- *(deploy)* rewrite git@github.com:olivecasazza/ SSH URLs to HTTPS before nix build (#86)
- *(def-78)* add ssh:// git URL rewrite for agentctx (#88)
- *(def-78)* add ssh:// git URL rewrite for agentctx (v3)
- *(k8s)* handle bare production tag in deploy-production sed (#91)
- *(def-85)* restore the 157 files deleted by #88 (#93)
- *(def-85)* restore main tree deleted by PR #88 (#92)
- *(ci)* replace magic-nix-cache with cachix-action to avoid HTTP 418 throttle (#87)
- *(ci)* pin cachix/cachix-action to v17 (#94)
- *(server)* repair crossword-server test compile errors [DEF-90] (#95)
- *(ci)* pin cachix/cachix-action to v17 in deploy-staging [DEF-69] (#97)


## [0.1.44](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.43...v0.1.44) - 2026-08-18

### Added

- *(server)* GET /api/grids/:id REST endpoint for generated grids (DEF-59)

### Other

- changelog for v0.1.43 [skip ci]


## [0.1.43](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.42...v0.1.43) - 2026-08-18

### Added

- *(server)* add job:create RBAC gate and POST /api/jobs endpoint (#70)
- *(web)* consolidate footer and dock (GH-58) (#71)

### Fixed

- *(web)* scale board cell font with the panel, not the viewport (GH-#56) (#69)
- *(web)* login title-panel UI bug (GH-57) (#73)

### Other

- changelog for v0.1.42 [skip ci]


## [0.1.42](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.41...v0.1.42) - 2026-08-18

### Added

- *(admin)* collapse admin routes into one panel-kit view with dock nav (#68)

### Documentation

- postmortem for GH-#60 vipPass column drift (#66)

### Other

- changelog for v0.1.41 [skip ci]


## [0.1.41](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.40...v0.1.41) - 2026-08-16

### Added

- *(chart)* optional Kueue queueing for the game-seed CronJob

### Other

- changelog for v0.1.40 [skip ci]

### Performance

- *(server)* batch candidate embedding through the ONNX session


## [0.1.40](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.39...v0.1.40) - 2026-08-12

### Fixed

- *(web)* content-size panels, add a How to Play tutorial, repair light theme
- *(db)* repair prod schema drift and delete the adoption path
- *(server)* reap generation jobs whose owner is gone

### Other

- changelog for v0.1.39 [skip ci]
- apply migrations to a throwaway postgres before shipping


## [0.1.39](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.38...v0.1.39) - 2026-08-10

### Fixed

- *(web)* cancel the reset form's native submit with a real DOM listener

### Other

- changelog for v0.1.38 [skip ci]


## [0.1.38](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.37...v0.1.38) - 2026-08-10

### Other

- changelog for v0.1.37 [skip ci]
- *(e2e)* guard the reset form against dropping its token


## [0.1.37](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.36...v0.1.37) - 2026-08-10

### Fixed

- *(web)* never navigate from outside the Dioxus runtime
- *(web)* stop the reset form reloading the page and dropping the token

### Other

- changelog for v0.1.36 [skip ci]
- *(e2e)* fresh-start regression spec — the path the canary never walked


## [0.1.36](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.35...v0.1.36) - 2026-08-09

### Added

- *(server)* open puzzle generation to players behind the Pro quota
- daily puzzle — game.getDaily with DailyPick persistence
- *(web)* Create-your-own tab on the pre-game view
- admin dashboard, Joined column, and mobile read-only mode

### Fixed

- *(mail)* point SMTP at Cloudflare Email Service
- *(mail)* send as noreply@noreply.casazza.io

### Other

- changelog for v0.1.35 [skip ci]


## [0.1.35](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.34...v0.1.35) - 2026-08-09

### Added

- *(web)* shared UI atoms, date/error helpers, and a global toast host
- *(web)* shared auth guard with return-to redirects, router-native login
- *(web)* shell chrome — nav-aware header, mobile tab bar, admin strip
- *(web)* shared ConfirmModal and Drawer
- *(web)* Next Up panel on game completion — rematch and next puzzle
- *(web)* confirm-gated discount delete, redemption bars, amount validation
- *(web)* public team directory, per-action team toasts, surfaced errors
- *(web)* admin users search, filters, detail drawer, set-password
- *(web)* generator max-attempts control, presets, jobs tooling
- *(server)* gridSize, gameId, and fill counts on gameList.get rows
- *(web)* games library redesign — Continue, Featured, Library, Progress
- *(server)* pre-start grid silhouette and completed-game solve time
- *(web)* pre-game brief with grid silhouette and co-op invite
- *(web)* signed-in home is a play-now dashboard
- *(web)* Your Result hero and share row on game completion
- *(server)* stats.getUserHistory — full per-user match history
- *(server)* resend-verification, change-password, real subscription cancel
- *(web)* stats depth — streaks, heatmap, bests, trends, match log
- *(web)* profile becomes the full account surface
- *(web)* static centered auth layout for all four auth pages
- *(ci)* sortable main-build image tags for continuous staging
- *(mail)* send over Workspace SMTP instead of Resend
- *(ci)* continuous staging via deploy-staging on every main build

### Fixed

- *(ci)* let the scheduled canary ride out transient staging blips

### Other

- changelog for v0.1.34 [skip ci]
- *(web)* tokenize every hardcoded color and delete rounded corners


## [0.1.34](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.33...v0.1.34) - 2026-08-09

### Other

- changelog for v0.1.33 [skip ci]


## [0.1.33](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.32...v0.1.33) - 2026-08-08

### Added

- *(web)* show Free/Pro pricing before sign-in

### Fixed

- *(ci)* give the release canary 45m for the image build + rollout

### Other

- changelog for v0.1.32 [skip ci]


## [0.1.32](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.31...v0.1.32) - 2026-07-28

### Added

- *(lobby)* shared game-list component with search, metadata, and a11y
- *(auth)* send verification emails and add a password-reset flow

### Fixed

- *(e2e)* unbreak the canary so the demo video publishes again
- *(ci)* test release canaries against what staging actually runs

### Other

- changelog for v0.1.31 [skip ci]


## [0.1.31](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.30...v0.1.31) - 2026-07-27

### Other

- changelog for v0.1.30 [skip ci]


## [0.1.30](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.29...v0.1.30) - 2026-07-27

### Fixed

- *(ci)* don't fail the build when auto-merge can't be enabled
- *(web)* content-address the wasm bundle so assets can't go stale

### Other

- changelog for v0.1.29 [skip ci]
- *(buildbot)* expose .#checks + add buildbot-nix per-repo config (#45)


## [0.1.29](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.28...v0.1.29) - 2026-07-26

### Fixed

- *(auth)* fail closed on weak secrets outside local; let seeded admins log in

### Other

- changelog for v0.1.28 [skip ci]


## [0.1.28](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.27...v0.1.28) - 2026-07-26

### Added

- *(auth)* confirm-password field on signup

### Fixed

- *(server)* stop CDNs pairing new wasm glue with stale snippets

### Other

- changelog for v0.1.27 [skip ci]


## [0.1.27](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.26...v0.1.27) - 2026-07-26

### Fixed

- *(ci)* never rebase the generated k8s manifest when bumping staging
- *(chart)* route every ingress path to service.port

### Other

- changelog for v0.1.26 [skip ci]


## [0.1.26](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.25...v0.1.26) - 2026-07-26

### Fixed

- *(game)* board grid clipping + compact Active Clue panel + mobile stats in demo (#42)
- *(game)* keep the board on camera while typing on mobile
- *(ci)* include the whole workspace in the release changelog
- *(ci)* generate release notes from the tag range, not the whole history


## [0.1.25](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.24...v0.1.25) - 2026-07-25

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.24](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.23...v0.1.24) - 2026-07-25

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.23](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.22...v0.1.23) - 2026-07-21

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.22](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.21...v0.1.22) - 2026-07-20

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.21](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.20...v0.1.21) - 2026-07-20

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.19](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.18...v0.1.19) - 2026-07-20

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.18](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.17...v0.1.18) - 2026-07-19

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.17](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.16...v0.1.17) - 2026-07-19

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.16](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.15...v0.1.16) - 2026-07-19

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.15](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.14...v0.1.15) - 2026-07-19

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.14](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.13...v0.1.14) - 2026-07-19

### Added

- *(coop)* join-by-link invites + live per-player presence on the board
- *(games)* platform game ownership + weekly seed CronJob
- *(app)* APP_ENV-driven runtime config + feature flags
- *(billing)* port Lemon Squeezy webhook so purchases grant Pro
- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount
- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard
- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out

## [0.1.9](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.8...v0.1.9) - 2026-07-17

### Added

- *(coop)* join-by-link invites + live per-player presence on the board

## [0.1.7](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.6...v0.1.7) - 2026-07-03

### Fixed

- *(games)* clean platform game titles + exclude Platform user from leaderboard

## [0.1.6](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.5...v0.1.6) - 2026-07-03

### Fixed

- *(security)* scope stats player list + head-to-head to teammates
- *(security)* close prod auth backdoors + IDOR, harden payments/teams (pre-prod audit)

## [0.1.5](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.4...v0.1.5) - 2026-07-03

### Added

- *(games)* platform game ownership + weekly seed CronJob

## [0.1.4](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.3...v0.1.4) - 2026-07-02

### Added

- *(app)* APP_ENV-driven runtime config + feature flags

## [0.1.3](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.2...v0.1.3) - 2026-07-02

### Added

- *(billing)* port Lemon Squeezy webhook so purchases grant Pro

## [0.1.1](https://github.com/olivecasazza/definitely-not-crosswords/compare/v0.1.0...v0.1.1) - 2026-07-01

### Added

- *(staging)* beta banner + bug-report link, and port Pro checkout with env discount

## [0.1.0](https://github.com/olivecasazza/definitely-not-crosswords/releases/tag/v0.1.0) - 2026-06-30

### Added

- *(server)* build crossword-server in the nix flake via a vendored onnxruntime
- *(server)* serve the wasm frontend single-origin (WEB_DIST)
- *(desktop)* add Tauri desktop crate + fix flake to build it
- *(server)* port ONNX crossword generator to Rust
- *(backend)* next-auth login endpoints — Rust can issue session cookies
- *(backend)* tRPC WebSocket subscriptions — live multiplayer on Rust
- *(backend)* port all tRPC routers to Rust (sqlx) — verified vs Postgres
- *(backend)* wire JWE auth + /api/auth/session + router-module fan-out
- *(backend)* Rust tRPC server slice — Axum + sqlx, proven end-to-end

### Other

- *(backend)* add port deps (uuid, scrypt, reqwest, chrono) for router fan-out
