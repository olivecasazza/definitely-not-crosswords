#!/usr/bin/env bash
# Verify a DEPLOYED crossword build by reading the artifact it actually serves.
#
# Per DEF-110 a green deploy run is not a health signal, and the acceptance
# criteria for a release are about the served bytes, not the badge. This script
# is the executable form of those criteria: it reads the live site and fails
# loudly if the release is not really in front of users.
#
# Usage: scripts/verify-release-copy.sh [BASE_URL] [EXPECTED_VERSION]
#   BASE_URL         default https://crosswords.casazza.io
#   EXPECTED_VERSION default: read from the deployed Cargo.toml on disk, i.e.
#                    "whatever this checkout would tag". Pass it explicitly to
#                    assert a specific release instead.
#
#   EXPECTED_PRO_CHECKOUT=true|false  optional. Unset (the default) either
#                    proCheckout value passes, which is what a release gate
#                    wants. Set it to pin one host to one answer, e.g.
#                    EXPECTED_PRO_CHECKOUT=false scripts/verify-release-copy.sh
#
# Two non-obvious things this has to get right, both learned the hard way:
#
#   1. The copy lives in the .wasm, NOT in the .js. index.html loads
#      /_assets/<hash>/crossword-web.js, but that file is only the wasm-bindgen
#      glue (79 KB); it references crossword-web_bg.wasm next to it via
#      `new URL('crossword-web_bg.wasm', import.meta.url)`. Every Rust string
#      literal is in that .wasm (3.3 MB). Grepping the .js finds none of them
#      and reports a perfectly healthy release as missing all its copy.
#
#   2. <hash> is a content hash, so it changes on every build. It cannot be
#      hardcoded; it is discovered from the served index.html each run.
set -euo pipefail

BASE_URL="${1:-https://crosswords.casazza.io}"
BASE_URL="${BASE_URL%/}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
EXPECTED_VERSION="${2:-$(sed -nE 's/^version = "([^"]+)"$/\1/p' "$REPO_ROOT/client/Cargo.toml" | head -1)}"

if [ -z "$EXPECTED_VERSION" ]; then
  echo "::error::could not read the workspace version from client/Cargo.toml; pass it as \$2" >&2
  exit 2
fi

if [ -n "${EXPECTED_PRO_CHECKOUT:-}" ] \
  && [ "$EXPECTED_PRO_CHECKOUT" != true ] \
  && [ "$EXPECTED_PRO_CHECKOUT" != false ]; then
  echo "::error::EXPECTED_PRO_CHECKOUT must be true or false, got '$EXPECTED_PRO_CHECKOUT'" >&2
  exit 2
fi

fail=0
fail() { echo "  FAIL  $*"; fail=1; }
pass() { echo "  ok    $*"; }

echo "Verifying $BASE_URL serves release $EXPECTED_VERSION"

# --- version ---------------------------------------------------------------
config="$(curl -fsS --max-time 30 "$BASE_URL/api/config")" || {
  echo "::error::GET $BASE_URL/api/config failed — the site is not answering at all" >&2
  exit 1
}
served="$(printf '%s' "$config" | sed -nE 's/.*"version":"([^"]*)".*/\1/p')"
echo
echo "GET /api/config -> $config"
if [ "$served" = "$EXPECTED_VERSION" ]; then
  pass "/api/config version is $served"
else
  fail "/api/config reports version '${served:-<none>}', want '$EXPECTED_VERSION'"
fi

# The Pro CTA gate (DEF-166). The bundle hides the purchase button on
# `features.proCheckout === false`, which the server derives from the same
# `billing.lemonSqueezy.enabled` flag that gates credential injection. A
# release whose /api/config lost the flag ships a bundle that can no longer
# describe whether buying Pro is possible, so assert the deployed answer
# rather than trusting the diff.
#
# The served .wasm is not evidence of this flag, and the copy assertions below
# cannot stand in for it: both copy variants are compiled into one binary and
# the branch between them is a runtime read of /api/config, so the same bundle
# serves either value. Only /api/config can say which one this host returns.
#
# EXPECTED_PRO_CHECKOUT is optional. Unset — the default, and what the release
# gate uses — either value passes, because a release may legitimately ship
# with billing on or off. Set it to pin one host to one answer (DEF-199).
pro_checkout="$(printf '%s' "$config" | grep -oE '"proCheckout":(true|false)' | head -1 || true)"
if [ -z "$pro_checkout" ]; then
  fail "/api/config has no boolean proCheckout feature flag — the Pro CTA cannot be gated"
elif [ -n "${EXPECTED_PRO_CHECKOUT:-}" ]; then
  if [ "$pro_checkout" = "\"proCheckout\":$EXPECTED_PRO_CHECKOUT" ]; then
    pass "/api/config proCheckout matches expected $EXPECTED_PRO_CHECKOUT ($pro_checkout)"
  else
    fail "$BASE_URL /api/config proCheckout is ${pro_checkout#'"proCheckout":'}, want $EXPECTED_PRO_CHECKOUT"
  fi
else
  pass "/api/config exposes boolean proCheckout ($pro_checkout)"
fi

# --- locate the served wasm ------------------------------------------------
index="$(curl -fsS --max-time 30 "$BASE_URL/")" || {
  echo "::error::GET $BASE_URL/ failed" >&2
  exit 1
}
# Hex-hash only, and never the first match blindly: the served index.html
# carries a literal "/_assets/<64-hex>/crossword-web.js" inside the boot-card
# comment (client/flake.nix's BOOT CSS note), and an unanchored first-match
# grep fetched that placeholder and 404'd on every run.
glue="$(printf '%s' "$index" | grep -o '/_assets/[0-9a-f]\{16,\}/crossword-web\.js' | head -1 || true)"
if [ -z "$glue" ]; then
  echo "::error::no /_assets/*/crossword-web.js in $BASE_URL/ — the bundle layout changed; update this script" >&2
  exit 1
fi
wasm_url="$BASE_URL$(dirname "$glue")/crossword-web_bg.wasm"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fsS --max-time 300 -o "$tmp/app.wasm" "$wasm_url" || {
  echo "::error::GET $wasm_url failed" >&2
  exit 1
}
echo
echo "wasm $wasm_url ($(wc -c < "$tmp/app.wasm") bytes)"

# --- copy assertions -------------------------------------------------------
# The approved home copy (DEF-102) and the priced Pro CTA must be in the binary.
echo
echo "copy that must be present:"
while IFS= read -r s; do
  [ -n "$s" ] || continue
  if grep -qaF -- "$s" "$tmp/app.wasm"; then pass "$s"; else fail "missing: $s"; fi
done <<'STRINGS'
Solve the same grid together
Team solve stats
See how fast your crew finishes, together.
Upgrade to Pro — $10/year
STRINGS

# The priced CTA and the not-yet-purchasable state must BOTH ship (DEF-166).
# `features.proCheckout === false` removes the purchase control, so the copy
# that explains why the price is announced but not buyable has to be in the
# bundle too — otherwise the gate trades a dead button for a silent gap.
if grep -qaF -- "teams of 10. Opening soon." "$tmp/app.wasm"; then
  pass "Pro is described as opening soon where checkout is unavailable"
else
  fail "missing: the Pro \"Opening soon\" fallback copy"
fi

# Superseded copy must be gone. LAUNCH50 is exempt: the admin discount field
# placeholder is still customer-reachable for staff, and the second occurrence
# in web/src/components/ui.rs is a source comment. Neither is a regression.
echo
echo "superseded copy that must be absent:"
for s in "Race the grid" "Stats & rankings"; do
  if grep -qaF -- "$s" "$tmp/app.wasm"; then fail "still present: $s"; else pass "absent: $s"; fi
done
n_launch50="$(grep -caF -- "LAUNCH50" "$tmp/app.wasm" || true)"
if [ "$n_launch50" -le 2 ]; then
  pass "LAUNCH50 occurrences: $n_launch50 (<=2 sanctioned: admin placeholder + ui.rs comment)"
else
  fail "LAUNCH50 occurrences: $n_launch50 — a customer-facing discount field appears to be back"
fi

echo
if [ "$fail" -ne 0 ]; then
  echo "::error::$BASE_URL does NOT match release $EXPECTED_VERSION — see the FAIL lines above."
  exit 1
fi
echo "$BASE_URL is serving release $EXPECTED_VERSION with the approved copy."
