#!/usr/bin/env bash
# Every committed walk step list is runnable by steps-walk.py, and a malformed one fails the check.
# run-tests: no-host-screen: JSON validation only; no guest is booted
# run-tests: inputs richos/app/scripts/steps-walk-data.test.sh richos/app/scripts/testvm/steps-walk.py docs/verification/2026-10-01-nightly-33-candidate-walk/steps/walk3.json docs/verification/2026-10-01-nightly-33-candidate-walk/steps/walk5.json docs/verification/2026-10-01-nightly-33-candidate-walk/steps/walk7.json docs/verification/2026-10-01-nightly-33-candidate-walk/steps/walk8.json docs/verification/2026-10-05-nightly-36-candidate-walk/steps/walkA.json docs/verification/2026-10-05-nightly-36-candidate-walk/steps/walkB.json docs/verification/2026-10-05-nightly-36-candidate-walk/steps/walkC.json docs/verification/2026-10-05-nightly-36-candidate-walk/steps/walkD.json docs/verification/2026-10-05-nightly-37-candidate-walk/steps/walkE.json docs/verification/2026-10-06-nightly-39-candidate-walk/steps/walkS.json docs/verification/2026-10-05-s8-sidebar-toggle-walk/steps/walk.json docs/verification/2026-10-06-connected-folders-walk/steps/walk.json docs/verification/2026-10-08-nightly-45-candidate-walk/steps/walkI.json docs/verification/2026-10-08-nightly-46-candidate-walk/steps/walkS.json docs/verification/2026-10-08-nightly-46-candidate-walk/steps/walkT.json docs/verification/2026-10-08-nightly-47-candidate-walk/steps/walkS.json docs/verification/2026-10-08-nightly-47-candidate-walk/steps/walkT.json docs/verification/2026-10-09-codex-review-switch-walk/steps/walk.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkB.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkB2.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkB3.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkC.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkD.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkK.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkP.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkP2.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkQ.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkR.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkS.json
# run-tests: covers docs/verification/2026-10-01-nightly-33-candidate-walk/steps/walk3.json docs/verification/2026-10-01-nightly-33-candidate-walk/steps/walk5.json docs/verification/2026-10-01-nightly-33-candidate-walk/steps/walk7.json docs/verification/2026-10-01-nightly-33-candidate-walk/steps/walk8.json docs/verification/2026-10-05-nightly-36-candidate-walk/steps/walkA.json docs/verification/2026-10-05-nightly-36-candidate-walk/steps/walkB.json docs/verification/2026-10-05-nightly-36-candidate-walk/steps/walkC.json docs/verification/2026-10-05-nightly-36-candidate-walk/steps/walkD.json docs/verification/2026-10-05-nightly-37-candidate-walk/steps/walkE.json docs/verification/2026-10-06-nightly-39-candidate-walk/steps/walkS.json docs/verification/2026-10-05-s8-sidebar-toggle-walk/steps/walk.json docs/verification/2026-10-06-connected-folders-walk/steps/walk.json docs/verification/2026-10-08-nightly-45-candidate-walk/steps/walkI.json docs/verification/2026-10-08-nightly-46-candidate-walk/steps/walkS.json docs/verification/2026-10-08-nightly-46-candidate-walk/steps/walkT.json docs/verification/2026-10-08-nightly-47-candidate-walk/steps/walkS.json docs/verification/2026-10-08-nightly-47-candidate-walk/steps/walkT.json docs/verification/2026-10-09-codex-review-switch-walk/steps/walk.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkB.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkB2.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkB3.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkC.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkD.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkK.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkP.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkP2.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkQ.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkR.json docs/verification/2026-10-10-nightly-48-candidate-walk/steps/walkS.json
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
TOOL="$HERE/testvm/steps-walk.py"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/steps-walk-data.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT

# Every step list under a docs/verification/*/steps/ directory is declared above, so a new one
# cannot be committed without this suite claiming it.
declared="$(grep '^# run-tests: covers ' "${BASH_SOURCE[0]}" | sed 's/^# run-tests: covers //' | tr ' ' '\n' | sort)"
cd "$ROOT"
files=(docs/verification/*/steps/*.json)
found="$(printf '%s\n' "${files[@]}" | sort)"
if [ "$declared" != "$found" ]; then
  echo "FAIL: step lists on disk differ from this suite's covers row" >&2
  diff <(echo "$declared") <(echo "$found") >&2 || true
  exit 1
fi
echo "ok    covers row names every committed step list"

python3 -B "$TOOL" --check "${files[@]}"
echo "ok    every committed step list is valid"

# The check must fail on a malformed list: an unknown op, a missing argument, a wrong type.
for bad in '[{"op":"nonsense"}]' '[{"op":"push","src":"a"}]' '[{"op":"wait","seconds":"6"}]' '{"op":"wait"}' '[]' 'not json'; do
  printf '%s' "$bad" > "$TMP/bad.json"
  if python3 -B "$TOOL" --check "$TMP/bad.json" >/dev/null; then
    echo "FAIL: malformed step list accepted: $bad" >&2
    exit 1
  fi
done
echo "ok    malformed step lists fail the check"
