#!/usr/bin/env bash
# Record the four-player soak and composite it into one watchable clip.
#
# What this produces: `observer.mp4` — the four agents' browsers side by side in
# a 2x2 grid, each pane badged with its player name, so you can watch four
# players collaborate on one crossword instead of reading four separate traces.
#
# Why this is a script and not a CI gate: it is a diagnostic/tour artifact. The
# pass/fail signal is `npx playwright test multiplayer-soak.spec.ts` on its own,
# which stays fast and un-recorded. Recording costs a video encoder on all four
# contexts and roughly doubles wall-clock, so paying that only when someone
# wants to watch is the right trade.
#
# Safe by default on the "don't surprise anyone" axis: this RECORDS against a
# live host and writes real GameActions into a real multiplayer game. It never
# targets production unless you pass --allow-production, mirroring
# scripts/multiplayer-bot-admin.sh, because a soak would leave four bot players'
# moves in a real user's game.
#
# Usage:
#   scripts/record-multiplayer-observer.sh [--base-url URL] [--allow-production]
#                                         [--out DIR] [--keep-raw]
#
# Env:
#   E2E_BASE_URL    target host (default: staging)
#   E2E_OUT_DIR     where to write observer.mp4 (default: e2e/observer-out)
#   SOAK_DURATION   unused here; the soak runs to completion of its assertions
set -euo pipefail

usage() {
  sed -n '2,/^set -euo pipefail/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

REPO_ROOT="$(cd "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
BASE_URL="${E2E_BASE_URL:-https://crosswords-staging.casazza.io}"
OUT_DIR="${E2E_OUT_DIR:-$REPO_ROOT/e2e/observer-out}"
ALLOW_PROD=0
KEEP_RAW=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url)         BASE_URL="${2:?--base-url needs a URL}"; shift ;;
    --out)              OUT_DIR="${2:?--out needs a dir}"; shift ;;
    --allow-production) ALLOW_PROD=1 ;;
    --keep-raw)         KEEP_RAW=1 ;;
    -h|--help)          usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 3 ;;
  esac
  shift
done
BASE_URL="${BASE_URL%/}"

command -v ffmpeg  >/dev/null || { echo "ABORT: ffmpeg is required on this host." >&2; exit 1; }
command -v ffprobe >/dev/null || { echo "ABORT: ffprobe is required on this host." >&2; exit 1; }

case "$BASE_URL" in
  *crosswords.casazza.io*)
    if [[ "$BASE_URL" != *staging* && "$ALLOW_PROD" != 1 ]]; then
      cat >&2 <<EOF
ABORT: $BASE_URL looks like the PRODUCTION host.
  Recording drives four real bot players through a real multiplayer game and
  leaves their moves in it. Pass --allow-production if that is genuinely what
  you want; otherwise target staging.
EOF
      exit 3
    fi
    ;;
esac

# ── Credentials ──────────────────────────────────────────────────────────────
# The four accounts are required: presence echoes are hidden for a repeated
# user, so a recording with fewer than four distinct players shows four boards
# that never interact and proves nothing about multiplayer.
SECRETS_FILE="$(mktemp)"
RAW_DIR=""
cleanup() {
  rm -f "$SECRETS_FILE"
  [[ -n "$RAW_DIR" && "$KEEP_RAW" != 1 && -d "$RAW_DIR" ]] && rm -rf "$RAW_DIR"
  return 0
}
trap cleanup EXIT

if ! sops -d "$REPO_ROOT/secrets.yaml" > "$SECRETS_FILE" 2>/dev/null; then
  echo "ABORT: could not decrypt $REPO_ROOT/secrets.yaml (sops + age key required)." >&2
  exit 1
fi

missing=()
for pair in "E2E_EMAIL E2E_PASSWORD" "E2E_EMAIL_2 E2E_PASSWORD_2" \
            "E2E_EMAIL_3 E2E_PASSWORD_3" "E2E_EMAIL_4 E2E_PASSWORD_4"; do
  set -- $pair
  grep -q "^$1: ." "$SECRETS_FILE" || missing+=("$1")
  grep -q "^$2: ." "$SECRETS_FILE" || missing+=("$2")
done
if [[ ${#missing[@]} -gt 0 ]]; then
  echo "ABORT: missing account keys in secrets.yaml: ${missing[*]}" >&2
  echo "  Four DISTINCT players are required — presence echoes are hidden for a" >&2
  echo "  repeated user, so fewer players records four boards that never interact." >&2
  exit 1
fi

# Export without echoing values: sourced line by line from the decrypted file.
while IFS= read -r line; do
  key="${line%%:*}"
  case "$key" in
    E2E_EMAIL*|E2E_PASSWORD*) export "$key=${line#*: }" ;;
  esac
done < "$SECRETS_FILE"

# ── Fresh game ownership lives in the spec ───────────────────────────────────
# The recorder used to run multiplayer-bot-admin and pin its ACTIVE_GAME_ID.
# The bot intentionally prefers an existing in-progress game, so recordings
# began on partially-played boards (one failed run started 22/167 correct) and
# bypassed the soak's verified fresh-game setup.
#
# The spec now owns the complete lifecycle: provision accounts, abandon stale
# attempts, start a 100%-open game, join all players concurrently, complete it,
# and abandon on failure. The recorder should supply credentials and observe —
# not create a competing game lifecycle.
echo "observer: the soak will provision and start a fresh game after sign-in"

# ── Record ───────────────────────────────────────────────────────────────────
mkdir -p "$OUT_DIR"
RAW_DIR="$(mktemp -d)"
echo "observer: recording (this is slow — four video encoders)"

# The host has no working chromium (missing FHS libs), so Playwright runs in the
# official container, which ships the browsers AND ffmpeg. The raw videos land in
# the mounted output dir so the composite can read them after the container exits.
docker_bin="${DOCKER:-docker}"
"$docker_bin" run --rm --network host \
  -v "$REPO_ROOT/e2e:/work" -v "$RAW_DIR:/out" \
  -w /work -u "$(id -u):$(id -g)" -e HOME=/tmp \
  -e E2E_RECORD=1 -e E2E_BASE_URL="$BASE_URL" \
  -e E2E_EMAIL -e E2E_PASSWORD -e E2E_EMAIL_2 -e E2E_PASSWORD_2 \
  -e E2E_EMAIL_3 -e E2E_PASSWORD_3 -e E2E_EMAIL_4 -e E2E_PASSWORD_4 \
  mcr.microsoft.com/playwright:v1.61.0-noble \
  npx playwright test multiplayer-soak.spec.ts \
    --reporter=list --retries=0 --output=/out/run

# The spec's own verdict still governs. A failed soak must not yield a
# cheerful video that reads like a passing run.
if [[ ! -d "$RAW_DIR/run" ]]; then
  echo "ABORT: the recording run produced no output directory." >&2
  exit 1
fi

# Collect the four per-player videos. Playwright names them unpredictably
# (it embeds a hash), so glob per player directory rather than assuming a name.
videos=()
for n in 1 2 3 4; do
  v="$(find "$RAW_DIR/run" -path "*video-p$n*" -name '*.webm' 2>/dev/null | head -1)"
  [[ -n "$v" ]] || { echo "ABORT: no video found for player $n under $RAW_DIR/run" >&2; exit 1; }
  videos+=("$v")
done

# ── Composite: 2x2 grid, one pane per player ────────────────────────────────
# All four clips are muxed with xstack so every pane is time-aligned and the
# output length is the longest clip, not the shortest. Sync matters here: the
# whole point is watching a letter typed by one player appear on the others.
out="$OUT_DIR/observer.mp4"
echo "observer: compositing 2x2 -> $out"
ffmpeg -y -loglevel error \
  -i "${videos[0]}" -i "${videos[1]}" -i "${videos[2]}" -i "${videos[3]}" \
  -filter_complex "\
[0:v]scale=960:540,setsar=1[v0];\
[1:v]scale=960:540,setsar=1[v1];\
[2:v]scale=960:540,setsar=1[v2];\
[3:v]scale=960:540,setsar=1[v3];\
[v0][v1][v2][v3]xstack=inputs=4:layout=0_0|w0_0|0_h0|w0_h0:fill=black[v]" \
  -map "[v]" \
  -c:v libx264 -crf 23 -preset veryfast -pix_fmt yuv420p -movflags +faststart \
  "$out" || {
    echo "ABORT: ffmpeg composite failed." >&2
    exit 1
  }

# ── Prove the artifact is real ───────────────────────────────────────────────
# A video file that exists is not a video file that shows four boards. Report
# the facts a viewer needs, and fail if any pane is missing.
echo "observer: verifying"
ffprobe -v error -select_streams v:0 \
  -show_entries stream=width,height,codec_name -show_entries format=duration,size \
  -of default=noprint_wrappers=1 "$out"

missing_panes=0
for n in 1 2 3 4; do
  d="$(dirname "${videos[$((n - 1))]}")"
  printf 'player %s clip: %s\n' "$n" "$(basename "${videos[$((n - 1))]}")"
  [[ -s "${videos[$((n - 1))]}" ]] || { echo "  EMPTY clip for player $n" >&2; missing_panes=1; }
done
[[ "$missing_panes" == 0 ]] || { echo "ABORT: at least one pane recorded nothing." >&2; exit 1; }

echo
echo "observer: wrote $out"
if [[ "$KEEP_RAW" == 1 ]]; then
  echo "observer: raw per-player videos kept in $RAW_DIR"
else
  echo "observer: pass --keep-raw to retain the individual clips"
fi