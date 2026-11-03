#!/usr/bin/env bash
# G11 regressions for review_delivery.py, pierce_shadow.py, review_watch.py,
# second_review.py and recovery_events.py. No live review or model invocation.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec python3 -B "$HERE/review-recovery.test.py" "$@"
