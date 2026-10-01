#!/usr/bin/env bash
# Guard tests for alpha-readout.sh (DEF-154, DEF-153).
#
# The Pro-conversion block is the one place in the readout where a missing
# measurement and a real zero look identical, and the difference is the whole
# point of the metric: "nobody converted" and "nobody could have converted" are
# opposite conclusions about the positioning. So every case here is a way the
# block could print a confident number it did not measure:
#
#   - a host still serving a build from before `isPro` landed (no key at all)
#   - an `isPro` that is null or non-boolean rather than absent
#   - an empty ALPHA_SINCE window with no row to carry the field
#   - a low count against `proCheckout: false`, which is the billing flag
#   - a low count against a build that cannot report `proCheckout` at all
#   - a `vipPass` admin override counted as a purchase
#
# Each must render UNKNOWN or a labelled count, never a bare 0. curl is stubbed
# so no case reaches a real host; the stub answers /api/config from STUB_CONFIG,
# the leaderboard from STUB_LEADERBOARD and user.listForAdmin from STUB_USERS,
# which is how each host build is simulated without one.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/alpha-readout.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

STUB_CONFIG='{"environment":"staging","buildSha":"deadbeef","version":"0.0.0-test","features":{"proCheckout":true}}'
STUB_LEADERBOARD='[{"result":{"data":[{"id":"u1","name":"A","gamesPlayed":0,"totalScore":0,"totalCorrect":0,"totalIncorrect":0,"accuracy":0,"email":null}]}}]'
STUB_USERS='[{"result":{"data":[]}}]'
export STUB_CONFIG STUB_LEADERBOARD STUB_USERS

mkdir -p "$WORK/bin"
cat > "$WORK/bin/curl" <<'STUB'
#!/usr/bin/env bash
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -X|-H|-d|--data-urlencode|-m|-o|-D) shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
case "$url" in
  */api/config) printf '%s' "$STUB_CONFIG" ;;
  */api/auth/callback/credentials)
    printf 'HTTP/1.1 200 OK\r\nset-cookie: next-auth.session-token=stub-session; Path=/; HttpOnly\r\n\r\n' ;;
  */api/trpc/stats.getGlobalLeaderboard) printf '%s' "$STUB_LEADERBOARD" ;;
  */api/trpc/user.listForAdmin) printf '%s' "$STUB_USERS" ;;
  *) printf '%s' '[]' ;;
esac
STUB
chmod +x "$WORK/bin/curl"
export PATH="$WORK/bin:$PATH"

passed=0
failed=0

# expect <label> <expected-exit> <substring> <env...> -- <args...>
# The env split is `--` so a case can drive both the stub payloads and the
# script's own env without the two sets colliding.
expect() {
  local label="$1" want="$2" want_line="$3"
  shift 3
  local -a envs=()
  while [ "${1:-}" != "--" ]; do envs+=("$1"); shift; done
  shift
  out="$(env "${envs[@]}" "$SCRIPT" "$@" 2>&1)"
  rc=$?
  if [ "$rc" != "$want" ]; then
    printf '  FAIL  %-52s exit %s, want %s\n' "$label" "$rc" "$want"
    printf '%s\n' "$out" | sed 's/^/        /'
    failed=$((failed + 1))
    return
  fi
  if ! printf '%s\n' "$out" | grep -qF -- "$want_line"; then
    printf '  FAIL  %-52s exit ok, but output lacks:\n        %s\n' "$label" "$want_line"
    printf '%s\n' "$out" | sed 's/^/        /'
    failed=$((failed + 1))
    return
  fi
  printf '  ok    %-52s exit %s | %s\n' "$label" "$rc" "$want_line"
  passed=$((passed + 1))
}

# expect_absent <label> <substring> <env...> -- <args...>
# The negative assertion: a case that must NOT print a line. Used for the one
# thing DEF-153 forbids outright — a bare `conversions: 0`.
expect_absent() {
  local label="$1" bad="$2"
  shift 2
  local -a envs=()
  while [ "${1:-}" != "--" ]; do envs+=("$1"); shift; done
  shift
  out="$(env "${envs[@]}" "$SCRIPT" "$@" 2>&1)"
  rc=$?
  if printf '%s\n' "$out" | grep -qF -- "$bad"; then
    printf '  FAIL  %-52s output contains forbidden: %s\n' "$label" "$bad"
    printf '%s\n' "$out" | sed 's/^/        /'
    failed=$((failed + 1))
    return
  fi
  printf '  ok    %-52s exit %s | no "%s"\n' "$label" "$rc" "$bad"
  passed=$((passed + 1))
}

# An admin session by cookie keeps the cases to one env var and skips the
# login round-trip except in the case that is about the login itself.
COOKIE='ALPHA_SESSION_COOKIE=stub-session'

# Rows that carry the additive field. `p@` is a purchase; `v@` is Pro only by
# admin override, so it must land on the vipPass line and not in the count.
USERS_WITH_ISPRO='[{"result":{"data":[
  {"email":"free@example.com","role":"USER","vipPass":false,"emailVerified":"2026-10-13T00:00:00.000Z","createdAt":"2026-10-13T00:00:00.000Z","isPro":false},
  {"email":"p@example.com","role":"USER","vipPass":false,"emailVerified":"2026-10-13T00:00:00.000Z","createdAt":"2026-10-13T00:00:00.000Z","isPro":true},
  {"email":"v@example.com","role":"USER","vipPass":true,"emailVerified":"2026-10-13T00:00:00.000Z","createdAt":"2026-10-13T00:00:00.000Z","isPro":true}
]}}]'

# The same rows on a build that predates 5ead8b1: no `isPro` key at all. This
# is what both live hosts serve today (v0.1.76 / c446d23d596d).
USERS_NO_ISPRO='[{"result":{"data":[
  {"email":"free@example.com","role":"USER","vipPass":false,"emailVerified":"2026-10-13T00:00:00.000Z","createdAt":"2026-10-13T00:00:00.000Z"},
  {"email":"p@example.com","role":"USER","vipPass":false,"emailVerified":"2026-10-13T00:00:00.000Z","createdAt":"2026-10-13T00:00:00.000Z"}
]}}]'

echo "alpha-readout.sh — a conversion count must be measured or UNKNOWN, never a bare 0 (DEF-154)"
echo

echo "tier 1 needs no credential and the conversion line is UNKNOWN without one"
STUB_CONFIG='{"environment":"production","buildSha":"c446d23d596d","version":"0.1.76","features":{"proCheckout":false}}'
expect "no session: still prints tier 1" 0 'accounts:' -- 
expect "no session: conversions UNKNOWN" 0 'conversions: UNKNOWN (no admin session)' -- 
expect_absent "no session: never a bare 0" 'conversions: 0' -- 

echo
echo "a build from before isPro landed cannot report a count"
STUB_USERS="$USERS_NO_ISPRO"
STUB_CONFIG='{"environment":"staging","buildSha":"c446d23d596d","version":"0.1.76","features":{"proCheckout":true}}'
expect "endpoint missing isPro" 0 'conversions: UNKNOWN (endpoint missing isPro)' "$COOKIE" -- 
expect_absent "missing isPro: never 0" 'conversions: 0' "$COOKIE" -- 
expect "missing isPro: names the build that landed it" 0 'PR #193' "$COOKIE" -- 

echo
echo "an empty window has no row to prove the field is served"
STUB_USERS='[{"result":{"data":[{"email":"old@example.com","role":"USER","vipPass":false,"emailVerified":null,"createdAt":"2026-01-01T00:00:00.000Z","isPro":false}]}}]'
expect "empty SINCE window is UNKNOWN, not 0" 0 'UNKNOWN (endpoint missing isPro — no account row to confirm the field)' \
  "$COOKIE" ALPHA_SINCE=2026-10-12 -- 
expect_absent "empty window: never 0" 'conversions: 0' "$COOKIE" ALPHA_SINCE=2026-10-12 -- 

echo
echo "a served field, a live checkout: the count, its denominator, the split"
STUB_USERS="$USERS_WITH_ISPRO"
STUB_CONFIG='{"environment":"staging","buildSha":"deadbeef","version":"9.9.9","features":{"proCheckout":true}}'
expect "counts purchases only, with denominator" 0 'conversions: 1 of 3 accounts (checkout LIVE)' "$COOKIE" -- 
expect "vipPass is a separate line, not summed in" 0 'vipPass-only Pro (manual admin override, NOT a purchase): 1' "$COOKIE" -- 
expect "header states the build it measured" 0 'build:         deadbeef' "$COOKIE" -- 
expect "header states the checkout state" 0 'pro checkout:  LIVE' "$COOKIE" -- 

echo
echo "checkout not live: the count is the billing flag, and the line says so"
STUB_CONFIG='{"environment":"production","buildSha":"c446d23d596d","version":"0.1.76","features":{"proCheckout":false}}'
expect "labels the checkout state in-line" 0 'conversions: 1 of 3 accounts (checkout NOT LIVE)' "$COOKIE" -- 
expect "separates could-not from did-not" 0 'BILLING FLAG, NOT DEMAND' "$COOKIE" -- 

echo
echo "a build that cannot report proCheckout cannot interpret its own count"
STUB_CONFIG='{"environment":"production","buildSha":"c446d23d596d","version":"0.1.50","features":{}}'
expect "checkout UNKNOWN, not assumed live" 0 'conversions: 1 of 3 accounts (checkout UNKNOWN)' "$COOKIE" -- 
expect "says the split is unmeasured" 0 'cannot be split into "did not" and' "$COOKIE" -- 

echo
echo "an isPro that is not a boolean is not a served field"
STUB_USERS='[{"result":{"data":[
  {"email":"n@example.com","role":"USER","vipPass":false,"emailVerified":null,"createdAt":"2026-10-13T00:00:00.000Z","isPro":null}
]}}]'
STUB_CONFIG='{"environment":"staging","buildSha":"deadbeef","version":"9.9.9","features":{"proCheckout":true}}'
expect "isPro:null is UNKNOWN" 0 'UNKNOWN (endpoint missing isPro)' "$COOKIE" -- 

echo
echo "a session that is not admin-capable is a marker, and tier 1 still stands"
STUB_USERS='[{"error":{"message":"UNAUTHORIZED"}}]'
expect "non-admin session is a marker" 1 'conversions: UNKNOWN (user.listForAdmin unavailable' "$COOKIE" -- 
expect "…and tier 1 was still printed" 1 'accounts:' "$COOKIE" -- 
expect_absent "non-admin session: never 0" 'conversions: 0' "$COOKIE" -- 

echo
echo "ALPHA_SINCE filters the window it claims to"
# The bug this pins: `SINCE="$SINCE" printf … | jq` sets the var for printf only,
# so jq read env.SINCE as null, `select(.createdAt >= null)` kept every row, and
# the window silently became "all time". Pinned because the conversion
# denominator is computed over this same window.
STUB_USERS='[{"result":{"data":[
  {"email":"before@example.com","role":"USER","vipPass":false,"emailVerified":null,"createdAt":"2026-01-01T00:00:00.000Z","isPro":true},
  {"email":"inwave@example.com","role":"USER","vipPass":false,"emailVerified":null,"createdAt":"2026-10-13T00:00:00.000Z","isPro":true}
]}}]'
STUB_CONFIG='{"environment":"staging","buildSha":"deadbeef","version":"9.9.9","features":{"proCheckout":true}}'
expect "denominator excludes pre-wave accounts" 0 'conversions: 1 of 1 accounts (checkout LIVE)' \
  "$COOKIE" ALPHA_SINCE=2026-10-12 -- 
expect_absent "a pre-wave Pro is not counted" 'conversions: 2 of 2' "$COOKIE" ALPHA_SINCE=2026-10-12 -- 
expect "the window is named in the output" 0 'created on/after 2026-10-12' "$COOKIE" ALPHA_SINCE=2026-10-12 -- 
expect "no SINCE means all time, both rows" 0 'conversions: 2 of 2 accounts (checkout LIVE)' "$COOKIE" -- 

echo
echo "login failure is its own hard stop"
STUB_USERS='[{"result":{"data":[]}}]'
STUB_CONFIG='{"environment":"staging","buildSha":"x","version":"x","features":{}}'
# The happy-path stub always returns a cookie, so the no-cookie path needs a
# login answer without a set-cookie line. Last case in the file because it
# replaces the stub.
cat > "$WORK/bin/curl" <<'STUB'
#!/usr/bin/env bash
url=""
while [ $# -gt 0 ]; do
  case "$1" in
    -X|-H|-d|--data-urlencode|-m|-o|-D) shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
case "$url" in
  */api/config) printf '%s' "$STUB_CONFIG" ;;
  */api/auth/callback/credentials) printf 'HTTP/1.1 200 OK\r\n\r\n' ;;
  */api/trpc/stats.getGlobalLeaderboard) printf '%s' "$STUB_LEADERBOARD" ;;
  *) printf '%s' '[]' ;;
esac
STUB
chmod +x "$WORK/bin/curl"
expect "no session cookie from login is a hard stop" 1 'got no session cookie' \
  ALPHA_ADMIN_EMAIL=a@example.com ALPHA_ADMIN_PASSWORD=wrong -- 

if bash -n "$SCRIPT" 2>"$WORK/syntax.txt"; then
  printf '  ok    %-52s clean\n' "bash -n"
  passed=$((passed + 1))
else
  printf '  FAIL  %-52s %s\n' "bash -n" "$(cat "$WORK/syntax.txt")"
  failed=$((failed + 1))
fi

echo
echo "passed: $passed  failed: $failed"
[ "$failed" -eq 0 ]
