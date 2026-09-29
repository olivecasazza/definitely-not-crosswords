// Manifest-only stand-in for the panel-kit crate. See ../README.md: this exists
// so `client/web/Cargo.toml`'s absolute path dep can be repointed at
// something that loads in a container with no nix dev shell. It is not panel-kit
// and must never be used to build `crossword-web` or `crossword-desktop`.
