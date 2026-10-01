#!/usr/bin/env bash
# Publish a Prometheus text-exposition file to the pushgateway.
#
# The crosswords server exposes no /metrics route, so every test-side series
# (CI suite results, k6 load scenario output) has to be pushed rather than
# scraped. This is the shared uploader for both, so the job definitions stay
# small and the validation stays in one place.
#
# Usage:
#   scripts/publish-test-metrics.sh [--dry-run] METRICS_FILE
#   METRICS_FILE=$METRICS_FILE scripts/publish-test-metrics.sh
#
#   METRICS_FILE     path to a file in Prometheus text exposition format.
#                    Defaults to $METRICS_FILE. Required either way.
#   PUSHGATEWAY_URL  pushgateway base URL, no trailing path.
#                    Default http://pushgateway.monitoring.svc.cluster.local:9091
#                    (the in-cluster Service name).
#   JOB              pushgateway job label.  Default definitely-not-crosswords-tests.
#   INSTANCE         pushgateway instance label. Default: $HOSTNAME, else
#                    "local".
#
# The push is a PUT to /metrics/job/<job>/instance/<instance>, which REPLACES
# that group. That is the right verb for a CI run: a re-run overwrites the last
# result instead of leaving stale series behind for the retention period.
#
# --dry-run validates the file and prints the payload and the target URL without
# sending anything. It still requires a valid exposition file — a dry run that
# silently accepted garbage would be worse than no dry run at all.
set -euo pipefail

DRY_RUN=0
METRICS_PATH=""

usage() {
  sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed -e 's/^# \{0,1\}//' -e '/^set -euo/d'
}

for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    -h | --help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "::error::unknown option '$arg' (see --help)" >&2
      exit 2
      ;;
    *)
      if [ -n "$METRICS_PATH" ]; then
        echo "::error::only one METRICS_FILE may be given (got '$METRICS_PATH' and '$arg')" >&2
        exit 2
      fi
      METRICS_PATH="$arg"
      ;;
  esac
  shift
done

if [ -z "$METRICS_PATH" ] && [ "$#" -gt 0 ]; then
  METRICS_PATH="$1"
fi
METRICS_PATH="${METRICS_PATH:-${METRICS_FILE:-}}"

if [ -z "$METRICS_PATH" ]; then
  echo "::error::no metrics file given: pass one as \$1 or set METRICS_FILE" >&2
  exit 2
fi

if [ ! -e "$METRICS_PATH" ]; then
  echo "::error::metrics file '$METRICS_PATH' does not exist" >&2
  exit 2
fi

if [ ! -r "$METRICS_PATH" ]; then
  echo "::error::metrics file '$METRICS_PATH' is not readable" >&2
  exit 2
fi

# A pushgateway rejects a payload it cannot parse with an opaque 400, and an
# empty file would be accepted and delete the group's metrics. Require at least
# one # HELP line: that is the cheapest reliable proof the file is exposition
# format and not, say, k6's raw JSON summary.
help_lines="$(grep -c '^# HELP ' "$METRICS_PATH" || true)"
sample_lines="$(grep -cvE '^[[:space:]]*(#|$)' "$METRICS_PATH" || true)"
if [ "${help_lines:-0}" -eq 0 ] || [ "${sample_lines:-0}" -eq 0 ]; then
  echo "::error::'$METRICS_PATH' is not Prometheus exposition format:" \
    "expected at least one '# HELP' line and one sample line, found ${help_lines:-0} HELP and ${sample_lines:-0} sample lines" >&2
  exit 3
fi

PUSHGATEWAY_URL="${PUSHGATEWAY_URL:-http://pushgateway.monitoring.svc.cluster.local:9091}"
PUSHGATEWAY_URL="${PUSHGATEWAY_URL%/}"

if [ -z "$PUSHGATEWAY_URL" ]; then
  echo "::error::PUSHGATEWAY_URL is empty — there is nowhere to push metrics to" >&2
  exit 2
fi

JOB="${JOB:-definitely-not-crosswords-tests}"
INSTANCE="${INSTANCE:-${HOSTNAME:-local}}"
PUSH_URL="${PUSHGATEWAY_URL}/metrics/job/${JOB}/instance/${INSTANCE}"

if [ "$DRY_RUN" -eq 1 ]; then
  echo "dry run: not sending to ${PUSH_URL}"
  echo "--- payload (${METRICS_PATH}) ---"
  cat "$METRICS_PATH"
  echo "--- end payload ---"
  exit 0
fi

# Fail loudly and specifically rather than letting curl's exit code stand in
# for an explanation. --fail-with-body surfaces the pushgateway's own error.
if ! curl --fail-with-body --show-error --silent \
  --max-time 30 --request PUT \
  --header 'Content-Type: text/plain; version=0.0.4' \
  --data-binary "@${METRICS_PATH}" \
  "$PUSH_URL"; then
  echo "::error::PUT ${PUSH_URL} failed — the pushgateway is unreachable or rejected the payload (its response, if any, is above)" >&2
  exit 1
fi

echo "Pushed ${sample_lines} sample(s) from ${METRICS_PATH} to ${PUSH_URL}"
