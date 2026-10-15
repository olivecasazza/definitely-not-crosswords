#!/bin/bash
# Run a cargo command in the `client/` workspace on a runner where the default
# toolchain cannot build it. Works around two environmental facts:
#
#   1. `client/web/Cargo.toml` pins panel-kit and panel-kit-core as git deps on
#      `github.com/olivecasazza/panel-kit.git`, tag `v1.1.1`. That resolves on
#      its own, so it does not break the manifest — it just means cargo fetches
#      panel-kit itself, on its own schedule, ignoring the revision this repo
#      pins. The pin the runner checks (fetch-panel-kit.sh) and the code the
#      build compiles are then two different facts. So both git deps are
#      temporarily rewritten into path deps at the real panel-kit, checked out at
#      the revision `client/Cargo.lock` pins (see `fetch-panel-kit.sh` and
#      `panel-kit-pin.sh`) — the same file cargo resolves, so the two cannot
#      disagree (DEF-330; it used to read client/flake.nix, which could). If that
#      checkout is unavailable — offline, no git — they fall back to the
#      manifest-only shim in `panel-kit-shim/`, which loads the manifest but
#      cannot type-check anything. `[patch]` does not help: cargo resolves
#      patches after the manifest is loaded, and the dep here has to resolve to
#      *a* directory first.
#   2. `/paperclip/bin/cc` -> `zigcc` appends link flags unconditionally, so zig
#      tries to link even for `-c` and fails. `./cc` in this directory fixes that.
#
# `web/Cargo.toml` and `Cargo.lock` are backed up and restored on exit, whatever
# cargo does, so the tree is left clean.
#
# Usage:  scripts/runner-toolchain/with-cargo.sh <cargo subcommand> [args...]
# Example:
#   scripts/runner-toolchain/with-cargo.sh nextest run -p crossword-server
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SHIM="$HERE/panel-kit-shim"
SUBCMD="${1:?usage: with-cargo.sh <cargo subcommand> [args...]}"
shift

# Prefer the real panel-kit at the lock-pinned rev: that is the only way
# `crossword-web`/`crossword-desktop` can be type-checked here. The shim is the
# offline fallback, and it is not panel-kit — say so rather than let a caller
# believe a shim build verified the frontend.
PANEL_KIT="$(bash "$HERE/fetch-panel-kit.sh" 2>/dev/null)" || PANEL_KIT=""
if [ -n "$PANEL_KIT" ]; then
  echo "panel-kit: $PANEL_KIT" >&2
else
  if [ ! -d "$SHIM" ]; then
    echo "missing panel-kit shim at $SHIM" >&2
    exit 1
  fi
  echo "panel-kit: unavailable, falling back to the manifest-only shim at $SHIM" >&2
  echo "panel-kit: crossword-web and crossword-desktop CANNOT be type-checked" >&2
  PANEL_KIT="$SHIM"
fi

# Locate the client/ workspace root (the dir holding the workspace Cargo.toml).
# Walk up from the cwd, then fall back to the copy next to this script.
CLIENT_DIR="$PWD"
while [ "$CLIENT_DIR" != "/" ] && [ ! -f "$CLIENT_DIR/web/Cargo.toml" ]; do
  CLIENT_DIR="$(dirname "$CLIENT_DIR")"
done
if [ ! -f "$CLIENT_DIR/web/Cargo.toml" ] && [ "$CLIENT_DIR" = "/" ]; then
  CLIENT_DIR="$(cd "$HERE/../../client" && pwd)"
fi
if [ ! -f "$CLIENT_DIR/web/Cargo.toml" ]; then
  echo "could not find the client/ workspace at or above $PWD" >&2
  exit 1
fi
if [ ! -f "$CLIENT_DIR/web/Cargo.toml" ]; then
  echo "$CLIENT_DIR is not the client/ workspace (no web/Cargo.toml)" >&2
  exit 1
fi

RUNDIR="$(mktemp -d)"
trap 'rm -rf "$RUNDIR"' EXIT

cp "$CLIENT_DIR/web/Cargo.toml" "$RUNDIR/web-Cargo.toml.orig"
if [ -f "$CLIENT_DIR/Cargo.lock" ]; then
  cp "$CLIENT_DIR/Cargo.lock" "$RUNDIR/Cargo.lock.orig"
fi

restore() {
  cp "$RUNDIR/web-Cargo.toml.orig" "$CLIENT_DIR/web/Cargo.toml"
  if [ -f "$RUNDIR/Cargo.lock.orig" ]; then
    cp "$RUNDIR/Cargo.lock.orig" "$CLIENT_DIR/Cargo.lock"
  fi
}
trap restore EXIT

REL="$(realpath --relative-to="$CLIENT_DIR/web" "$PANEL_KIT")"
PK_REL="$REL/crates/panel-kit-core"

# Since DEF-244 the deps are git deps, not path deps, so the old
# `/home/olive/Repositories/panel-kit` repoint matched nothing and cargo went on
# to resolve panel-kit itself — the checkout we just fetched was never compiled
# against. Rewrite each `panel-kit* = { git = "<url>"… }` dep into a path dep at
# the fetched checkout, so the revision `fetch-panel-kit.sh` resolved is the one
# cargo builds. The tag is matched with `[^}]*` rather than spelled out so that
# bumping the pin in web/Cargo.toml cannot silently turn this into another no-op.
#
# `-core` is listed first and both patterns are `^`-anchored: `panel-kit =` must
# not be allowed to match the head of `panel-kit-core =`.
sed -i -E \
  -e "s#^panel-kit-core = \{ git = \"[^\"]*panel-kit\.git\"[^}]*\}\$#panel-kit-core = { path = \"$PK_REL\" }#" \
  -e "s#^panel-kit = \{ git = \"[^\"]*panel-kit\.git\"[^}]*\}\$#panel-kit = { path = \"$REL\" }#" \
  "$CLIENT_DIR/web/Cargo.toml"

# Both deps must now be path deps. A leftover `git =` means the pattern drifted
# from the manifest again — cargo would resolve panel-kit itself and quietly build
# something other than the pinned checkout, so fail loudly instead.
if grep -qE '^(panel-kit|panel-kit-core) = \{.*git =' "$CLIENT_DIR/web/Cargo.toml"; then
  echo "repointing panel-kit failed; web/Cargo.toml still declares a panel-kit git dep" >&2
  exit 1
fi
# And both must point where we said they point. The grep above only proves the
# git dep is gone; this proves the checkout actually took.
if ! grep -qE "^panel-kit = \{ path = \"$REL\" \}\$" "$CLIENT_DIR/web/Cargo.toml" ||
  ! grep -qE "^panel-kit-core = \{ path = \"$PK_REL\" \}\$" "$CLIENT_DIR/web/Cargo.toml"; then
  echo "repointing panel-kit failed; expected panel-kit deps at $REL in web/Cargo.toml" >&2
  grep -nE '^panel-kit(-core)? = ' "$CLIENT_DIR/web/Cargo.toml" >&2 || true
  exit 1
fi

export PATH="$HERE:/paperclip/bin:$HOME/.cargo/bin:$PATH"
# `ort` static-links the vendored libonnxruntime.a; without this the link fails
# with `undefined symbol: OrtGetApiBase`. See client/flake.nix for the nix
# shell's equivalent.
export ORT_LIB_LOCATION="${ORT_LIB_LOCATION:-/paperclip/ort-lib}"
export ORT_SKIP_DOWNLOAD="${ORT_SKIP_DOWNLOAD:-1}"

cd "$CLIENT_DIR" || exit 1
cargo "$SUBCMD" "$@"
