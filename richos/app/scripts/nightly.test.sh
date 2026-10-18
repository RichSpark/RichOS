#!/usr/bin/env bash
# run-tests: inputs richos/app/scripts/nightly.test.sh richos/app/scripts/nightly.test.py richos/app/scripts/nightly.py richos/app/scripts/lib/github_access.py richos/app/scripts/make-release.sh richos/app/stable-promotions.json
# run-tests: covers richos/app/scripts/nightly.py richos/app/scripts/lib/github_access.py richos/app/stable-promotions.json
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 "$here/nightly.test.py"
echo '  PASS  nightly versions, real Git reservations, channel races and publication failures'
