#!/bin/bash
# Guards the bug runner-toolchain/fetch-panel-kit.sh exists to fix: the runner
# must be able to type-check crossword-web, which needs the REAL panel-kit, not
# the manifest-only shim. Exits non-zero on regression.
#
# This does not run cargo — a full `cargo check -p crossword-web` is minutes and
# belongs to the PR that changes this script. It checks the three things that
# actually break: the rev the runner resolves, that it is the rev cargo
# compiles, and that the checkout it points at is panel-kit rather than the shim.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
LOCK="$REPO_ROOT/client/Cargo.lock"
SHIM="$HERE/panel-kit-shim"

fail() { echo "FAIL: $*" >&2; exit 1; }

[ -f "$LOCK" ] || fail "no Cargo.lock at $LOCK"

# 1. The rev must be the one cargo actually resolves — the lock's, not a second
#    copy of it. `panel-kit-pin.sh` is the only thing in the toolchain that looks
#    a revision up, and both the fetch and this test read through it, so there is
#    no second place for a pin to be recorded and nothing to reconcile.
read -r PINNED _ < <(bash "$HERE/panel-kit-pin.sh") \
  || fail "panel-kit-pin.sh could not resolve a panel-kit rev from $LOCK"
[ -n "$PINNED" ] || fail "panel-kit-pin.sh resolved an empty rev from $LOCK"

# 2. DRIFT GUARD (DEF-330). The runner's rev and cargo's rev must be the same
#    fact. This used to read the rev out of client/flake.nix, so a flake pin that
#    disagreed with the lock passed CI green: measured before this change, a
#    flake pinned at 7517e673 against a lock pinned at 503f46c3 printed PASS and
#    exited 0, and the runner compiled a commit no `cargo build` would ever
#    produce. `panel-kit.url` no longer exists (DEF-330 deleted the input), so
#    there is no third copy to compare — but the pin it carried must not have
#    been left behind in the lock either. The lock is the single source of
#    truth, and these assert the shape that makes it usable: both crates named
#    panel-kit resolve from that one git repository, at that one rev.
#
# A `panel-kit` entry the lock does not resolve as a pinned git commit is the
# failure this whole change exists to prevent, so it is asserted rather than
# assumed: no registry source, no `?branch=` source with no `#rev`.
grep -qE "^source = \"git\+https://github\.com/olivecasazza/panel-kit\.git\?[^\"]*#$PINNED\"\$" "$LOCK" \
  || fail "client/Cargo.lock does not pin panel-kit at $PINNED from the panel-kit git repo"

# 3. fetch-panel-kit.sh must resolve to a checkout at exactly that rev.
GOT="$(bash "$HERE/fetch-panel-kit.sh")" || fail "fetch-panel-kit.sh exited non-zero"
[ -n "$GOT" ] || fail "fetch-panel-kit.sh printed no path"
[ -d "$GOT/.git" ] || fail "$GOT is not a git checkout"
[ "$(git -C "$GOT" rev-parse HEAD)" = "$PINNED" ] || fail "$GOT is at $(git -C "$GOT" rev-parse HEAD), Cargo.lock pins $PINNED"

# 4. It must be panel-kit, not the shim. The shim's lib.rs is one line; the
#    real crate is the whole point of this script.
LINES="$(wc -l <"$GOT/src/lib.rs")"
[ "$LINES" -gt 1 ] || fail "$GOT/src/lib.rs is $LINES line(s) — this is the shim, not panel-kit"
[ -f "$GOT/crates/panel-kit-core/Cargo.toml" ] || fail "$GOT has no crates/panel-kit-core/Cargo.toml"

# 5. with-cargo.sh must repoint web/Cargo.toml at the checkout while cargo runs,
#    and must leave the tree clean afterwards. The repoint is undone by a trap on
#    exit, so it cannot be asserted after the fact: run a copy of the toolchain
#    whose `cargo` is a stub that snapshots the manifest, then assert on that.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cp -r "$HERE" "$TMP/runner-toolchain"
cat >"$TMP/runner-toolchain/cargo" <<'EOF'
#!/bin/bash
# Stub: with-cargo.sh puts its own directory first on PATH, so this shadows the
# real cargo. It records what web/Cargo.toml looked like mid-run, then exits 0
# without building anything.
cp "$PWD/web/Cargo.toml" "$SNAPSHOT"
echo "stub cargo: $*" >&2
EOF
chmod +x "$TMP/runner-toolchain/cargo"

mkdir -p "$TMP/client/web"
cp "$REPO_ROOT/client/web/Cargo.toml" "$TMP/client/web/Cargo.toml"
cp "$LOCK" "$TMP/client/Cargo.lock"

(cd "$TMP/client" && \
  CLIENT_LOCK="$LOCK" \
  PANEL_KIT_CACHE_DIR="${PANEL_KIT_CACHE_DIR:-$HOME/.cache/runner-toolchain}" \
  SNAPSHOT="$TMP/during-run.toml" \
  bash "$TMP/runner-toolchain/with-cargo.sh" check -p crossword-web) >"$TMP/out.log" 2>&1 \
  || { cat "$TMP/out.log" >&2; fail "with-cargo.sh exited non-zero"; }

[ -f "$TMP/during-run.toml" ] || fail "the cargo stub never ran, so nothing was asserted"

# No panel-kit git dep may survive the repoint. This is the assertion DEF-244
# invalidated: the old guard here only checked for the `/home/olive/...` host
# path, which stopped existing from that change, so it passed vacuously while
# `with-cargo.sh`'s sed matched nothing and cargo quietly resolved panel-kit
# itself. The leftover git dep is the rot; this is what catches it.
grep -qE '^panel-kit(-core)? = \{.*git =' "$TMP/during-run.toml" \
  && fail "web/Cargo.toml still declares a panel-kit git dep mid-run, so cargo would resolve panel-kit itself"
grep -q "path = \"[^\"]*panel-kit-$PINNED\"" "$TMP/during-run.toml" \
  || fail "web/Cargo.toml did not point at the pinned checkout mid-run"
grep -q "path = \"[^\"]*panel-kit-$PINNED/crates/panel-kit-core\"" "$TMP/during-run.toml" \
  || fail "panel-kit-core was not repointed at the pinned checkout"

# The real tree must be byte-identical to what git has: the trap restores it.
git -C "$REPO_ROOT" diff --quiet -- client/web/Cargo.toml client/Cargo.lock \
  || fail "with-cargo.sh left client/web/Cargo.toml or client/Cargo.lock modified"

echo "PASS: runner-toolchain resolves panel-kit $PINNED (the revision client/Cargo.lock pins) and repoints web/Cargo.toml at it"
