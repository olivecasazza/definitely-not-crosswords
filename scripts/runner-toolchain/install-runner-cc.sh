#!/bin/bash
# Wire this repo's fixed `cc` wrapper into the runner's default PATH, so a fresh
# agent session gets it with no manual export. This is the fix for DEF-325.
#
# The defect: /paperclip/bin/cc is a symlink to /paperclip/bin/zigcc, which appends
# `-l:libstdc++.so.6 -lm -ldl -lpthread` unconditionally, so zig attempts a link
# even for `-c` and every C dependency in the workspace fails:
#
#   ld.lld: error: undefined symbol: main
#   ld.lld: error: attempted static link of dynamic object .../libstdc++.so.6
#
# A correct wrapper was already sitting in ~/.cache/runner-toolchain/bin, which is
# NOT on the default PATH, so every agent that did not know to export it hit the
# broken compiler. Rather than fix zigcc in place, this repoints the `cc`/`gcc`/
# `c++`/`g++` entries in /paperclip/bin at the wrapper this repo owns, so the
# behaviour is reproducible from a committed script and survives a cache wipe.
#
# Why /paperclip/bin and not PATH ordering: it is already first on PATH in every
# session, and it is a writable Longhorn PVC rather than an image layer, so the
# change persists across runs without needing the runner image rebuilt. The old
# zigcc is left on disk untouched, so this is reversible.
#
# Idempotent: re-running reports "already installed" and exits 0.
#
# Usage:  scripts/runner-toolchain/install-runner-cc.sh [--check|--uninstall]
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="${RUNNER_BIN_DIR:-/paperclip/bin}"
# Resolve the wrapper rather than using $HERE/cc by path, so the installed copy
# stays a symlink to the repo's wrapper: update the repo, re-run, done.
WRAPPER="$HERE/cc"
NAMES="cc gcc c++ g++"
OLD_TARGET="/paperclip/bin/zigcc"

mode="install"
case "${1:-}" in
  --check) mode="check" ;;
  --uninstall) mode="uninstall" ;;
  "") ;;
  *)
    echo "usage: $0 [--check|--uninstall]" >&2
    exit 2
    ;;
esac

fail() {
  echo "install-runner-cc: $*" >&2
  exit 1
}

# `command -v cc` must resolve to the wrapper. This is the acceptance criterion,
# checked the way a fresh session sees it.
verify() {
  local resolved
  resolved="$(command -v cc 2>/dev/null || true)"
  if [ -z "$resolved" ]; then
    fail "cc is not on PATH at all; PATH=$PATH"
  fi
  local real
  real="$(readlink -f "$resolved")"
  case "$real" in
    *zigcc*)
      fail "cc still resolves to the broken zigcc ($real)"
      ;;
  esac
  if [ "$real" != "$WRAPPER" ]; then
    echo "note: cc resolves to $real, not the repo wrapper $WRAPPER" >&2
  fi
  echo "cc -> $real"
}

if [ "$mode" = check ]; then
  verify
  echo "PASS: the runner's default cc is not zigcc"
  exit 0
fi

if [ "$mode" = uninstall ]; then
  for name in $NAMES; do
    target="$(readlink -f "$BIN_DIR/$name" 2>/dev/null || true)"
    case "$target" in
      "$WRAPPER" | "$HERE"/*)
        ln -sfn "$OLD_TARGET" "$BIN_DIR/$name"
        echo "restored $BIN_DIR/$name -> $OLD_TARGET"
        ;;
    esac
  done
  echo "Uninstalled. cc is back to $(readlink -f "$(command -v cc)" 2>/dev/null || echo unknown)"
  exit 0
fi

[ -x "$WRAPPER" ] || fail "wrapper $WRAPPER is missing or not executable"
[ -d "$BIN_DIR" ] || fail "$BIN_DIR does not exist"
[ -w "$BIN_DIR" ] || fail "$BIN_DIR is not writable by uid $(id -u); needs the runner operator"
[ -x "$OLD_TARGET" ] || fail "$OLD_TARGET is gone; cannot recognise what to replace"

# Refuse to clobber anything we did not install. If /paperclip/bin/cc is already
# the wrapper this is a no-op; if it is something else entirely (a real distro
# gcc, a future runner fix) then silently repointing it would be the wrong move.
current="$(readlink -f "$BIN_DIR/cc" 2>/dev/null || true)"
case "$current" in
  "$WRAPPER")
    echo "already installed: $BIN_DIR/cc -> $current"
    verify
    exit 0
    ;;
  "$OLD_TARGET") ;;
  "")
    fail "$BIN_DIR/cc does not exist; refusing to guess what should replace it"
    ;;
  *)
    fail "$BIN_DIR/cc resolves to $current, which this script did not install.
  Re-run with RUNNER_BIN_DIR pointing at a different directory, or fix /paperclip/bin by hand."
    ;;
esac

# Backup the original target once, so --uninstall is exact.
if [ ! -e "$BIN_DIR/cc.def325-orig" ]; then
  cp -P "$BIN_DIR/cc" "$BIN_DIR/cc.def325-orig"
  echo "backed up the original $BIN_DIR/cc -> $OLD_TARGET"
fi

for name in $NAMES; do
  # `c99` and `gcc` are symlinks in the same dir; repoint by name so whichever of
  # cc/gcc/c++/g++ a build system asks for lands on the wrapper. `gcc` is a
  # relative symlink to `cc`, so it follows automatically; the rest are not, and
  # must each be set.
  ln -sfn "$WRAPPER" "$BIN_DIR/$name"
  echo "installed $BIN_DIR/$name -> $WRAPPER"
done

# Only now assert it, through PATH, the way the next agent will see it.
verify

# And prove the whole point: the default cc, resolved through PATH with no
# export, compiles.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
printf 'int f(void) { return 0; }\n' >"$tmp/t.c"
if ! cc -c "$tmp/t.c" -o "$tmp/t.o" 2>"$tmp/err"; then
  echo "install-runner-cc: installed cc still fails to compile:" >&2
  cat "$tmp/err" >&2
  exit 1
fi
[ -f "$tmp/t.o" ] || fail "installed cc compiled but produced no object file"
echo "PASS: the runner's default cc now compiles -c cleanly"