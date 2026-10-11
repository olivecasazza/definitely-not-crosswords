#!/usr/bin/env bash
# Orchestrate and UNBLOCK a multiplayer verification run against a deployed host.
#
# This is not an LLM agent. It is a boring, deterministic, idempotent shell
# orchestrator, and it stays boring on purpose: it runs before the Playwright
# soak and the k6 load scenario, and those two harnesses must be able to trust
# the answer it gives them. Everything it does is a fixed sequence of HTTP calls
# against non-admin product paths, in a fixed order, with no branching that
# depends on anything but the responses.
#
# Why no admin credential: there isn't one. The repo has no admin secret wired
# into CI and this script must not be the reason one appears. The only
# non-admin path to a login-capable account is `user.signup`
# (client/backend/server/src/routers/user.rs), which serves it with no email
# verification and no role. So provisioning here is signup-or-reconcile, and
# "already exists" is success — re-running is the intended way to use this.
#
# Safe by default, exactly like `scripts/alpha-invite-send.sh`, which is the
# repo's established convention for anything that writes to a live host: with
# no flags this runs every READ-ONLY check (healthz, /api/config, the served
# bundle identity, a real login, `gameList.get`) and prints what it WOULD do.
# Mutating a live site — signup, `activeGame.start`, `activeGame.join` — needs
# `--apply`.
#
# Two failures this refuses to paper over, because both of them produce a
# green run that proves nothing:
#
#   1. A deploy rolls asynchronously. The pod answering `/api/healthz` can be
#      the PREVIOUS pod for a while after the tag is bumped — this repo
#      learned that the hard way in `.github/workflows/deploy-staging.yml` and
#      `deploy-production.yml`, which both poll healthz in a bounded loop and
#      then additionally check the served version before declaring the deploy
#      live. A single curl proves nothing here, so healthz is polled with the
#      same bounded-budget shape, and the served version is reported rather
#      than assumed. The budget is bounded because an unbounded wait is how a
#      CI job hangs until the runner kills it.
#   2. A version string is release identity, not content identity (DEF-152).
#      Two deploys of the same version can serve different bundles. So the
#      `/_assets/<16-hex>/crossword-web.js` content hash from `/` is recorded
#      next to the version, and both appear in the table and in the
#      machine-readable output. A mismatch between what you meant to deploy and
#      what the host serves is exactly what a soak cannot see from inside
#      itself.
#
# Usage:
#   scripts/multiplayer-bot-admin.sh [--apply] [--base-url URL]
#                                    [--allow-production]
#                                    [--healthz-budget N] [--healthz-interval SECS]
#
# Env:
#   E2E_BASE_URL                    default for --base-url (default: staging).
#   E2E_EMAIL / E2E_PASSWORD         player 1
#   E2E_EMAIL_2 / E2E_PASSWORD_2     player 2
#   E2E_EMAIL_3 / E2E_PASSWORD_3     player 3
#   E2E_EMAIL_4 / E2E_PASSWORD_4     player 4
#
#   CI supplies these from sops-decrypted secrets.yaml. Locally, decrypt with
#   `sops -d secrets.yaml` and export them; never paste a password on a command
#   line, where it lands in shell history.
#
# Production: this CREATES FOUR REAL ACCOUNTS on the real site. It refuses to
# touch a production host without `--allow-production`, and warns loudly when
# given one. Same posture as `e2e/tests/pro-checkout.spec.ts`, which stops
# short of completing a real charge.
#
# There is deliberately no `set -x` anywhere in this file, and no password ever
# reaches stdout: no password may reach a log through shell tracing.
#
# Exit codes:
#   0  everything a run needs is in place
#   1  preflight / validation failure (host down, bad config, missing account)
#   2  applied mode completed with failures — PARTIAL STATE, see the messages
#   3  usage
set -euo pipefail


# Advisory low-open-cell threshold, in cells. See the warning block below for
# why it is 10 and not 0, and why the check is advisory rather than fatal.
MIN_OPEN_CELLS="${MIN_OPEN_CELLS:-10}"
APPLY=0
ALLOW_PRODUCTION=0
BASE_URL="${E2E_BASE_URL:-https://crosswords-staging.casazza.io}"
HEALTHZ_BUDGET="${HEALTHZ_BUDGET:-15}"
HEALTHZ_INTERVAL="${HEALTHZ_INTERVAL:-5}"
CURL_TIMEOUT="${CURL_TIMEOUT:-20}"

# Print the header comment, keyed to the `set -euo pipefail` line rather than a
# line number so editing the header above cannot truncate --help. Same trick as
# alpha-invite-send.sh.
usage() {
  sed -n '2,/^set -euo pipefail/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --apply)            APPLY=1 ;;
    --allow-production) ALLOW_PRODUCTION=1 ;;
    --base-url)         BASE_URL="${2:?--base-url needs a URL}"; shift ;;
    --healthz-budget)   HEALTHZ_BUDGET="${2:?--healthz-budget needs a count}"; shift ;;
    --healthz-interval) HEALTHZ_INTERVAL="${2:?--healthz-interval needs seconds}"; shift ;;
    -h|--help)          usage; exit 0 ;;
    *)                  echo "unknown argument: $1" >&2; usage >&2; exit 3 ;;
  esac
  shift
done
BASE_URL="${BASE_URL%/}"

for c in curl jq; do
  command -v "$c" >/dev/null || { echo "$c is required" >&2; exit 1; }
done

TMPD="$(mktemp -d)"
trap 'rm -rf "$TMPD"' EXIT

# ── http helpers ────────────────────────────────────────────────────────────
# Response bodies land in files under $TMPD, never in argv.

# GET, body to $2, http code on stdout.
http_get() { # url outfile
  curl -sS --max-time "$CURL_TIMEOUT" -o "$2" -w '%{http_code}' "$1" 2>"$TMPD/err" || true
}

# POST JSON, body to $TMPD/body, http code on stdout.
http_post() { # url json
  curl -sS --max-time "$CURL_TIMEOUT" -o "$TMPD/body" -w '%{http_code}' \
    -H 'Content-Type: application/json' \
    -H "Cookie: $3" --data-binary "$2" "$1" 2>"$TMPD/err" || true
}

# tRPC over HTTP: request body {"0": <input>}; answer is
# [{result:{data}}] on success, [{error:{message}}] on failure. Echoes the data
# on stdout and puts the message in a FILE, not a global: every caller invokes
# this inside a command substitution, and a global set in a subshell never
# reaches the caller. Read it back with trpc_err.
trpc() { # proc json-input [cookie]
  local proc="$1" input="$2" cookie="${3:-}"
  local body code
  body="$(jq -cn --argjson i "$input" '{"0":$i}')"
  if [[ -n "$cookie" ]]; then
    code="$(http_post "$BASE_URL/api/trpc/$proc" "$body" "$cookie")"
  else
    code="$(http_post "$BASE_URL/api/trpc/$proc" "$body" '')"
  fi
  : > "$TMPD/trpc_err"
  if [[ "$code" != "200" ]]; then
    printf 'http %s: %s' "$code" \
      "$(jq -c . < "$TMPD/body" 2>/dev/null | cut -c1-200)" > "$TMPD/trpc_err"
    return 1
  fi
  if jq -e '.[0].result.data != null' "$TMPD/body" >/dev/null 2>&1; then
    jq -c '.[0].result.data' "$TMPD/body"
    return 0
  fi
  jq -r '.[0].error.message // "unknown tRPC error"' "$TMPD/body" 2>/dev/null \
    > "$TMPD/trpc_err" || echo 'unparseable tRPC response' > "$TMPD/trpc_err"
  return 1
}

# The message from the last failed trpc call.
trpc_err() { cat "$TMPD/trpc_err"; }

# Credentials login. Echoes the session cookie value, or nothing on failure.
# The password is an argument, so it is never printed; nothing here echoes
# either argument.
login_cookie() { # email password
  curl -sS --max-time "$CURL_TIMEOUT" -D "$TMPD/login.headers" -o /dev/null \
    -H 'Content-Type: application/x-www-form-urlencoded' \
    --data-urlencode "email=$1" \
    --data-urlencode "password=$2" \
    --data-urlencode 'callbackUrl=/' \
    "$BASE_URL/api/auth/callback/credentials" 2>"$TMPD/err" || true
  sed -n 's/^set-cookie: *next-auth\.session-token=\([^;]*\).*/\1/ip' "$TMPD/login.headers" \
    | tail -1 | tr -d '\r'
}

die() { # message [exit-code]
  printf '\nFAIL: %s\n' "$1" >&2
  exit "${2:-1}"
}

# ── accounts ────────────────────────────────────────────────────────────────
# ACCOUNTS holds triples (index, email, password) per player, so a password is
# only ever passed as a function argument and never appears in a variable the
# reporting code iterates over.
ACCOUNTS=()
load_accounts() {
  local i email_var pass_var email pass
  for i in 1 2 3 4; do
    if [[ "$i" == 1 ]]; then
      email_var=E2E_EMAIL;     pass_var=E2E_PASSWORD
    else
      email_var="E2E_EMAIL_$i"; pass_var="E2E_PASSWORD_$i"
    fi
    email="${!email_var:-}"; pass="${!pass_var:-}"
    if [[ -z "$email" || -z "$pass" ]]; then
      die "missing account $i: $email_var / $pass_var are not both set.
  The multiplayer harness needs four distinct accounts. They live
  sops-encrypted in secrets.yaml: decrypt with \`sops -d secrets.yaml\` and
  export them. This script will not guess them, and it will not run on a
  subset — a run with three players is not a four-player run." 1
    fi
    ACCOUNTS+=("$i" "$email" "$pass")

  done
}

# Load the roster before the production guard: refusing to touch the live site
# must not require four accounts to be configured first.
load_accounts

# Field 1 = email, field 2 = password, for player $1.
acct_field() { # player field
  # ACCOUNTS is a flat triple per player: index, email, password. The email
  # sits one past the player index; the password one past that.
  local idx=$(( ($1 - 1) * 3 + 1 ))
  if [[ "$2" == 1 ]]; then
    printf '%s' "${ACCOUNTS[$idx]}"
  else
    printf '%s' "${ACCOUNTS[$((idx + 1))]}"
  fi
}

# A deterministic username derived from the address, matching
# alpha-invite-send.sh: never chosen, always reported back. Cut to 24 chars
# because that is what the column tolerates.
username_for() { # email
  printf '%s' "$1" | sed -E 's/^[^@]*@/@/; s/[^a-zA-Z0-9]+/-/g; s/^-+//; s/-+$//' | cut -c1-24
}

# ── production guard ───────────────────────────────────────────────────────
case "$BASE_URL" in
  *//crosswords.casazza.io|https://crosswords.casazza.io)
    if [[ "$ALLOW_PRODUCTION" != 1 ]]; then
      die "$BASE_URL looks like the PRODUCTION host. This script creates up to
  four REAL accounts on the LIVE site and joins them to a live multiplayer game.
  Pass --allow-production if that is genuinely what you want; otherwise target
  staging with --base-url https://crosswords-staging.casazza.io." 3
    fi
    {
      echo '###############################################################'
      echo "#  WARNING: --allow-production against $BASE_URL"
      echo '#  This creates up to four REAL accounts on the LIVE site and'
      echo '#  joins them to a live multiplayer game. The product has no'
      echo '#  account-deletion path, so this is not reversible from here.'
      echo '###############################################################'
    } >&2
    ;;
esac

# ── capability 2: preflight ─────────────────────────────────────────────────
echo '== preflight =='
HOST_ENVIRONMENT='(unknown)'
HOST_VERSION='(unknown)'
MAIL_DELIVERY='(unknown)'
BUNDLE_HASH='(unknown)'

code=000
for i in $(seq 1 "$HEALTHZ_BUDGET"); do
  code="$(http_get "$BASE_URL/api/healthz" "$TMPD/healthz")"
  if [[ "$code" == 200 ]]; then
    echo "  healthz 200 after $i/$HEALTHZ_BUDGET poll(s)"
    break
  fi
  echo "  healthz $code — retry $i/$HEALTHZ_BUDGET"
  [[ "$i" == "$HEALTHZ_BUDGET" ]] || sleep "$HEALTHZ_INTERVAL"
done
if [[ "$code" != 200 ]]; then
  die "$BASE_URL/api/healthz never returned 200 in $HEALTHZ_BUDGET polls (~$(( HEALTHZ_BUDGET * HEALTHZ_INTERVAL / 60 ))m).
  The host is not serving. Check the deploy (Flux reconcile, pod readiness,
  ingress) before running any multiplayer verification — a soak against a host
  that is not serving proves nothing, and reporting it as a pass is worse than
  not running it."
fi

code="$(http_get "$BASE_URL/api/config" "$TMPD/config")"
[[ "$code" == 200 ]] || die "$BASE_URL/api/config returned $code.
  The shared bundle cannot resolve its runtime config without it, so nothing
  downstream is trustworthy. This usually means the pod is up but not routing
  /api."

HOST_ENVIRONMENT="$(jq -r '.environment // "(missing)"' "$TMPD/config")"
HOST_VERSION="$(jq -r '.version // "(missing)"' "$TMPD/config")"
MAIL_DELIVERY="$(jq -r '.features.mailDelivery // "(missing)"' "$TMPD/config")"

case "$HOST_ENVIRONMENT" in
  staging | production | local) : ;;
  *) die "/api/config reports environment '$HOST_ENVIRONMENT'.
  Expected 'staging' or 'production'. Either the host is not the one you think
  it is, or APP_ENV is unset in the deployment. Fix that before running against
  it — a soak against the wrong environment proves nothing about the one you
  care about." ;;
esac
case "$MAIL_DELIVERY" in
  smtp | log) : ;;
  *) die "/api/config reports features.mailDelivery '$MAIL_DELIVERY'. Expected 'smtp' or 'log'.
  Any other value means the mailer is in a state this script cannot reason
  about, and 'log' silently drops outbound mail while still returning 200." ;;
esac
echo "  environment=$HOST_ENVIRONMENT version=$HOST_VERSION mailDelivery=$MAIL_DELIVERY"

# Content identity, not release identity. DEF-152: the version string says which
# release was built, not which bytes are being served, so the bundle content
# hash is recorded next to it.
code="$(http_get "$BASE_URL/" "$TMPD/index.html")"
[[ "$code" == 200 ]] || die "$BASE_URL/ returned $code.
  The app shell is not being served, so there is no bundle to identify."
BUNDLE_HASH="$(grep -oE '/_assets/[0-9a-f]{16}/crossword-web\.js' "$TMPD/index.html" | head -1 \
  | sed -E 's#^/_assets/([0-9a-f]{16})/.*#\1#' || true)"
if [[ -z "$BUNDLE_HASH" ]]; then
  BUNDLE_HASH='(not-found)'
  echo '  NOTE: no /_assets/<16-hex>/crossword-web.js in the served shell.'
  echo '        Bundle CONTENT identity is unverified for this host; the'
  echo '        version above is the only identity available.'
else
  echo "  bundle=$BUNDLE_HASH (content hash, independent of the version string)"
fi

# ── capability 1: account provisioning ──────────────────────────────────────
echo
echo '== accounts =='
PROVISIONED=(no no no no)
PROVISION_DETAIL=()

if [[ "$APPLY" == 1 ]]; then
  for i in 1 2 3 4; do
    email="$(acct_field "$i" 1)"
    payload="$(jq -cn --arg e "$email" --arg p "$(acct_field "$i" 2)" \
      --arg n "Multiplayer Bot $i" --arg u "$(username_for "$email")" \
      '{email:$e, name:$n, username:$u, password:$p}')"
    if out="$(trpc user.signup "$payload")"; then
      PROVISIONED[$((i - 1))]=yes
      echo "  player $i: provisioned (userId $(jq -r '.userId // "?"' <<< "$out"))"
    elif [[ "$(trpc_err)" == *[Ee][Xx][Ii][Ss][Tt]* ]]; then
      # Re-signup is the only non-admin path to an account, so "already exists"
      # is a success: the account is there, which is all this script needs.
      PROVISIONED[$((i - 1))]=yes
      echo "  player $i: already exists — reconciled (idempotent)"
    else
      PROVISION_DETAIL+=("  player $i: signup failed: $(trpc_err)")
      echo "  player $i: signup FAILED: $(trpc_err)"
    fi
  done
else
  for i in 1 2 3 4; do
    email="$(acct_field "$i" 1)"
    if [[ -n "$(login_cookie "$email" "$(acct_field "$i" 2)")" ]]; then
      PROVISIONED[$((i - 1))]=yes
      echo "  player $i: exists (login OK)"
    else
      echo "  player $i: WOULD provision (no working login for $email)"
    fi
  done
  echo '  (read-only mode: re-run with --apply to create missing accounts)'
fi

# ── capability 3: game provisioning ─────────────────────────────────────────
echo
echo '== game =='
COOKIE1="$(login_cookie "$(acct_field 1 1)" "$(acct_field 1 2)")"
[[ -n "$COOKIE1" ]] || die "player 1 could not log in at $BASE_URL.
  Either the credentials were rejected, or /api/auth/callback/credentials is
  not serving. If the account does not exist yet, provision it with --apply
  first. A multiplayer run needs a logged-in owner, so this stops here."
SESSION1="next-auth.session-token=$COOKIE1"

if ! list="$(trpc gameList.get 'null' "$SESSION1")"; then
  die "gameList.get failed for player 1: $(trpc_err)
  Without the lobby there is no way to choose a game to soak, and no way to
  report which game the run covered."
fi

CLUES=0; FILLED=0; CORRECT=0; TOTAL=0

# Prefer a genuinely in-progress game: some cells filled, not all of them
# correct. That is the lobby's "— IN PROGRESS" row, and it is the only shape a
# soak can exercise, because a soak needs open cells to fill.
#
# Among the in-progress games, pick the one with the FEWEST correct cells — the
# most open cells. Sorting by the most-filled first is backwards: it hands the
# run the puzzle closest to finished, i.e. the one where every remaining action
# is the last action. Verified on staging: with a single in-progress game at
# 155/161 correct, that difference is the whole run.
ACTIVE_GAME_ID="$(jq -r '[ .[]
      | select(.type == "ActiveGame")
      | select((.filledCount // 0) > 0 and (.correctCount // 0) < (.totalCells // 0))
    ] | sort_by(.correctCount // 0, .filledCount // 0) | .[0].id // empty' <<< "$list")"
GAME_ID="$(jq -r --arg a "$ACTIVE_GAME_ID" \
  '[ .[] | select(.id == $a) ][0].gameId // empty' <<< "$list")"
GAME_SOURCE='existing in-progress game'

if [[ -z "$ACTIVE_GAME_ID" ]]; then
  FILLED="$(jq -r '[ .[] | select(.type == "ActiveGame") ][0].filledCount // 0' <<< "$list")"
  CORRECT="$(jq -r '[ .[] | select(.type == "ActiveGame") ][0].correctCount // 0' <<< "$list")"
  TOTAL="$(jq -r '[ .[] | select(.type == "ActiveGame") ][0].totalCells // 0' <<< "$list")"
  if [[ "$TOTAL" -gt 0 && "$CORRECT" -ge "$TOTAL" ]]; then
    echo "  WARNING: the only active game is finished ($CORRECT/$TOTAL correct)."
    echo '           Starting a fresh game instead. A soak against a completed'
    echo '           puzzle proves nothing.'
  elif [[ "$TOTAL" -gt 0 && "$CORRECT" -ge $(( TOTAL - MIN_OPEN_CELLS )) ]]; then
    echo "  NOTE: the only active game is nearly finished ($CORRECT/$TOTAL correct,"
    echo "        $(( TOTAL - CORRECT )) open cells). Starting a fresh game instead."
  fi
  # The parent puzzle id is `gameId` on EVERY row gameList.get returns, and the
  # row `type` is only ever ActiveGame or CompletedGame — never "Game". So this
  # used to filter `select(.type == "Game")`, which matches nothing at all, and
  # the "no published unstarted Game" fallback below was unreachable dead code.
  # It presented as an exhausted puzzle pool; it was this filter.
  CANDIDATE="$(jq -r '[ .[] | .gameId // empty ] | unique | .[0] // empty' <<< "$list")"
  [[ -n "$CANDIDATE" ]] || die "no playable game for player 1 at $BASE_URL.
  The lobby has no in-progress ActiveGame and no published unstarted Game, so
  there is nothing to soak. Publish a Game (or seed one) and re-run. Do not
  read this as a pass — it is the absence of a subject."
  if [[ "$APPLY" == 1 ]]; then
    if ! started="$(trpc activeGame.start "$(jq -cn --arg g "$CANDIDATE" '{gameId:$g}')" \
        "$SESSION1")"; then
      die "activeGame.start failed for game $CANDIDATE: $(trpc_err)
  The game is published but could not be started. Check the server log for a
  schema or permission problem before retrying — a repeated call will fail the
  same way."
    fi
    ACTIVE_GAME_ID="$(jq -r '.id // empty' <<< "$started")"
    [[ -n "$ACTIVE_GAME_ID" ]] || die "activeGame.start returned no id for game $CANDIDATE.
  A run without an activeGameId cannot proceed, so the harnesses must not start."
    GAME_ID="$CANDIDATE"
    GAME_SOURCE='newly started (activeGame.start)'
  else
    ACTIVE_GAME_ID=''
    GAME_ID="$CANDIDATE"
    GAME_SOURCE="read-only: nothing in progress; would call activeGame.start on $CANDIDATE"
    echo "  read-only: no in-progress game; would start $CANDIDATE"
  fi
fi

# A fresh game is all-open, so a started game has 0 filled of N total.
OPEN_CELLS=0
LOW_OPEN_CELLS=0
if [[ -n "$ACTIVE_GAME_ID" ]]; then
  CLUES="$(jq -r --arg g "$GAME_ID" \
    '[ .[] | select(.id == $g or .gameId == $g) ][0].clues // 0' <<< "$list")"
  ROW="$(jq -c --arg a "$ACTIVE_GAME_ID" '[ .[] | select(.id == $a) ][0] // {}' <<< "$list")"
  if [[ "$GAME_SOURCE" == 'newly started (activeGame.start)' ]]; then
    FILLED=0; CORRECT=0
  else
    FILLED="$(jq -r '.filledCount // 0' <<< "$ROW")"
    CORRECT="$(jq -r '.correctCount // 0' <<< "$ROW")"
  fi
  TOTAL="$(jq -r '.totalCells // 0' <<< "$ROW")"
  OPEN_CELLS=$(( TOTAL - CORRECT ))

  # Low-open-cell warning. Advisory, never fatal — a nearly-finished puzzle is
  # still a legitimate thing to report on, and the operator may be soaking the
  # completion path on purpose. But it must be impossible to miss, because a
  # run against a 96%-complete puzzle exercises almost nothing and still exits
  # 0. Hence it also lands in the summary table, marked, not only on stderr.
  #
  # The threshold is 10 open cells OR fewer than 5% of the grid, whichever is
  # hit first. Both halves are needed: 10 absolute keeps a small grid honest
  # (a 15-cell puzzle with 6 open is already nearly done), and 5% keeps a large
  # grid from passing 200 open cells on a 5,000-cell grid where a soak would
  # never finish. 10 cells is roughly what one k6 VU changes per iteration, so
  # below that the scenario runs out of work before the load profile does.
  if [[ "$TOTAL" -gt 0 && ( "$OPEN_CELLS" -lt "$MIN_OPEN_CELLS" \
      || "$OPEN_CELLS" -lt $(( TOTAL / 20 )) ) ]]; then
    LOW_OPEN_CELLS=1
    echo "  WARNING: the chosen game has only $OPEN_CELLS open cells ($CORRECT/$TOTAL correct)."
    echo "           Threshold: under $MIN_OPEN_CELLS cells or under 5% of the grid."
    echo '           A soak here exercises almost nothing and would still exit 0.'
    echo '           Publish or start a fresher game before trusting this run.'
  fi
fi

# ── capability 4: join the other three ──────────────────────────────────────
echo
echo '== membership =='
JOINED='1/4'
if [[ "$APPLY" == 1 && -n "$ACTIVE_GAME_ID" ]]; then
  join_ok=1
  for i in 2 3 4; do
    ck="$(login_cookie "$(acct_field "$i" 1)" "$(acct_field "$i" 2)")"
    if [[ -z "$ck" ]]; then
      join_ok=0
      echo "  player $i: LOGIN FAILED — cannot join"
      continue
    fi
    if trpc activeGame.join "$(jq -cn --arg i "$ACTIVE_GAME_ID" '{id:$i}')" \
        "next-auth.session-token=$ck" >/dev/null; then
      echo "  player $i: joined (idempotent)"
    else
      join_ok=0
      echo "  player $i: join FAILED: $(trpc_err)"
    fi
  done
  if [[ "$join_ok" == 1 ]]; then JOINED='4/4'; else JOINED='partial'; fi
else
  if [[ -n "$ACTIVE_GAME_ID" ]]; then
    JOINED='1/4 (read-only: would join players 2-4)'
    echo "  players 2-4: WOULD join $ACTIVE_GAME_ID"
  else
    JOINED='(read-only: would start a game, then join players 2-4)'
    echo '  players 2-4: WOULD join, once a game exists'
  fi
fi

# ── capability 5: health summary + exit code ────────────────────────────────
echo
echo '== summary =='
row() { printf '  %-24s %s\n' "$1" "$2"; }
row host "$BASE_URL"
if [[ "$APPLY" == 1 ]]; then row mode 'APPLY (mutating)'; else row mode 'read-only (dry run)'; fi
row environment "$HOST_ENVIRONMENT"
row version "$HOST_VERSION"
row 'bundle content hash' "$BUNDLE_HASH"
for i in 1 2 3 4; do row "player $i provisioned" "${PROVISIONED[$((i - 1))]}"; done
row activeGameId "${ACTIVE_GAME_ID:-(none)}"
row gameId "${GAME_ID:-(none)}"
row 'game source' "$GAME_SOURCE"
row clues "$CLUES"
row 'cells filled' "$FILLED"
row 'cells correct' "$CORRECT / $TOTAL"
# The warning has to be in the TABLE, not only on the stream: a table is what
# gets pasted into an issue, and an advisory buried above it is the advisory
# that gets lost.
if [[ "$LOW_OPEN_CELLS" == 1 ]]; then
  row 'open cells' "*** WARNING: only $OPEN_CELLS open ***"
else
  row 'open cells' "$OPEN_CELLS"
fi
row joined "$JOINED"

# Machine-readable, for the Playwright soak and the k6 load scenario to consume
# as env vars rather than each re-deriving its own target game.
echo
echo "ACTIVE_GAME_ID=$ACTIVE_GAME_ID"
echo "GAME_ID=$GAME_ID"
echo "HOST_VERSION=$HOST_VERSION"
echo "BUNDLE_HASH=$BUNDLE_HASH"
if [[ "$APPLY" == 1 ]]; then MODE=apply; else MODE=dry-run; fi
echo "MODE=$MODE"
# So a harness can refuse to soak a puzzle it has nothing to do on, rather than
# relying on a human having read the table.
echo "OPEN_CELLS=$OPEN_CELLS"
echo "LOW_OPEN_CELLS=$LOW_OPEN_CELLS"

# Never exit 0 on a partial state: a green run that provisioned two of four
# accounts must not read as success.
missing=0
for i in 1 2 3 4; do
  [[ "${PROVISIONED[$((i - 1))]}" == yes ]] || missing=1
done
if [[ "$APPLY" == 1 ]]; then
  if [[ "$missing" == 1 ]]; then
    printf '\nPARTIAL: not every account is provisioned.\n' >&2
    printf '%s\n' "${PROVISION_DETAIL[@]+"${PROVISION_DETAIL[@]}"}" >&2
    printf '%s\n' 'Do not start the soak or the load test: some players cannot log' \
      'in, so the run would report green while covering fewer than four.' >&2
    exit 2
  fi
  if [[ "$JOINED" == partial ]]; then
    printf '\nPARTIAL: some players could not join %s.\n' "$ACTIVE_GAME_ID" >&2
    printf '%s\n' 'A multiplayer soak with a single member is not a multiplayer run.' >&2
    exit 2
  fi
fi
exit 0
