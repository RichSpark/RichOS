#!/usr/bin/env bash
#
# ci-shard-selection.test.sh — a requested unit the inventory rejects must stop
# the sharded selector, not quietly vanish from the selection.
#
#   Q1  --units-file with valid units plus --shard lists shard i (the control).
#   Q2  --units-file naming one valid unit and one nonexistent unit, with
#       --shard, exits NON-ZERO and lists nothing. (Hunt P5-35: the planner's
#       failure was swallowed and the valid unit alone was listed with exit 0.)
#   Q3  the same file without --shard is still refused (the unsharded path
#       already checked; this pins that it stays so).
#
# SHARD_SH names another copy of ci-shard.sh inside a scripts/ directory that has
# its own ci-units.sh beside it (used to show Q2 fails before the fix).
#
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_DIR="${SHARD_SRC_DIR:-$SCRIPT_DIR}"

PASS=0; FAIL=0
SANDBOX="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/ci-shard-selection-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

ok()  { PASS=$((PASS + 1)); printf '  ok   %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf '  FAIL %s\n' "$1"; }

export CLAUDE_CONFIG_DIR="$SANDBOX/cfg"
export RICHOS_MACHINE_WORKERS="$SANDBOX/machine" RICHOS_ENGINE_PASS_DIR="$SANDBOX/slot"
unset RICHOS_WORKER_TOKENS RICHOS_WORKER_SLOT_HELD RICHOS_WORKER_BORROW_LOCK RICHOS_VERIFICATION_CONTAMINATION
mkdir -p "$CLAUDE_CONFIG_DIR/state"

E="$SANDBOX/engine"
mkdir -p "$E/scripts/lib"
printf '1.0.0-test\n' > "$E/VERSION"
for f in ci-shard.sh ci-units.sh; do cp "$SRC_DIR/$f" "$E/scripts/$f"; chmod +x "$E/scripts/$f"; done
for f in ci-receipts.py leak-canary.sh record-canary.sh tree-witness.sh proc_tree.py operator_fences.py worker_tokens.py engine_pass.py machine_settings.py; do
    cp "$SCRIPT_DIR/lib/$f" "$E/scripts/lib/$f"
done
printf '#!/usr/bin/env bash\nexit 0\n' > "$E/scripts/lib/green.test.sh"; chmod +x "$E/scripts/lib/green.test.sh"
SH="$E/scripts/ci-shard.sh"

printf 'scripts/lib/green.test.sh\n' > "$SANDBOX/good.txt"
printf 'scripts/lib/green.test.sh\nscripts/lib/does-not-exist.test.sh\n' > "$SANDBOX/bad.txt"

OUT="$(bash "$SH" --units-file "$SANDBOX/good.txt" --shard 1/1 --list 2>&1)"; RC=$?
if [ "$RC" = 0 ] && grep -q 'green.test.sh' <<<"$OUT"; then ok "Q1  a valid restricted set lists its unit under --shard"; else bad "Q1  (rc=$RC): $OUT"; fi

OUT="$(bash "$SH" --units-file "$SANDBOX/bad.txt" --shard 1/1 --list 2>&1)"; RC=$?
if [ "$RC" != 0 ] && ! grep -q '^scripts/lib/green.test.sh' <<<"$OUT"; then ok "Q2  a nonexistent requested unit stops the sharded selection"; else bad "Q2  (rc=$RC): $OUT"; fi

OUT="$(bash "$SH" --units-file "$SANDBOX/bad.txt" --list 2>&1)"; RC=$?
if [ "$RC" != 0 ]; then ok "Q3  the unsharded selection still refuses it"; else bad "Q3  (rc=$RC): $OUT"; fi

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
