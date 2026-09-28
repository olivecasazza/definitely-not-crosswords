#!/usr/bin/env bash
# Test matrix for scripts/alpha-invite-send.sh (DEF-228).
#
# The load-bearing cases are the ones where a consent record can be wrong: a
# withdrawal that dirties a tracked file, and a withdrawal dated before the
# consent it withdraws. Both are asserted by running the script and reading the
# result, not by inspecting the code:
#
#   * every --withdraw leaves the worktree byte-identical (`git status
#     --porcelain` empty, before and after)
#   * no roster can produce consent_withdrawn_at < consent_recorded_at
#   * a withdrawal actually takes the address out of the wave, whatever the
#     roster says afterwards
#   * the consent gate is unchanged: no wave consent still refuses every row
#
# curl is stubbed so no request ever leaves the host and no account is created
# on staging. git is real: the whole point of several cases is whether a file
# inside a worktree is touched.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/alpha-invite-send.sh"
REPO="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

unset ALPHA_ROSTER ALPHA_WITHDRAWALS ALPHA_LOG XDG_STATE_HOME || true
export GIT_AUTHOR_NAME="alpha-invite test" GIT_AUTHOR_EMAIL="test@localhost"
export GIT_COMMITTER_NAME="$GIT_AUTHOR_NAME" GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"

TODAY="$(date -u +%F)"
FUTURE="$(date -u -d '+30 days' +%F)"
PAST="$(date -u -d '-30 days' +%F)"

# ── curl stub: /api/config says smtp, everything else answers "no" ────────────
mkdir -p "$WORK/bin"
cat > "$WORK/bin/curl" <<'STUB'
#!/usr/bin/env bash
out=""
url=""
want_out=0
for a in "$@"; do
  if [ "$want_out" = 1 ]; then out="$a"; want_out=0; continue; fi
  case "$a" in
    -o) want_out=1 ;;
    -*) ;;
    *)  url="$a" ;;
  esac
done
case "$url" in
  */api/config) printf '{"environment":"stub","features":{"mailDelivery":"smtp"}}' ;;
  *)            printf '[]' ;;
esac
STUB
chmod +x "$WORK/bin/curl"
export PATH="$WORK/bin:$PATH"

# ── a tracked roster inside a real worktree ──────────────────────────────────
REPO_GIT="$WORK/repo"
mkdir -p "$REPO_GIT/data/crossword"
git -C "$WORK" init -q repo
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

roster_with_row_date() {
  # A roster whose only consenting row carries $1 as consent_recorded_at.
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

# A roster with no consent answer at all — the default-deny case (DEF-120).
NO_CONSENT_ROSTER="$WORK/no-consent.json"
cat > "$NO_CONSENT_ROSTER" <<'JSON'
{
  "roster": [
    { "email": "a@example.com", "name": "A" },
    { "email": "b@example.com", "name": "B" },
    { "email": "c@example.com", "name": "C" }
  ]
}
JSON

# A roster whose single row has never consented.
UNCONSENTED_ROSTER="$WORK/unconsented.json"
cat > "$UNCONSENTED_ROSTER" <<'JSON'
{ "roster": [ { "email": "cold@example.com", "name": "Cold" } ] }
JSON

passed=0
failed=0
out=""
rc=0

# run [-C DIR] [ENV=VAL ...] -- args...
run() {
  local dir="$1"; shift
  local -a envs=()
  while [[ "${1:-}" == *=* ]]; do envs+=("$1"); shift; done
  out="$(cd "$dir" && env "${envs[@]}" "$SCRIPT" "$@" 2>&1)"
  rc=$?
}

check() {
  local label="$1" want_rc="$2" want="$3" detail
  if [[ "$rc" != "$want_rc" ]]; then
    printf '  FAIL  %-46s exit %s, want %s\n' "$label" "$rc" "$want_rc"
    printf '        %s\n' "$(printf '%s' "$out" | tail -3 | tr '\n' '|')"
    failed=$((failed + 1))
    return 1
  fi
  if [[ -n "$want" ]] && ! printf '%s' "$out" | grep -qF -- "$want"; then
    printf '  FAIL  %-46s exit ok, but output lacks: %s\n' "$label" "$want"
    failed=$((failed + 1))
    return 1
  fi
  printf '  ok    %-46s exit %s | %s\n' "$label" "$rc" "$want"
  passed=$((passed + 1))
  return 0
}

# A file inside a worktree must be byte-identical after a run.
check_tree_clean() {
  local label="$1" dir="$2" dirty
  dirty="$(git -C "$dir" status --porcelain)"
  if [[ -n "$dirty" ]]; then
    printf '  FAIL  %-46s worktree dirty: %s\n' "$label" "$(printf '%s' "$dirty" | tr '\n' '|')"
    failed=$((failed + 1))
    return 1
  fi
  printf '  ok    %-46s git status --porcelain empty\n' "$label"
  passed=$((passed + 1))
  return 0
}

# The same, for one path in a tree that may legitimately hold other work — the
# real repo is checked out with the fix in it, so only the roster is asserted.
snapshot() {
  sha256sum "$1" 2>/dev/null | cut -d' ' -f1 || echo missing
}

check_unchanged() {
  local label="$1" file="$2" before="$3" after
  after="$(snapshot "$file")"
  if [[ "$after" != "$before" ]]; then
    printf '  FAIL  %-46s %s was rewritten\n' "$label" "$file"
    failed=$((failed + 1))
    return 1
  fi
  printf '  ok    %-46s %s byte-identical\n' "$label" "$(basename "$file")"
  passed=$((passed + 1))
  return 0
}

check_absent() {
  local label="$1" path="$2"
  if [[ -e "$path" ]]; then
    printf '  FAIL  %-46s %s exists\n' "$label" "$path"
    failed=$((failed + 1))
    return 1
  fi
  printf '  ok    %-46s %s not created\n' "$label" "$path"
  passed=$((passed + 1))
  return 0
}

ledger_field() {
  # The ledger is append-only, so the newest record is the last line.
  tail -1 "$2" | jq -r "$1" 2>/dev/null
}

echo "alpha-invite-send.sh — withdrawal ledger and consent dates (DEF-228)"
echo

echo "the reported defects"
run "$REPO_GIT" --roster "$TRACKED_ROSTER" --withdraw row@example.com \
    --withdrawals "$WORK/ledger.jsonl"
check "withdraw records, leaves the worktree clean" 0 "withdrew row@example.com from the wave"
check_tree_clean "withdraw, tracked roster" "$REPO_GIT"

if [[ "$(ledger_field '.consent_withdrawn_at' "$WORK/ledger.jsonl")" == "$TODAY" ]] \
   && [[ "$(ledger_field '.consent_recorded_at' "$WORK/ledger.jsonl")" == "$PAST" ]] \
   && [[ "$(ledger_field '.email' "$WORK/ledger.jsonl")" == "row@example.com" ]]; then
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
run "$REPO" --roster "$EXAMPLE" --withdraw partial@example.com
check "the DEF-228 command now refuses" 1 "before the consent it withdraws was recorded on"
check "…and names both fields" 1 "field consent_recorded_at"
check_unchanged "example roster in the real repo" "$REPO/$EXAMPLE" "$before"

echo
echo "no input produces a withdrawal that predates the consent"
roster_with_row_date "$FUTURE" > "$WORK/future.json"
run "$REPO_GIT" --roster "$WORK/future.json" --withdraw row@example.com \
    --withdrawals "$WORK/future-ledger.jsonl"
check "future consent_recorded_at refuses" 1 "consent_withdrawn_at"
check "…naming the date it would have written" 1 "$TODAY"
check "…and the date it precedes" 1 "$FUTURE"
check_absent "…and writes no ledger" "$WORK/future-ledger.jsonl"

roster_with_row_date "2026-02-31" > "$WORK/impossible.json"
run "$REPO_GIT" --roster "$WORK/impossible.json" --withdraw row@example.com \
    --withdrawals "$WORK/impossible-ledger.jsonl"
check "a date that is not a real day refuses" 1 "is not a real calendar date"
check_absent "…and writes no ledger" "$WORK/impossible-ledger.jsonl"

roster_with_row_date "$TODAY" > "$WORK/today.json"
run "$REPO_GIT" --roster "$WORK/today.json" --withdraw row@example.com \
    --withdrawals "$WORK/today-ledger.jsonl"
check "the same day as the consent is allowed" 0 "withdrew row@example.com"
if [[ "$(ledger_field '.consent_withdrawn_at' "$WORK/today-ledger.jsonl")" == "$TODAY" ]]; then
  printf '  ok    %-46s withdrawn == recorded == %s\n' "…and never earlier" "$TODAY"
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s\n' "…and never earlier" "$(cat "$WORK/today-ledger.jsonl" 2>&1)"
  failed=$((failed + 1))
fi

echo
echo "a withdrawal has to be able to land somewhere"
run "$REPO_GIT" HOME= XDG_STATE_HOME= ALPHA_WITHDRAWALS= \
    --roster "$TRACKED_ROSTER" --withdraw row@example.com
check "no ledger configured refuses" 1 "nowhere to record this"
check_tree_clean "…rather than dirtying the worktree" "$REPO_GIT"

run "$REPO_GIT" --roster "$TRACKED_ROSTER" --withdraw row@example.com \
    --withdrawals "$REPO_GIT/withdrawals.jsonl"
check "a ledger inside the worktree refuses" 1 "inside a git worktree"
check_absent "…and is not created" "$REPO_GIT/withdrawals.jsonl"
check_tree_clean "…and leaves the tree clean" "$REPO_GIT"

run "$REPO_GIT" --roster "$TRACKED_ROSTER" --withdraw nobody@example.com \
    --withdrawals "$WORK/ledger.jsonl"
check "an address that is not in the roster refuses" 1 "withdraw: no row for nobody@example.com"

printf 'not json at all\n' > "$WORK/broken.jsonl"
run "$REPO_GIT" --roster "$TRACKED_ROSTER" --withdrawals "$WORK/broken.jsonl"
check "an unreadable ledger fails closed" 1 "not readable JSON"

run "$REPO_GIT" --roster "$TRACKED_ROSTER" --withdraw row@example.com \
    --withdrawals "$WORK/ledger.jsonl"
check "a repeated withdraw is idempotent" 0 "already withdrawn"
if [[ "$(wc -l < "$WORK/ledger.jsonl")" -eq 1 ]]; then
  printf '  ok    %-46s still 1 line\n' "…and appends nothing"
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s lines\n' "…and appends nothing" "$(wc -l < "$WORK/ledger.jsonl")"
  failed=$((failed + 1))
fi

run "$REPO_GIT" --roster "$UNCONSENTED_ROSTER" --withdraw cold@example.com \
    --withdrawals "$WORK/ledger.jsonl"
check "a row with no consent still withdraws" 0 "do-not-contact"
if [[ "$(ledger_field '.consent_recorded_at' "$WORK/ledger.jsonl")" == "null" ]]; then
  printf '  ok    %-46s consent_recorded_at=null\n' "…recorded as a do-not-contact"
  passed=$((passed + 1))
else
  printf '  FAIL  %-46s %s\n' "…recorded as a do-not-contact" "$(ledger_field '.consent_recorded_at' "$WORK/ledger.jsonl")"
  failed=$((failed + 1))
fi

echo
echo "a withdrawal takes the address out of the wave"
run "$REPO_GIT" --roster "$TRACKED_ROSTER" --withdrawals "$WORK/ledger.jsonl"
check "the ledger is reported" 0 "WITHDRAWAL LEDGER: 1 row(s) will NOT be emailed"
check "…naming the address and the date" 0 "row@example.com (withdrawn $TODAY)"
if printf '%s' "$out" | grep -qF "── row 1: row@example.com"; then
  printf '  FAIL  %-46s row 1 is still rendered as emailable\n' "the withdrawn row is not walked"
  failed=$((failed + 1))
else
  printf '  ok    %-46s row 1 absent from the walk\n' "the withdrawn row is not walked"
  passed=$((passed + 1))
fi
check "…while the others are" 0 "── row 0: wave@example.com"

# consent: true in the roster does not undo a recorded withdrawal.
jq '.roster[1].consent = true' "$REPO_GIT/$TRACKED_ROSTER" > "$WORK/reconsented.json"
run "$REPO_GIT" --roster "$WORK/reconsented.json" --withdrawals "$WORK/ledger.jsonl"
check "roster consent:true does not re-enable" 0 "WITHDRAWAL LEDGER: 1 row(s) will NOT be emailed"
if printf '%s' "$out" | grep -qF "── row 1: row@example.com"; then
  printf '  FAIL  %-46s a re-run of consent:true walked the row\n' "the gate still refuses"
  failed=$((failed + 1))
else
  printf '  ok    %-46s the gate still refuses\n' "the gate still refuses"
  passed=$((passed + 1))
fi

echo
echo "the consent gate is unchanged"
run "$REPO_GIT" --roster "$NO_CONSENT_ROSTER"
check "no wave consent refuses every row" 0 "CONSENT GATE: 3 row(s) will NOT be emailed"
if printf '%s' "$out" | grep -qF "── row"; then
  printf '  FAIL  %-46s a row was walked\n' "…and walks nothing"
  failed=$((failed + 1))
else
  printf '  ok    %-46s and walks nothing\n' "…and walks nothing"
  passed=$((passed + 1))
fi

run "$REPO_GIT" --roster "$NO_CONSENT_ROSTER" --withdraw a@example.com \
    --withdrawals "$WORK/ledger.jsonl"
check "a row with no consent can still withdraw" 0 "withdrew a@example.com"
run "$REPO_GIT" --roster "$NO_CONSENT_ROSTER" --withdrawals "$WORK/ledger.jsonl"
check "a withdraw does not unblock the others" 0 "CONSENT GATE: 2 row(s) will NOT be emailed"
check "…the withdrawn row is refused too" 0 "WITHDRAWAL LEDGER: 1 row(s) will NOT be emailed"
if printf '%s' "$out" | grep -qF "── row"; then
  printf '  FAIL  %-46s a row was walked\n' "…and the wave is still empty"
  failed=$((failed + 1))
else
  printf '  ok    %-46s and the wave is still empty\n' "…and the wave is still empty"
  passed=$((passed + 1))
fi

run "$REPO_GIT" --roster "$TRACKED_ROSTER"
check "the tracked roster dry-runs clean" 0 "skipped (consent):  1"
check "…reporting no withdrawals by default" 0 "withdrawn:          0"
check_tree_clean "a plain dry-run" "$REPO_GIT"

before="$(snapshot "$REPO/$EXAMPLE")"
run "$REPO" --roster "$EXAMPLE"
check "the committed example roster dry-runs" 0 "rows:               3"
check_unchanged "…and the real repo roster is untouched" "$REPO/$EXAMPLE" "$before"

echo
echo "usage"
run "$REPO" --help
check "--help exits 0" 0 "ALPHA_WITHDRAWALS"
check "…and documents the ledger" 0 "--withdrawals PATH"
run "$REPO" --nope
check "an unknown flag is still usage (3)" 3 "unknown argument: --nope"
run "$REPO" --roster "$WORK/does-not-exist.json"
check "a missing roster still refuses" 1 "ABORT: no roster at"

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
