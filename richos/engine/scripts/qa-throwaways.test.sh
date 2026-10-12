#!/usr/bin/env bash
#
# qa-throwaways.test.sh — behavioral tests for scripts/qa-throwaways.sh.
#
# THE FIXTURE IS SYNTHETIC ON PURPOSE. The counting rules were derived from a
# real walk (ray-opus-vm8, agent ab5634d45f8c0eccb, 1091 rows), and that
# transcript is the operator's own record: it carries brief text, host paths
# and captured output, and it is not this repository's to commit. So each case
# below builds the smallest transcript that exercises one rule, in the same
# JSONL shape the harness writes, and the real walk stays where it is.
#
# Every case that follows exists because a plausible counter gets it wrong:
# a Write-only counter misses two thirds of vm8's helpers; a redirect counter
# without the heredoc/inline distinction counts `python3 - <<PY`; a path
# counter counts a script twice for being copied into the VM; and a counter
# with no toolkit arm reports a walk that ADDED a committed tool as the worst
# offender of the run.
#
# Run directly: scripts/qa-throwaways.test.sh
# Exit 0 = every case passed; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOL="$SCRIPT_DIR/qa-throwaways.sh"

PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d -t qa-throwaways-test.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); shift; [ "$#" -gt 0 ] && printf '        %s\n' "$*"; }

[ -x "$TOOL" ] || { echo "FATAL: $TOOL missing or not executable" >&2; exit 1; }

# --- fixture builders ------------------------------------------------------
# A transcript row in the harness's shape. `write_row <file> <tool> <json-input>`
write_row() {
    python3 - "$@" <<'PY'
import json, sys
path, tool, raw = sys.argv[1], sys.argv[2], sys.argv[3]
row = {"type": "assistant", "isSidechain": True,
       "message": {"role": "assistant",
                   "content": [{"type": "tool_use", "id": "t", "name": tool,
                                "input": json.loads(raw)}]}}
with open(path, "a", encoding="utf-8") as fh:
    fh.write(json.dumps(row) + "\n")
PY
}
write_raw() { printf '%s\n' "$2" >> "$1"; }

run() {  # run <transcript> [args...] -> OUT, RC
    OUT="$("$TOOL" "$@" 2>&1)"
    RC=$?
    return 0
}

echo "qa-throwaways.test.sh"
echo

# ---------------------------------------------------------------------------
# A. A run that wrote nothing is 0 and exits 0
# ---------------------------------------------------------------------------
T="$SANDBOX/clean.jsonl"
: > "$T"
write_row "$T" Bash '{"command":"ls -la /tmp"}'
write_row "$T" Read '{"file_path":"/repo/docs/plan.md"}'
write_row "$T" Write '{"file_path":"/repo/docs/audit.md","content":"# audit"}'
run "$T"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "0 helper scripts"; then
    ok "A1 a walk that wrote no helper is 0, exit 0"
else
    bad "A1 a walk that wrote no helper is 0, exit 0" "rc=$RC out=$OUT"
fi
# The Markdown audit is not a helper — the rule that keeps every walk from
# reporting at least one throwaway for writing its own report.
if printf '%s' "$OUT" | grep -q "audit.md"; then
    bad "A2 a Markdown document is never counted" "$OUT"
else
    ok "A2 a Markdown document is never counted"
fi

# ---------------------------------------------------------------------------
# B. Write of a script counts; exit 1 with the path named
# ---------------------------------------------------------------------------
T="$SANDBOX/writes.jsonl"
: > "$T"
write_row "$T" Write '{"file_path":"/scratch/ray9/measure.py","content":"x"}'
write_row "$T" Write '{"file_path":"/scratch/ray9/guest.sh","content":"x"}'
write_row "$T" Write '{"file_path":"/scratch/ray9/poke.applescript","content":"x"}'
run "$T"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "3 helper scripts" \
   && printf '%s' "$OUT" | grep -q "measure.py" \
   && printf '%s' "$OUT" | grep -q "poke.applescript"; then
    ok "B1 three Writes are three scripts, exit 1, each named"
else
    bad "B1 three Writes are three scripts, exit 1, each named" "rc=$RC out=$OUT"
fi

# ---------------------------------------------------------------------------
# C. THE HEREDOC ARM. A Write-only counter saw 14 of vm8's 23 write events.
# ---------------------------------------------------------------------------
T="$SANDBOX/heredoc.jsonl"
: > "$T"
write_row "$T" Bash '{"command":"cat > /scratch/ray9/click.sh <<'\''EOF'\''\nosascript -e 1\nEOF\nchmod +x /scratch/ray9/click.sh"}'
write_row "$T" Bash '{"command":"tee /scratch/ray9/wins.py <<PY\nprint(1)\nPY"}'
run "$T"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "2 helper scripts" \
   && printf '%s' "$OUT" | grep -q "click.sh" && printf '%s' "$OUT" | grep -q "wins.py"; then
    ok "C1 a heredoc redirect and a tee are authorship"
else
    bad "C1 a heredoc redirect and a tee are authorship" "rc=$RC out=$OUT"
fi

# An inline heredoc writes NO file. This is the false positive that would make
# the number meaningless: every agent runs `python3 - <<PY` dozens of times.
T="$SANDBOX/inline.jsonl"
: > "$T"
write_row "$T" Bash '{"command":"python3 - <<'\''PY'\''\nprint(2)\nPY"}'
write_row "$T" Bash '{"command":"cat /repo/scripts/build.sh | head -20"}'
write_row "$T" Bash '{"command":"diff a.py b.py > /tmp/out.txt"}'
run "$T"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "0 helper scripts"; then
    ok "C2 an inline heredoc, a read and a redirect to a non-script are not authorship"
else
    bad "C2 an inline heredoc, a read and a redirect to a non-script are not authorship" "rc=$RC out=$OUT"
fi

# ---------------------------------------------------------------------------
# D. A COPY IS NOT A SECOND SCRIPT. vm8 base64'd measure.py and analyze.py into
#    the guest; counting the transfers reports 19 helpers where there are 17.
# ---------------------------------------------------------------------------
T="$SANDBOX/copies.jsonl"
: > "$T"
write_row "$T" Write '{"file_path":"/scratch/ray9/measure.py","content":"x"}'
write_row "$T" Bash '{"command":"B64=$(base64 -i measure.py | tr -d '\''\\n'\'') && ./guest.sh \"echo $B64 | base64 -d > /Users/admin/measure.py; chmod +x /Users/admin/measure.py\""}' # machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
run "$T"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "1 helper script " \
   && printf '%s' "$OUT" | grep -q "copied to /Users/admin/measure.py" && printf '%s' "$OUT" | grep -q "1 later copy"; then # machine-path-exempt: isolated guest or negative fixture input; no storage default
    ok "D1 a transfer into the guest is one script and a visible copy"
else
    bad "D1 a transfer into the guest is one script and a visible copy" "rc=$RC out=$OUT"
fi

# Rewriting the same helper is one script, and the run count is shown, because
# "written 3x" is the signal that the walk was iterating on its own tool.
T="$SANDBOX/rewrite.jsonl"
: > "$T"
write_row "$T" Write '{"file_path":"/scratch/ray9/send.sh","content":"v1"}'
write_row "$T" Write '{"file_path":"/scratch/ray9/send.sh","content":"v2"}'
write_row "$T" Write '{"file_path":"/scratch/ray9/send.sh","content":"v3"}'
run "$T"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "1 helper script " \
   && printf '%s' "$OUT" | grep -q "written 3x"; then
    ok "D2 three rewrites of one helper are one script, shown as 3x"
else
    bad "D2 three rewrites of one helper are one script, shown as 3x" "rc=$RC out=$OUT"
fi

# ---------------------------------------------------------------------------
# E. ADDING A TOOL TO THE COMMITTED TOOLKIT IS THE WANTED BEHAVIOR.
#    A counter without this arm reports the one walk that did the right thing
#    as an offender, which is how a measurement teaches the wrong lesson.
# ---------------------------------------------------------------------------
T="$SANDBOX/toolkit.jsonl"
: > "$T"
write_row "$T" Write '{"file_path":"/repo/richos/app/scripts/qa/settle.py","content":"x"}'
write_row "$T" Write '{"file_path":"/scratch/ray9/poke.sh","content":"x"}'
run "$T"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "1 helper script " \
   && printf '%s' "$OUT" | grep -q "1 addition to the committed toolkit" \
   && printf '%s' "$OUT" | grep -q "settle.py"; then
    ok "E1 a committed toolkit addition is reported, not counted"
else
    bad "E1 a committed toolkit addition is reported, not counted" "rc=$RC out=$OUT"
fi

# Only the toolkit addition, and the run is clean: exit 0.
T="$SANDBOX/toolkit-only.jsonl"
: > "$T"
write_row "$T" Write '{"file_path":"/repo/richos/app/scripts/qa/settle.py","content":"x"}'
run "$T"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "0 helper scripts"; then
    ok "E2 a walk whose only script went into the toolkit is clean, exit 0"
else
    bad "E2 a walk whose only script went into the toolkit is clean, exit 0" "rc=$RC out=$OUT"
fi

# ---------------------------------------------------------------------------
# F. IT REFUSES RATHER THAN REPORTING A ZERO IT DID NOT MEASURE.
#    The toolkit's own rule: never print a number it did not measure.
# ---------------------------------------------------------------------------
run "$SANDBOX/no-such-file.jsonl"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "no transcript at"; then
    ok "F1 a missing transcript is exit 2 with a sentence, never 0"
else
    bad "F1 a missing transcript is exit 2 with a sentence, never 0" "rc=$RC out=$OUT"
fi

T="$SANDBOX/not-a-transcript.txt"
printf 'this is a log file\nnot JSON at all\n' > "$T"
run "$T"
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q "not a transcript"; then
    ok "F2 a file with no JSON in it is exit 2, not a clean walk"
else
    bad "F2 a file with no JSON in it is exit 2, not a clean walk" "rc=$RC out=$OUT"
fi

run "$SANDBOX"
if [ "$RC" -eq 2 ]; then
    ok "F3 a directory is exit 2"
else
    bad "F3 a directory is exit 2" "rc=$RC out=$OUT"
fi

run
if [ "$RC" -eq 2 ]; then
    ok "F4 no argument is usage, exit 2"
else
    bad "F4 no argument is usage, exit 2" "rc=$RC out=$OUT"
fi

# A truncated row (the harness's last line during a live run) is skipped, and
# the rows around it are still counted — a killed walk still gets a number.
T="$SANDBOX/truncated.jsonl"
: > "$T"
write_row "$T" Write '{"file_path":"/scratch/ray9/a.sh","content":"x"}'
write_raw "$T" '{"type":"assistant","message":{"role":"assist'
run "$T"
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q "1 helper script "; then
    ok "F5 a truncated last row is skipped, the rest still counted"
else
    bad "F5 a truncated last row is skipped, the rest still counted" "rc=$RC out=$OUT"
fi

# ---------------------------------------------------------------------------
# G. --json, for the lander and anything that has to compare two runs
# ---------------------------------------------------------------------------
T="$SANDBOX/json.jsonl"
: > "$T"
write_row "$T" Write '{"file_path":"/scratch/ray9/one.sh","content":"x"}'
write_row "$T" Bash '{"command":"cat > /scratch/ray9/two.py <<EOF\nx\nEOF"}'
OUT="$("$TOOL" "$T" --json 2>/dev/null)"; RC=$?
COUNT="$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["count"])' 2>/dev/null)"
EVENTS="$(printf '%s' "$OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin)["write_events"])' 2>/dev/null)"
if [ "$RC" -eq 1 ] && [ "$COUNT" = "2" ] && [ "$EVENTS" = "2" ]; then
    ok "G1 --json is valid JSON carrying count and write_events, exit unchanged"
else
    bad "G1 --json is valid JSON carrying count and write_events, exit unchanged" "rc=$RC count=$COUNT out=$OUT"
fi

# --help answers without needing a transcript, and exits 0.
OUT="$("$TOOL" --help 2>&1)"; RC=$?
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q "WHAT COUNTS"; then
    ok "G2 --help states the counting rules, exit 0"
else
    bad "G2 --help states the counting rules, exit 0" "rc=$RC"
fi

echo
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
