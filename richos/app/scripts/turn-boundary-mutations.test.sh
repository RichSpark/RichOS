#!/usr/bin/env bash
# turn-boundary-mutations.test.sh — the turn-boundary mutation harness still applies, without cargo.
#
#   scripts/turn-boundary-mutations.test.sh
#
# CEO ruling §88: work he gives typed, aloud or from the phone starts the same way. Every entrance
# that runs his turns adopts at its boundary, and scripts/turn-boundary-mutations.py takes each
# door away in turn and requires a named test to go red. That full run needs cargo per mutant and
# is run by hand under reserve.py; this suite proves the cheap half on every change to the files
# the mutants live in: each mutant still applies exactly once and names tests that exist. A
# mutant whose site has moved proves nothing, and this is where that is noticed.
#
# run-tests: inputs richos/engine/.env.example richos/engine/scripts/lib/machine_settings.fixtures.json richos/app/scripts/turn-boundary-mutations.test.sh richos/app/scripts/turn-boundary-mutations.py richos/app/crates/richos-core/src/spine.rs richos/app/crates/richos-core/tests/ended_turn_tests.rs richos/app/src-tauri/src/main.rs richos/app/src-tauri/src/phone/bridge.rs richos/app/src-tauri/src/turn_boundary_tests.rs
# run-tests: covers richos/app/scripts/turn-boundary-mutations.py
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$APP/../.." && pwd)"
PASS=0
FAIL=0
ok() { PASS=$((PASS + 1)); echo "  ok   $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL $1"; }

SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/turn-boundary-mutations-test.XXXXXX")"
trap 'rm -rf "$SCRATCH"' EXIT

# 1. On the tree as it is, every mutant applies.
if out="$(python3 "$HERE/turn-boundary-mutations.py" --check 2>&1)"; then
  ok "every mutant applies exactly once and names tests that exist ($(printf '%s\n' "$out" | tail -1))"
else
  bad "the harness no longer applies to the tree:"
  printf '%s\n' "$out" | sed 's/^/       /'
fi

# 2. Negative control: the same check over a copy with the boot's door removed must refuse, or
#    step 1 proves nothing.
FILES="richos/app/crates/richos-core/src/spine.rs richos/app/crates/richos-core/tests/ended_turn_tests.rs richos/app/src-tauri/src/main.rs richos/app/src-tauri/src/phone/bridge.rs richos/app/src-tauri/src/turn_boundary_tests.rs"
for f in $FILES; do
  mkdir -p "$SCRATCH/$(dirname "$f")"
  cp "$ROOT/$f" "$SCRATCH/$f"
done
python3 - "$SCRATCH/richos/app/src-tauri/src/main.rs" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
door = "            adopt_at_the_turn_boundary(&mut spine, &work);\n"
assert text.count(door) == 1, "the boot's door is not in main.rs exactly once"
open(path, "w").write(text.replace(door, ""))
PY
if out="$(TURN_BOUNDARY_CHECK_ROOT="$SCRATCH" python3 "$HERE/turn-boundary-mutations.py" --check 2>&1)"; then
  bad "a copy with the boot's door removed still passed --check"
elif printf '%s\n' "$out" | grep -q "FAIL M10 boot"; then
  ok "a copy with the boot's door removed is refused, naming M10"
else
  bad "a copy with the boot's door removed failed, but not on M10:"
  printf '%s\n' "$out" | sed 's/^/       /'
fi

# 3. Negative control for the export: a crate that compiles in a file outside the exported roots
#    must be refused by --check, naming the file. The unmutated baseline cannot compile without
#    every such file, and it is 20 minutes into a cargo build before it says so.
cp "$ROOT/richos/app/src-tauri/src/main.rs" "$SCRATCH/richos/app/src-tauri/src/main.rs"  # undo step 2's mutation
rm -rf "$SCRATCH/richos/app/crates/probe"
mkdir -p "$SCRATCH/richos/app/crates/probe/src"
printf 'const X: &str = include_str!("../../../../engine/not-exported/probe.txt");\n' > "$SCRATCH/richos/app/crates/probe/src/lib.rs"
if out="$(TURN_BOUNDARY_CHECK_ROOT="$SCRATCH" python3 "$HERE/turn-boundary-mutations.py" --check 2>&1)"; then
  bad "a crate including a file outside the export still passed --check"
elif printf '%s\n' "$out" | grep -q "FAIL export misses richos/engine/not-exported/probe.txt"; then
  ok "an include outside the export is refused, naming richos/engine/not-exported/probe.txt"
else
  bad "an include outside the export failed, but not by naming the file:"
  printf '%s\n' "$out" | sed 's/^/       /'
fi

echo "turn-boundary-mutations.test.sh: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
