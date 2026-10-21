#!/usr/bin/env bash
# work-walk.test.sh — the work path's crash-matrix host, proved without a VM, a model or a network.
#
#   scripts/work-walk.test.sh
#
# The host (crates/richos-core/examples/work_walk.rs) is what W4 runs in the test VM in place of
# the app's window (richos-hq docs/plans/2026-09-27-work-path-answer-delivery-design.md §4.2): the
# real WorkHost on the production work lease, the durable question store and inbox, boot recovery
# and the question worker's pass. What can be proved without a guest is proved here: it builds as
# the product would (every crash point empty) and as the matrix builds it (crash-points), it
# serves the back end's question tool exactly as the app does, and it refuses to host without an
# engine, writing nothing. W4's guest runner was retired and then deleted with the operator
# probes (2026-10-21).
#
# run-tests: inputs richos/app/scripts/work-walk.test.sh richos/app/crates/richos-core richos/app/scripts/bin/cargo richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/cargo-cache-env.sh richos/app/scripts/lib/cargo-target.sh
# run-tests: covers richos/app/crates/richos-core/examples/work_walk.rs
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib/cargo-cache-env.sh"
APP="$(cd "$HERE/.." && pwd)"
PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL $1"; }

if ! command -v cargo >/dev/null 2>&1; then
  echo "work-walk.test.sh: no cargo on PATH; the host cannot be built." >&2
  exit 2
fi
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/work-walk-test.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT

# W1: it builds as a product build would, with every crash point an empty function.
if ( cd "$APP" && cargo build --quiet -p richos-core --example work_walk ); then
  ok "W1 builds without crash points"
else
  bad "W1 the example did not build without crash points"
fi
# W2: and as the VM matrix builds it. The binary the rest of this suite runs is this one.
if ! ( cd "$APP" && cargo build --quiet -p richos-core --example work_walk --features crash-points ); then
  echo "work-walk.test.sh: the example did not build with crash-points" >&2
  exit 1
fi
ok "W2 builds with crash-points"
. "$HERE/lib/cargo-target.sh"
WALK="$(cargo_target_dir "$APP")/debug/examples/work_walk"
[ -x "$WALK" ] || { echo "work-walk.test.sh: no binary at $WALK" >&2; exit 1; }

# W3: no mode is a usage refusal, not a host.
err="$("$WALK" 2>&1 >/dev/null </dev/null)"; code=$?
if [ "$code" != 0 ] && [ "${err#*usage: work_walk host}" != "$err" ]; then ok "W3 no mode refuses with its usage"; else bad "W3 exit $code: $err"; fi

# W4: the back end's question tool is the app's own server: it speaks MCP and lists `ask`.
reply="$(printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | "$WALK" --questions-mcp "$SCRATCH/no-scope.json")"
case "$reply" in
  *'"serverInfo":{"name":"richos_questions"'*'"name":"ask"'*) ok "W4 the question server answers and lists ask" ;;
  *) bad "W4 got: $reply" ;;
esac

# W5: no engine, no host, and nothing written under the data folder it was given.
err="$("$WALK" host "$SCRATCH/no-engine" "$SCRATCH/no-runtime" "$SCRATCH/data" 2>&1 >/dev/null </dev/null)"; code=$?
if [ "$code" != 0 ]; then ok "W5 host refuses without an engine (exit $code)"; else bad "W5 exit 0: $err"; fi
if [ ! -e "$SCRATCH/data" ]; then ok "W5 a refusal writes nothing"; else bad "W5 the refusal wrote $SCRATCH/data"; fi

echo "work-walk.test.sh: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
