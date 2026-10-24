#!/usr/bin/env bash
# guard-ceo-ask-first.sh: RETIRED 2026-10-24 (orchestration reliability R3: an unrelated
# prepared decision never vetoes a dispatch). Registered nowhere. A session started before the
# land still runs this path from its cached hook table, so it resolves here and does nothing
# instead of failing as "not found". Remove after the supported upgrade window.
cat >/dev/null 2>&1 || true   # read the payload, so its writer never meets a closed pipe
exit 0
