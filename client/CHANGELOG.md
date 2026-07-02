# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
