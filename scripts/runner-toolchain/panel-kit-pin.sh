#!/bin/bash
# THE panel-kit revision, read from `client/Cargo.lock` — the one file cargo
# itself resolves. Prints `<rev> <url>` on one line; diagnostics on stderr.
#
# WHY THE LOCK AND NOT client/flake.nix (DEF-330).
#
# Three files used to name one revision:
#
#   client/web/Cargo.toml  the git dep + `tag = "v1.1.1"` a developer reads
#   client/Cargo.lock      the `rev` cargo actually resolves and COMPILES
#   client/flake.nix       the `rev` the runner toolchain checked out
#
# Nothing reconciled them. `fetch-panel-kit.sh` sedded the flake, so the runner
# checked out one revision while `cargo build` compiled another — measured
# before this change: with the flake pinned at 7517e673 and the lock at
# 503f46c3, `test-panel-kit.sh` printed PASS and exited 0. That is the same
# class of bug DEF-327 fixed one layer down (the runner fetching a revision
# nothing compiled), reintroduced one layer up, invisible to every test.
#
# Reading the lock makes the two the same fact BY CONSTRUCTION: this resolver
# is the only place a revision is looked up, both `fetch-panel-kit.sh` and
# `test-panel-kit.sh` call it, and it is derived from the file cargo reads.
# There is no second copy left to drift.
#
# The `panel-kit` flake input that used to carry a third copy is DELETED
# (DEF-330). `nix` was never reading it — DEF-324 removed the only consumer,
# the `vendor-panel-kit` copy in `src` — so it was inert for the build and
# load-bearing only for this sed. Bumping the pin is now a one-line change in
# web/Cargo.toml plus a `cargo update`, with no nix side to keep in step.
#
# Usage:  panel-kit-pin.sh            # resolve against the repo's own lock
#         CLIENT_LOCK=… panel-kit-pin.sh
# Exit codes: 0 = resolved, 1 = not resolvable (no lock, no entry, >1 distinct
# rev). Callers fall back to the manifest-only shim.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
# CLIENT_LOCK exists so the test can exercise this resolver against a lockfile
# it controls, without touching the real one; see test-panel-kit.sh.
LOCK="${CLIENT_LOCK:-$REPO_ROOT/client/Cargo.lock}"

if [ ! -f "$LOCK" ]; then
  echo "panel-kit-pin: no Cargo.lock at $LOCK" >&2
  exit 1
fi

# Cargo.lock TOML: a `[[package]]` table per resolved crate, and the git source
# is the `source` line inside it —
#
#   [[package]]
#   name = "panel-kit"
#   version = "1.1.1"
#   source = "git+https://github.com/olivecasazza/panel-kit.git?tag=v1.1.1#<40 hex>"
#
# Both `panel-kit` and `panel-kit-core` carry it (two crates, one repository),
# so the source line is keyed to the table's own `name =` rather than matched
# as a bare string. Anchored to `name = "panel-kit"` with `(-core)?` so neither
# `panel-kit` nor `panel-kit-core` can be mistaken for a future
# `panel-kit-extra`, and anchored with `[0-9a-f]{40}` so a `?branch=` source (no
# `#rev`) or a registry source is not silently accepted as a pinned commit.
#
# awk, not sed: `source` may legally sit either before or after `name` inside
# the same table, and only the table that named itself panel-kit counts. sed
# cannot see table boundaries; awk can. Reads the whole lock once.
read_rev() {
  awk -v want="^name = \"panel-kit(-core)?\"$" '
    /^\[\[package\]\]$/ { in_pkg = 1; name = ""; src = ""; next }
    in_pkg && /^name = /  && name == "" { name = $0 }
    in_pkg && /^source = / && src == "" { src = $0 }
    in_pkg && name ~ want && src != "" {
      if (match(src, /#[0-9a-f]{40}"/)) print substr(src, RSTART + 1, 40)
      else print ""
      in_pkg = 0
    }
  ' "$LOCK" | sort -u
}

REVS="$(read_rev || true)"
if [ -z "$REVS" ]; then
  echo "panel-kit-pin: could not read a panel-kit rev out of $LOCK" >&2
  exit 1
fi

# Two DIFFERENT revs would mean the lock itself pins two panel-kit commits —
# cargo resolves one of them and we would be guessing which. Refuse rather than
# pick the first line; this is a broken lockfile, not something to paper over.
COUNT="$(printf '%s\n' "$REVS" | grep -c . || true)"
if [ "$COUNT" -ne 1 ]; then
  echo "panel-kit-pin: $LOCK names $COUNT distinct panel-kit revs:" >&2
  printf '%s\n' "$REVS" | sed 's/^/  /' >&2
  echo "panel-kit-pin: refusing to guess which one cargo resolves" >&2
  exit 1
fi

# The git URL, from the same `source` line, minus the `git+` prefix cargo adds
# and the `?query#rev` suffix. Checked out by clone URL, so a `?tag=`/`?branch=`
# query on the lock entry must not leak into it.
URL="$(awk -v want="^name = \"panel-kit(-core)?\"$" '
  /^\[\[package\]\]$/ { in_pkg = 1; name = ""; src = ""; next }
  in_pkg && /^name = /  && name == "" { name = $0 }
  in_pkg && /^source = / && src == "" { src = $0 }
  in_pkg && name ~ want && src != "" {
    line = src
    sub(/^source = "/, "", line)
    sub(/#.*$/, "", line)
    sub(/\?.*$/, "", line)
    sub(/^git\+/, "", line)
    print line
    in_pkg = 0
  }
' "$LOCK" | sort -u | head -n1)"

case "$URL" in
  https://* | http://* | ssh://* | git@* | /*) ;;
  *)
    echo "panel-kit-pin: refused to clone a non-URL source from $LOCK: $URL" >&2
    exit 1
    ;;
esac

echo "$REVS $URL"