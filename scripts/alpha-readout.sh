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
#   Tier 2 — opt-in, needs an admin session, adds the three things tier 1 cannot
#     see: how many people signed up, how many verified their email, and how many
#     converted to Pro.
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
#     The Pro-conversion count (DEF-154) is the number that says whether the
#     positioning worked, and it is the easiest one in this file to report
#     dishonestly. Two traps, both closed below:
#       - `vipPass` is a manual admin override (UPDATE "User" SET "vipPass"),
#         not a purchase. It is reported on its own line, never folded into the
#         conversion count, or the readout would quote hand-granted passes as
#         revenue.
#       - A low count means nothing on its own. Pro checkout is gated on
#         credentials the chart injects, and the chart value is invisible from
#         outside the cluster — so "0 converted" and "0 could have converted"
#         print identically. `/api/config` now answers `features.proCheckout`
#         (DEF-166), derived from the same condition the chart gates on, so the
#         readout reports that flag next to the count and names the build it
#         measured. A count against `proCheckout: false` is the billing flag,
#         not demand, and the script says so in the same line.
#
# Env:
#   ALPHA_SINCE=2026-10-12  only count accounts created on/after this date,
#                            i.e. the wave the invite actually reached.
set -euo pipefail

BASE_URL="${1:-https://crosswords.casazza.io}"
BASE_URL="${BASE_URL%/}"
# Exported, not `SINCE="$SINCE" printf … | jq`. That prefix sets the variable for
# `printf` only — the two are separate processes in a pipeline — so jq read
# env.SINCE as null and `select(.createdAt >= null)` compared every row against
# null, which in jq's total order is below every string. ALPHA_SINCE silently
# matched everything and the window never filtered. It only *looked* right: the
# window line prints the value it was given, and an empty ALPHA_SINCE was
# already printing "all time". Fixing it here because the conversion denominator
# is computed over this same window, and a denominator that silently counts
# accounts from before the wave is worse than no denominator.
export SINCE="${ALPHA_SINCE:-}"

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

# GET /api/config — unauthenticated, and the only place from outside the cluster
# that says whether Pro checkout is actually live. Deliberately best-effort: a
# host that does not answer it still gets a readout, it just gets the honest
# UNKNOWN instead of a number that would be read as demand.
config() {
  curl -sS -m 20 "$BASE_URL/api/config" 2>/dev/null || true
}

# jq shared by the table formatters: pad to a width without exploding when a
# value is wider than the column.
# printf reuses its format string per argument, so a {1..N} range pads the wrong
# way. rule N is the reliable way to get N dashes.
rule() { printf '  %s\n' "$(printf '%.0s-' $(seq 1 "$1"))"; }

JQ_PAD='def pad($w): (. | tostring) as $s | $s + (if ($w - ($s | length)) > 0 then " " * ($w - ($s | length)) else " " end);'

# ── Which build am I reading? (DEF-153) ──────────────────────────────────────
# A conversion count is not a fact about the product, it is a fact about one
# build on one host at one moment, and the number alone cannot say which. Every
# host serves the same bundle for staging and production, so `environment` plus
# `buildSha` is the provenance that makes the rest of this run quotable. Both
# live hosts were measured at v0.1.76 / c446d23d596d on 2026-10-01, which is
# *before* the isPro field landed (5ead8b1) — that is why a run against them
# today reports conversions UNKNOWN rather than 0.
cfg="$(config)"
if printf '%s' "$cfg" | jq -e . >/dev/null 2>&1; then
  CFG_ENV="$(printf '%s' "$cfg" | jq -r '.environment // "unknown"')"
  CFG_BUILD="$(printf '%s' "$cfg" | jq -r '.buildSha // "unknown"')"
  # proCheckout is absent on builds predating DEF-166, which is itself the
  # answer: a host that cannot say whether checkout is live cannot interpret a
  # low count either.
  case "$(printf '%s' "$cfg" | jq -r 'if (.features.proCheckout | type) == "boolean" then (.features.proCheckout|tostring) else "null" end')" in
    true)  PRO_CHECKOUT="LIVE" ;;
    false) PRO_CHECKOUT="NOT LIVE" ;;
    *)      PRO_CHECKOUT="UNKNOWN" ;;
  esac
else
  CFG_ENV="unknown"
  CFG_BUILD="unknown"
  PRO_CHECKOUT="UNKNOWN"
fi

echo "alpha readout  $(date -u '+%Y-%m-%dT%H:%MZ')"
echo "host:          $BASE_URL"
echo "environment:   $CFG_ENV"
echo "build:         $CFG_BUILD"
echo "pro checkout:  $PRO_CHECKOUT"

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
  # Said out loud, not omitted. DEF-153 AC#3: if the conversion line cannot be
  # produced it must be absent-or-UNKNOWN, never a 0 — and a reader who sees no
  # line at all is left to assume it was 0.
  echo "    - how many converted to Pro"
  echo
  echo "  conversions: UNKNOWN (no admin session)"
  echo "  Re-run with ALPHA_ADMIN_EMAIL/ALPHA_ADMIN_PASSWORD or a session cookie."
  exit 0
fi

users="$(trpc user.listForAdmin null "$cookie" | unwrap)"
if [[ "$users" == SERVER_ERROR:* || "$users" == UNEXPECTED_ENVELOPE:* ]]; then
  echo "  ! $users" >&2
  echo "  The session is not admin-capable, or it expired. Re-copy a fresh" >&2
  echo "  next-auth.session-token and re-run; tier 1 above still stands." >&2
  # Marker for this block, on stdout, so the run still carries an explicit
  # conversion answer rather than none. Tier 1 is already printed and still
  # valid; exiting 1 here keeps the failure from being read as a clean run.
  echo "  conversions: UNKNOWN (user.listForAdmin unavailable — no admin-capable session)"
  exit 1
fi

printf '%s' "$users" | jq -r '
  (if env.SINCE == "" then . else [ .[] | select((.createdAt // "") >= env.SINCE) ] end) as $in
  | "  window:             \(if env.SINCE == "" then "all time" else "created on/after " + env.SINCE end)",
    "  accounts:           \($in | length)",
    "  email verified:     \([ $in[] | select(.emailVerified != null) ] | length)",
    "  never verified:     \([ $in[] | select(.emailVerified == null) ] | length)"
'
echo
rule 67
printf '  %-34s%10s%10s%13s\n' email role verified created
printf '%s' "$users" | jq -r "$JQ_PAD"'
  (if env.SINCE == "" then . else [ .[] | select((.createdAt // "") >= env.SINCE) ] end)
  | sort_by(.createdAt // "") | reverse | .[]
  | "  " + ((.email // "-") | pad(34))
    + ((.role // "?") | pad(10))
    + ((if .emailVerified == null then "no" else "yes" end) | pad(10))
    + (.createdAt // "-")
'

# ── Pro conversion (DEF-154) ────────────────────────────────────────────────
# Sourced from the additive `isPro` on `user.listForAdmin`, which is computed
# server-side by `subscription::is_pro` — the same predicate `subscription
# .getStatus` grants on, so this count cannot disagree with what the app treats
# as Pro. Notably that predicate already folds in `vipPass`, so a naive
# `select(.isPro)` here WOULD count hand-granted admin passes as conversions.
# They are separated below instead.
#
# Every branch of this block is written so the reader cannot mistake a missing
# measurement for a zero. `isPro` is additive and additive means optional: a host
# still serving a build from before 5ead8b1 answers with rows that simply have
# no `isPro` key, and `select(.isPro)` over those is empty — which would render
# as a confident, entirely fictional "0". So the field's presence is checked
# first, and its absence is reported as UNKNOWN.
echo
echo "-- Pro conversion"
rule 44

conv="$(printf '%s' "$users" | jq -r '
  (if env.SINCE == "" then . else [ .[] | select((.createdAt // "") >= env.SINCE) ] end) as $in
  | ($in | length) as $n
  # `type` catches all three ways the field can be unusable: absent (null),
  # explicitly null, or not a boolean. Only a real boolean counts as served.
  | ([ $in[] | select((.isPro | type) != "boolean") ] | length) as $unserved
  | ([ $in[] | select(.isPro == true and (.vipPass // false) != true) ] | length) as $purchased
  | ([ $in[] | select(.isPro == true and (.vipPass // false) == true) ] | length) as $vip
  | [ $n, $unserved, $purchased, $vip ] | @tsv
')"

conv_n="$(printf '%s' "$conv" | cut -f1)"
conv_unserved="$(printf '%s' "$conv" | cut -f2)"
conv_purchased="$(printf '%s' "$conv" | cut -f3)"
conv_vip="$(printf '%s' "$conv" | cut -f4)"

# The counts below are only printed once the field is known to be served. Until
# then every count is a 0-that-is-not-a-0 and must not be rendered as one.
if [[ "$conv_unserved" -gt 0 ]]; then
  echo "  conversions: UNKNOWN (endpoint missing isPro)"
  echo
  echo "  $conv_unserved of $conv_n account row(s) in this window carry no boolean isPro, so this"
  echo "  host is serving a build from before the field landed (5ead8b1, PR #193). The count is"
  echo "  NOT 0 — it is unmeasured. Re-run after the next deploy reaches this host; compare the"
  echo "  build line at the top of this readout against origin/main."
elif [[ "$conv_n" -eq 0 ]]; then
  # An empty window cannot distinguish "new endpoint, nobody signed up" from
  # "old endpoint, nobody signed up" — there is no row to carry the field.
  echo "  conversions: UNKNOWN (endpoint missing isPro — no account row to confirm the field)"
  echo
  echo "  Nothing in this window to inspect. Not 0: with no rows, an old build and a new build"
  echo "  look identical. Re-run without ALPHA_SINCE, or after a wider wave."
else
  # The one number, with its denominator and the billing state it was measured
  # under. The flag is in the same line, not a footnote: DEF-153 AC#2.
  case "$PRO_CHECKOUT" in
    "LIVE")
      echo "  conversions: $conv_purchased of $conv_n accounts (checkout LIVE)"
      ;;
    "NOT LIVE")
      echo "  conversions: $conv_purchased of $conv_n accounts (checkout NOT LIVE)"
      echo
      echo "  Nobody could have converted on this host: the chart gates the LemonSqueezy"
      echo "  credentials off and the Pro button is not purchasable, so this number is the"
      echo "  BILLING FLAG, NOT DEMAND. Read it as \"did nobody try\", never as \"did the"
      echo "  positioning fail\". Any value here is uninterpretable as conversion."
      ;;
    *)
      echo "  conversions: $conv_purchased of $conv_n accounts (checkout UNKNOWN)"
      echo
      echo "  This build does not report features.proCheckout, so whether anyone COULD have"
      echo "  converted is unmeasured. A low count here cannot be split into \"did not\" and"
      echo "  \"could not\" — do not quote it as a demand signal."
      ;;
  esac
  # Never folded into the line above. `vipPass` is set by an admin with a SQL
  # UPDATE, so counting it as revenue is the specific wrong number DEF-154 names.
  echo
  echo "  vipPass-only Pro (manual admin override, NOT a purchase): $conv_vip"
  echo "  counted above as conversions (isPro && !vipPass); reported here, never summed into it"
fi

# ── The one line the alpha is actually waiting on ───────────────────────────
echo
printf '%s' "$users" | jq -r '
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
