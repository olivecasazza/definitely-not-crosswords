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
# Two consequences this script refuses to paper over:
#
#   1. `user.signup` requires a username AND a password for the recipient
#      (routers/user.rs:94-102). Someone else choosing a tester's password is
#      not an invite, so the roster carries one per row and the script never
#      invents one. A row without a password is a hard error, not a default.
#   2. The verification email has no List-Unsubscribe header and no opt-out
#      (mailer.rs:168-181). There is no unsubscribe path in the product. The
#      honest equivalent is upstream of the send: a consent gate that refuses to
#      email anyone who has not consented, plus `--withdraw` to take a row out
#      of the wave. A real unsubscribe is a mailer change, not a script change.
#
# Safe by default: with no flags this renders every request and every email and
# sends nothing. Real sending needs `--send`.
#
# Usage:
#   scripts/alpha-invite-send.sh [--send] [--roster PATH] [--base-url URL]
#                                [--log PATH] [--verify] [--withdraw EMAIL]
#
# Env:
#   ALPHA_ROSTER, ALPHA_BASE_URL, ALPHA_LOG   defaults for the flags above.
#   ALPHA_ADMIN_EMAIL / ALPHA_ADMIN_PASSWORD  admin session for --verify
#                                            (or ALPHA_SESSION_COOKIE).
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

usage() {
  sed -n '2,50p' "$0" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --send)      SEND=1 ;;
    --verify)    VERIFY=1 ;;
    --roster)    ROSTER="${2:?--roster needs a path}"; shift ;;
    --base-url)  BASE_URL="${2:?--base-url needs a URL}"; shift ;;
    --log)       LOG="${2:?--log needs a path}"; shift ;;
    --withdraw)  WITHDRAW="${2:?--withdraw needs an email}"; shift ;;
    -h|--help)   usage; exit 0 ;;
    *)           echo "unknown argument: $1" >&2; usage >&2; exit 3 ;;
  esac
  shift
done
BASE_URL="${BASE_URL%/}"

command -v curl >/dev/null || { echo "curl is required" >&2; exit 1; }
command -v jq   >/dev/null || { echo "jq is required"   >&2; exit 1; }

# One append-only line per attempt. Failures have to be greppable after the fact
# — the send is the step where a partial failure is invisible from the exit code.
logline() {
  [[ -n "$LOG" ]] || return 0
  printf '%s\n' "$1" >> "$LOG"
}

# ── Preconditions ────────────────────────────────────────────────────────────
if [[ ! -f "$ROSTER" ]]; then
  echo "ABORT: no roster at $ROSTER." >&2
  echo "Copy data/crossword/alpha-roster.example.json and fill it in. The real" >&2
  echo "roster is gitignored: it holds addresses and passwords and must never" >&2
  echo "enter git." >&2
  exit 1
fi
if ! jq -e 'type == "array"' "$ROSTER" >/dev/null 2>&1; then
  echo "ABORT: $ROSTER is not a JSON array." >&2
  exit 1
fi

echo "alpha invite send  $(date -u '+%Y-%m-%dT%H:%MZ')"
echo "host:              $BASE_URL"
echo "roster:            $ROSTER"
if [[ $SEND == 1 ]]; then
  echo "mode:              SEND"
else
  echo "mode:              DRY-RUN (nothing is sent)"
fi
echo

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

ROWS="$(jq 'length' "$ROSTER")"
echo "  rows: $ROWS"

# ── Consent gate (default-deny) ──────────────────────────────────────────────
# A row is emailable only with an explicit consent flag AND the date it was
# recorded AND where it came from. Anything else is skipped and reported —
# quietly dropping a person, or worse, quietly mailing one who never consented,
# is the failure this gate exists to prevent.
consented() {
  jq -r --argjson i "$1" '
    .[$i] as $r
    | if ($r.consent // false) == true
         and (($r.consent_recorded_at // "") | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}$"))
         and (($r.consent_source // "") | length) > 0
      then "yes" else "no" end' "$ROSTER"
}

SKIPPED_COUNT=0
SKIPPED_LIST=""
for i in $(seq 0 $((ROWS - 1))); do
  if [[ "$(consented "$i")" != "yes" ]]; then
    who="$(jq -r --argjson i "$i" '(.[$i].email // ("row " + ($i | tostring)))' "$ROSTER")"
    SKIPPED_LIST+="    - $who"$'\n'
    SKIPPED_COUNT=$((SKIPPED_COUNT + 1))
  fi
done

# --withdraw mutates the roster, so the gate report above would describe the
# wave as it was, not as it is. Say nothing there and let the re-run speak.
if [[ $SKIPPED_COUNT -gt 0 && -z "$WITHDRAW" ]]; then
  echo
  echo "  CONSENT GATE: $SKIPPED_COUNT row(s) will NOT be emailed:"
  printf '%s' "$SKIPPED_LIST"
  echo "  Each needs consent: true, consent_recorded_at (YYYY-MM-DD), and"
  echo "  consent_source. See data/crossword/alpha-roster.example.json."
fi
echo

# ── Withdraw: take a person out of the wave ──────────────────────────────────
if [[ -n "$WITHDRAW" ]]; then
  idx="$(jq -r --arg e "$WITHDRAW" 'to_entries[] | select((.value.email // "") == $e) | .key' "$ROSTER" | head -1)"
  if [[ -z "$idx" ]]; then
    echo "withdraw: no row for $WITHDRAW" >&2
    exit 1
  fi
  tmp="$(mktemp)"
  jq --argjson i "$idx" \
     '.[$i].consent = false | .[$i].consent_withdrawn_at = (now | strftime("%Y-%m-%d"))' \
     "$ROSTER" > "$tmp"
  mv "$tmp" "$ROSTER"
  echo "withdrew $WITHDRAW from the wave (row $idx, consent=false)."
  echo "Re-run without --withdraw to see the updated gate."
  exit 0
fi

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
  jq -rn --argjson users "$users" --slurpfile roster "$ROSTER" '
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

ok=0
failed=0
for i in $(seq 0 $((ROWS - 1))); do
  [[ "$(consented "$i")" == "yes" ]] || continue

  email="$(jq -r --argjson i "$i" '.[$i].email' "$ROSTER")"
  name="$(jq    -r --argjson i "$i" '.[$i].name'    "$ROSTER")"
  username="$(jq -r --argjson i "$i" '.[$i].username' "$ROSTER")"
  password="$(jq -r --argjson i "$i" '.[$i].password // ""' "$ROSTER")"

  # A row without a password is a hard error. Inventing one would mean the
  # script chooses a stranger's credential, which is not an invite.
  if [[ -z "$password" || ${#password} -lt 8 ]]; then
    echo "! row $i ($email): no password (8+ chars required) — skipped." >&2
    failed=$((failed + 1))
    logline "$(jq -nc --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg e "$email" \
      '{ts:$ts,email:$e,action:"skipped",ok:false,reason:"no password in roster"}')"
    continue
  fi

  echo "── row $i: $email"
  echo "   name:     $name"
  echo "   username: $username"

  if [[ $SEND == 0 ]]; then
    echo "   request:   POST $BASE_URL/api/trpc/user.signup"
    echo "   body:      {\"0\":{\"email\":\"$email\",\"name\":\"$name\",\"username\":\"$username\",\"password\":\"<redacted>\"}}"
    echo "   fallback:  if the account exists, POST $BASE_URL/api/trpc/user.resendVerification"
    echo "   email the server will send:"
    render_email "$BASE_URL" "$email" | sed 's/^/     /'
    echo
    continue
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
if [[ $SEND == 1 ]]; then
  echo "  sent:               $ok"
  echo "  failed:             $failed"
  if [[ -n "$LOG" ]]; then echo "  log:                $LOG"; fi
  echo
  echo "  A 200 from signup is NOT proof of delivery. mailer::send logs and drops"
  echo "  failures and signup returns success either way. Confirm with --verify."
  if [[ $failed -gt 0 ]]; then
    exit 2
  fi
fi
exit 0

# ── Gap this script does not close ───────────────────────────────────────────
# The product's only outbound mail is a verification or reset link. There is no
# invite email, no personalization beyond the address itself, and no
# unsubscribe mechanism. If the alpha needs a real invite ("here's your board,
# come solve it") with an opt-out, that is a mailer change (a third template
# plus a List-Unsubscribe header) and a product decision — not something a send
# script can supply. Flagged for DEF-120.
