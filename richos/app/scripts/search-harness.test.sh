#!/usr/bin/env bash
# search-harness.test.sh — the archive-search harness as a program (search PRD v3.1, "Delivery and
# acceptance": "Build a small reusable archive-search harness first"): inventory, query with Super
# Finder on and off, compare over the committed synthetic question set, and the refusal of a
# compare output inside this published repository. Stand-in embedder, synthetic folder; no model.
# run-tests: no-host-screen: a command-line program over temporary folders; nothing is launched on any screen
# run-tests: inputs richos/app/scripts/search-harness.test.sh richos/app/scripts/search-harness.test.py richos/app/Cargo.toml richos/app/Cargo.lock richos/app/crates/richos-search richos/app/third-party/qdrant-edge
# run-tests: covers richos/app/crates/richos-search/src/bin/harness.rs richos/app/crates/richos-search/tests/fixtures/standin-questions.json
set -euo pipefail
cd "$(dirname "$0")/.."
# No clock of this suite's own around the build: it waits on Cargo's lock behind whatever else
# builds (the same rule replay.test.sh follows).
RICHOS_SEARCH_HARNESS="$(cargo build --quiet -p richos-search --bin richos-search-harness --message-format=json \
  | python3 -c 'import json,sys
for line in sys.stdin:
    r=json.loads(line)
    if r.get("reason")=="compiler-artifact" and r["target"]["name"]=="richos-search-harness" and r.get("executable"):
        print(r["executable"])')"
[ -x "$RICHOS_SEARCH_HARNESS" ] || { echo "richos-search-harness was not built" >&2; exit 1; }
RICHOS_SEARCH_HARNESS="$RICHOS_SEARCH_HARNESS" PYTHONDONTWRITEBYTECODE=1 python3 scripts/search-harness.test.py
