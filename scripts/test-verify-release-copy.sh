#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/verify-release-copy.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

unset EXPECTED_PRO_CHECKOUT

STUB_VERSION="0.0.0-test"
STUB_BASE="https://stub.invalid"

export STUB_VERSION
export STUB_INDEX='<html><head><script type="module" src="/_assets/deadbeef/crossword-web.js"></script></head></html>'
export STUB_WASM='fake wasm
Solve the same grid together
Team solve stats
See how fast your crew finishes, together.
Upgrade to Pro — $10/year
teams of 10. Opening soon.
LAUNCH50
'

mkdir -p "$WORK/bin"
cat > "$WORK/bin/curl" <<'STUB'
#!/usr/bin/env bash
out=""
url=""
want_out=0
for a in "$@"; do
  if [ "$want_out" = 1 ]; then
    out="$a"
    want_out=0
    continue
  fi
  case "$a" in
    -o) want_out=1 ;;
    -*) ;;
    *) url="$a" ;;
  esac
done
case "$url" in
  */api/config)
    if [ -n "${STUB_NO_FLAG:-}" ]; then
      printf '{"version":"%s"}' "$STUB_VERSION"
    else
      printf '{"version":"%s","proCheckout":%s}' "$STUB_VERSION" "$STUB_PRO_CHECKOUT"
    fi
    ;;
  *crossword-web_bg.wasm) printf '%s' "$STUB_WASM" > "$out" ;;
  *) printf '%s' "$STUB_INDEX" ;;
esac
STUB
chmod +x "$WORK/bin/curl"
export PATH="$WORK/bin:$PATH"

rc=0
out=""

run() {
  STUB_PRO_CHECKOUT="$1"
  export STUB_PRO_CHECKOUT
  if [ -n "$2" ]; then
    out="$(EXPECTED_PRO_CHECKOUT="$2" "$SCRIPT" "$STUB_BASE" "$STUB_VERSION" 2>&1)"
  else
    out="$("$SCRIPT" "$STUB_BASE" "$STUB_VERSION" 2>&1)"
  fi
  rc=$?
}

passed=0
failed=0

expect() {
  local label="$1" want_rc="$2" want_line="$3" line
  line="$(printf '%s\n' "$out" | grep -E '^  (ok|FAIL)' | grep -E 'proCheckout' | head -1 || true)"
  if [ "$rc" != "$want_rc" ]; then
    printf '  FAIL  %-42s exit %s, want %s\n' "$label" "$rc" "$want_rc"
    failed=$((failed + 1))
    return
  fi
  if [ -n "$want_line" ] && ! printf '%s\n' "$out" | grep -qF -- "$want_line"; then
    printf '  FAIL  %-42s exit ok, but output lacks %s\n' "$label" "$want_line"
    failed=$((failed + 1))
    return
  fi
  printf '  ok    %-42s exit %s | %s\n' "$label" "$rc" "$line"
  passed=$((passed + 1))
}

echo "verify-release-copy.sh — proCheckout expectation (DEF-199)"
echo
echo "unset x {matching, mismatching}: the default still accepts either value"
run false ""
expect "unset, served false" 0 'ok    /api/config exposes boolean proCheckout ("proCheckout":false)'
run true ""
expect "unset, served true" 0 'ok    /api/config exposes boolean proCheckout ("proCheckout":true)'

echo
echo "set x matching: passes, and names the expected value"
run true "true"
expect "expect=true, served true" 0 'ok    /api/config proCheckout matches expected true'
run false "false"
expect "expect=false, served false" 0 'ok    /api/config proCheckout matches expected false'

echo
echo "set x mismatching: fails, and names the host and both values"
run false "true"
expect "expect=true, served false" 1 'FAIL  https://stub.invalid /api/config proCheckout is false, want true'
run true "false"
expect "expect=false, served true" 1 'FAIL  https://stub.invalid /api/config proCheckout is true, want false'

echo
echo "regressions the rewrite must not introduce"
STUB_NO_FLAG=1 run false "false"
expect "flag missing, expectation set" 1 'has no boolean proCheckout feature flag'
unset STUB_NO_FLAG
run false "yes"
expect "EXPECTED_PRO_CHECKOUT=yes" 2 '::error::EXPECTED_PRO_CHECKOUT must be true or false, got'

echo
echo "passed: $passed  failed: $failed"
[ "$failed" -eq 0 ]
