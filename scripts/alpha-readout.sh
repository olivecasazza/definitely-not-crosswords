#!/usr/bin/env bash
# Read the closed-alpha engagement numbers off the SERVED build.
#
# Per DEF-110 a green deploy run is not a health signal, and the alpha's
# questions — did the cohort sign up, did anyone finish a solve, is
# verification mail landing at all — are questions about the bytes a player
# actually gets. This script is the executable form of those questions, so the
# Oct 15 readout is one command instead of a bespoke query written under time
# pressure by whoever happens to hold the only admin session.
#
# Usage: scripts/alpha-readout.sh [BASE_URL]
#   BASE_URL  default https://crosswords.casazza.io (production).
#             Pass https://crosswords-staging.casazza.io for staging.
#
# Two tiers, because the part that matters most needs no credential at all:
#
#   Tier 1 — public, always runs, zero secrets.
#     stats.getGlobalLeaderboard is deliberately unauthenticated (DEF-123: a
#     leaderboard is the product loop) and returns per-player gamesPlayed /
#     totalScore / totalCorrect. So "did anybody actually solve something" is
#     answerable by anyone with curl. That is why the engagement datapoint does
#     not have to wait on an admin grant.
#
#   Tier 2 — opt-in, needs an admin session, adds the two things tier 1 cannot
#     see: how many people signed up, and how many verified their email.
#     Set ALPHA_SESSION_COOKIE (a `next-auth.session-token` value), or
#     ALPHA_ADMIN_EMAIL + ALPHA_ADMIN_PASSWORD to log in here. Without either,
#     the script still prints tier 1 and says exactly what is therefore unknown.
#
#     The verified/unverified split is the only outside-in signal that the
#     mailer is alive: mailer::send returns Ok(()) after a tracing::warn! when
#     SMTP_USER/SMTP_PASSWORD are unset, and signup is not gated on
#     verification, so a dead mailer is invisible from signup itself. Zero
#     verified accounts across a real cohort is the tell.
#
# Env:
#   ALPHA_SINCE=2026-10-12  only count accounts created on/after this date,
#                            i.e. the wave the invite actually reached.
set -euo pipefail

BASE_URL="${1:-https://crosswords.casazza.io}"
BASE_URL="${BASE_URL%/}"
SINCE="${ALPHA_SINCE:-}"

# tRPC over HTTP takes {"0": input} and answers [{result:{data}}] or
# [{error:{message,...}}]; the server reads body["0"] (main.rs trpc_post).
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
# readout turns into a fabricated number.
unwrap() {
  jq -r 'if .[0].result then .[0].result.data
         elif .[0].error then "SERVER_ERROR: " + (.[0].error.message // "unknown")
         else "UNEXPECTED_ENVELOPE: " + tostring end'
}

# jq shared by the table formatters: pad to a width without exploding when a
# value is wider than the column.
# printf reuses its format string per argument, so a {1..N} range pads the wrong
# way. rule N is the reliable way to get N dashes.
rule() { printf '  %s\n' "$(printf '%.0s-' $(seq 1 "$1"))"; }

JQ_PAD='def pad($w): (. | tostring) as $s | $s + (if ($w - ($s | length)) > 0 then " " * ($w - ($s | length)) else " " end);'

echo "alpha readout  $(date -u '+%Y-%m-%dT%H:%MZ')"
echo "host:          $BASE_URL"

# ── Tier 1: public, no credentials ──────────────────────────────────────────
echo
echo "-- engagement (public, no credentials)"
rule 44
lb="$(trpc stats.getGlobalLeaderboard null | unwrap)"
if [[ "$lb" == SERVER_ERROR:* || "$lb" == UNEXPECTED_ENVELOPE:* ]]; then
  echo "  ! $lb" >&2
  echo "  This host did not answer stats.getGlobalLeaderboard. If it is" >&2
  echo "  production that is a real outage, not a script problem: stop and" >&2
  echo "  escalate before quoting any number from this run." >&2
  exit 1
fi

printf '%s' "$lb" | jq -r '
  "  accounts:                   \(length)",
  "  with a completed solve:     \([ .[] | select((.gamesPlayed // 0) > 0) ] | length)",
  "  completed solves:           \([ .[].gamesPlayed // 0 ] | add // 0)",
  "  accounts with a correct guess: \([ .[] | select((.totalCorrect // 0) > 0) ] | length)",
  "  total correct guesses:      \([ .[].totalCorrect // 0 ] | add // 0)",
  "  total incorrect guesses:    \([ .[].totalIncorrect // 0 ] | add // 0)"
'

echo
rule 61
printf '  %-22s%7s%6s%9s%8s %8s\n' name solves score correct wrong accuracy
printf '%s' "$lb" | jq -r "$JQ_PAD"'
  sort_by(-(.gamesPlayed // 0), -(.totalScore // 0)) | .[]
  | "  " + ((.name // "Anonymous Player") | pad(22))
    + (.gamesPlayed // 0 | tostring | pad(7))
    + (.totalScore // 0 | tostring | pad(6))
    + (.totalCorrect // 0 | tostring | pad(9))
    + (.totalIncorrect // 0 | tostring | pad(8))
    + ((.accuracy // 0) | tostring | pad(7)) + " %"
'

# ── What tier 1 can and cannot answer ────────────────────────────────────────
# Tier 1 is anonymous, so it answers "did anybody solve" exactly, and "which
# tester solved" not at all. visible_email() masks every address for an
# unauthenticated caller, and display names are user-chosen and NOT unique —
# measured against both live hosts 2026-09-27: production 5 rows / 4 distinct
# names ("Olive Casazza" x2), staging 23 rows / 21 ("Community Probe" x2,
# "Olive Casazza" x2). So a per-tester claim read off the table above is a
# guess, and on Oct 15 that guess would be quoted as a datapoint. Say so here,
# where the reader is, rather than let them infer attribution that isn't there.
echo
echo "-- attribution"
rule 44
printf '%s' "$lb" | jq -r '
  ([ .[] | (.name // "Anonymous Player") ] | unique) as $names
  | ([ .[] | (.name // "Anonymous Player") ] | group_by(.) | map(select(length > 1) | {n: .[0], c: length})) as $dupes
  | "  accounts:               \(length)",
    "  distinct display names: \($names | length)",
    "  email visible on a row: \([ .[] | select(.email != null) ] | length)",
    "",
    "  Tier 1 is anonymous: it gives AGGREGATES exactly, and never WHO.",
    (if ($dupes | length) == 0
     then "  No colliding display names right now, but they are user-chosen and need not stay unique."
     else "  COLLIDING display names right now: "
          + ($dupes | map("\(.n) x\(.c)") | join(", "))
          + "\n  Two accounts share one name, so which-tester-solved is not answerable from this tier."
     end),
    "  Per-tester attribution needs tier 2 below, which is admin-gated."
'

# ── Tier 2: admin session, opt-in ───────────────────────────────────────────
echo
echo "-- signups + mail health (needs an admin session)"
rule 44

cookie="${ALPHA_SESSION_COOKIE:-}"
if [[ -z "$cookie" && -n "${ALPHA_ADMIN_EMAIL:-}" && -n "${ALPHA_ADMIN_PASSWORD:-}" ]]; then
  # auth_routes::credentials is a next-auth-style form post that answers with
  # the encrypted next-auth.session-token cookie.
  headers="$(curl -sS -m 30 -D - -o /dev/null -X POST \
    "$BASE_URL/api/auth/callback/credentials" \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data-urlencode "email=$ALPHA_ADMIN_EMAIL" \
    --data-urlencode "password=$ALPHA_ADMIN_PASSWORD" \
    --data-urlencode 'callbackUrl=/')"
  cookie="$(printf '%s' "$headers" \
    | grep -i '^set-cookie:' \
    | sed -n 's/.*next-auth\.session-token=\([^;]*\).*/\1/p' | head -1 || true)"
  if [[ -z "$cookie" ]]; then
    echo "  ! logged in as $ALPHA_ADMIN_EMAIL but got no session cookie:" >&2
    echo "    wrong credentials, or this host is not serving the build you think." >&2
    exit 1
  fi
  echo "  (logged in as $ALPHA_ADMIN_EMAIL)"
elif [[ -z "$cookie" ]]; then
  echo "  SKIPPED: no ALPHA_SESSION_COOKIE and no ALPHA_ADMIN_EMAIL/PASSWORD."
  echo "  That is the point of this script: tier 1 above needed neither."
  echo
  echo "  Still UNKNOWN without an admin session, and not inferable from tier 1:"
  echo "    - how many people signed up (the leaderboard omits users who never played)"
  echo "    - how many verified email, i.e. whether the mailer is alive at all"
  echo "  Re-run with ALPHA_ADMIN_EMAIL/ALPHA_ADMIN_PASSWORD or a session cookie."
  exit 0
fi

users="$(trpc user.listForAdmin null "$cookie" | unwrap)"
if [[ "$users" == SERVER_ERROR:* || "$users" == UNEXPECTED_ENVELOPE:* ]]; then
  echo "  ! $users" >&2
  echo "  The session is not admin-capable, or it expired. Re-copy a fresh" >&2
  echo "  next-auth.session-token and re-run; tier 1 above still stands." >&2
  exit 1
fi

SINCE="$SINCE" printf '%s' "$users" | jq -r '
  (if env.SINCE == "" then . else [ .[] | select((.createdAt // "") >= env.SINCE) ] end) as $in
  | "  window:             \(if env.SINCE == "" then "all time" else "created on/after " + env.SINCE end)",
    "  accounts:           \($in | length)",
    "  email verified:     \([ $in[] | select(.emailVerified != null) ] | length)",
    "  never verified:     \([ $in[] | select(.emailVerified == null) ] | length)"
'
echo
rule 67
printf '  %-34s%10s%10s%13s\n' email role verified created
SINCE="$SINCE" printf '%s' "$users" | jq -r "$JQ_PAD"'
  (if env.SINCE == "" then . else [ .[] | select((.createdAt // "") >= env.SINCE) ] end)
  | sort_by(.createdAt // "") | reverse | .[]
  | "  " + ((.email // "-") | pad(34))
    + ((.role // "?") | pad(10))
    + ((if .emailVerified == null then "no" else "yes" end) | pad(10))
    + (.createdAt // "-")
'

# ── The one line the alpha is actually waiting on ───────────────────────────
echo
SINCE="$SINCE" printf '%s' "$users" | jq -r '
  (if env.SINCE == "" then . else [ .[] | select((.createdAt // "") >= env.SINCE) ] end) as $in
  | ($in | length) as $n
  | ([ $in[] | select(.emailVerified != null) ] | length) as $v
  | if $n == 0 then
      "mail health: UNKNOWN - no account in this window, so verification mail has never been exercised."
    elif $v == 0 then
      "mail health: DEAD OR UNREAD - \($n) account(s), 0 verified. Either SMTP_USER/SMTP_PASSWORD"
      + " are unset (mailer::send warns and returns Ok) or nobody clicked the link."
      + "\n             Signup is NOT gated on verification, so testers can still play - but password"
      + " reset cannot work, so someone must be ready to set a forgotten password by hand."
    elif $v == $n then
      "mail health: OK - \($v)/\($n) verified."
    else
      "mail health: PARTIAL - \($v)/\($n) verified."
    end
'
