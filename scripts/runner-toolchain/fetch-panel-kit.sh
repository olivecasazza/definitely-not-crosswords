#!/bin/bash
# Materialise the real panel-kit at the revision `client/Cargo.lock` pins, so
# `crossword-web` and `crossword-desktop` can actually be type-checked in this
# container. Prints the checkout path on stdout; diagnostics on stderr.
#
# The revision comes from `panel-kit-pin.sh`, which reads `client/Cargo.lock` —
# the file cargo itself resolves. So the revision checked out here and the
# revision cargo compiles are the same fact by construction, with no second
# copy of the pin anywhere to drift (DEF-330). This used to read the rev out of
# `client/flake.nix` instead, which let the runner check out a different commit
# from the one cargo compiled; that flake input is now deleted outright.
#
# The checkout is cached per revision under
# ${PANEL_KIT_CACHE_DIR:-~/.cache/runner-toolchain} and reused, so repeat runs
# cost nothing. Set PANEL_KIT_CACHE_DIR to move it.
#
# Exit codes: 0 = a usable checkout is on stdout, 1 = unavailable (offline, no
# git, rev moved). Callers fall back to the manifest-only shim.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
# CLIENT_LOCK exists so the test can point the resolver at a lockfile it
# controls while running a copy of this script out of a temp dir; see
# test-panel-kit.sh. panel-kit-pin.sh defaults to the repo's own lock.
read -r REV URL < <(bash "$HERE/panel-kit-pin.sh" 2>/dev/null) || {
  echo "fetch-panel-kit: could not resolve the panel-kit pin from Cargo.lock" >&2
  exit 1
}
if [ -z "$REV" ] || [ -z "$URL" ]; then
  echo "fetch-panel-kit: panel-kit-pin.sh resolved no rev/url from Cargo.lock" >&2
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

if ! git clone --quiet --no-checkout "$URL" "$CLONE" 2>"$STAGE/clone.log"; then
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
  if ! git clone --quiet "$URL" "$CLONE" 2>"$STAGE/clone.log"; then
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
