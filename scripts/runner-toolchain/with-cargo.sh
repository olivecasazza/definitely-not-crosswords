#!/bin/bash
# Run a cargo command in the `client/` workspace on a runner where the default
# toolchain cannot build it. Works around two environmental facts:
#
#   1. `client/web/Cargo.toml` pins panel-kit and panel-kit-core at the absolute
#      release-plz host path (/home/olive/Repositories/panel-kit), which does not
#      exist here, so cargo cannot even load the workspace manifest. Both path
#      deps are temporarily repointed at the manifest-only shim in
#      `panel-kit-shim/` (same crate names/versions as `client/Cargo.lock`).
#      `[patch]` does not help: cargo fails while loading the manifest, before
#      patch resolution.
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

if [ ! -d "$SHIM" ]; then
  echo "missing panel-kit shim at $SHIM" >&2
  exit 1
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

REL="$(realpath --relative-to="$CLIENT_DIR/web" "$SHIM")"
sed -i \
  -e "s#path = \"/home/olive/Repositories/panel-kit/crates/panel-kit-core\"#path = \"$REL/crates/panel-kit-core\"#" \
  -e "s#path = \"/home/olive/Repositories/panel-kit\"#path = \"$REL\"#" \
  "$CLIENT_DIR/web/Cargo.toml"

export PATH="$HERE:/paperclip/bin:$HOME/.cargo/bin:$PATH"
# `ort` static-links the vendored libonnxruntime.a; without this the link fails
# with `undefined symbol: OrtGetApiBase`. See client/flake.nix for the nix
# shell's equivalent.
export ORT_LIB_LOCATION="${ORT_LIB_LOCATION:-/paperclip/ort-lib}"
export ORT_SKIP_DOWNLOAD="${ORT_SKIP_DOWNLOAD:-1}"

cd "$CLIENT_DIR" || exit 1
cargo "$SUBCMD" "$@"
