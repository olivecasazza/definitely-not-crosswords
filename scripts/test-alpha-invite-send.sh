#!/usr/bin/env bash
# Guard tests for alpha-invite-send.sh (DEF-120, DEF-228).
#
# The script's whole value is refusing to report a send that did not happen, so
# every case here is a way a send could silently look finished: a host that logs
# mail instead of sending it, a roster nobody consented, a roster that is empty
# or truncated, a row that cannot be sent. Each must be a hard stop, never a
# green exit.
#
# The second half is DEF-228: a withdrawal must not dirty a tracked file, must
# never be dated before the consent it withdraws, and must actually take the
# address out of the wave. Those cases assert effects on the world rather than
# exit codes alone, so they bring their own helpers and a real git worktree.
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
echo "a withdrawal is recorded, not applied to the roster (DEF-228)"
# A withdrawal lands in the ledger, never in the roster — the second half below
# is where that is asserted. What matters here is the effect: the row leaves
# the wave, and the roster still says what it always said.
#
# The wave date is a month back, not 2026-10-01: DEF-228 refuses a withdrawal
# dated before the consent it withdraws, so a roster whose consent is dated
# ahead cannot be withdrawn until that date (or until the date is corrected).
cat > "$WORK/withdraw.json" <<JSON
{"consent":{"answer":"all","recorded_at":"$(date -u -d '-30 days' +%F)","source":"s"},
 "roster":[{"email":"a@example.com","name":"A"},{"email":"b@example.com","name":"B"}]}
JSON
run "withdraw removes the row" 0 --roster "$WORK/withdraw.json" \
    --withdraw a@example.com --withdrawals "$WORK/withdrawals.jsonl"
if grep -q '"consent": false' "$WORK/withdraw.json"; then
  printf '  FAIL  %-46s %s\n' "roster is not rewritten" 'consent was written back'
  failed=$((failed + 1))
else
  printf '  ok    %-46s %s\n' "roster is not rewritten" 'no consent change in the file'
  passed=$((passed + 1))
fi
expect "withdrawn row is skipped on re-run" 0 \
  'WITHDRAWAL LEDGER: 1 row(s) will NOT be emailed' \
  --roster "$WORK/withdraw.json" --withdrawals "$WORK/withdrawals.jsonl"

# ── DEF-228: the withdrawal ledger ───────────────────────────────────────────
# A withdrawal is a consent record, so it is checked against the world it
# touches: a tracked file must come out of it byte-identical, and a date is only
# ever recorded when it can be ordered against the consent it retracts.
REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_GIT="$WORK/repo"
TODAY="$(date -u +%F)"
FUTURE="$(date -u -d '+30 days' +%F)"
PAST="$(date -u -d '-30 days' +%F)"
export GIT_AUTHOR_NAME="alpha-invite test" GIT_AUTHOR_EMAIL="test@localhost"
export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME" GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
unset ALPHA_ROSTER ALPHA_WITHDRAWALS ALPHA_LOG XDG_STATE_HOME || true

git -C "$WORK" init -q repo
mkdir -p "$REPO_GIT/data/crossword"
cat > "$REPO_GIT/data/crossword/roster.json" <<JSON
{
  "consent": { "answer": "all", "recorded_at": "$PAST", "source": "issue-card:cb560df2" },
  "roster": [
    { "email": "wave@example.com", "name": "Wave Consent" },
    { "email": "row@example.com", "name": "Row Consent", "consent": true,
      "consent_recorded_at": "$PAST", "consent_source": "asked directly" },
    { "email": "nope@example.com", "name": "No Consent", "consent": false }
  ]
}
JSON
git -C "$REPO_GIT" add -A
git -C "$REPO_GIT" commit -qm "tracked roster"
TRACKED_ROSTER="data/crossword/roster.json"

# run_in <dir> <label> <expected-exit> [VAR=VAL ...] <args...> — the harness
# above has no way to say "in this directory" or "with no HOME", and both
# matter here: the worktree cases must run inside a repo, and the no-ledger
# case must run with the default path unset.
run_in() {
  local dir="$1" label="$2" want="$3"; shift 3
  local -a envs=()
  while [[ "${1:-}" == *=* ]]; do envs+=("$1"); shift; done
  out="$(cd "$dir" && env STUB_CONFIG="$STUB_CONFIG" "${envs[@]}" "$SCRIPT" "$@" 2>&1)"
  rc=$?
  if [ "$rc" = "$want" ]; then
    printf '  ok    %-46s exit %s\n' "$label" "$rc"
    passed=$((passed + 1))
  else
    printf '  FAIL  %-46s exit %s, want %s\n' "$label" "$rc" "$want"
    printf '%s\n' "$out" | tail -4 | sed 's/^/        /'
    failed=$((failed + 1))
  fi
}

# check_in <dir> <label> <expected-exit> <substring> [VAR=VAL ...] <args...>
check_in() {
  local dir="$1" label="$2" want="$3" line="$4"; shift 4
  local -a envs=()
  while [[ "${1:-}" == *=* ]]; do envs+=("$1"); shift; done
  out="$(cd "$dir" && env STUB_CONFIG="$STUB_CONFIG" "${envs[@]}" "$SCRIPT" "$@" 2>&1)"
  rc=$?
  if [ "$rc" != "$want" ]; then
    printf '  FAIL  %-46s exit %s, want %s\n' "$label" "$rc" "$want"
    printf '%s\n' "$out" | tail -4 | sed 's/^/        /'
    failed=$((failed + 1))
    return
  fi
  if ! printf '%s\n' "$out" | grep -qF -- "$line"; then
    printf '  FAIL  %-46s exit ok, but output lacks: %s\n' "$label" "$line"
    failed=$((failed + 1))
    return
  fi
  printf '  ok    %-46s exit %s | %s\n' "$label" "$rc" "$line"
  passed=$((passed + 1))
}

check_tree_clean() {
  local label="$1" dirty
  dirty="$(git -C "$REPO_GIT" status --porcelain)"
  if [ -n "$dirty" ]; then
    printf '  FAIL  %-46s worktree dirty: %s\n' "$label" "$(printf '%s' "$dirty" | tr '\n' '|')"
    failed=$((failed + 1))
    return
  fi
  printf '  ok    %-46s git status --porcelain empty\n' "$label"
  passed=$((passed + 1))
}

check_absent() {
  local label="$1" path="$2"
  if [ -e "$path" ]; then
    printf '  FAIL  %-46s %s exists\n' "$label" "$path"
    failed=$((failed + 1))
    return
  fi
  printf '  ok    %-46s %s not created\n' "$label" "$(basename "$path")"
  passed=$((passed + 1))
}

snapshot() { sha256sum "$1" 2>/dev/null | cut -d' ' -f1 || echo missing; }

check_unchanged() {
  local label="$1" file="$2" before="$3"
  if [ "$(snapshot "$file")" != "$before" ]; then
    printf '  FAIL  %-46s %s was rewritten\n' "$label" "$(basename "$file")"
    failed=$((failed + 1))
    return
  fi
  printf '  ok    %-46s %s byte-identical\n' "$label" "$(basename "$file")"
  passed=$((passed + 1))
}

# The newest record in an append-only ledger.
ledger_field() { tail -1 "$2" | jq -r "$1" 2>/dev/null; }

echo
echo "the reported defects"
run_in "$REPO_GIT" "withdraw records, leaves the worktree clean" 0 \
    --roster "$TRACKED_ROSTER" --withdraw row@example.com \
    --withdrawals "$WORK/ledger.jsonl"
check_tree_clean "withdraw, tracked roster"

if [ "$(ledger_field '.consent_withdrawn_at' "$WORK/ledger.jsonl")" = "$TODAY" ] \
   && [ "$(ledger_field '.consent_recorded_at' "$WORK/ledger.jsonl")" = "$PAST" ] \
   && [ "$(ledger_field '.email' "$WORK/ledger.jsonl")" = "row@example.com" ]; then
  printf '  ok    %-46s withdrawn=%s recorded=%s\n' \
    "ledger record" "$(ledger_field '.consent_withdrawn_at' "$WORK/ledger.jsonl")" \
    "$(ledger_field '.consent_recorded_at' "$WORK/ledger.jsonl")"
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s\n' "ledger record" "$(cat "$WORK/ledger.jsonl" 2>&1)"
  failed=$((failed + 1))
fi

# The exact command from DEF-228, against the committed example roster, whose
# row consent is dated in the future relative to the system clock.
EXAMPLE="data/crossword/alpha-roster.example.json"
before="$(snapshot "$REPO/$EXAMPLE")"
check_in "$REPO" "the DEF-228 command now refuses" 1 \
  'before the consent it withdraws was recorded on' \
  --roster "$EXAMPLE" --withdraw partial@example.com
check_in "$REPO" "…and names both fields" 1 \
  'field consent_recorded_at' \
  --roster "$EXAMPLE" --withdraw partial@example.com
check_unchanged "example roster in the real repo" "$REPO/$EXAMPLE" "$before"

echo
echo "no input produces a withdrawal that predates the consent"
roster_with_row_date() {
  cat <<JSON
{
  "consent": { "answer": "all", "recorded_at": "$PAST", "source": "issue-card:cb560df2" },
  "roster": [
    { "email": "wave@example.com", "name": "Wave Consent" },
    { "email": "row@example.com", "name": "Row Consent", "consent": true,
      "consent_recorded_at": "$1", "consent_source": "asked directly" }
  ]
}
JSON
}
roster_with_row_date "$FUTURE" > "$WORK/future.json"
check_in "$REPO_GIT" "future consent_recorded_at refuses" 1 \
  'consent_withdrawn_at' \
  --roster "$WORK/future.json" --withdraw row@example.com \
  --withdrawals "$WORK/future-ledger.jsonl"
check_in "$REPO_GIT" "…naming the date it would have written" 1 "$TODAY" \
  --roster "$WORK/future.json" --withdraw row@example.com \
  --withdrawals "$WORK/future-ledger.jsonl"
check_in "$REPO_GIT" "…and the date it precedes" 1 "$FUTURE" \
  --roster "$WORK/future.json" --withdraw row@example.com \
  --withdrawals "$WORK/future-ledger.jsonl"
check_absent "…and writes no ledger" "$WORK/future-ledger.jsonl"

roster_with_row_date "2026-02-31" > "$WORK/impossible.json"
check_in "$REPO_GIT" "a date that is not a real day refuses" 1 \
  'is not a real calendar date' \
  --roster "$WORK/impossible.json" --withdraw row@example.com \
  --withdrawals "$WORK/impossible-ledger.jsonl"
check_absent "…and writes no ledger" "$WORK/impossible-ledger.jsonl"

roster_with_row_date "$TODAY" > "$WORK/today.json"
check_in "$REPO_GIT" "the same day as the consent is allowed" 0 \
  'withdrew row@example.com' \
  --roster "$WORK/today.json" --withdraw row@example.com \
  --withdrawals "$WORK/today-ledger.jsonl"
if [ "$(ledger_field '.consent_withdrawn_at' "$WORK/today-ledger.jsonl")" = "$TODAY" ]; then
  printf '  ok    %-46s withdrawn == recorded == %s\n' "…and never earlier" "$TODAY"
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s\n' "…and never earlier" "$(cat "$WORK/today-ledger.jsonl" 2>&1)"
  failed=$((failed + 1))
fi

echo
echo "a withdrawal has to be able to land somewhere"
run_in "$REPO_GIT" "no ledger configured refuses" 1 \
    HOME= XDG_STATE_HOME= ALPHA_WITHDRAWALS= \
    --roster "$TRACKED_ROSTER" --withdraw row@example.com
check_tree_clean "…rather than dirtying the worktree"

check_in "$REPO_GIT" "a ledger inside the worktree refuses" 1 \
  'inside a git worktree' \
  --roster "$TRACKED_ROSTER" --withdraw row@example.com \
  --withdrawals "$REPO_GIT/withdrawals.jsonl"
check_absent "…and is not created" "$REPO_GIT/withdrawals.jsonl"
check_tree_clean "…and leaves the tree clean"

check_in "$REPO_GIT" "an address that is not in the roster refuses" 1 \
  'withdraw: no row for nobody@example.com' \
  --roster "$TRACKED_ROSTER" --withdraw nobody@example.com \
  --withdrawals "$WORK/ledger.jsonl"

printf 'not json at all\n' > "$WORK/broken.jsonl"
check_in "$REPO_GIT" "an unreadable ledger fails closed" 1 \
  'not readable JSON' \
  --roster "$TRACKED_ROSTER" --withdrawals "$WORK/broken.jsonl"

check_in "$REPO_GIT" "a repeated withdraw is idempotent" 0 \
  'already withdrawn' \
  --roster "$TRACKED_ROSTER" --withdraw row@example.com \
  --withdrawals "$WORK/ledger.jsonl"
if [ "$(wc -l < "$WORK/ledger.jsonl")" -eq 1 ]; then
  printf '  ok    %-46s still 1 line\n' "…and appends nothing"
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s lines\n' "…and appends nothing" "$(wc -l < "$WORK/ledger.jsonl")"
  failed=$((failed + 1))
fi

echo '{"roster":[{"email":"cold@example.com","name":"Cold"}]}' > "$WORK/unconsented.json"
check_in "$REPO_GIT" "a row with no consent still withdraws" 0 \
  'do-not-contact' \
  --roster "$WORK/unconsented.json" --withdraw cold@example.com \
  --withdrawals "$WORK/ledger.jsonl"
if [ "$(ledger_field '.consent_recorded_at' "$WORK/ledger.jsonl")" = "null" ]; then
  printf '  ok    %-46s consent_recorded_at=null\n' "…recorded as a do-not-contact"
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s\n' "…recorded as a do-not-contact" \
    "$(ledger_field '.consent_recorded_at' "$WORK/ledger.jsonl")"
  failed=$((failed + 1))
fi

echo
echo "a withdrawal takes the address out of the wave"
check_in "$REPO_GIT" "the ledger is reported" 0 \
  'WITHDRAWAL LEDGER: 1 row(s) will NOT be emailed' \
  --roster "$TRACKED_ROSTER" --withdrawals "$WORK/ledger.jsonl"
check_in "$REPO_GIT" "…naming the address and the date" 0 \
  "row@example.com (withdrawn $TODAY)" \
  --roster "$TRACKED_ROSTER" --withdrawals "$WORK/ledger.jsonl"
if printf '%s\n' "$out" | grep -qF "── row 1: row@example.com"; then
  printf '  FAIL  %-46s row 1 is still rendered as emailable\n' "the withdrawn row is not walked"
  failed=$((failed + 1))
else
  printf '  ok    %-46s row 1 absent from the walk\n' "the withdrawn row is not walked"
  passed=$((passed + 1))
fi
check_in "$REPO_GIT" "…while the others are" 0 \
  '── row 0: wave@example.com' \
  --roster "$TRACKED_ROSTER" --withdrawals "$WORK/ledger.jsonl"

# consent: true in the roster does not undo a recorded withdrawal.
jq '.roster[1].consent = true' "$REPO_GIT/$TRACKED_ROSTER" > "$WORK/reconsented.json"
check_in "$REPO_GIT" "roster consent:true does not re-enable" 0 \
  'WITHDRAWAL LEDGER: 1 row(s) will NOT be emailed' \
  --roster "$WORK/reconsented.json" --withdrawals "$WORK/ledger.jsonl"
if printf '%s\n' "$out" | grep -qF "── row 1: row@example.com"; then
  printf '  FAIL  %-46s a re-run of consent:true walked the row\n' "the gate still refuses"
  failed=$((failed + 1))
else
  printf '  ok    %-46s the gate still refuses\n' "the gate still refuses"
  passed=$((passed + 1))
fi

echo
echo "the consent gate is unchanged"
echo '[{"email":"a@example.com","name":"A"},{"email":"b@example.com","name":"B"},{"email":"c@example.com","name":"C"}]' > "$WORK/noconsent3.json"
check_in "$REPO_GIT" "no wave consent refuses every row" 0 \
  'CONSENT GATE: 3 row(s) will NOT be emailed' \
  --roster "$WORK/noconsent3.json"
if printf '%s\n' "$out" | grep -qF "── row"; then
  printf '  FAIL  %-46s a row was walked\n' "…and walks nothing"
  failed=$((failed + 1))
else
  printf '  ok    %-46s and walks nothing\n' "…and walks nothing"
  passed=$((passed + 1))
fi

check_in "$REPO_GIT" "a row with no consent can still withdraw" 0 \
  'withdrew a@example.com' \
  --roster "$WORK/noconsent3.json" --withdraw a@example.com \
  --withdrawals "$WORK/ledger.jsonl"
check_in "$REPO_GIT" "a withdraw does not unblock the others" 0 \
  'CONSENT GATE: 2 row(s) will NOT be emailed' \
  --roster "$WORK/noconsent3.json" --withdrawals "$WORK/ledger.jsonl"
check_in "$REPO_GIT" "…the withdrawn row is refused too" 0 \
  'WITHDRAWAL LEDGER: 1 row(s) will NOT be emailed' \
  --roster "$WORK/noconsent3.json" --withdrawals "$WORK/ledger.jsonl"
if printf '%s\n' "$out" | grep -qF "── row"; then
  printf '  FAIL  %-46s a row was walked\n' "…and the wave is still empty"
  failed=$((failed + 1))
else
  printf '  ok    %-46s and the wave is still empty\n' "…and the wave is still empty"
  passed=$((passed + 1))
fi

check_in "$REPO_GIT" "the tracked roster dry-runs clean" 0 \
  'skipped (consent):  1' \
  --roster "$TRACKED_ROSTER"
check_in "$REPO_GIT" "…reporting no withdrawals by default" 0 \
  'withdrawn:          0' \
  --roster "$TRACKED_ROSTER"
check_tree_clean "a plain dry-run"

before="$(snapshot "$REPO/$EXAMPLE")"
check_in "$REPO" "the committed example roster dry-runs" 0 \
  'rows:               3' \
  --roster "$EXAMPLE"
check_unchanged "…and the real repo roster is untouched" "$REPO/$EXAMPLE" "$before"

echo
echo "usage"
run "--help exits 0" 0 --help
expect "…and documents the ledger" 0 'ALPHA_WITHDRAWALS' --help
expect "…and the flag" 0 '--withdrawals PATH' --help
run "an unknown flag is still usage (3)" 3 --nope
run "a missing roster still refuses" 1 --roster "$WORK/does-not-exist.json"

if bash -n "$SCRIPT" 2>"$WORK/syntax.txt"; then
  printf '  ok    %-46s clean\n' "bash -n"
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s\n' "bash -n" "$(cat "$WORK/syntax.txt")"
  failed=$((failed + 1))
fi

echo
echo "passed: $passed  failed: $failed"
[ "$failed" -eq 0 ]
