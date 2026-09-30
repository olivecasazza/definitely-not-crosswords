#!/bin/bash
# Materialise the real panel-kit at the revision `flake.nix` pins, so
# `crossword-web` and `crossword-desktop` can actually be type-checked in this
# container. Prints the checkout path on stdout; diagnostics on stderr.
#
# The revision is read out of the flake rather than duplicated here, so the
# runner and the nix build can never drift apart. `client/flake.nix` vendors
# the same input into the workspace source for the same reason.
#
# The checkout is cached per revision under
# ${PANEL_KIT_CACHE_DIR:-~/.cache/runner-toolchain} and reused, so repeat runs
# cost nothing. Set PANEL_KIT_CACHE_DIR to move it.
#
# Exit codes: 0 = a usable checkout is on stdout, 1 = unavailable (offline, no
# git, rev moved). Callers fall back to the manifest-only shim.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
# CLIENT_FLAKE exists so the test can point at the real flake while running a
# copy of this script out of a temp dir; see test-panel-kit.sh.
FLAKE="${CLIENT_FLAKE:-$REPO_ROOT/client/flake.nix}"

if [ ! -f "$FLAKE" ]; then
  echo "fetch-panel-kit: no flake.nix at $FLAKE" >&2
  exit 1
fi

# `panel-kit.url = "github:olivecasazza/panel-kit/<rev>";` — the rev is the
# last path segment. Anchored to the panel-kit input so a future flake input
# with a similar URL cannot be picked up by accident.
REV="$(sed -n 's#.*panel-kit\.url = "github:olivecasazza/panel-kit/\([0-9a-f]\{40\}\)".*#\1#p' "$FLAKE")"
if [ -z "$REV" ]; then
  echo "fetch-panel-kit: could not read the pinned panel-kit rev from $FLAKE" >&2
  exit 1
fi

CACHE_ROOT="${PANEL_KIT_CACHE_DIR:-$HOME/.cache/runner-toolchain}"
DEST="$CACHE_ROOT/panel-kit-$REV"

# Already at the right revision: reuse it. `git rev-parse` also fails on a
# half-written checkout, which sends us down the rebuild path.
if [ -d "$DEST/.git" ] && [ "$(git -C "$DEST" rev-parse HEAD 2>/dev/null)" = "$REV" ]; then
  echo "$DEST"
  exit 0
fi

command -v git >/dev/null 2>&1 || {
  echo "fetch-panel-kit: git is not on PATH" >&2
  exit 1
}

# Rebuild into a sibling temp dir and rename into place, so a concurrent run or
# an interrupted fetch never leaves a half-populated cache entry behind.
mkdir -p "$CACHE_ROOT"
# git clone insists on a non-existent destination; mktemp hands us an existing
# empty dir, so clone one level down.
STAGE="$(mktemp -d "$CACHE_ROOT/.panel-kit-XXXXXX")"
CLONE="$STAGE/repo"
cleanup() { rm -rf "$STAGE"; }
trap cleanup EXIT

if ! git clone --quiet --no-checkout https://github.com/olivecasazza/panel-kit.git "$CLONE" 2>"$STAGE/clone.log"; then
  echo "fetch-panel-kit: clone failed:" >&2
  cat "$STAGE/clone.log" >&2
  exit 1
fi

# A shallow fetch of one rev keeps the checkout small; --depth 1 on a specific
# commit needs the server to allow it, so fall back to a full shallow clone.
if ! git -C "$CLONE" fetch --quiet --depth 1 origin "$REV" 2>"$STAGE/fetch.log"; then
  echo "fetch-panel-kit: shallow fetch of $REV failed, retrying full:" >&2
  cat "$STAGE/fetch.log" >&2
  rm -rf "$STAGE"
  STAGE="$(mktemp -d "$CACHE_ROOT/.panel-kit-XXXXXX")"
  CLONE="$STAGE/repo"
  if ! git clone --quiet https://github.com/olivecasazza/panel-kit.git "$CLONE" 2>"$STAGE/clone.log"; then
    echo "fetch-panel-kit: clone failed:" >&2
    cat "$STAGE/clone.log" >&2
    exit 1
  fi
fi

if ! git -C "$CLONE" checkout --quiet "$REV" 2>"$STAGE/checkout.log"; then
  echo "fetch-panel-kit: checkout of $REV failed:" >&2
  cat "$STAGE/checkout.log" >&2
  exit 1
fi

# Sanity-check that this is really panel-kit and not an empty tree, so a
# surprising upstream layout fails here rather than as 28 opaque rustc errors
# an hour into a build.
for f in Cargo.toml crates/panel-kit-core/Cargo.toml src/lib.rs; do
  if [ ! -f "$CLONE/$f" ]; then
    echo "fetch-panel-kit: $f missing from the panel-kit checkout" >&2
    exit 1
  fi
done

rm -rf "$DEST"
mv "$CLONE" "$DEST"
trap - EXIT

echo "$DEST"
