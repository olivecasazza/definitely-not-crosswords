#!/usr/bin/env bash
# Guard tests for alpha-invite-send.sh (DEF-120).
#
# The script's whole value is refusing to report a send that did not happen, so
# every case here is a way a send could silently look finished: a host that logs
# mail instead of sending it, a roster nobody consented, a roster that is empty
# or truncated, a row that cannot be sent. Each must be a hard stop, never a
# green exit.
#
# curl is stubbed so no case can reach a real host. The stub answers /api/config
# from STUB_CONFIG, which is how the DEF-201 abort path is exercised without
# pointing the script at production.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/alpha-invite-send.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

STUB_CONFIG='{"environment":"staging","features":{"mailDelivery":"smtp"},"version":"0.0.0-test"}'

mkdir -p "$WORK/bin"
cat > "$WORK/bin/curl" <<'STUB'
#!/usr/bin/env bash
url=""
want_out=0
out=""
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
  */api/config) printf '%s' "$STUB_CONFIG" ;;
  *) printf '%s' '[{"result":{"data":{"success":true}}}]' ;;
esac
STUB
chmod +x "$WORK/bin/curl"
export PATH="$WORK/bin:$PATH"

passed=0
failed=0

# run <label> <expected-exit> <args...>
run() {
  local label="$1" want="$2"
  shift 2
  out="$(STUB_CONFIG="$STUB_CONFIG" "$SCRIPT" "$@" 2>&1)"
  rc=$?
  if [ "$rc" = "$want" ]; then
    printf '  ok    %-46s exit %s\n' "$label" "$rc"
    passed=$((passed + 1))
  else
    printf '  FAIL  %-46s exit %s, want %s\n' "$label" "$rc" "$want"
    printf '%s\n' "$out" | sed 's/^/        /'
    failed=$((failed + 1))
  fi
}

# expect <label> <expected-exit> <substring> <args...>
expect() {
  local label="$1" want="$2" want_line="$3"
  shift 3
  out="$(STUB_CONFIG="$STUB_CONFIG" "$SCRIPT" "$@" 2>&1)"
  rc=$?
  if [ "$rc" != "$want" ]; then
    printf '  FAIL  %-46s exit %s, want %s\n' "$label" "$rc" "$want"
    printf '%s\n' "$out" | sed 's/^/        /'
    failed=$((failed + 1))
    return
  fi
  if ! printf '%s\n' "$out" | grep -qF -- "$want_line"; then
    printf '  FAIL  %-46s exit ok, but output lacks:\n        %s\n' "$label" "$want_line"
    printf '%s\n' "$out" | sed 's/^/        /'
    failed=$((failed + 1))
    return
  fi
  printf '  ok    %-46s exit %s | %s\n' "$label" "$rc" "$want_line"
  passed=$((passed + 1))
}

# A roster that is consented and well-formed, so the happy path has a baseline.
cat > "$WORK/good.json" <<'JSON'
{"consent":{"answer":"all","recorded_at":"2026-10-01","source":"issue-card:cb560df2"},
 "roster":[{"email":"a@example.com","name":"A"},
           {"email":"b@example.com","name":"B","consent":false}]}
JSON

echo "alpha-invite-send.sh — the send must never look finished when it did not happen (DEF-120)"
echo

echo "happy path"
run "consented roster, dry-run" 0 --roster "$WORK/good.json"
expect "renders the email the product will send" 0 \
  'Subject: Verify your email — Definitely Not Crosswords' --roster "$WORK/good.json"
expect "derives a username from the address" 0 \
  'username: a (derived from the address)' --roster "$WORK/good.json"

echo
echo "DEF-201: a host that logs mail instead of sending it must abort"
STUB_CONFIG='{"environment":"staging","features":{"mailDelivery":"log"},"version":"t"}'
expect "mailDelivery=log aborts" 1 \
  "ABORT: mailDelivery is 'log', not 'smtp'." --roster "$WORK/good.json"
STUB_CONFIG='{"environment":"staging","features":{},"version":"t"}'
expect "mailDelivery absent aborts" 1 \
  "ABORT: mailDelivery is 'absent', not 'smtp'." --roster "$WORK/good.json"
STUB_CONFIG='{"environment":"staging","features":{"mailDelivery":"smtp"},"version":"t"}'

echo
echo "the roster must never read as a finished wave"
: > "$WORK/empty.json"
expect "empty file aborts" 1 \
  'did not yield a JSON array of roster rows' --roster "$WORK/empty.json"
printf 'not json{' > "$WORK/bad.json"
expect "malformed JSON aborts" 1 \
  'is neither a JSON array nor an object with a roster array' --roster "$WORK/bad.json"
echo '{"roster":[]}' > "$WORK/zero.json"
expect "zero-row roster aborts" 1 \
  'contains zero roster rows' --roster "$WORK/zero.json"

echo
echo "consent is default-deny"
echo '[{"email":"a@example.com","name":"A"}]' > "$WORK/noconsent.json"
expect "no consent anywhere: row is skipped" 0 \
  'CONSENT GATE: 1 row(s) will NOT be emailed' --roster "$WORK/noconsent.json"
echo '{"consent":{"answer":"none","recorded_at":"2026-10-01","source":"s"},"roster":[{"email":"a@example.com","name":"A"}]}' > "$WORK/none.json"
expect "wave answer 'none' emails nobody" 0 \
  'CONSENT GATE: 1 row(s) will NOT be emailed' --roster "$WORK/none.json"
echo '{"consent":{"answer":"partial","recorded_at":"2026-10-01","source":"s"},"roster":[{"email":"a@example.com","name":"A"}]}' > "$WORK/partial.json"
expect "wave 'partial' grants nothing by itself" 0 \
  'CONSENT GATE: 1 row(s) will NOT be emailed' --roster "$WORK/partial.json"
echo '{"consent":{"answer":"all","recorded_at":"2026-10-01"},"roster":[{"email":"a@example.com","name":"A"}]}' > "$WORK/nosource.json"
expect "wave consent missing its source is ignored" 0 \
  'CONSENT GATE: 1 row(s) will NOT be emailed' --roster "$WORK/nosource.json"

echo
echo "a row that cannot be sent stops the wave before anything is mailed"
echo '{"consent":{"answer":"all","recorded_at":"2026-10-01","source":"s"},"roster":[{"email":"not-an-email","name":"A"}]}' > "$WORK/bademail.json"
expect "unusable email aborts preflight" 1 \
  'no usable email address' --roster "$WORK/bademail.json"
echo '{"consent":{"answer":"all","recorded_at":"2026-10-01","source":"s"},"roster":[{"email":"a@example.com","name":"A","password":"short"}]}' > "$WORK/shortpw.json"
expect "a too-short supplied password aborts" 1 \
  'supplies a password under 8 characters' --roster "$WORK/shortpw.json"

echo
echo "withdrawn consent takes a person out of the wave"
# Withdraw a row that has no explicit flag, so this also proves an explicit
# `consent: false` beats a wave-level "all" — otherwise withdrawing someone the
# wave covers would still email them.
cat > "$WORK/withdraw.json" <<'JSON'
{"consent":{"answer":"all","recorded_at":"2026-10-01","source":"s"},
 "roster":[{"email":"a@example.com","name":"A"},{"email":"b@example.com","name":"B"}]}
JSON
run "withdraw removes the row" 0 --roster "$WORK/withdraw.json" --withdraw a@example.com
if grep -q '"consent": false' "$WORK/withdraw.json" && grep -q 'consent_withdrawn_at' "$WORK/withdraw.json"; then
  printf '  ok    %-46s %s\n' "roster records the withdrawal" 'consent=false + consent_withdrawn_at'
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s\n' "roster records the withdrawal" 'not written back'
  cat "$WORK/withdraw.json" | sed 's/^/        /'
  failed=$((failed + 1))
fi
expect "withdrawn row is skipped on re-run" 0 \
  'CONSENT GATE: 1 row(s) will NOT be emailed' --roster "$WORK/withdraw.json"

echo
echo "passed: $passed  failed: $failed"
[ "$failed" -eq 0 ]
