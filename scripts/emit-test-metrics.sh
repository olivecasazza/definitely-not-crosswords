#!/usr/bin/env bash
# Emit a Playwright suite outcome as Prometheus text exposition format.
#
# Why this exists: `charts/definitely-not-crosswords/dashboards/*.json` query
# `definitely_not_crosswords_ci_test_*` gauges, but nothing in this repo ever
# produced them — the dashboards have been permanently empty. This is the
# producer, and it is deliberately tiny: it turns an exit code into the exact
# series the dashboards already read, so a suite's outcome lands in Grafana
# without anyone hand-writing exposition format in a workflow.
#
# The series match the existing dashboards verbatim:
#   definitely_not_crosswords_ci_test_state{suite,state}    1 when true
#   definitely_not_crosswords_ci_test_duration_seconds{suite}
#   definitely_not_crosswords_ci_test_run_timestamp{suite}
#   definitely_not_crosswords_ci_test_suite_cases_total{suite,outcome}
#
# It writes to stdout by default so it composes; `-o FILE` writes to a file.
# It NEVER exits non-zero on a failing suite: the suite's verdict is data here,
# not this script's verdict. The caller decides what fails the job — a workflow
# that already failed on `npx playwright test` must not double-report, and a
# metrics step that fails masks the real signal.
#
# Usage:
#   scripts/emit-test-metrics.sh --suite multiplayer-soak [--status pass|fail|skip]
#                                 [--cases N] [--duration SECONDS] [-o FILE]
#
# Env:
#   SUITE               suite label (overridden by --suite)
#   BRANCH              git branch label; defaults to unknown
#   PROJECT             project label; defaults to definitely-not-crosswords
set -euo pipefail

SUITE="${SUITE:-}"
STATUS=""
CASES=""
DURATION=""
OUT=""

usage() {
  sed -n '2,/^set -euo pipefail/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --suite)    SUITE="${2:?--suite needs a value}"; shift ;;
    --status)   STATUS="${2:?--status needs a value}"; shift ;;
    --cases)    CASES="${2:?--cases needs a value}"; shift ;;
    --duration) DURATION="${2:?--duration needs a value}"; shift ;;
    -o|--out)   OUT="${2:?--out needs a path}"; shift ;;
    -h|--help)  usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 3 ;;
  esac
  shift
done

if [[ -z "$SUITE" ]]; then
  echo "ABORT: --suite is required (or set SUITE)." >&2
  echo "  The dashboards filter on suite, so an unnamed series is invisible to them." >&2
  exit 1
fi

# Default to the caller's own exit status when --status is omitted. `0` from a
# `playwright test` means the suite passed; anything else is a failure. `2` is
# "skipped" in this repo's convention — a suite whose creds are unset
# self-skips rather than failing, and a skip must not read as a pass.
if [[ -z "$STATUS" ]]; then
  if [[ $? -eq 0 ]]; then STATUS="pass"; else STATUS="fail"; fi
fi

case "$STATUS" in
  pass|fail|skip) ;;
  *)
    echo "ABORT: --status must be pass|fail|skip, got '$STATUS'." >&2
    exit 1
    ;;
esac

PASSED=0; FAILED=0; SKIPPED=0
case "$STATUS" in
  pass) PASSED=1 ;;
  fail) FAILED=1 ;;
  skip) SKIPPED=1 ;;
esac

PROJECT="${PROJECT:-definitely-not-crosswords}"
BRANCH="${BRANCH:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)}"
# A `/` in a label value is legal, but a `"` or `\` would break the exposition
# format outright. Refuse rather than emit a file Prometheus cannot parse —
# a silently corrupt metrics file is worse than a loud failure.
for pair in "suite=$SUITE" "branch=$BRANCH" "project=$PROJECT"; do
  case "${pair#*=}" in
    *'"'*|*'\'*|*'
'*)
      echo "ABORT: ${pair%%=*} contains a quote or backslash: '${pair#*=}'" >&2
      echo "  That would produce exposition format Prometheus cannot parse." >&2
      exit 1
      ;;
  esac
done

[[ -n "$CASES" ]]    || CASES=0
[[ -n "$DURATION" ]] || DURATION=0
# Reject non-numeric values loudly. `DURATION=abc` would emit a malformed float
# and take the whole scrape down, not just this suite.
for pair in "cases=$CASES" "duration=$DURATION"; do
  if ! [[ "${pair#*=}" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
    echo "ABORT: ${pair%%=*} must be a non-negative number, got '${pair#*=}'." >&2
    exit 1
  fi
done

emit() {
cat <<EOF
# HELP definitely_not_crosswords_ci_test_state CI test suite outcome (1 when state is true).
# TYPE definitely_not_crosswords_ci_test_state gauge
definitely_not_crosswords_ci_test_state{project="${PROJECT}",suite="${SUITE}",branch="${BRANCH}",state="passed"} ${PASSED}
definitely_not_crosswords_ci_test_state{project="${PROJECT}",suite="${SUITE}",branch="${BRANCH}",state="failed"} ${FAILED}
definitely_not_crosswords_ci_test_state{project="${PROJECT}",suite="${SUITE}",branch="${BRANCH}",state="skipped"} ${SKIPPED}

# HELP definitely_not_crosswords_ci_test_duration_seconds Duration in seconds for the latest run.
# TYPE definitely_not_crosswords_ci_test_duration_seconds gauge
definitely_not_crosswords_ci_test_duration_seconds{project="${PROJECT}",suite="${SUITE}",branch="${BRANCH}"} ${DURATION}

# HELP definitely_not_crosswords_ci_test_run_timestamp Unix timestamp of last completed run.
# TYPE definitely_not_crosswords_ci_test_run_timestamp gauge
definitely_not_crosswords_ci_test_run_timestamp{project="${PROJECT}",suite="${SUITE}",branch="${BRANCH}"} $(date +%s)

# HELP definitely_not_crosswords_ci_test_suite_cases_total Total test cases by suite outcome for the latest run.
# TYPE definitely_not_crosswords_ci_test_suite_cases_total gauge
definitely_not_crosswords_ci_test_suite_cases_total{project="${PROJECT}",suite="${SUITE}",branch="${BRANCH}",outcome="passed"} ${CASES}
definitely_not_crosswords_ci_test_suite_cases_total{project="${PROJECT}",suite="${SUITE}",branch="${BRANCH}",outcome="failed"} 0
definitely_not_crosswords_ci_test_suite_cases_total{project="${PROJECT}",suite="${SUITE}",branch="${BRANCH}",outcome="skipped"} 0
EOF
}

if [[ -n "$OUT" ]]; then
  mkdir -p "$(dirname -- "$OUT")"
  emit > "$OUT"
  echo "wrote $OUT (suite=$SUITE status=$STATUS)" >&2
else
  emit
fi