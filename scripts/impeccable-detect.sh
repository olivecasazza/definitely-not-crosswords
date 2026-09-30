#!/usr/bin/env bash
# Run impeccable's design detector over a file or directory tree and print a
# real finding count.
#
# WHY THIS EXISTS
#
# `impeccable detect` writes its human-readable findings to STDERR and leaves
# STDOUT empty. That is deliberate - the tool documents it as "findings go to
# stderr so stdout stays available for structured output" - but it means a
# working scan and a scan that looked at nothing are byte-identical on stdout.
#
# This bit the repo once already: a directory-wide pass over client/web/src
# was read off stdout, came back empty, and was reported as "the detector's
# directory walk is broken". It was not. Given a file with a known
# layout-transition finding, `impeccable detect <dir>` recurses correctly,
# reports it, and its --json output is well-formed. The bug was in the
# invocation, not the tool. A detector that reports "clean" when it did not
# look is worse than no detector, so this wrapper exists to make the correct
# stream the default and the exit code the source of truth.
#
# USAGE
#   scripts/impeccable-detect.sh [path ...]        # default: client/web/src
#   scripts/impeccable-detect.sh --json [path ...]  # raw JSON on stdout
#
# EXIT STATUS
#   0  no primary findings (advisories may still be listed)
#   1  usage error / detector not found
#   2  at least one finding, or a target could not be scanned
#
# The tool's own exit code is passed through rather than reimplemented, so
# this stays correct if upstream changes its contract.

set -euo pipefail

json=0
if [ "${1:-}" = "--json" ]; then
  json=1
  shift
fi

targets=("$@")
if [ "${#targets[@]}" -eq 0 ]; then
  targets=("client/web/src")
fi

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$repo_root"

# Resolve the detector through the launcher, which owns the binary search
# path (IMPECCABLE_BIN, sibling binary, version-pinned cache, PATH).
launcher=""
for candidate in \
  "${IMPECCABLE_SKILL_DIR:-}/scripts/impeccable" \
  "$HOME/.claude/skills/impeccable/scripts/impeccable"
do
  if [ -x "$candidate" ]; then
    launcher="$candidate"
    break
  fi
done

if [ -z "$launcher" ]; then
  echo "impeccable-detect: cannot find the impeccable launcher." >&2
  echo "Set IMPECCABLE_SKILL_DIR, or install the skill (nixlab:" >&2
  echo "modules/home/impeccable-design/default.nix symlinks it to" >&2
  echo "~/.claude/skills/impeccable)." >&2
  exit 1
fi

# JSON on stdout: the tool already does this correctly, so pass it through
# without capturing stderr into the stream.
if [ "$json" -eq 1 ]; then
  set +e
  "$launcher" detect "${targets[@]}" --json
  status=$?
  set -e
  exit "$status"
fi

# Text mode: merge the findings stream so the output is actually visible, and
# let the tool's exit code carry the verdict.
set +e
"$launcher" detect "${targets[@]}" 2>&1
status=$?
set -e

# A non-zero status here means findings exist (2) or a target failed to scan
# (1). Say which, so a scan failure is never mistaken for a clean tree.
if [ "$status" -ne 0 ]; then
  echo >&2
  echo "impeccable-detect: exit ${status} — findings above, or a target could not be scanned." >&2
  echo "impeccable-detect: an empty report with a non-zero status is a SCAN FAILURE, not a clean tree." >&2
fi

exit "$status"
