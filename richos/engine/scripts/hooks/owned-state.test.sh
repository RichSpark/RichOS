#!/usr/bin/env bash
#
# owned-state.test.sh — the standing-ownership layer: the declaration parser,
#                       the verdict, the report and the gate.
#
# THE CASES THIS SUITE EXISTS FOR ARE 4f AND 4i, and they must be read with 4a.
#
#   4a  proves the gate BITES: an unrelated dispatch is refused while a system
#       the orchestrator owns is standing with nothing done about it.
#   4f  proves the gate LETS GO: once anything at all has been done this
#       session, every later dispatch is silent. That is the case that keeps
#       this guard alive. This project has killed three guards (g11/g12/g13) in
#       a single day by making them broad enough that waiving became the daily
#       habit, and a gate demanding a disposition for each of six systems would
#       have been the fourth. If 4f ever goes red the guard has become a wall.
#   4i  proves the ESCAPE HATCH IS NOT A HOLE. The first version of this hook
#       had one: an ack reading `owned-state-ack: alpha - ...` contains the
#       word `alpha`, matched alpha's own `match:` pattern, and was recorded as
#       a dispatch ADDRESSING the very system it asked to defer — so a bare,
#       malformed ack sailed through as work. Found by running the thing.
#
# AND SECTION 7, added 2026-09-25, is the case this gate failed in production.
# A keyword in a prompt counted as "addressing" a system, the escalation
# backlog reached 143 (the oldest 19 days, 13 for the CEO), and the gate
# cleared itself session after session. 4h now asserts a keyword is REFUSED;
# section 7 runs the SHIPPED escalations row against the SHIPPED list output
# and proves the three things that do count (a real drop, nothing past the
# tolerated age, a named triage) and the ones that do not. Every one of them
# is proven load-bearing by owned-state.mutation.sh.
#
# Exit codes: 0 all cases pass; 1 a case failed.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
HOOK="$SCRIPT_DIR/guard-owned-state.sh"
LIB="$ENGINE_ROOT/scripts/lib/owned-systems.py"
REPORT="$ENGINE_ROOT/scripts/owned-state.sh"

# The governed repository is DECLARED, never inherited from the launching
# session: run from a session seated elsewhere, every case below would pass by
# standing down.
unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0
ok()  { PASS=$((PASS + 1)); printf '  PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf '  FAIL  %s%s\n' "$1" "${2:+ — $2}"; }

SB="$(cd "$(mktemp -d -t owned-state.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SB"' EXIT

ENTITY="$SB/entity"
mkdir -p "$ENTITY"

write_config() {   # <extra lines...>
    {
        printf 'PROTECTED_PATHS=""\n'
        printf 'OWNED_SYSTEMS_DECLARATION="owned-systems.declaration"\n'
        while [ $# -gt 0 ]; do printf '%s\n' "$1"; shift; done
    } >"$ENTITY/orchestration.config"
}

# The fixture inventory. `alpha` is always unhealthy, `beta` always healthy.
# Both are `bash -c` rows on purpose: a `-c` command has no program on disk,
# and the first version of the runner reported every such row as "the declared
# check is not on disk: 'echo" — an authoritative-looking verdict about the
# wrong object, which is the defect class this whole engine keeps finding in
# itself. Case 1c pins it.
write_declaration() {
    cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: alpha
title: A system that is always unhealthy
check: bash -c 'echo "ALPHA IS RED: 3 things wrong"; echo noise; exit 1'
evidence: (ALPHA)
match: (alpha|widget)
timeout: 10
why: the always-red fixture row

id: beta
title: A system that is always healthy
check: bash -c 'exit 0'
match: (beta|sprocket)
timeout: 10
why: the always-green fixture row
DECL
}

write_config
write_declaration

reset_state() { rm -rf "$ENTITY/.claude/state"; }

payload() {   # <session> <prompt>
    SESSION="$1" PROMPT="$2" python3 -c '
import json, os
print(json.dumps({"tool_name": "Agent", "session_id": os.environ["SESSION"],
                  "tool_input": {"subagent_type": "mark", "name": "mark-sonnet-t1",
                                 "prompt": os.environ["PROMPT"]}}))
'
}

RC=0
OUT=""
# THE PAYLOAD IS AN ARGUMENT, NOT A PIPE, and that is not a style choice. The
# first version of this suite piped the payload in — `payload s1 "..." |
# run_hook` — which runs run_hook in a SUBSHELL, so every OUT and RC it set was
# discarded and all fourteen gate cases were asserting against a stale verdict
# from an earlier section. They all reported the same rc=0. A suite that cannot
# see its own subject is the "green over nothing" failure this engine keeps
# finding; it is worth a comment so nobody reintroduces the pipe.
run_hook() {   # <payload json>
    OUT="$(printf '%s' "$1" | RICHOS_ENTITY_ROOT="$ENTITY" bash "$HOOK" 2>&1)"; RC=$?
    return 0
}

report_json() {
    RICHOS_ENTITY_ROOT="$ENTITY" python3 "$LIB" report --entity "$ENTITY" \
        --engine "$ENGINE_ROOT" --json --refresh 2>&1
}

echo "=== 1. the declaration parser refuses what it cannot run ==="

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: alpha
check: bash -c 'exit 0'

id: alpha
check: bash -c 'exit 0'
DECL
reset_state
OUT="$(report_json)"; RC=$?
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q 'same id twice'; then
    ok "1a  a duplicate id is refused, because an id is what an ack names"
else
    bad "1a  a duplicate id was accepted" "rc=$RC"
fi

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: alpha
title: no check at all
DECL
reset_state
OUT="$(report_json)"; RC=$?
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q 'no .id:. or no .check:'; then
    ok "1b  a row with no check is refused, never silently dropped"
else
    bad "1b  a row with no check did not refuse" "rc=$RC"
fi

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: alpha
check: bash -c 'echo "3 OUTSTANDING"; exit 1'
measure: [0-9]+ OUTSTANDING
DECL
reset_state
OUT="$(report_json)"; RC=$?
if [ "$RC" -eq 2 ] && printf '%s' "$OUT" | grep -q 'no capture group'; then
    ok "1d  a measure with no capture group is refused: a measure that can never read a number is a wall"
else
    bad "1d  a measure that cannot read a number was accepted" "rc=$RC"
fi

write_declaration
reset_state
OUT="$(report_json)"; RC=$?
if printf '%s' "$OUT" | grep -q 'not on disk'; then
    bad "1c  a bash -c row was reported as a missing program"
else
    ok "1c  a 'bash -c' row is RUN, not misreported as absent from disk"
fi

echo ""
echo "=== 2. the verdict ==="

if printf '%s' "$OUT" | python3 -c '
import json, sys
d = json.load(sys.stdin)
s = {x["id"]: x for x in d["systems"]}
assert s["alpha"]["status"] == "UNHEALTHY", s["alpha"]
assert s["beta"]["status"] == "HEALTHY", s["beta"]
' 2>/dev/null; then
    ok "2a  a failing check is UNHEALTHY and a passing one is HEALTHY"
else
    bad "2a  the two fixture rows did not produce the two verdicts"
fi

if printf '%s' "$OUT" | python3 -c '
import json, sys
d = json.load(sys.stdin)
ev = [x for x in d["systems"] if x["id"] == "alpha"][0]["evidence"]
assert any("ALPHA IS RED" in e for e in ev), ev
assert not any("noise" in e for e in ev), ev
' 2>/dev/null; then
    ok "2b  the evidence quoted is the line the row's own pattern selected"
else
    bad "2b  the evidence was not selected by the declared pattern"
fi

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: gamma
title: a check that does not exist
check: bash /nowhere/at/all/missing-check.sh
match: (gamma)
timeout: 10
why: the absent-check fixture
DECL
reset_state
OUT="$(report_json)"; RC=$?
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q '"status": "UNKNOWN"' \
   && printf '%s' "$OUT" | grep -q 'an absent check is not a passing check'; then
    ok "2c  a check that is not on disk is UNKNOWN and stands — never HEALTHY"
else
    bad "2c  an absent check did not produce a standing UNKNOWN" "rc=$RC"
fi

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: delta
title: a check whose 2 means it could not decide
check: bash -c 'echo "could not reach the API"; exit 2'
unknown-exit: 2
match: (delta)
timeout: 10
why: the declared-unknown fixture

id: epsilon
title: a check whose 2 is a real finding
check: bash -c 'echo "two things are wrong"; exit 2'
match: (epsilon)
timeout: 10
why: the same code, the opposite meaning
DECL
reset_state
OUT="$(report_json)"
if printf '%s' "$OUT" | python3 -c '
import json, sys
s = {x["id"]: x for x in json.load(sys.stdin)["systems"]}
assert s["delta"]["status"] == "UNKNOWN", s["delta"]
assert s["epsilon"]["status"] == "UNHEALTHY", s["epsilon"]
' 2>/dev/null; then
    ok "2d  unknown-exit is per row: the same exit 2 means UNKNOWN in one row and a finding in another"
else
    bad "2d  unknown-exit was applied globally or not at all"
fi

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: zeta
title: a system this repository does not have
jurisdiction: test -f /nowhere/at/all/marker
check: bash -c 'exit 1'
match: (zeta)
timeout: 10
why: the out-of-jurisdiction fixture
DECL
reset_state
OUT="$(report_json)"; RC=$?
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q 'OUT-OF-JURISDICTION'; then
    ok "2e  a row whose jurisdiction does not hold is n/a here and does not stand"
else
    bad "2e  an out-of-jurisdiction row was judged anyway" "rc=$RC"
fi

echo ""
echo "=== 3. what gets demanded, and why it cannot be rotated away ==="

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: red
title: a genuine red
check: bash -c 'echo "the real defect"; exit 1'
match: (redthing)
timeout: 10
why: fixture

id: absent
title: a missing instrument
check: bash /nowhere/at/all/missing-check.sh
match: (absentthing)
timeout: 10
why: fixture
DECL
reset_state
DEMAND="$(RICHOS_ENTITY_ROOT="$ENTITY" python3 "$LIB" demanded --entity "$ENTITY" \
          --engine "$ENGINE_ROOT" --session sX --refresh 2>&1)"
if printf '%s' "$DEMAND" | grep -q '"demanded": "red"'; then
    ok "3a  a real UNHEALTHY outranks an UNKNOWN when only one can be demanded"
else
    bad "3a  the missing instrument was demanded ahead of the real defect"
fi

# Age decides within a rank. Backdate `absent` to a week ago and make BOTH
# unhealthy: the older one must be demanded even though it sorts second by id.
cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: red
title: a recent red
check: bash -c 'exit 1'
match: (redthing)
timeout: 10
why: fixture

id: absent
title: an old red
check: bash -c 'exit 1'
match: (absentthing)
timeout: 10
why: fixture
DECL
reset_state
RICHOS_ENTITY_ROOT="$ENTITY" python3 "$LIB" demanded --entity "$ENTITY" \
    --engine "$ENGINE_ROOT" --session sX --refresh >/dev/null 2>&1
python3 - "$ENTITY" <<'AGE'
import json, os, sys, time
p = os.path.join(sys.argv[1], ".claude", "state", "owned-state-seen.json")
seen = json.load(open(p))
seen["absent"] = time.time() - 7 * 86400
json.dump(seen, open(p, "w"))
AGE
DEMAND="$(RICHOS_ENTITY_ROOT="$ENTITY" python3 "$LIB" demanded --entity "$ENTITY" \
          --engine "$ENGINE_ROOT" --session sY --refresh 2>&1)"
if printf '%s' "$DEMAND" | grep -q '"demanded": "absent"' \
   && printf '%s' "$DEMAND" | grep -q '7.0 days ago'; then
    ok "3b  the OLDEST standing system is the one demanded, with its age printed"
else
    bad "3b  age did not decide which system was demanded"
fi

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: red
title: now healthy
check: bash -c 'exit 0'
match: (redthing)
timeout: 10
why: fixture
DECL
RICHOS_ENTITY_ROOT="$ENTITY" python3 "$LIB" report --entity "$ENTITY" \
    --engine "$ENGINE_ROOT" --json --refresh >/dev/null 2>&1
if python3 - "$ENTITY" <<'CLEARED'
import json, os, sys
p = os.path.join(sys.argv[1], ".claude", "state", "owned-state-seen.json")
seen = json.load(open(p))
sys.exit(0 if "red" not in seen else 1)
CLEARED
then
    ok "3c  a system observed healthy loses its first-observed stamp, so the clock restarts honestly"
else
    bad "3c  a healed system kept its age"
fi

echo ""
echo "=== 4. the gate ==="

write_declaration
reset_state

# Orchestration reliability R3 (2026-10-24): "An unrelated ... old health incident remains
# owned in its current queue and existing watcher, but does not veto a repair, inspection,
# ordinary answer or independent dispatch ... Do not require a waiver sentence or repeated
# acknowledgement." The standing system is told to the LEAD once per session and system.
told() { # stdout is one PreToolUse additionalContext naming the system and its evidence
    printf '%s' "$OUT" | python3 -c 'import json,sys
d = json.loads(sys.stdin.readline() or "{}")
h = d.get("hookSpecificOutput") or {}
c = h.get("additionalContext", "")
sys.exit(0 if h.get("hookEventName") == "PreToolUse" and "OWNED SYSTEM STANDING" in c
         and "alpha" in c and "ALPHA IS RED: 3 things wrong" in c and "systemMessage" not in d else 1)'
}

run_hook "$(payload s1 "Please refactor the login screen.")"
if [ "$RC" -eq 0 ] && told; then
    ok "4a  an unrelated dispatch goes AHEAD, and the lead is told the system and its evidence verbatim"
else
    bad "4a  an unrelated dispatch was refused or not told" "rc=$RC $OUT"
fi

run_hook "$(payload s1 "Something else entirely, unrelated to anything.")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "4b  told ONCE per session and system: the next dispatch is silent"
else
    bad "4b  the same system was told again in the same session" "rc=$RC $OUT"
fi

run_hook "$(payload s2 "Something else entirely, unrelated to anything.")"
if [ "$RC" -eq 0 ] && told; then
    ok "4c  a new session is told again: it did not hear it"
else
    bad "4c  a new session was not told" "rc=$RC $OUT"
fi

run_hook "$(payload s3 "$(printf 'Refactor.\nowned-state-ack: alpha — the founder is on a live demo right now\n')")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ] \
   && grep -q 'how=ack' "$ENTITY/.claude/state/owned-state-acks.log" 2>/dev/null \
   && grep -q 'live demo' "$ENTITY/.claude/state/owned-state-acks.log" 2>/dev/null; then
    ok "4d  an ack is still recorded with its reason when one is given, though nothing requires it"
else
    bad "4d  a well-formed ack was not recorded" "rc=$RC"
fi

run_hook "$(payload s4 "$(printf 'Refactor.\nowned-state-ack: alpha\n')")"
if [ "$RC" -eq 0 ] && told; then
    ok "4e  a bare ack line is not needed and silences nothing: the dispatch goes ahead and is told"
else
    bad "4e  a bare ack line changed the outcome" "rc=$RC $OUT"
fi

run_hook "$(payload s6 "Fix the alpha widget pipeline end to end.")"
if [ "$RC" -eq 0 ] && told \
   && ! grep -q 'session=s6' "$ENTITY/.claude/state/owned-state-dispositions.log" 2>/dev/null; then
    ok "4h  a prompt that merely MENTIONS a standing system goes ahead and is told; nothing is recorded as handled"
else
    bad "4h  a keyword in the prompt was recorded as handling the system" "rc=$RC"
fi

run_hook '{"tool_name":"Bash","session_id":"s5","tool_input":{"command":"ls"}}'
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "4j  a payload for another tool is silent — this gate governs dispatches only"
else
    bad "4j  the gate fired on a non-Agent call" "rc=$RC"
fi

run_hook 'this is not json at all'
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -qi 'UNGATED'; then
    ok "4k  an unparseable payload FAILS OPEN and says so — the ack line cannot be read from it either"
else
    bad "4k  an unparseable payload was silently allowed or wrongly refused" "rc=$RC"
fi

write_config 'OWNED_STATE_GATE="0"'
run_hook "$(payload s6 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "4l  the declared off switch stands the gate down silently"
else
    bad "4l  OWNED_STATE_GATE=0 did not stand the gate down" "rc=$RC"
fi
write_config

write_config 'OWNED_SYSTEMS_DECLARATION="no-such-declaration-file"'
run_hook "$(payload s7 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -qi 'UNGATED'; then
    ok "4m  a declared-but-unreadable inventory fails open LOUDLY — never the same as a clean run"
else
    bad "4m  a missing declaration was silent, which is a defense reporting 'on' while protecting nothing" "rc=$RC"
fi
write_config

cat >"$ENTITY/owned-systems.declaration" <<'DECL'
id: beta
title: A system that is always healthy
check: bash -c 'exit 0'
match: (beta)
timeout: 10
why: the always-green fixture row
DECL
reset_state
run_hook "$(payload s8 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "4n  with every system healthy the gate is completely silent"
else
    bad "4n  the gate fired with nothing standing" "rc=$RC"
fi

echo ""
echo "=== 5. the positive probe ==="

write_declaration
reset_state
OUT="$(RICHOS_ENTITY_ROOT="$ENTITY" bash "$REPORT" "$ENTITY" --refresh 2>&1)"; RC=$?
if [ "$RC" -eq 1 ] \
   && printf '%s' "$OUT" | grep -q 'UNHEALTHY. alpha' \
   && printf '%s' "$OUT" | grep -q 'ok.*beta' \
   && printf '%s' "$OUT" | grep -q 'via  :'; then
    ok "5a  the report prints EVERY system including the healthy ones, and which command answered"
else
    bad "5a  the report hid the healthy rows or the deciding command" "rc=$RC"
fi

OUT="$(RICHOS_ENTITY_ROOT="$ENTITY" bash "$REPORT" "$ENTITY" --line 2>&1)"; RC=$?
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q 'OWNED SYSTEMS: 1 standing'; then
    ok "5b  the one-line form names the count and the systems"
else
    bad "5b  the one-line form did not report the standing system" "rc=$RC"
fi

echo ""
echo "=== 6. the registered-executables check, two-sided ==="

# THE NEGATIVE CONTROL COMES FIRST, deliberately. A check whose only evidence
# is "it says everything is fine on a machine where everything is fine" is the
# scanner-over-an-empty-corpus this engine keeps catching in itself. The defect
# it exists for is a REAL one from 2026-09-10 — three scripts committed 644 —
# so the first thing proven is that it sees exactly that.
XCHECK="$ENGINE_ROOT/scripts/owned-state-checks/registered-executables.py"
XB="$SB/xsandbox"
mkdir -p "$XB/engine/hooks" "$XB/engine/scripts/hooks" "$XB/entity/.claude" "$XB/agents"

printf '#!/usr/bin/env bash\nexit 0\n' >"$XB/engine/scripts/hooks/runnable.sh"
chmod +x "$XB/engine/scripts/hooks/runnable.sh"
# no-bit.sh is registered as a DIRECT command (no interpreter before it): only a
# program run directly needs the executable bit. Since P5-09 (498beb576) a script
# run as `bash <path>` needs only to be readable, so the 2026-09-10 defect is
# reproduced with the direct form; registered-executables.test.sh E1 covers the
# interpreter form.
printf '#!/usr/bin/env bash\nexit 0\n' >"$XB/engine/scripts/hooks/no-bit.sh"
chmod 644 "$XB/engine/scripts/hooks/no-bit.sh"

cat >"$XB/engine/hooks/hooks.json" <<'HJ'
{"hooks": {"PreToolUse": [{"matcher": "Agent", "hooks": [
  {"type": "command", "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/hooks/runnable.sh"},
  {"type": "command", "command": "${CLAUDE_PLUGIN_ROOT}/scripts/hooks/no-bit.sh"}
]}]}}
HJ

RC=0
OUT="$(python3 "$XCHECK" --entity "$XB/entity" --engine "$XB/engine" --launch-agents "$XB/agents" 2>&1)" || RC=$?
if [ "$RC" -eq 1 ] \
   && printf '%s' "$OUT" | grep -q 'NOT RUNNABLE.*no-bit.sh' \
   && ! printf '%s' "$OUT" | grep -q 'runnable.sh —'; then
    ok "6a  NEGATIVE CONTROL: a wired hook with no executable bit is NAMED, and its runnable sibling is not"
else
    bad "6a  the 644 defect of 2026-09-10 would not have been caught" "rc=$RC"
fi

chmod +x "$XB/engine/scripts/hooks/no-bit.sh"
RC=0
OUT="$(python3 "$XCHECK" --entity "$XB/entity" --engine "$XB/engine" --launch-agents "$XB/agents" 2>&1)" || RC=$?
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q 'exist and are executable'; then
    ok "6b  and it goes green the moment the bit is set — the check tracks the fact, not the file list"
else
    bad "6b  a fully executable surface did not pass" "rc=$RC"
fi

rm -f "$XB/engine/scripts/hooks/no-bit.sh"
RC=0
OUT="$(python3 "$XCHECK" --entity "$XB/entity" --engine "$XB/engine" --launch-agents "$XB/agents" 2>&1)" || RC=$?
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q 'MISSING.*no-bit.sh'; then
    ok "6c  a wired hook that is not on disk at all is a different sentence from one that is not runnable"
else
    bad "6c  a missing wired hook was not distinguished" "rc=$RC"
fi

RC=0
OUT="$(python3 "$XCHECK" --entity "$SB/nowhere" --engine "$SB/nowhere" --launch-agents "$XB/agents" 2>&1)" || RC=$?
if [ "$RC" -eq 2 ]; then
    ok "6d  no readable registration surface is UNKNOWN, never a clean bill of health"
else
    bad "6d  an unreadable surface reported a verdict" "rc=$RC"
fi

echo ""
echo "=== 7. escalations: addressed means the backlog got SMALLER, never a keyword ==="

# THE SHIPPED ROW AGAINST THE SHIPPED LIST OUTPUT. The row is copied out of the
# engine's own declaration rather than restated here, so its `measure:`,
# `backlog:`, `since:` and `triage:` patterns are tested against the real text
# of `escalate.sh list`. A fixture row would test a pattern nobody ships, and a
# rewording of the list header would then silently stop the gate measuring
# anything while this suite stayed green.
#
# The incident these cases rebuild (2026-09-25): 143 escalations outstanding,
# the oldest 19 days, 13 for the CEO, and the gate cleared for session after
# session because a prompt contained a keyword.
ESC_PY="$ENGINE_ROOT/scripts/lib/escalations.py"
ESC_LEDGER="$SB/esc/escalations.jsonl"
mkdir -p "$SB/esc"
export RICHOS_ESCALATION_LEDGER="$ESC_LEDGER"

write_config
if ! python3 - "$ENGINE_ROOT/owned-systems.declaration" "$ENTITY/owned-systems.declaration" <<'ROW'
import sys
src, dst = sys.argv[1], sys.argv[2]
rec, recs = [], []
for line in open(src, encoding="utf-8"):
    if line.startswith("#"):
        continue
    if not line.strip():
        if rec:
            recs.append(rec)
        rec = []
        continue
    rec.append(line)
if rec:
    recs.append(rec)
row = [r for r in recs if r and r[0].strip() == "id: escalations"]
if len(row) != 1:
    sys.exit(1)
open(dst, "w", encoding="utf-8").write("".join(row[0]))
ROW
then
    bad "7-  the shipped declaration has no single 'escalations' row to test"
fi

esc_reset() { reset_state; : >"$ESC_LEDGER"; }

esc_add() {   # <id> <for> <hours-ago>
    python3 - "$ESC_LEDGER" "$1" "$2" "$3" <<'ADD'
import json, sys
from datetime import datetime, timedelta, timezone
path, rid, audience, hours = sys.argv[1], sys.argv[2], sys.argv[3], float(sys.argv[4])
raised = (datetime.now(timezone.utc) - timedelta(hours=hours)).replace(microsecond=0)
row = {"event": "Escalation", "id": rid,
       "raised": raised.isoformat().replace("+00:00", "Z"),
       "teammate": "fixture-" + rid, "state": "proceeding", "for": audience,
       "title": "fixture escalation " + rid,
       "question": "a fixture question long enough to be a question"}
with open(path, "a", encoding="utf-8") as fh:
    fh.write(json.dumps(row) + "\n")
ADD
}

esc_ack() {   # <id>
    python3 "$ESC_PY" ack --id "$1" \
        --disposition "handled by the owned-state suite; this is a fixture row" >/dev/null 2>&1
}

disposed() {   # <session> <how>  — exit 0 if the ledger holds that disposition
    grep -q "session=$1"$'\t'".*how=$2" "$ENTITY/.claude/state/owned-state-dispositions.log" 2>/dev/null
}

# Three escalations, raised three days ago, one of them for the CEO.
esc_reset
esc_add esc-fx-a ceo 72
esc_add esc-fx-b lead 72
esc_add esc-fx-c lead 72

# --- 7a. THE KEYWORD DISPATCH ----------------------------------------------
# Every keyword the old `match:` pattern held, and the 2026-09-18 shape: a
# dispatch that talks about escalations without doing anything to the backlog.
toldsys() { printf '%s' "$OUT" | grep -q 'OWNED SYSTEM STANDING'; }
run_hook "$(payload e1 "Please look at the escalation protocol. Run escalate.sh list, and use --disposition on the esc-2026 items when you get to them.")"
if [ "$RC" -eq 0 ] && toldsys \
   && printf '%s' "$OUT" | grep -q 'this dispatch goes ahead): escalations' \
   && printf '%s' "$OUT" | grep -q 'oldest item raised 3.0 days ago' \
   && ! disposed e1 '[a-z]*'; then
    ok "7a  a KEYWORD-ONLY dispatch about escalations goes ahead, told dated by its oldest item; nothing is recorded as handled"
else
    bad "7a  a keyword-only dispatch satisfied the gate" "rc=$RC"
fi

# --- 7b. TOLD ONCE; THE KEYWORD DISPATCH STILL HANDLED NOTHING -------------
run_hook "$(payload e1 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ] && ! disposed e1 '[a-z]*'; then
    ok "7b  the next dispatch in that session is silent (told once), and the keyword dispatch is still not recorded as handling it"
else
    bad "7b  the keyword dispatch cleared the session, or the age is not the item's own" "rc=$RC"
fi

# --- 7c. A REAL DROP IN THE COUNT IS ALLOWED -------------------------------
esc_ack esc-fx-a
run_hook "$(payload e1 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ] && disposed e1 progress \
   && grep -q 'fell from 3 to 2' "$ENTITY/.claude/state/owned-state-dispositions.log"; then
    ok "7c  once the outstanding count FALLS this session, the next dispatch is allowed and recorded as progress"
else
    bad "7c  a real drop in the count did not clear the session" "rc=$RC"
fi

# --- 7d. THE NEXT SESSION IS ASKED AGAIN -----------------------------------
run_hook "$(payload e2 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && toldsys && printf '%s' "$OUT" | grep -q 'size 2; 2 past the tolerated age'; then
    ok "7d  a new session is asked again while old items remain, with the size printed"
else
    bad "7d  a drop in one session carried over to the next" "rc=$RC"
fi

# --- 7e. NOTHING PAST THE TOLERATED AGE: allowed, even at the same count ---
# Isolated from the count on purpose: two fresh escalations arrive and the two
# old ones are acknowledged, so the count is exactly the baseline e2 was
# refused at. Only the backlog condition can clear this.
esc_add esc-fx-d lead 1
esc_add esc-fx-e ceo 1
esc_ack esc-fx-b
esc_ack esc-fx-c
run_hook "$(payload e2 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ] && disposed e2 progress \
   && grep -q 'nothing is past its tolerated age' "$ENTITY/.claude/state/owned-state-dispositions.log"; then
    ok "7e  once every escalation older than 24h is acknowledged, dispatch is allowed at an unchanged count"
else
    bad "7e  the backlog-age condition did not clear the session" "rc=$RC"
fi
run_hook "$(payload e3 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ]; then
    ok "7i  and a new session is not asked at all: an hour-old escalation is news, not neglect"
else
    bad "7i  escalations younger than the tolerated age were demanded" "rc=$RC"
fi

# --- 7f. A NAMED TRIAGE OF THE LEDGER --------------------------------------
esc_reset
esc_add esc-fx-f ceo 72
run_hook "$(payload e4 "Triage every outstanding row in ~/.claude/state/escalations.jsonl and acknowledge each one with its disposition.")"
if [ "$RC" -eq 0 ] && [ -z "$OUT" ] && disposed e4 triage; then
    ok "7f  a triage dispatch that names the escalation ledger file is allowed and recorded as triage"
else
    bad "7f  a triage naming the ledger was refused or not recorded" "rc=$RC"
fi

# --- 7g. THE ACK LINE: needs its reason, and stops no clock ----------------
run_hook "$(payload e5 "$(printf 'Refactor.\nowned-state-ack: escalations — ok\n')")"
if [ "$RC" -eq 0 ] && toldsys && ! disposed e5 ack; then
    ok "7k  an escalations ack with a token reason records nothing: the lead is told as if no line were there"
else
    bad "7k  a token reason cleared the escalations demand" "rc=$RC"
fi
SEEN_BEFORE="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("escalations",""))' \
               "$ENTITY/.claude/state/owned-state-seen.json" 2>/dev/null)"
run_hook "$(payload e5 "$(printf 'Refactor.\nowned-state-ack: escalations — the founder is on a live demo right now\n')")"
ACK_RC=$RC
SEEN_AFTER="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("escalations",""))' \
              "$ENTITY/.claude/state/owned-state-seen.json" 2>/dev/null)"
run_hook "$(payload e6 "Refactor the login screen.")"
if [ "$ACK_RC" -eq 0 ] && [ -n "$SEEN_BEFORE" ] && [ "$SEEN_BEFORE" = "$SEEN_AFTER" ] \
   && [ "$RC" -eq 0 ] && toldsys && printf '%s' "$OUT" | grep -q 'oldest item raised 3.0 days ago'; then
    ok "7g  a reasoned ack clears ONE session, leaves the age clock untouched, and the next session is refused at the same age"
else
    bad "7g  the ack reset the age or carried past its session" "ack rc=$ACK_RC seen $SEEN_BEFORE -> $SEEN_AFTER, next rc=$RC"
fi

# --- 7h. THE BASELINE IS FRESH, NEVER THE CACHE ----------------------------
# A cached verdict from BEFORE the previous session's triage would hand the next
# session a free "drop" it never made. Session e7 is refused at 5 and caches
# it; two are acknowledged; session e8, inside the cache's lifetime, must take
# its baseline at 3 and stay refused while nothing changes.
esc_reset
for i in 1 2 3 4 5; do esc_add "esc-fx-h$i" lead 72; done
run_hook "$(payload e7 "Refactor the login screen.")"
esc_ack esc-fx-h1
esc_ack esc-fx-h2
run_hook "$(payload e8 "Refactor the login screen.")"
FIRST_RC=$RC; FIRST_TOLD=0; toldsys && FIRST_TOLD=1
run_hook "$(payload e8 "Refactor the login screen.")"
if [ "$FIRST_RC" -eq 0 ] && [ "$FIRST_TOLD" -eq 1 ] && [ "$RC" -eq 0 ] && [ -z "$OUT" ] && ! disposed e8 progress; then
    ok "7h  a new session's baseline is taken FRESH: a drop made before it began is not credited to it"
else
    bad "7h  a stale cached count was used as the baseline and credited a drop nobody made" "rc=$FIRST_RC then $RC"
fi

# --- 7j. THE OLD KEYWORD DISPOSITIONS CLEAR NOTHING ------------------------
# The live ledger holds eleven `how=addressed` rows written by keyword matches.
# They stay, as history, and must not clear a session.
printf 'at=2026-09-18T16:03:08Z\tsession=e9\tsystem=escalations\thow=addressed\tagent=legacy\treason=dispatch matched this system'"'"'s declared match pattern\n' \
    >>"$ENTITY/.claude/state/owned-state-dispositions.log"
run_hook "$(payload e9 "Refactor the login screen.")"
if [ "$RC" -eq 0 ] && toldsys; then
    ok "7j  a legacy keyword 'addressed' row in the ledger clears nothing"
else
    bad "7j  a legacy keyword disposition still clears a session" "rc=$RC"
fi

unset RICHOS_ESCALATION_LEDGER

# --- 8. THE MUTATION HARNESS -----------------------------------------------
# Every case above is evidence of nothing until it has been watched going red
# for the right reason. owned-state.mutation.sh removes one property at a time
# from a throwaway copy of the engine and asserts THIS suite fails at the named
# case. It is run from here so the runner that discovers *.test.sh runs it too.
if [ -x "$SCRIPT_DIR/owned-state.mutation.sh" ] && [ -z "${RICHOS_MUTATION_INNER:-}" ]; then
    echo ""
    echo "=== 8. running the mutation harness: owned-state.mutation.sh ==="
    if "$SCRIPT_DIR/owned-state.mutation.sh"; then
        ok "8a  every mutation was caught at its named case"
    else
        bad "8a  a mutation survived — a property this suite claims to prove is not proven"
    fi
fi

echo ""
printf '  %s passed, %s FAILED\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
