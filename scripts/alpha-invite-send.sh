#!/usr/bin/env bash
# Send the closed-alpha invite wave — or, by default, render exactly what it
# would send without sending anything.
#
# Written for DEF-120. The premise it encodes, taken from the source rather than
# from a plan:
#
#   The product has no invite feature. `team.invite` is a database-only insert
#   and sends no mail (routers/team.rs). The only outbound path in the whole
#   backend is the mailer, and the mailer only ever sends two things: a
#   verification link and a password-reset link (server/src/mailer.rs). So an
#   "invite" is not a thing this product does — what it can do is create an
#   account and email that person a 24h verification link, which is the closest
#   thing to an invite that exists. This script drives that path.
#
# Three consequences this script refuses to paper over:
#
#   1. `user.signup` requires a username AND a password for the recipient
#      (routers/user.rs:94-102). Neither is invented per person. A username is
#      derived from the address the person gave and reported, never chosen. A
#      password is machine-generated at send time, unique per row, never printed
#      and never written to the roster — the recipient replaces it themselves
#      through the product's own `user.requestPasswordReset` flow, which is the
#      only password path that does not require anyone to pick a stranger's
#      credential. A row that supplies its own password is validated, not
#      trusted.
#   2. The verification email has no List-Unsubscribe header and no opt-out
#      (mailer.rs:168-181). There is no unsubscribe path in the product. The
#      honest equivalent is upstream of the send: a consent gate that refuses to
#      email anyone who has not consented, plus `--withdraw` to take a row out
#      of the wave. A real unsubscribe is a mailer change, not a script change.
#      A withdrawal is recorded in a ledger OUTSIDE the repo and honoured by
#      that same gate. It is never written back into the roster (DEF-228): the
#      roster is a tracked file when it is the example, so a `git commit -a`
#      after a withdrawal would stage a consent change nobody reviewed.
#   3. The roster accepts the shape the DEF-120 card actually collects. A bare
#      `Name <email>` list plus one consent answer is enough — consent is
#      recorded once for the wave rather than hand-filled twelve times, and a
#      `consent: "partial"` answer still requires each consented row to be
#      marked. Answering the card must not produce a wave of zero.
#
# Safe by default: with no flags this renders every request and every email and
# sends nothing. Real sending needs `--send`.
#
# Usage:
#   scripts/alpha-invite-send.sh [--send] [--roster PATH] [--base-url URL]
#                                [--log PATH] [--verify] [--withdraw EMAIL]
#                                [--withdrawals PATH]
#
# Env:
#   ALPHA_ROSTER, ALPHA_BASE_URL, ALPHA_LOG   defaults for the flags above.
#   ALPHA_WITHDRAWALS                         the withdrawal ledger. Defaults
#                                            to ~/.local/state/alpha-invite/
#                                            withdrawals.jsonl. Must be outside
#                                            the git worktree; a send honours
#                                            every address in it.
#   ALPHA_ADMIN_EMAIL / ALPHA_ADMIN_PASSWORD  admin session for --verify
#                                            (or ALPHA_SESSION_COOKIE).
#
# `--withdraw EMAIL` records the withdrawal and exits 0 without sending. The
# roster is not written, ever — the address lands in the ledger instead, and the
# consent gate below refuses it on the next run whatever the roster says.
#
# Exit codes: 0 ok, 1 preflight/validation failure, 2 send completed with
# failures (see the log), 3 usage.
set -euo pipefail

SEND=0
WITHDRAW=""
ROSTER="${ALPHA_ROSTER:-data/crossword/alpha-roster.json}"
BASE_URL="${ALPHA_BASE_URL:-https://crosswords-staging.casazza.io}"
LOG="${ALPHA_LOG:-}"
VERIFY=0
# The withdrawal ledger is append-only and lives outside the repo. It is a
# separate file from --log on purpose: the send path truncates --log at the
# start of a wave, and a consent record must not be the thing a wave overwrites.
if [[ -n "${ALPHA_WITHDRAWALS:-}" ]]; then
  WITHDRAWALS="$ALPHA_WITHDRAWALS"
elif [[ -n "${XDG_STATE_HOME:-}" ]]; then
  WITHDRAWALS="$XDG_STATE_HOME/alpha-invite/withdrawals.jsonl"
elif [[ -n "${HOME:-}" ]]; then
  WITHDRAWALS="$HOME/.local/state/alpha-invite/withdrawals.jsonl"
else
  WITHDRAWALS=""
fi

# Print the header comment. Keyed to the `set -euo pipefail` line rather than a
# line number, so editing the header above cannot silently truncate --help.
usage() {
  sed -n '2,/^set -euo pipefail/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --send)        SEND=1 ;;
    --verify)      VERIFY=1 ;;
    --roster)      ROSTER="${2:?--roster needs a path}"; shift ;;
    --base-url)    BASE_URL="${2:?--base-url needs a URL}"; shift ;;
    --log)         LOG="${2:?--log needs a path}"; shift ;;
    --withdraw)    WITHDRAW="${2:?--withdraw needs an email}"; shift ;;
    --withdrawals) WITHDRAWALS="${2:?--withdrawals needs a path}"; shift ;;
    -h|--help)     usage; exit 0 ;;
    *)             echo "unknown argument: $1" >&2; usage >&2; exit 3 ;;
  esac
  shift
done
BASE_URL="${BASE_URL%/}"

command -v curl    >/dev/null || { echo "curl is required"    >&2; exit 1; }
command -v jq      >/dev/null || { echo "jq is required"      >&2; exit 1; }
command -v openssl >/dev/null || { echo "openssl is required" >&2; exit 1; }

# One append-only line per attempt. Failures have to be greppable after the fact
# — the send is the step where a partial failure is invisible from the exit code.
logline() {
  [[ -n "$LOG" ]] || return 0
  printf '%s\n' "$1" >> "$LOG"
}

# The git worktree that would contain a path, or empty if it is outside every
# repo. Resolving the parent rather than the file itself lets a ledger be created
# on demand. A consent record must not be written where `git commit -a` could
# sweep it in, so this is the check that keeps the tree clean.
worktree_root() {
  local dir="$1"
  while [[ ! -d "$dir" ]]; do
    dir="$(dirname -- "$dir")"
    [[ "$dir" == "/" ]] && return 0
  done
  git -C "$dir" rev-parse --show-toplevel 2>/dev/null || true
}

# A YYYY-MM-DD that names a real calendar day, as epoch seconds, or empty.
# jq's strptime normalises (2026-02-31 becomes 2026-03-02), so the round trip is
# what separates a date from a string that is merely shaped like one. jq rather
# than `date -d` on purpose: the dependency set is curl, jq and openssl, and
# `date -d` is GNU-only. An unparseable date yields nothing, which the caller
# reads as "refuse" — fail closed.
date_epoch() {
  jq -rn --arg d "$1" '
    ($d | try (strptime("%Y-%m-%d") | mktime | strftime("%Y-%m-%d")) catch null) as $back
    | if $back == $d then ($d | strptime("%Y-%m-%d") | mktime) else empty end'
}

# The withdrawal ledger: one JSON object per line, outside the repo. The gate
# honours it, so a withdrawal takes a person out of the wave without the roster
# being touched. An unreadable ledger is a fail-closed abort, not a silent
# "nothing is withdrawn" — the send would otherwise mail someone who opted out.
# WITHDRAWN (addresses) and WITHDRAWN_DATES (address -> withdrawal date) are
# deliberately global: the gate reads them per row.
load_withdrawals() {
  WITHDRAWN='[]'
  WITHDRAWN_DATES='{}'
  [[ -f "$WITHDRAWALS" ]] || return 0
  local parsed
  if ! parsed="$(jq -s '
      [ .[] | select((.event // "withdraw") == "withdraw") ]
      | { emails: [ .[] | (.email // empty) | select(length > 0) ],
          dates:  (reduce .[] as $r ({}; .[$r.email] = ($r.consent_withdrawn_at // ""))) }' \
      "$WITHDRAWALS" 2>&1)"; then
    echo "ABORT: the withdrawal ledger at $WITHDRAWALS is not readable JSON." >&2
    printf '  %s\n' "${parsed%%$'\n'*}" >&2
    echo "  A withdrawal that cannot be read is a withdrawal that is not" >&2
    echo "  honoured. Fix the ledger, or point --withdrawals at the right one," >&2
    echo "  and re-run." >&2
    exit 1
  fi
  WITHDRAWN="$(jq -r '.emails' <<< "$parsed")"
  WITHDRAWN_DATES="$(jq -r '.dates' <<< "$parsed")"
}

# ── Preconditions ────────────────────────────────────────────────────────────
if [[ ! -f "$ROSTER" ]]; then
  echo "ABORT: no roster at $ROSTER." >&2
  echo "Copy data/crossword/alpha-roster.example.json and fill it in. The real" >&2
  echo "roster is gitignored: it holds addresses and must never enter git." >&2
  exit 1
fi

# The roster is either a bare array (one object per person) or an object with
# `roster` plus a wave-level `consent` block. The object form exists because the
# DEF-120 card collects one consent answer for the wave, not three fields per
# person; requiring the per-row fields would mean the card's own answer could
# not be executed without hand-editing twelve rows first.
ROSTER_KIND="$(jq -r 'if type == "array" then "array" elif (.roster | type) == "array" then "object" else "invalid" end' "$ROSTER" 2>/dev/null || echo invalid)"
if [[ "$ROSTER_KIND" == invalid ]]; then
  echo "ABORT: $ROSTER is neither a JSON array nor an object with a roster array." >&2
  exit 1
fi

TMPD="$(mktemp -d)"
trap 'rm -rf "$TMPD"' EXIT
if [[ "$ROSTER_KIND" == object ]]; then
  jq '.roster' "$ROSTER" > "$TMPD/roster.json"
else
  jq '.' "$ROSTER" > "$TMPD/roster.json"
fi

# The extracted rows must be a non-empty array. Both cases below used to fall
# through as a green run, which is the one shape this script refuses to produce:
#
#   * An empty or truncated file. `jq 'length'` on an empty file prints nothing
#     and still exits 0, so ROWS became "" and the walk below iterated zero
#     times and exited 0. A wave that mailed nobody reported success.
#   * `{"roster":[]}`, which parses cleanly and yields length 0. Same green run,
#     same "nothing left to do" reading.
#
# A truncated roster is an ordinary way to lose a file (a partial write, a
# heredoc that lost its stdin — which is how this was found). It must be a hard
# stop, not an empty summary.
if ! jq -e 'type == "array"' "$TMPD/roster.json" >/dev/null 2>&1; then
  echo "ABORT: $ROSTER did not yield a JSON array of roster rows." >&2
  echo "  The file is empty or truncated — a partial write, or a redirect" >&2
  echo "  whose stdin went nowhere. Restore the real roster and re-run." >&2
  exit 1
fi
ROWS="$(jq 'length' "$TMPD/roster.json")"
if [[ "$ROWS" -eq 0 ]]; then
  echo "ABORT: $ROSTER contains zero roster rows." >&2
  echo "  A zero-row wave exits 0 and reads like a finished send. Nothing was" >&2
  echo "  emailed and nothing is wrong with the host — the roster is the problem." >&2
  exit 1
fi

# Wave-level consent, honoured only when all three provenance fields are present
# — the same three the per-row gate demands. `all` covers the wave; `partial`
# deliberately does not, because which subset agreed is per-person information
# this script has no other way to know.
WAVE_CONSENT_RAW="$(jq -r '
  (if type == "object" then (.consent // {}) else {} end)
  | if ((.answer   // "") | test("^(all|partial|none)$"))
    and ((.recorded_at // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))
    and (((.source // "") | length) > 0)
    then "\(.answer)|\(.recorded_at)|\(.source)" else "" end' "$ROSTER")"
WAVE_ANSWER="${WAVE_CONSENT_RAW%%|*}"
WAVE_RECORDED=""
WAVE_SOURCE=""
if [[ -n "$WAVE_CONSENT_RAW" ]]; then
  WAVE_RECORDED="$(cut -d'|' -f2 <<< "$WAVE_CONSENT_RAW")"
  WAVE_SOURCE="$(cut -d'|' -f3 <<< "$WAVE_CONSENT_RAW")"
fi

# When this wave is planned to go out. A separate field from the consent date
# because "we agreed to this on the 1st" and "this sends on the 12th" are
# different facts, and the wave is routinely the second while the card that
# collects the first has not been answered yet. Optional; validated for shape
# only, and a future value is the point of it, not an error.
WAVE_SEND_ON="$(jq -r 'if type == "object" then (.wave.send_on // "") else "" end' "$ROSTER")"

# ── Consent dates cannot be in the future (DEF-229) ───────────────────────────
# A consent date is a claim about a human act: somebody agreed on that day. A
# day that has not happened yet is not a claim, it is a planned wave wearing a
# consent field's name — and the roster format cannot tell which one was meant,
# so the only safe reading is the one that does not mail. Both the wave answer
# and the per-row dates are checked, and either refuses the whole run rather
# than skipping: a roster carrying a consent date that never happened has a data
# error in it, and sending the other eleven rows would report a wave built on
# that error as a finished send.
#
# The mirror image already exists on the withdrawal side (DEF-228), which
# refuses to date a withdrawal before the consent it retracts. This closes the
# other direction: a consent may not be dated after the mail that cites it.
TODAY="$(date -u +%F)"
TODAY_EPOCH="$(date_epoch "$TODAY")"

abort_bad_consent_date() {
  local field="$1" when="$2" who="$3"
  if [[ -z "$(date_epoch "$when")" ]]; then
    echo "ABORT: $who has $field '$when', which is not a real calendar date." >&2
    echo "  jq normalises 2026-02-31 to 2026-03-02, so this is checked by" >&2
    echo "  round trip and the value above is a string, not a day. Correct it" >&2
    echo "  to the day the person actually agreed (YYYY-MM-DD) and re-run." >&2
  else
    echo "ABORT: $who has $field '$when', which is after today ($TODAY)." >&2
    echo "  A consent date is the day somebody actually agreed. A day that has" >&2
    echo "  not happened yet is a scheduled wave, not consent, and sending on" >&2
    echo "  it would mail before the agreement it claims to rest on." >&2
    echo "  Either correct $field to the day the person agreed, or move the" >&2
    echo "  planned send to wave.send_on (YYYY-MM-DD, may be in the future)." >&2
  fi
  exit 1
}

if [[ -n "$WAVE_RECORDED" ]]; then
  if [[ -z "$(date_epoch "$WAVE_RECORDED")" \
     || "$TODAY_EPOCH" -lt "$(date_epoch "$WAVE_RECORDED")" ]]; then
    abort_bad_consent_date "consent.recorded_at" "$WAVE_RECORDED" "the wave answer"
  fi
fi

if [[ -n "$WAVE_SEND_ON" && -z "$(date_epoch "$WAVE_SEND_ON")" ]]; then
  echo "ABORT: wave.send_on is '$WAVE_SEND_ON', which is not a real calendar date." >&2
  echo "  Correct it to the day the wave is planned to go out (YYYY-MM-DD)." >&2
  exit 1
fi

for i in $(seq 0 $((ROWS - 1))); do
  row_when="$(jq -r --argjson i "$i" '.[$i].consent_recorded_at // ""' "$TMPD/roster.json")"
  [[ -z "$row_when" ]] && continue
  if [[ -z "$(date_epoch "$row_when")" \
     || "$TODAY_EPOCH" -lt "$(date_epoch "$row_when")" ]]; then
    row_who="$(jq -r --argjson i "$i" '(.[$i].email // ("row " + ($i | tostring)))' "$TMPD/roster.json")"
    abort_bad_consent_date "consent_recorded_at" "$row_when" "row $i ($row_who)"
  fi
done

# Loaded before the header so the ledger in force is printed on every run, and
# before the gate so a withdrawn address cannot be emailed.
load_withdrawals

echo "alpha invite send  $(date -u '+%Y-%m-%dT%H:%MZ')"
echo "host:              $BASE_URL"
echo "roster:            $ROSTER"
if [[ -n "$WAVE_ANSWER" ]]; then
  echo "wave consent:      $WAVE_ANSWER (recorded $WAVE_RECORDED, source: $WAVE_SOURCE)"
else
  echo "wave consent:      none recorded"
fi
if [[ -n "$WAVE_SEND_ON" ]]; then
  if [[ "$(date_epoch "$WAVE_SEND_ON")" -gt "$TODAY_EPOCH" ]]; then
    echo "wave scheduled:    $WAVE_SEND_ON (future — a plan, not a consent;"
    echo "                   today's date is what gates sending)"
  else
    echo "wave scheduled:    $WAVE_SEND_ON"
  fi
fi
if [[ -n "$WITHDRAW" ]]; then
  echo "withdrawals:       ${WITHDRAWALS:-(none configured)} — this run records $WITHDRAW"
elif [[ "$(jq 'length' <<< "$WITHDRAWN")" -gt 0 ]]; then
  echo "withdrawals:       $WITHDRAWALS ($(jq -r 'unique | join(", ")' <<< "$WITHDRAWN"))"
else
  echo "withdrawals:       none recorded${WITHDRAWALS:+ ($WITHDRAWALS)}"
fi
if [[ $SEND == 1 ]]; then
  echo "mode:              SEND"
else
  echo "mode:              DRY-RUN (nothing is sent)"
fi
echo

# ── Withdraw: take a person out of the wave ──────────────────────────────────
# Placed before the network preflight on purpose: a withdrawal is a local ledger
# write, and the ability to withdraw must not depend on the target host being up
# or on mailDelivery being smtp. Staging being down is exactly when someone asks
# to be taken off the list.
#
# DEF-228. Two things this block used to do are gone, and both are deliberate:
#
#   * It rewrote the roster in place. That file is tracked when it is the
#     example and gitignored when it is real, and `mv` over either dirties the
#     working tree — so the next `git commit -a` silently stages a consent
#     change nobody reviewed. The roster is now never written at all. The
#     withdrawal lands in the ledger and the gate below honours it, which is the
#     same effect with nothing left in the tree to commit.
#
#   * It stamped `consent_withdrawn_at` from the system clock. The wave is the
#     Oct 12 wave, so consent dates legitimately sit in the future relative to
#     "now", and a naive `date +%F` wrote a withdrawal three days before the
#     consent it withdrew. A ledger that can say "withdrawn before it was
#     granted" is not a consent ledger, so the preflight refuses instead of
#     emitting one.
if [[ -n "$WITHDRAW" ]]; then
  command -v git >/dev/null || { echo "git is required for --withdraw" >&2; exit 1; }

  idx="$(jq -r --arg e "$WITHDRAW" 'to_entries[] | select((.value.email // "") == $e) | .key' "$TMPD/roster.json" | head -1)"
  if [[ -z "$idx" ]]; then
    echo "withdraw: no row for $WITHDRAW" >&2
    exit 1
  fi

  # The consent this withdrawal retracts: the row's own date, or the wave's
  # when the row inherits a wave `all` answer. A row with no recorded consent
  # has nothing to date a withdrawal against; it is recorded as a
  # do-not-contact instead, which is the safer reading of "take me off".
  recorded="$(jq -r --argjson i "$idx" --arg wa "$WAVE_ANSWER" --arg wr "$WAVE_RECORDED" '
      .[$i] as $r
      | (if ($r.consent_recorded_at // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
         then $r.consent_recorded_at
         elif $wa == "all" and (($r | has("consent")) | not)
              and ($wr | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))
         then $wr
         else "" end)' "$TMPD/roster.json")"

  today="$(date -u +%F)"
  if [[ -n "$recorded" ]]; then
    recorded_epoch="$(date_epoch "$recorded")"
    if [[ -z "$recorded_epoch" ]]; then
      echo "ABORT: row $idx ($WITHDRAW) has consent_recorded_at '$recorded'," >&2
      echo "  which is not a real calendar date. A withdrawal cannot be ordered" >&2
      echo "  against it. Correct the date to the day the person actually agreed" >&2
      echo "  (YYYY-MM-DD) and re-run." >&2
      exit 1
    fi
    if [[ "$(date_epoch "$today")" -lt "$recorded_epoch" ]]; then
      echo "ABORT: withdrawing $WITHDRAW would record consent_withdrawn_at" >&2
      echo "  $today, before the consent it withdraws was recorded on" >&2
      echo "  $recorded (row $idx, field consent_recorded_at)." >&2
      echo "  A withdrawal that precedes the consent is a record this script" >&2
      echo "  will not emit. Either the roster date is wrong — correct it, with" >&2
      echo "  the source, to the day the person actually agreed — or the" >&2
      echo "  withdrawal is premature: re-run it on or after $recorded." >&2
      exit 1
    fi
  else
    echo "  note: row $idx ($WITHDRAW) has no consent_recorded_at, so there is" >&2
    echo "  nothing to date the withdrawal against. Recorded as a do-not-contact." >&2
  fi

  if [[ -z "$WITHDRAWALS" ]]; then
    echo "ABORT: --withdraw has nowhere to record this: --withdrawals PATH or" >&2
    echo "  ALPHA_WITHDRAWALS. The roster is never modified, so without a ledger" >&2
    echo "  the withdrawal would be a no-op and the next send would still mail" >&2
    echo "  $WITHDRAW. Put it outside the repo, e.g." >&2
    echo "  ~/.local/state/alpha-invite/withdrawals.jsonl" >&2
    exit 1
  fi
  if [[ -e "$WITHDRAWALS" && ! -f "$WITHDRAWALS" ]]; then
    echo "ABORT: $WITHDRAWALS exists and is not a regular file." >&2
    exit 1
  fi
  if [[ -n "$(worktree_root "$WITHDRAWALS")" ]]; then
    echo "ABORT: the withdrawal ledger $WITHDRAWALS is inside a git worktree." >&2
    echo "  A consent record must not be written where 'git commit -a' could" >&2
    echo "  sweep it in. Point --withdrawals at a path outside the repo." >&2
    exit 1
  fi
  ledger_dir="$(dirname -- "$WITHDRAWALS")"
  if [[ ! -d "$ledger_dir" ]] && ! mkdir -p -- "$ledger_dir"; then
    echo "ABORT: cannot create $ledger_dir for the withdrawal ledger." >&2
    exit 1
  fi
  if [[ -f "$WITHDRAWALS" && ! -w "$WITHDRAWALS" ]]; then
    echo "ABORT: the withdrawal ledger $WITHDRAWALS is not writable." >&2
    exit 1
  fi

  if jq -e --arg e "$WITHDRAW" 'index($e) != null' <<< "$WITHDRAWN" >/dev/null 2>&1; then
    prior="$(jq -r --arg e "$WITHDRAW" '.[$e] // "an unknown date"' <<< "$WITHDRAWN_DATES")"
    echo "withdraw: $WITHDRAW is already withdrawn ($prior) in $WITHDRAWALS."
    echo "  Nothing appended. To invite them again, delete that line from the"
    echo "  ledger: a re-consent is a new, dated act, not a re-run of this."
    exit 0
  fi

  jq -nc --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
         --arg e "$WITHDRAW" --argjson row "$idx" --arg roster "$ROSTER" \
         --arg rec "$recorded" --arg wd "$today" \
         --arg by "${SUDO_USER:-${USER:-unknown}}@$(hostname -s 2>/dev/null || echo unknown)" \
         '{ts:$ts,event:"withdraw",email:$e,row:$row,roster:$roster,
           consent_recorded_at:(if $rec == "" then null else $rec end),
           consent_withdrawn_at:$wd,by:$by}' >> "$WITHDRAWALS"

  echo "withdrew $WITHDRAW from the wave (row $idx)."
  echo "  recorded in $WITHDRAWALS as consent_withdrawn_at=$today"
  echo "  the roster was not modified. Re-run without --withdraw to see the gate."
  exit 0
fi

# ── Pre-flight: would mail even leave this host? ─────────────────────────────
# DEF-201: with SMTP_USER/SMTP_PASSWORD unset the mailer logs the body and
# returns Ok, and signup still answers {"success":true}. A green HTTP response
# and a dead transport are indistinguishable from outside the cluster — which
# is how 12 invites could be "sent" into a pod log. /api/config reports the live
# mode, so check it before trusting any 200.
config="$(curl -sS -m 30 "$BASE_URL/api/config")"
mode="$(printf '%s' "$config" | jq -r '.features.mailDelivery // "absent"')"
env_name="$(printf '%s' "$config" | jq -r '.environment // "unknown"')"
echo "  /api/config: environment=$env_name mailDelivery=$mode"
if [[ "$mode" != "smtp" ]]; then
  echo "  ABORT: mailDelivery is '$mode', not 'smtp'." >&2
  echo "  This host will log the email bodies and drop them. No invite would" >&2
  echo "  leave the cluster. Fix SMTP_USER/SMTP_PASSWORD on this deploy first" >&2
  echo "  (DEF-201) — do not read a 200 from signup as proof of delivery." >&2
  exit 1
fi
echo

echo "  rows: $ROWS"

# ── Consent gate (default-deny) ──────────────────────────────────────────────
# A row is emailable only with an explicit consent flag AND the date it was
# recorded AND where it came from. Anything else is skipped and reported —
# quietly dropping a person, or worse, quietly mailing one who never consented,
# is the failure this gate exists to prevent.
#
# A row qualifies on its own, or inherits a wave-level `all` answer. A wave
# `partial` answer grants nothing here: which people agreed is per-person
# information, and a wave-level "some" would let the script guess.
#
# The withdrawal ledger overrides both. It is a recorded "do not send", so it
# wins whatever the roster says — including `consent: true`, and including a
# re-run after someone edits the roster by hand (DEF-228). Nothing here can
# un-withdraw an address; only deleting its line from the ledger can, and that
# is a human decision made in a consent record rather than by this script.
consented() {
  jq -r --argjson i "$1" --arg wa "$WAVE_ANSWER" --argjson w "$WITHDRAWN" '
    .[$i] as $r
    | (if ($r.consent // false) == true
          and (($r.consent_recorded_at // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))
          and (($r.consent_source // "") | length) > 0
       then "yes" else "no" end) as $own
    | if ($w | index($r.email // "")) != null then "no"
      elif $own == "yes" then "yes"
      elif $wa == "all" and (($r | has("consent")) | not) then "yes"
      else "no" end' "$TMPD/roster.json"
}

SKIPPED_COUNT=0
SKIPPED_LIST=""
WITHDRAWN_COUNT=0
WITHDRAWN_LIST=""
for i in $(seq 0 $((ROWS - 1))); do
  if [[ "$(consented "$i")" != "yes" ]]; then
    who="$(jq -r --argjson i "$i" '(.[$i].email // ("row " + ($i | tostring)))' "$TMPD/roster.json")"
    if jq -e --arg e "$who" 'index($e) != null' <<< "$WITHDRAWN" >/dev/null 2>&1; then
      WITHDRAWN_LIST+="    - $who (withdrawn $(jq -r --arg e "$who" '.[$e] // "date not recorded"' <<< "$WITHDRAWN_DATES"))"$'\n'
      WITHDRAWN_COUNT=$((WITHDRAWN_COUNT + 1))
    else
      SKIPPED_LIST+="    - $who"$'\n'
      SKIPPED_COUNT=$((SKIPPED_COUNT + 1))
    fi
  fi
done

# --withdraw records the withdrawal, so the gate report below would describe the
# wave as it was, not as it is. Say nothing there and let the re-run speak.
if [[ $SKIPPED_COUNT -gt 0 && -z "$WITHDRAW" ]]; then
  echo
  echo "  CONSENT GATE: $SKIPPED_COUNT row(s) will NOT be emailed:"
  printf '%s' "$SKIPPED_LIST"
  echo "  Each needs consent: true, consent_recorded_at (YYYY-MM-DD), and"
  echo "  consent_source. See data/crossword/alpha-roster.example.json."
fi
if [[ $WITHDRAWN_COUNT -gt 0 && -z "$WITHDRAW" ]]; then
  echo
  echo "  WITHDRAWAL LEDGER: $WITHDRAWN_COUNT row(s) will NOT be emailed:"
  printf '%s' "$WITHDRAWN_LIST"
  echo "  Recorded in the ledger, not in the roster. consent:true here does not"
  echo "  undo a withdrawal; only deleting that line from $WITHDRAWALS does."
fi
echo

# ── The email the product will actually send ─────────────────────────────────
# Reconstructed verbatim from mailer.rs:168-181. The token is minted server-side
# per request (routers/user.rs:158-177), so a dry-run cannot show the live one —
# it shows the shape and says so. A reader must not mistake this for a rendered
# real email.
render_email() {
  local origin="$1" recipient="$2"
  cat <<HTML
From:    Definitely Not Crosswords <noreply@noreply.casazza.io>
To:      ${recipient}
Subject: Verify your email — Definitely Not Crosswords

<p>Welcome! Confirm this address to finish setting up your account:</p>
<p><a href="${origin}/auth/verify-email?token=token_&lt;server-generated&gt;">Verify my email</a></p>
<p>Or paste this link into your browser:<br>${origin}/auth/verify-email?token=token_&lt;server-generated&gt;</p>
<p>This link expires in 24 hours. If you didn't sign up, ignore this email.</p>

(the token is minted by the server per request; a dry-run cannot show the live one)
HTML
}

# tRPC over HTTP takes {"0": input} and answers [{result:{data}}] or
# [{error:{message}}]; the server reads body["0"] (main.rs trpc_post).
trpc() {
  local proc="$1" input="$2" cookie="${3:-}"
  if [[ -n "$cookie" ]]; then
    curl -sS -m 45 -X POST "$BASE_URL/api/trpc/$proc" \
      -H 'Content-Type: application/json' \
      -H "Cookie: next-auth.session-token=$cookie" \
      -d "{\"0\":${input}}"
  else
    curl -sS -m 45 -X POST "$BASE_URL/api/trpc/$proc" \
      -H 'Content-Type: application/json' \
      -d "{\"0\":${input}}"
  fi
}

# The data payload, or a one-line marker. A silently empty result here is how a
# send turns into a fabricated success.
unwrap() {
  jq -r 'if .[0].result then .[0].result.data
         elif .[0].error then "SERVER_ERROR: " + (.[0].error.message // "unknown")
         else "UNEXPECTED_ENVELOPE: " + tostring end'
}

if [[ $VERIFY == 1 ]]; then
  # Did the mail land? The only honest outside-in answer available: an admin
  # read of "emailVerified" per address. It proves the recipient clicked the
  # link; it does NOT prove the provider accepted the message, and it says
  # nothing for anyone who never opened it.
  cookie="${ALPHA_SESSION_COOKIE:-}"
  if [[ -z "$cookie" && -n "${ALPHA_ADMIN_EMAIL:-}" && -n "${ALPHA_ADMIN_PASSWORD:-}" ]]; then
    hdrs="$(curl -sS -m 30 -D - -o /dev/null -X POST \
      "$BASE_URL/api/auth/callback/credentials" \
      -H 'Content-Type: application/x-www-form-urlencoded' \
      --data-urlencode "email=$ALPHA_ADMIN_EMAIL" \
      --data-urlencode "password=$ALPHA_ADMIN_PASSWORD" \
      --data-urlencode 'callbackUrl=/')"
    cookie="$(printf '%s' "$hdrs" \
      | grep -i '^set-cookie:' \
      | sed -n 's/.*next-auth\.session-token=\([^;]*\).*/\1/p' | head -1 || true)"
  fi
  if [[ -z "$cookie" ]]; then
    echo "verify: needs ALPHA_SESSION_COOKIE or ALPHA_ADMIN_EMAIL/ALPHA_ADMIN_PASSWORD." >&2
    echo "  Without an admin session the verification state is not observable." >&2
    exit 1
  fi
  users="$(trpc user.listForAdmin null "$cookie" | unwrap)"
  if [[ "$users" == SERVER_ERROR:* || "$users" == UNEXPECTED_ENVELOPE:* ]]; then
    echo "verify: $users" >&2
    exit 1
  fi
  # One jq pass, joining the roster against the admin list. Doing this as a
  # `while read` loop that re-parses per row is both a subshell (so counters
  # would be lost) and a place to lose the -r flag and re-parse a quoted
  # string as JSON.
  echo "── verification state per roster row"
  jq -rn --argjson users "$users" --slurpfile roster "$TMPD/roster.json" '
    ($roster[0] | map(.email // empty)) as $emails
    | $emails[]
    | . as $e
    | (first($users[] | select((.email // "") == $e)) // null) as $m
    | "  \($e)\t"
      + (if $m == null then "no account"
         elif $m.emailVerified == null then "NOT verified"
         else "verified" end)
  '
  echo
  echo "  'no account' means the send never created the row — re-run and read the log."
  echo "  'NOT verified' is ambiguous: unread mail, or mail that never arrived (DEF-208)."
  exit 0
fi

# ── Walk the roster ──────────────────────────────────────────────────────────
if [[ -n "$LOG" ]]; then : > "$LOG"; fi

# A username is a product identifier, not a secret, and the card gives an email
# and a name. Deriving one from the address they supplied is not inventing
# anything about them; it is reading the local part. It is always reported, so a
# human can see every derived username before the send.
derive_username() {
  local base="${1%%@*}"
  printf '%s' "$base" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -e 's/[^a-z0-9._-]//g' -e 's/^[._-]*//' -e 's/[._-]*$//'
}

# 32 characters from 24 random bytes. Machine-generated, unique per row, never
# printed, never logged, never written back to the roster. The recipient replaces
# it through the product's own reset flow, so it never has to reach them.
gen_password() {
  openssl rand -base64 24 | tr -d '\n=+/' | cut -c1-32
}

# ── Preflight: nothing is sent until every emailable row is sendable ─────────
# A structurally broken row would otherwise fail mid-wave, after earlier rows had
# already been mailed. For 12 real addresses that is the difference between a
# clean abort and a partial send nobody can take back.
PROBLEMS=()
for i in $(seq 0 $((ROWS - 1))); do
  [[ "$(consented "$i")" == "yes" ]] || continue
  r_email="$(jq -r --argjson i "$i" '.[$i].email // ""'    "$TMPD/roster.json")"
  r_name="$(jq  -r --argjson i "$i" '.[$i].name // ""'       "$TMPD/roster.json")"
  r_user="$(jq  -r --argjson i "$i" '.[$i].username // ""'  "$TMPD/roster.json")"
  if [[ -z "$r_email" || "$r_email" != *"@"* || "$r_email" == *" "* ]]; then
    PROBLEMS+=("row $i: no usable email address (got '$r_email')")
    continue
  fi
  if [[ -z "$r_name" ]]; then
    PROBLEMS+=("row $i ($r_email): no name — user.signup rejects a missing name")
  fi
  if [[ -z "$r_user" && -z "$(derive_username "$r_email")" ]]; then
    PROBLEMS+=("row $i ($r_email): no username, and none is derivable from the address")
  fi
done
if [[ ${#PROBLEMS[@]} -gt 0 ]]; then
  echo "ABORT: ${#PROBLEMS[@]} emailable row(s) cannot be sent:" >&2
  printf '  - %s\n' "${PROBLEMS[@]}" >&2
  echo "Nothing was sent. Fix the roster and re-run." >&2
  exit 1
fi

ok=0
failed=0
generated=0
for i in $(seq 0 $((ROWS - 1))); do
  [[ "$(consented "$i")" == "yes" ]] || continue

  email="$(jq -r --argjson i "$i" '.[$i].email' "$TMPD/roster.json")"
  name="$(jq    -r --argjson i "$i" '.[$i].name'    "$TMPD/roster.json")"
  username="$(jq -r --argjson i "$i" '.[$i].username // ""' "$TMPD/roster.json")"
  password="$(jq -r --argjson i "$i" '.[$i].password // ""' "$TMPD/roster.json")"

  if [[ -z "$username" ]]; then
    username="$(derive_username "$email")"
    username_source="derived from the address"
  else
    username_source="from the roster"
  fi

  # A password that is there but too short is a mistake worth stopping on, not a
  # row to skip quietly: skipping is how a person silently never gets invited.
  if [[ -n "$password" && ${#password} -lt 8 ]]; then
    echo "ABORT: row $i ($email) supplies a password under 8 characters." >&2
    echo "  user.signup rejects it (routers/user.rs:99-101). Fix the roster," >&2
    echo "  or remove the password field and let one be generated." >&2
    exit 1
  fi

  echo "── row $i: $email"
  echo "   name:     $name"
  echo "   username: $username ($username_source)"

  if [[ $SEND == 0 ]]; then
    echo "   request:   POST $BASE_URL/api/trpc/user.signup"
    echo "   body:      {\"0\":{\"email\":\"$email\",\"name\":\"$name\",\"username\":\"$username\",\"password\":\"<redacted>\"}}"
    echo "   fallback:  if the account exists, POST $BASE_URL/api/trpc/user.resendVerification"
    if [[ -z "$password" ]]; then
      echo "   password:  generated at send time — machine-generated, unique per row,"
      echo "              never printed and never written to the roster. The recipient"
      echo "              sets their own from the login page's forgot-password link"
      echo "              (user.requestPasswordReset, routers/user.rs:218+)."
    fi
    echo "   email the server will send:"
    render_email "$BASE_URL" "$email" | sed 's/^/     /'
    echo
    continue
  fi

  if [[ -z "$password" ]]; then
    password="$(gen_password)"
    generated=$((generated + 1))
    echo "   password:  generated (value not shown)"
  fi

  # Real send. signup creates the account and mails the verification link; if the
  # address is already registered, resendVerification mails a fresh link to the
  # existing unverified account. Neither needs an admin session, and
  # resendVerification is deliberately non-enumerating (routers/user.rs:179-216).
  body="$(jq -nc --arg e "$email" --arg n "$name" --arg u "$username" --arg p "$password" \
    '{email:$e,name:$n,username:$u,password:$p}')"
  resp="$(trpc user.signup "$body" | unwrap)"
  action="user.signup"

  if [[ "$resp" == SERVER_ERROR:* && "$resp" == *"already exists"* ]]; then
    resp="$(trpc user.resendVerification "$(jq -nc --arg e "$email" '{email:$e}')" | unwrap)"
    action="user.resendVerification"
  fi

  if [[ "$resp" == SERVER_ERROR:* || "$resp" == UNEXPECTED_ENVELOPE:* ]]; then
    echo "   FAILED ($action): $resp"
    failed=$((failed + 1))
    logline "$(jq -nc --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg e "$email" \
      --arg a "$action" --arg r "$resp" '{ts:$ts,email:$e,action:$a,ok:false,error:$r}')"
  else
    echo "   sent via $action — $(jq -c . <<<"$resp")"
    ok=$((ok + 1))
    logline "$(jq -nc --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg e "$email" \
      --arg a "$action" '{ts:$ts,email:$e,action:$a,ok:true}')"
  fi
  echo
done

# ── Summary ──────────────────────────────────────────────────────────────────
echo "── summary"
echo "  rows:               $ROWS"
echo "  skipped (consent):  $SKIPPED_COUNT"
echo "  withdrawn:          $WITHDRAWN_COUNT"
if [[ $SEND == 1 ]]; then
  echo "  sent:               $ok"
  echo "  failed:             $failed"
  echo "  passwords generated: $generated (recipient sets their own; see below)"
  if [[ -n "$LOG" ]]; then echo "  log:                $LOG"; fi
  echo
  echo "  A 200 from signup is NOT proof of delivery. mailer::send logs and drops"
  echo "  failures and signup returns success either way. Confirm with --verify."
  if [[ $generated -gt 0 ]]; then
    echo
    echo "  Each generated password is unique and was never written anywhere. The"
    echo "  recipient cannot log in with it — that is the intent. They set their"
    echo "  own from the login page's forgot-password link, which emails a 1h reset"
    echo "  (user.requestPasswordReset → user.resetPassword)."
  fi
  if [[ $failed -gt 0 ]]; then
    exit 2
  fi
fi
exit 0

# ── Gaps this script does not close ──────────────────────────────────────────
# The product's only outbound mail is a verification or reset link. There is no
# invite email, no personalization beyond the address itself, and no
# unsubscribe mechanism. If the alpha needs a real invite ("here's your board,
# come solve it") with an opt-out, that is a mailer change (a third template
# plus a List-Unsubscribe header) and a product decision — not something a send
# script can supply. Flagged for DEF-120.
#
# The generated-password path depends on that reset flow existing and being
# discoverable from the login page. It does: `user.requestPasswordReset` mails a
# 1h token and `user.resetPassword` consumes it (routers/user.rs:218-300). If a
# future change removes the forgot-password link, this script must go back to
# requiring a password per row — an account a recipient cannot log into is not
# an invite.
