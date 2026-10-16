#!/usr/bin/env bash
# operator-walk.test.sh — the operator walk's back end and the operator mutation harness,
# proved without a VM, a model or a network.
#
#   scripts/operator-walk.test.sh
#
# The walk's back end (crates/richos-core/examples/operator_walk.rs) is what the W2 walk runs in
# the test VM in place of the app's window (operator back-end spec r3 §6 verification 5): it
# must answer the declaration gate exactly as the shell does, refuse to host his team without a
# valid declaration, and serve the report tool exactly as the app does. The mutation harness
# (scripts/operator-mutations.py) must still apply every mutant exactly once and name tests
# that exist; its full run needs cargo per mutant and is run by hand under reserve.py.
#
# run-tests: inputs richos/app/scripts/operator-probes/guest_probes.py richos/app/scripts/operator-probes/test/test_guest_probes.py richos/app/scripts/operator-walk.test.sh richos/app/crates/richos-core richos/app/scripts/operator-mutations.py richos/app/scripts/bin/cargo richos/app/scripts/lib/cargo_identity.py richos/app/scripts/lib/cargo-cache-env.sh richos/app/scripts/lib/cargo-target.sh
# run-tests: covers richos/app/scripts/operator-probes/guest_probes.py richos/app/scripts/operator-probes/test/test_guest_probes.py richos/app/crates/richos-core/examples/operator_walk.rs richos/app/scripts/operator-mutations.py
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/lib/cargo-cache-env.sh"
APP="$(cd "$HERE/.." && pwd)"
PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL $1"; }

if ! command -v cargo >/dev/null 2>&1; then
  echo "operator-walk.test.sh: no cargo on PATH; the walk's back end cannot be built." >&2
  exit 2
fi
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/operator-walk-test.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT

if ! ( cd "$APP" && cargo build --quiet -p richos-core --example operator_walk ); then
  echo "operator-walk.test.sh: the example did not build" >&2
  exit 1
fi
. "$HERE/lib/cargo-target.sh"
WALK="$(cargo_target_dir "$APP")/debug/examples/operator_walk"
[ -x "$WALK" ] || { echo "operator-walk.test.sh: no binary at $WALK" >&2; exit 1; }

# W1: no declaration is the product path, as the shell's gate says.
mkdir -p "$SCRATCH/none"
out="$("$WALK" gate "$SCRATCH/none")"
if [ "$out" = '{"gate":"product"}' ]; then ok "W1 no operator.json is the product path"; else bad "W1 got: $out"; fi

# W2: a present, broken declaration refuses, with his one sentence.
mkdir -p "$SCRATCH/broken"
printf '{ not json' > "$SCRATCH/broken/operator.json"
out="$("$WALK" gate "$SCRATCH/broken")"
case "$out" in
  *'"gate":"refused"'*'Your team is switched off on this Mac because '*'Rich can fix it.'*) ok "W2 a broken operator.json refuses with the sentence" ;;
  *) bad "W2 got: $out" ;;
esac

# W3: the walk will not host his team without a valid declaration, and says why.
err="$("$WALK" host "$SCRATCH/none" "$SCRATCH/state" "$SCRATCH/operator" 2>&1 >/dev/null </dev/null)"; code=$?
if [ "$code" = 2 ] && [ "${err#*product path}" != "$err" ]; then ok "W3 host refuses the product path (exit 2)"; else bad "W3 exit $code: $err"; fi
if [ ! -e "$SCRATCH/operator" ]; then ok "W3 a refusal writes nothing"; else bad "W3 the refusal wrote $SCRATCH/operator"; fi

# W4: the report tool is the app's own server: it speaks MCP and refuses without a scope.
reply="$(printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"report","arguments":{"kind":"update","text":"x"}}}' \
  | "$WALK" --operator-mcp "$SCRATCH/no-scope.json")"
case "$reply" in
  # serde_json writes keys in order, so the refusal's text comes before its isError flag.
  *'"serverInfo":{"name":"richos_operator"'*'report scope'*'"isError":true'*) ok "W4 the report server answers and refuses with no scope" ;;
  *) bad "W4 got: $reply" ;;
esac

# W6: the front desk's operator tools are the app's own server too: it speaks MCP, carries
# no behavioral instructions, and refuses without a scope.
reply="$(printf '%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"read","arguments":{}}}' \
  | "$WALK" --operator-desk-mcp "$SCRATCH/no-desk-scope.json")"
if printf '%s' "$reply" | grep -q '"serverInfo":{"name":"richos_operator"' \
  && ! printf '%s' "$reply" | grep -q '"instructions"' \
  && printf '%s' "$reply" | grep -q 'RichOS has not opened your team to this conversation yet'; then
  ok "W6 the front desk's operator tools answer without behavioral instructions and refuse with no scope"
else
  bad "W6 got: $reply"
fi

# The S6 walk must reject partial delivery, duplicate input and missing phone provenance.
if out="$(python3 "$APP/scripts/operator-probes/test/test_guest_probes.py" 2>&1)"; then
  ok "S6 probe-harness assertions: $out"
else
  bad "S6 probe-harness assertions: $out"
fi

# W5: every mutant of the mutation harness still applies, and names a test that exists.
if out="$(python3 "$APP/scripts/operator-mutations.py" --check 2>&1)"; then
  ok "W5 $out"
else
  bad "W5 $out"
fi

# W6: a name that is no mutant is refused (exit 2) before anything runs, alone or among valid
# names; it used to select nothing, print "0 of 0 proven" and exit 0.
FIRST="$(python3 "$APP/scripts/operator-mutations.py" --list | head -1 | cut -d' ' -f1)"
for SELECTION in "definitely-not-a-mutant" "$FIRST,definitely-not-a-mutant"; do
  out="$(python3 "$APP/scripts/operator-mutations.py" --only "$SELECTION" 2>&1)"
  code=$?
  if [ "$code" = 2 ] && printf '%s' "$out" | grep -Fq "NOT A MUTANT: definitely-not-a-mutant"; then
    ok "W6 --only $SELECTION is refused, exit 2"
  else
    bad "W6 --only $SELECTION exit $code: $(printf '%s' "$out" | head -c 200)"
  fi
done

echo "operator-walk.test.sh: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
