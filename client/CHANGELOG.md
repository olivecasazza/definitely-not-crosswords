# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
