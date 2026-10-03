#!/usr/bin/env bash
# Dry-run the deploy readiness steps' build assertions (DEF-288 staging, DEF-289 production).
#
# The steps cannot be verified by triggering a deploy: they fire on workflow_run / on
# release tags, and a continuous staging deploy of a :<sha> image is exactly the case
# that used to be unverifiable. The workflows also check this repo out nowhere — they
# clone nixlab — so the logic has to stay inline in the YAML and a test cannot just
# source a script from this tree.
#
# So this harness reads each step's own `run:` block out of its workflow, writes it to
# a temp file, and executes it verbatim against a stubbed curl/sleep. It exercises the
# shipped artifact, not a copy of it: edit the branch in the YAML and these cases
# change verdict.
#
# Both deploy workflows are covered because they carry the SAME readiness probe, and
# a fix on one of them is not a fix on the other: DEF-288 closed the `*)` arm on
# staging only, and deploy-production.yml kept skipping until DEF-289.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# label|workflow|job|step|base url
TARGETS=(
  "staging|$REPO_ROOT/.github/workflows/deploy-staging.yml|deploy-staging|Verify staging is actually serving|https://staging.invalid"
  "production|$REPO_ROOT/.github/workflows/deploy-production.yml|deploy-production|Verify production is actually serving|https://production.invalid"
)

# --- extract a step's run block ----------------------------------------------
# extract <workflow> <job> <step name> <outfile>
extract() {
  local workflow="$1" job="$2" step_name="$3" out="$4"
  if [ ! -f "$workflow" ]; then
    echo "::error::no $workflow" >&2
    return 2
  fi
  if ! python3 - "$workflow" "$job" "$step_name" > "$out" <<'PY'
import sys, yaml
wf_path, job_name, step_name = sys.argv[1], sys.argv[2], sys.argv[3]
with open(wf_path) as fh:
    wf = yaml.safe_load(fh)
for step in wf["jobs"][job_name]["steps"]:
    if step.get("name") == step_name:
        sys.stdout.write(step["run"])
        break
else:
    sys.stderr.write("no step named %r\n" % step_name)
    sys.exit(1)
PY
  then
    echo "::error::could not extract the '$step_name' step from $workflow" >&2
    return 2
  fi
  if [ ! -s "$out" ]; then
    echo "::error::extracted readiness script is empty — is the step renamed?" >&2
    return 2
  fi
  return 0
}

# --- stubs --------------------------------------------------------------------
mkdir -p "$WORK/bin"

# curl: healthz code from $STUB_HEALTHZ, config body from $STUB_CONFIG. The URL is
# whichever argument actually looks like one, so option values (--max-time 15, -w
# '%{http_code}') cannot be mistaken for it.
cat > "$WORK/bin/curl" <<'STUB'
#!/usr/bin/env bash
url=""
want_out=0
for a in "$@"; do
  case "$a" in
    http://*|https://*) url="$a" ;;
    -o) want_out=1 ;;
    *) if [ "$want_out" = 1 ]; then want_out=0; fi ;;
  esac
done
case "$url" in
  */api/healthz) printf '%s' "${STUB_HEALTHZ:-200}" ;;
  */api/config)  printf '%s' "${STUB_CONFIG:-{\}}" ;;
  *)             printf '%s' "${STUB_HEALTHZ:-200}" ;;
esac
STUB

# sleep is a no-op so the retry loop runs at full speed. Every call is recorded, so
# the cases can assert that a mismatch really is retried rather than verdicted.
cat > "$WORK/bin/sleep" <<'STUB'
#!/usr/bin/env bash
printf 'sleep %s\n' "${1:-}" >> "$STUB_SLEEP_LOG"
exit 0
STUB

chmod +x "$WORK/bin/curl" "$WORK/bin/sleep"
export PATH="$WORK/bin:$PATH"

# --- harness ------------------------------------------------------------------
rc=0
out=""
sleeps=0
: > "$WORK/sleeps"

# run <readiness script> <base url> <tag> <config> [healthz] [budget]
run() {
  local script="$1" base_url="$2" tag="$3" config="$4" healthz="${5:-200}" budget="${6:-3}"
  TAG="$tag" STUB_CONFIG="$config" STUB_HEALTHZ="$healthz" \
    BASE_URL="$base_url" POLL_INTERVAL=0 POLL_BUDGET="$budget" \
    STUB_SLEEP_LOG="$WORK/sleeps" \
    bash "$script" > "$WORK/out" 2>&1
  rc=$?
  out="$(cat "$WORK/out")"
  sleeps="$(wc -l < "$WORK/sleeps" 2>/dev/null || echo 0)"
  : > "$WORK/sleeps"
}

passed=0
failed=0

note_fail() { printf '  FAIL  %-46s %s\n' "$1" "$2"; failed=$((failed + 1)); }
note_ok()   { printf '  ok    %-46s %s\n' "$1" "${2:-}"; passed=$((passed + 1)); }

# expect <label> <want_rc> <want_substring...>
expect() {
  local label="$1" want_rc="$2" s missing=""
  shift 2
  if [ "$rc" != "$want_rc" ]; then
    printf '  FAIL  %-46s exit %s, want %s\n' "$label" "$rc" "$want_rc"
    printf '        %s\n' "$out"
    failed=$((failed + 1))
    return
  fi
  for s in "$@"; do
    printf '%s\n' "$out" | grep -qF -- "$s" || missing="$missing [$s]"
  done
  if [ -n "$missing" ]; then
    printf '  FAIL  %-46s exit %s, output lacks:%s\n' "$label" "$rc" "$missing"
    failed=$((failed + 1))
    return
  fi
  note_ok "$label" "exit $rc"
}

sha_tag="fedd69d5f2a8b3c4d5e6f708192a3b4c5d6e7f80"

# cases <label prefix> <readiness script> <base url>
cases() {
  local prefix="$1" script="$2" base="$3"

  echo
  echo "$prefix — continuous :<sha> deploys, the arm that used to be skipped"

  # The deploy that actually rolled: buildSha is the deployed sha, healthz is 200.
  run "$script" "$base" "$sha_tag" '{"environment":"staging","version":"0.1.49","buildSha":"fedd69d5f2a8"}'
  expect "$prefix sha tag, rolled build matches" 0 "serving expected build fedd69d5f2a8"

  # The DEF-124 shape: PR #111 merged, the deploy went green, and the host kept
  # serving the pre-merge build for days. healthz is 200 the whole time, so only
  # buildSha distinguishes the two — and it is the sha that differs.
  run "$script" "$base" "$sha_tag" '{"environment":"staging","version":"0.1.49","buildSha":"e606021c8629"}' 200 3
  expect "$prefix sha tag, stale pod still up -> fails" 1 \
    "but serving build e606021c8629, want fedd69d5f2a8" \
    "did not come up on $sha_tag" \
    "Do NOT treat this run as a healthy deploy"

  run "$script" "$base" "$sha_tag" '{"environment":"staging","version":"0.1.49"}' 200 2
  expect "$prefix sha tag, buildSha absent -> fails" 1 \
    "but serving build <none>, want fedd69d5f2a8"

  # The skip wording was the bug. If it reappears, the escape hatch is back.
  run "$script" "$base" "$sha_tag" '{"environment":"staging","version":"0.1.49","buildSha":"e606021c8629"}' 200 1
  if printf '%s\n' "$out" | grep -qF -- "skipping the version assertion"; then
    note_fail "$prefix sha tag, no 'skipping' escape hatch" "the step still skips the assertion for a :<sha> tag"
  else
    note_ok "$prefix sha tag, no 'skipping' escape hatch" "exit $rc"
  fi

  echo
  echo "$prefix — retry semantics: a mismatch is a retry condition, not a verdict"

  run "$script" "$base" "$sha_tag" '{"buildSha":"e606021c8629"}' 200 3
  if [ "$sleeps" -ge 2 ]; then
    note_ok "$prefix sha tag, mismatch is retried" "$sleeps retries"
  else
    note_fail "$prefix sha tag, mismatch is retried" "$sleeps retries, want >= 2"
  fi

  run "$script" "$base" "$sha_tag" '{"buildSha":"fedd69d5f2a8"}'
  if [ "$sleeps" -eq 0 ]; then
    note_ok "$prefix sha tag, match does not sleep"
  else
    note_fail "$prefix sha tag, match does not sleep" "slept $sleeps times on a match"
  fi

  echo
  echo "$prefix — release v-tag deploys, the pre-existing arm must not regress"

  run "$script" "$base" v0.1.83 '{"environment":"staging","version":"0.1.83","buildSha":"fedd69d5f2a8"}'
  expect "$prefix v tag, version matches" 0 "serving expected version 0.1.83"

  run "$script" "$base" v0.1.83 '{"environment":"staging","version":"0.1.82","buildSha":"fedd69d5f2a8"}' 200 2
  expect "$prefix v tag, wrong version -> fails" 1 \
    "but serving 0.1.82, want 0.1.83" \
    "did not come up on v0.1.83"

  echo
  echo "$prefix — healthz is still the retry trigger"

  run "$script" "$base" "$sha_tag" '{"buildSha":"fedd69d5f2a8"}' 502 2
  expect "$prefix sha tag, healthz 502 -> fails" 1 "healthz=502" "did not come up on $sha_tag"
}

echo "deploy readiness — build assertion (DEF-288 staging, DEF-289 production)"

for spec in "${TARGETS[@]}"; do
  IFS='|' read -r label workflow job step base <<< "$spec"
  script="$WORK/readiness-$label.sh"
  echo
  echo "=============================================================="
  echo "$workflow -> $job / \"$step\""
  echo "=============================================================="
  if ! extract "$workflow" "$job" "$step" "$script"; then
    exit 2
  fi
  cases "$label" "$script" "$base"
done

echo
echo "passed: $passed  failed: $failed"
[ "$failed" -eq 0 ]