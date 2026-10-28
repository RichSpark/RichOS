#!/usr/bin/env bash
# RichOS's live GitHub files and the phone probe name its repository at its home address,
# RichSpark/RichOS, and never at WebDevBooster/richos, its address before the GitHub move
# of 2026-10-28 (richos-hq docs/operations/2026-10-27-github-richcore-move.md). GitHub
# redirects the old address, so a file left on it still works; this keeps the files people
# open and the commands in them on the repository's real address.
# run-tests: no-host-screen: reads tracked text files only; no capture or input
# run-tests: inputs richos/app/scripts/github-home.test.sh .github richos/tools/phone-probe/server.js richos/tools/phone-probe/test/tls-verify.js
# run-tests: covers .github/ISSUE_TEMPLATE/bug_report.yml .github/ISSUE_TEMPLATE/config.yml .github/dependabot.yml .github/workflows/engine-run-record.yml .github/workflows/engine-self-verify.yml .github/workflows/packaging-ci.yml .github/workflows/ui-suite-ci.yml .github/README.md .github/SECURITY.md .github/workflows/README.md richos/tools/phone-probe/server.js richos/tools/phone-probe/test/tls-verify.js
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
cd "$ROOT"

HOME_REPO="RichSpark/RichOS"
FORMER_REPO="WebDevBooster/richos"
# Each of these names the repository, so each must name it at home.
NAMES_HOME=(
  .github/ISSUE_TEMPLATE/bug_report.yml
  .github/ISSUE_TEMPLATE/config.yml
  .github/dependabot.yml
  .github/workflows/engine-run-record.yml
  .github/workflows/engine-self-verify.yml
  .github/workflows/packaging-ci.yml
  .github/workflows/ui-suite-ci.yml
  .github/README.md
  .github/SECURITY.md
  .github/workflows/README.md
  richos/tools/phone-probe/server.js
  richos/tools/phone-probe/test/tls-verify.js
)

fail=0
for f in "${NAMES_HOME[@]}"; do
  if ! grep -Fq "$HOME_REPO" "$f"; then
    echo "FAIL  $f does not name $HOME_REPO"
    fail=1
  fi
done

# Nothing under .github, and nothing the probe prints or checks, names the old address.
old="$(grep -RIn -i -F "$FORMER_REPO" .github richos/tools/phone-probe/server.js richos/tools/phone-probe/test/tls-verify.js || true)"
if [ -n "$old" ]; then
  echo "FAIL  these lines name $FORMER_REPO, the address before the move:"
  printf '%s\n' "$old" | sed 's/^/        /'
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "PASS  ${#NAMES_HOME[@]} files name $HOME_REPO, and none under .github or in the phone probe names $FORMER_REPO"
