# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
