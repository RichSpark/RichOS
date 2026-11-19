#!/usr/bin/env bash
#
# failure-type.test.sh — WHEN HE PUTS "TYPE" AND "FAILURE" TOGETHER, THE
#                        REGISTER IS READ, A TYPE IS NAMED FROM IT BY NUMBER,
#                        AND THE INSTANCE IS COMMITTED, OR THE TURN DOES NOT END.
#
# Two hooks and one analyzer:
#   scripts/hooks/failure-type-lookup.sh       UserPromptSubmit: detect the words
#                                              in HIS message, hand the lead the
#                                              register's live type list and the rule
#   scripts/hooks/guard-failure-type-answer.sh Stop, BLOCKING: refuse the turn end
#                                              until (a) read, (b) named, (c) committed
#   scripts/lib/failure-type.py                both predicates
#
# EVERY CASE HAS ITS OPPOSITE. A case that passes by observing silence is paired
# with one that passes by observing speech on the same path, so a switched-off
# predicate cannot pass this suite.
#
#   FT01..FT05  the trigger predicate: corpus positives, negatives, the window
#               boundary, machine envelopes, and the private corpus when present
#   FT10..FT19  the UserPromptSubmit hook
#   FT30..FT59  the Stop gate
#   FT70..FT71  the two hooks together, in order
#
# THE CASE IDS ARE FOUR CHARACTERS AND NO ID IS A PREFIX OF ANOTHER, because the
# mutation harness greps `FAIL  <id>`.
#
# Usage: scripts/hooks/failure-type.test.sh
# Exit:  0 all cases passed, 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LIB="$ENGINE_ROOT/scripts/lib/failure-type.py"
UPS="$SCRIPT_DIR/failure-type-lookup.sh"
STOP="$SCRIPT_DIR/guard-failure-type-answer.sh"
CORPUS="$SCRIPT_DIR/failure-type.corpus.md"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2"; FAIL=$((FAIL + 1)); return 0; }

command -v python3 >/dev/null 2>&1 || { echo "ERROR: needs python3" >&2; exit 1; }
command -v git     >/dev/null 2>&1 || { echo "ERROR: needs git" >&2; exit 1; }

SANDBOX="$(cd "$(python3 -c 'import tempfile; print(tempfile.mkdtemp(prefix="failure-type."))')" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT
# A test never writes to the operator's registry.
export RICHOS_WORKSPACES_DIR="$SANDBOX/workspaces"

echo "=== failure type: read the register, name the type, commit the record ==="
echo ""

SEAT="$SANDBOX/seat"    # the repository the lead's session is seated in
REC="$SANDBOX/rec"      # the private record that holds the register
REL="docs/verification/all-lifecycle-failure-records.md"
REG="$REC/$REL"

mk_repo() { # <path>
    mkdir -p "$1"
    git -C "$1" init -q -b main
    git -C "$1" config user.email "tester@example.invalid"
    git -C "$1" config user.name  "tester"
    mkdir -p "$SANDBOX/nohooks"
    git -C "$1" config core.hooksPath "$SANDBOX/nohooks"
    printf 'seed\n' > "$1/README.md"
    git -C "$1" add -A >/dev/null 2>&1
    GIT_COMMITTER_DATE="2026-01-01T00:00:00Z" GIT_AUTHOR_DATE="2026-01-01T00:00:00Z" \
        git -C "$1" commit -qm seed >/dev/null 2>&1
}

write_register() { # <next-type-in-box> <next-section-in-box> [extra heading lines...]
    local nt="$1" ns="$2"
    shift 2
    mkdir -p "$(dirname "$REG")"
    {
        printf '# All lifecycle failure records (fixture)\n\n'
        printf '| | |\n|---|---|\n'
        printf '| **Next free type number** | **%s** |\n' "$nt"
        printf '| **Next free section number** | **%s** |\n\n' "$ns"
        printf '## The type catalog\n\n'
        printf '#### Type 1: A result that never arrived is read as a pass\n\n'
        printf '#### Type 2: A number is chosen because it sounds safe\n\n'
        printf '#### Type 3: A lookup that takes seconds is answered from memory instead\n\n'
        printf '## Part 5\n\n'
        printf '### 5.1 Type 3, again — 2026-01-01 — the fixture instance\n\n'
        for extra in "$@"; do printf '%s\n\n' "$extra"; done
    } > "$REG"
}

commit_register() { # <message> [committer-date]
    git -C "$REC" add "$REL" >/dev/null 2>&1
    if [ -n "${2:-}" ]; then
        GIT_COMMITTER_DATE="$2" GIT_AUTHOR_DATE="$2" git -C "$REC" commit -qm "$1" >/dev/null 2>&1
    else
        git -C "$REC" commit -qm "$1" >/dev/null 2>&1
    fi
    git -C "$REC" rev-parse HEAD
}

mk_repo "$SEAT"
mk_repo "$REC"
mkdir -p "$SEAT/.claude/state"
printf 'FAILURE_TYPE_REGISTER="../rec/%s"\n' "$REL" > "$SEAT/orchestration.config"
write_register 4 5.2
commit_register "fixture register" "2026-01-01T00:00:00Z" >/dev/null

HEAD3="A lookup that takes seconds is answered from memory instead"

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------
# ups_run <prompt> [extra-json-object]  -> UOUT, UERR, URC
ups_run() {
    local payload
    payload="$(SEAT="$SEAT" SESSION="${UPS_SESSION:-aaaa0001-0000-4000-8000-000000000000}" \
        PROMPT="$1" EXTRA="${2:-}" python3 -c '
import json, os
p = {"hook_event_name": "UserPromptSubmit", "session_id": os.environ["SESSION"],
     "cwd": os.environ["SEAT"], "prompt": os.environ["PROMPT"],
     "transcript_path": os.path.join(os.environ["SEAT"], "no-transcript.jsonl")}
if os.environ.get("EXTRA"):
    p.update(json.loads(os.environ["EXTRA"]))
print(json.dumps(p))')"
    UOUT="$(printf '%s' "$payload" | (cd "$SEAT" && RICHOS_ENTITY_ROOT="$SEAT" bash "$UPS" 2>"$SANDBOX/ups.err"))"
    URC=$?
    UERR="$(cat "$SANDBOX/ups.err" 2>/dev/null)"
}

# json_field <json> <python-expression over d>
json_field() {
    printf '%s' "$1" | python3 -c '
import json, sys
try:
    d = json.loads(sys.stdin.read())
except Exception:
    sys.exit(0)
expr = sys.argv[1]
v = eval(expr, {"d": d})
print(v if v is not None else "")' "$2" 2>/dev/null
}

ctx_of() { json_field "$1" '((d.get("hookSpecificOutput") or {}).get("additionalContext"))'; }
sys_of() { json_field "$1" 'd.get("systemMessage")'; }

NOW_EPOCH="$(date +%s)"
iso_at() { # <seconds-offset-from-now>
    python3 -c 'import sys, datetime
t = datetime.datetime.fromtimestamp(int(sys.argv[1]) + int(sys.argv[2]), datetime.timezone.utc)
print(t.strftime("%Y-%m-%dT%H:%M:%S.000Z"))' "$NOW_EPOCH" "$1"
}
TURN_START="$(iso_at -60)"

# mk_transcript <out> <spec-json>
#   spec: a list of steps, each one of
#     ["human", pid, ts, text]              a message he typed
#     ["queued", pid, ts, text]             a message he typed while the turn ran
#     ["machine", pid, ts, text]            a task notification
#     ["read", pid, ts, path, is_error]     a Read tool call and its result
#     ["bash", pid, ts, command, is_error]  a Bash tool call and its result
#     ["say", ts, text]                     assistant text
mk_transcript() {
    python3 - "$1" "$2" <<'PY'
import json, sys
out, spec = sys.argv[1], json.loads(sys.argv[2])
n = [0]
def uid():
    n[0] += 1
    return "u-%04d" % n[0]
rows = []
for st in spec:
    k = st[0]
    if k in ("human", "queued"):
        rows.append({"type": "user", "promptId": st[1], "timestamp": st[2], "uuid": uid(),
                     "origin": {"kind": "human"}, "promptSource": "typed" if k == "human" else "queued",
                     "isSidechain": False, "message": {"role": "user", "content": st[3]}})
    elif k == "machine":
        rows.append({"type": "user", "promptId": st[1], "timestamp": st[2], "uuid": uid(),
                     "origin": {"kind": "task-notification"}, "promptSource": "system",
                     "isSidechain": False, "message": {"role": "user", "content": st[3]}})
    elif k in ("read", "bash"):
        tid = "toolu_" + uid()
        name = "Read" if k == "read" else "Bash"
        inp = {"file_path": st[3]} if k == "read" else {"command": st[3]}
        rows.append({"type": "assistant", "promptId": None, "timestamp": st[2], "uuid": uid(),
                     "isSidechain": False,
                     "message": {"role": "assistant", "content": [
                         {"type": "tool_use", "id": tid, "name": name, "input": inp}]}})
        rows.append({"type": "user", "promptId": st[1], "timestamp": st[2], "uuid": uid(),
                     "isSidechain": False,
                     "message": {"role": "user", "content": [
                         {"type": "tool_result", "tool_use_id": tid, "is_error": bool(st[4]),
                          "content": "error" if st[4] else "ok"}]}})
    elif k == "say":
        rows.append({"type": "assistant", "promptId": None, "timestamp": st[1], "uuid": uid(),
                     "isSidechain": False,
                     "message": {"role": "assistant", "content": [{"type": "text", "text": st[2]}]}})
with open(out, "w") as fh:
    for r in rows:
        fh.write(json.dumps(r) + "\n")
PY
}

# stop_run <transcript> <prompt-id> <last-message> [stop_hook_active] [session] [extra-json]
#   -> SOUT, SERR, SRC
stop_run() {
    local payload
    payload="$(SEAT="$SEAT" T="$1" P="$2" M="$3" A="${4:-false}" \
        SESSION="${5:-bbbb0001-0000-4000-8000-000000000000}" EXTRA="${6:-}" python3 -c '
import json, os
p = {"hook_event_name": "Stop", "session_id": os.environ["SESSION"], "cwd": os.environ["SEAT"],
     "transcript_path": os.environ["T"], "last_assistant_message": os.environ["M"],
     "stop_hook_active": os.environ["A"] == "true"}
if os.environ["P"]:
    p["prompt_id"] = os.environ["P"]
if os.environ.get("EXTRA"):
    p.update(json.loads(os.environ["EXTRA"]))
print(json.dumps(p))')"
    SOUT="$(printf '%s' "$payload" | (cd "$SEAT" && RICHOS_ENTITY_ROOT="$SEAT" bash "$STOP" 2>"$SANDBOX/stop.err"))"
    SRC=$?
    SERR="$(cat "$SANDBOX/stop.err" 2>/dev/null)"
}

missing_set() { # the letters the refusal names as missing, e.g. "abc"
    local s=""
    printf '%s' "$SERR" | grep -q 'MISSING (a)' && s="${s}a"
    printf '%s' "$SERR" | grep -q 'MISSING (b)' && s="${s}b"
    printf '%s' "$SERR" | grep -q 'MISSING (c)' && s="${s}c"
    printf '%s' "$s"
}

# ===========================================================================
# 1. THE TRIGGER PREDICATE
# ===========================================================================
echo "--- the trigger: his words, literally ---"

classify() { # stdin: one message per line -> one 0/1 per line
    python3 "$LIB" classify 2>/dev/null
}

POS="$(sed -n 's/^+ //p' "$CORPUS")"
NEG="$(sed -n 's/^- //p' "$CORPUS")"
POS_N="$(printf '%s\n' "$POS" | grep -c . || true)"
NEG_N="$(printf '%s\n' "$NEG" | grep -c . || true)"
POS_HIT="$(printf '%s\n' "$POS" | classify | grep -c '^1$' || true)"
NEG_HIT="$(printf '%s\n' "$NEG" | classify | grep -c '^1$' || true)"
NEG_SEEN="$(printf '%s\n' "$NEG" | classify | grep -c '^[01]$' || true)"

if [ "$POS_N" -ge 20 ] && [ "$POS_HIT" = "$POS_N" ]; then
    ok "FT01 every positive corpus line triggers ($POS_HIT/$POS_N)"
else
    bad "FT01 every positive corpus line triggers" "triggered $POS_HIT of $POS_N"
fi

if [ "$NEG_N" -ge 8 ] && [ "$NEG_SEEN" = "$NEG_N" ] && [ "$NEG_HIT" = "0" ]; then
    ok "FT02 no negative corpus line triggers (0/$NEG_N, each one classified)"
else
    bad "FT02 no negative corpus line triggers" "triggered $NEG_HIT of $NEG_N; classified $NEG_SEEN"
fi

W6="$(printf 'type a b c d e failure\nfailure a b c d e type\n' | classify | tr -d '\n')"
W7="$(printf 'type a b c d e f failure\nfailure a b c d e f type\n' | classify | tr -d '\n')"
if [ "$W6" = "11" ] && [ "$W7" = "00" ]; then
    ok "FT03 the window: six words apart triggers in either order, seven does not"
else
    bad "FT03 the window boundary" "six apart: $W6 (want 11), seven apart: $W7 (want 00)"
fi

ENV_OUT="$(python3 - "$LIB" <<'PY'
import json, subprocess, sys
lib = sys.argv[1]
cases = [
    ("<task-notification>\n<summary>a failure of this type</summary>\n</task-notification>", 0),
    ("<teammate-message teammate_id=\"x\">what type of failure is this</teammate-message>", 0),
    ("Another Claude session sent a message: <agent-message from=\"x\">type of failure</agent-message>", 0),
    ("Stop hook feedback: the failure type gate refused", 0),
    ("<system-reminder>failure type list</system-reminder>ok thanks", 0),
    ("<pasted_content id=\"1\">the failure type register</pasted_content> please land it", 0),
    ("<system-reminder>noise</system-reminder>what type of failure is that?", 1),
    ("<pasted_content id=\"1\">long report</pasted_content> what type of failure was that?", 1),
]
out = []
for text, want in cases:
    r = subprocess.run([sys.executable, lib, "classify-json"], input=json.dumps(text),
                       capture_output=True, text=True)
    got = r.stdout.strip()
    out.append("%s:%s" % (want, got))
print(" ".join(out))
PY
)"
if [ "$ENV_OUT" = "0:0 0:0 0:0 0:0 0:0 0:0 1:1 1:1" ]; then
    ok "FT04 machine envelopes never trigger, and his words beside a stripped envelope still do"
else
    bad "FT04 machine envelopes" "want:got pairs: $ENV_OUT"
fi

# FT05 — his REAL messages, from the private record, when it is on this machine.
PRIVATE_CORPUS="${RICHOS_FAILURE_TYPE_PRIVATE_CORPUS:-}"
if [ -n "$PRIVATE_CORPUS" ] && [ -f "$PRIVATE_CORPUS" ]; then
    PC_OUT="$(python3 "$LIB" classify-corpus "$PRIVATE_CORPUS" 2>&1)"
    PC_RC=$?
    if [ "$PC_RC" = "0" ]; then
        ok "FT05 the private corpus of his real messages: $PC_OUT"
    else
        bad "FT05 the private corpus of his real messages" "$PC_OUT"
    fi
else
    echo "  NOT RUN  FT05 the private corpus (set RICHOS_FAILURE_TYPE_PRIVATE_CORPUS to its path). NOT counted as a pass."
fi

# ===========================================================================
# 2. THE UserPromptSubmit HOOK
# ===========================================================================
echo ""
echo "--- UserPromptSubmit: the register goes in front of the lead ---"

ups_run "what type of failure was that now??"
CTX="$(ctx_of "$UOUT")"
SYSM="$(sys_of "$UOUT")"
EVT="$(json_field "$UOUT" '((d.get("hookSpecificOutput") or {}).get("hookEventName"))')"
if [ "$URC" = "0" ] && [ "$EVT" = "UserPromptSubmit" ] \
   && printf '%s' "$CTX" | grep -qF "Type 1: A result that never arrived is read as a pass" \
   && printf '%s' "$CTX" | grep -qF "Type 2: A number is chosen because it sounds safe" \
   && printf '%s' "$CTX" | grep -qF "Type 3: $HEAD3" \
   && printf '%s' "$CTX" | grep -qF "Next free type number: 4" \
   && printf '%s' "$CTX" | grep -qF "next free section number: 5.2" \
   && printf '%s' "$CTX" | grep -qF "$REG"; then
    ok "FT10 his message carries the words: every Type N line, both next free numbers and the path reach the MODEL"
else
    bad "FT10 the lookup reaches the model" "rc $URC event '$EVT' ctx: $(printf '%s' "$CTX" | head -c 400)"
fi

if [ -n "$SYSM" ] && [ "$(printf '%s\n' "$SYSM" | grep -c .)" = "1" ] \
   && printf '%s' "$SYSM" | grep -q '3 types'; then
    ok "FT11 the person gets ONE line saying the lookup ran and how many types it handed over"
else
    bad "FT11 one line for the person" "systemMessage: ${SYSM:-<empty>}"
fi

# Live, never a copy: add a type to the file, do not commit it, ask again.
write_register 5 5.2 "### Type 4: A fourth fixture type added after the first lookup"
ups_run "failure type?"
CTX="$(ctx_of "$UOUT")"
if printf '%s' "$CTX" | grep -qF "Type 4: A fourth fixture type added after the first lookup" \
   && printf '%s' "$CTX" | grep -qF "Next free type number: 5"; then
    ok "FT12 the list is read LIVE from the file on every message (an uncommitted new type appears at once)"
else
    bad "FT12 the list is live" "ctx: $(printf '%s' "$CTX" | head -c 300)"
fi
write_register 4 5.2

ups_run "thanks, carry on with the build"
if [ "$URC" = "0" ] && [ -z "$UOUT" ]; then
    ok "FT13 a message without the words: silent, exit 0"
else
    bad "FT13 silent without the words" "rc $URC out: $UOUT"
fi

ups_run "<task-notification><summary>failure type register updated</summary></task-notification>"
if [ "$URC" = "0" ] && [ -z "$UOUT" ]; then
    ok "FT14 a task notification carrying the words is not his message: silent"
else
    bad "FT14 machine text is not his" "out: $UOUT"
fi

# Box and headings disagree: both are shown, and the disagreement is named.
write_register 9 5.2
ups_run "what type of failure is this?"
CTX="$(ctx_of "$UOUT")"
if printf '%s' "$CTX" | grep -qF "Next free type number: 9 (box)" \
   && printf '%s' "$CTX" | grep -qiF "highest heading is Type 3"; then
    ok "FT15 the box and the headings disagree: both numbers are shown and the disagreement is named"
else
    bad "FT15 box disagreement" "ctx: $(printf '%s' "$CTX" | head -c 400)"
fi
write_register 4 5.2

# The register is configured and missing: announced to both audiences.
mv "$REG" "$REG.away"
ups_run "what type of failure is this?"
CTX="$(ctx_of "$UOUT")"; SYSM="$(sys_of "$UOUT")"
mv "$REG.away" "$REG"
if [ "$URC" = "0" ] && printf '%s' "$CTX" | grep -q "COULD NOT READ" \
   && printf '%s' "$SYSM" | grep -q "COULD NOT READ"; then
    ok "FT16 a configured register that is missing is announced to the model AND the person, never silent"
else
    bad "FT16 missing register is loud" "rc $URC ctx: $CTX sys: $SYSM"
fi

# Unconfigured: silent without the words, announced with them.
cp "$SEAT/orchestration.config" "$SANDBOX/oc.keep"
: > "$SEAT/orchestration.config"
ups_run "what type of failure is this?"
CTX_ON="$(ctx_of "$UOUT")"
ups_run "carry on"
OUT_OFF="$UOUT"
cp "$SANDBOX/oc.keep" "$SEAT/orchestration.config"
if printf '%s' "$CTX_ON" | grep -q "NOT CONFIGURED" && [ -z "$OUT_OFF" ]; then
    ok "FT17 no register configured: silent on ordinary messages, announced when he uses the words"
else
    bad "FT17 unconfigured" "with words: ${CTX_ON:-<empty>} / without: ${OUT_OFF:-<empty>}"
fi

ups_run "what type of failure is this?" '{"agent_id": "a-worker", "agent_type": "zach"}'
if [ "$URC" = "0" ] && [ -z "$UOUT" ]; then
    ok "FT18 a payload from a teammate (agent_id) is not his message: silent"
else
    bad "FT18 teammates are not him" "out: $UOUT"
fi

# The budget: a register far past the measured channel cap still produces a
# context under it, and says what it left out instead of being dropped whole.
BIG=()
for i in $(seq 4 260); do BIG+=("### Type $i: Fixture type number $i with a description long enough to cost about ninety characters"); done
write_register 261 5.2 "${BIG[@]}"
ups_run "what type of failure is this?"
CTX="$(ctx_of "$UOUT")"
CTX_LEN="$(printf '%s' "$CTX" | wc -c | tr -d ' ')"
CTX_LINES="$(printf '%s\n' "$CTX" | wc -l | tr -d ' ')"
write_register 4 5.2
if [ "${CTX_LEN:-0}" -gt 1000 ] && [ "$CTX_LEN" -le 9990 ] && [ "$CTX_LINES" -le 190 ] \
   && printf '%s' "$CTX" | grep -q "NOT SHOWN"; then
    ok "FT19 260 types: the context stays under the channel cap ($CTX_LEN chars, $CTX_LINES lines) and names what it left out"
else
    bad "FT19 the budget" "chars ${CTX_LEN:-?} lines ${CTX_LINES:-?}; says NOT SHOWN: $(printf '%s' "$CTX" | grep -c 'NOT SHOWN')"
fi

# ===========================================================================
# 3. THE STOP GATE
# ===========================================================================
echo ""
echo "--- Stop: the turn does not end until read, named, committed ---"

T="$SANDBOX/t.jsonl"
P="prompt-0001"
ASK="what type of failure was that now??"
NAMED="That is Type 3: $HEAD3. Recorded as a new instance."
READ_OK="[\"read\", \"$P\", \"$(iso_at -50)\", \"$REG\", false]"

# FT30 — nothing done at all.
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"say\", \"$(iso_at -40)\", \"a landing failure, I think\"]]"
stop_run "$T" "$P" "a landing failure, I think" false "c0300000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "abc" ]; then
    ok "FT30 answered from recall: REFUSED, and the refusal names all three missing steps"
else
    bad "FT30 answered from recall is refused" "rc $SRC missing '$(missing_set)' stderr: $(printf '%s' "$SERR" | head -c 300)"
fi

# FT31 — read, nothing else.
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], $READ_OK]"
stop_run "$T" "$P" "It looks like a landing failure." false "c0310000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "bc" ]; then
    ok "FT31 read but not named or committed: refused for (b) and (c) only"
else
    bad "FT31 read only" "rc $SRC missing '$(missing_set)'"
fi

# FT32 — read and named, no commit.
stop_run "$T" "$P" "$NAMED" false "c0320000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "c" ]; then
    ok "FT32 read and named, nothing committed: refused for (c) only"
else
    bad "FT32 read and named" "rc $SRC missing '$(missing_set)'"
fi

# FT33 — all three.
write_register 4 5.3 "### 5.2 Type 3, again — today — answered from recall"
SHA33="$(commit_register "register 5.2: Type 3 again")"
stop_run "$T" "$P" "$NAMED" false "c0330000-0000-4000-8000-000000000000"
SYSM="$(sys_of "$SOUT")"
if [ "$SRC" = "0" ] && printf '%s' "$SYSM" | grep -q "Type 3" \
   && printf '%s' "$SYSM" | grep -q "${SHA33:0:7}"; then
    ok "FT33 read, named with the heading verbatim, committed: the turn ends, and the person is told which type and which commit"
else
    bad "FT33 all three" "rc $SRC sys: ${SYSM:-<empty>} stderr: $(printf '%s' "$SERR" | head -c 300)"
fi

# FT34 — a number that is not in the register.
stop_run "$T" "$P" "That is Type 99: Something invented." false "c0340000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "b" ]; then
    ok "FT34 a type number the register does not have does not count: refused for (b)"
else
    bad "FT34 invented number" "rc $SRC missing '$(missing_set)'"
fi

# FT35 — the right number, the heading from memory.
stop_run "$T" "$P" "That is Type 3: answering from recall." false "c0350000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "b" ] && printf '%s' "$SERR" | grep -qF "$HEAD3"; then
    ok "FT35 the right number with a paraphrased heading: refused for (b), and the refusal quotes the register's heading"
else
    bad "FT35 heading from memory" "rc $SRC missing '$(missing_set)' stderr: $(printf '%s' "$SERR" | head -c 300)"
fi

# FT36 — markdown and wrapping around the heading are not a paraphrase.
stop_run "$T" "$P" "**Type 3: A lookup that takes seconds is answered
from memory instead.**" false "c0360000-0000-4000-8000-000000000000"
if [ "$SRC" = "0" ]; then
    ok "FT36 bold, a line break and a final period around the verbatim heading still count"
else
    bad "FT36 formatting tolerated" "rc $SRC missing '$(missing_set)'"
fi

# FT37 — "new type", with the new heading committed this turn.
write_register 5 5.3 "### 5.2 Type 3, again — today — answered from recall" \
    "### Type 4: A promise is made with no mechanism behind it"
commit_register "register: Type 4" >/dev/null
stop_run "$T" "$P" "This is a new type, recorded as Type 4." false "c0370000-0000-4000-8000-000000000000"
if [ "$SRC" = "0" ]; then
    ok "FT37 \"new type\" with its Type 4 heading committed this turn: the turn ends"
else
    bad "FT37 new type" "rc $SRC missing '$(missing_set)' stderr: $(printf '%s' "$SERR" | head -c 300)"
fi

# FT38 — "new type" said, no new heading committed this turn. The two commits
# above are reset away, so no ref reaches them any more.
git -C "$REC" reset -q --hard HEAD~2
write_register 4 5.3 "### 5.2 Type 3, again — today — a wording change only"
commit_register "register: wording" >/dev/null
stop_run "$T" "$P" "This is a new type." false "c0380000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "b" ] && printf '%s' "$SERR" | grep -q "new type"; then
    ok "FT38 \"new type\" with no new Type heading committed: refused for (b), and the refusal says why"
else
    bad "FT38 new type without a heading" "rc $SRC missing '$(missing_set)' stderr: $(printf '%s' "$SERR" | head -c 300)"
fi

# FT39 — the only register commit is from BEFORE the turn.
git -C "$REC" reset -q --hard HEAD~1
write_register 4 5.3 "### 5.2 Type 3, again — earlier — before this turn"
commit_register "register 5.2 earlier" "$(iso_at -3600)" >/dev/null
stop_run "$T" "$P" "$NAMED" false "c0390000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "c" ]; then
    ok "FT39 a register commit from before the turn does not count: refused for (c)"
else
    bad "FT39 earlier commit" "rc $SRC missing '$(missing_set)'"
fi

# FT40 — a commit this turn that does not touch the register.
printf 'other\n' > "$REC/other.md"
git -C "$REC" add other.md >/dev/null 2>&1
git -C "$REC" commit -qm "unrelated record" >/dev/null 2>&1
stop_run "$T" "$P" "$NAMED" false "c0400000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "c" ]; then
    ok "FT40 a commit this turn that does not change the register does not count: refused for (c)"
else
    bad "FT40 unrelated commit" "rc $SRC missing '$(missing_set)'"
fi

# FT41 — a register commit on a BRANCH (a worktree) counts: the lead records in
# a workspace as often as on main.
git -C "$REC" checkout -q -b cc/record-branch
write_register 4 5.4 "### 5.2 Type 3, again — earlier — before this turn" \
    "### 5.3 Type 3, again — today — on a branch"
commit_register "register 5.3 on a branch" >/dev/null
git -C "$REC" checkout -q main
stop_run "$T" "$P" "$NAMED" false "c0410000-0000-4000-8000-000000000000"
if [ "$SRC" = "0" ]; then
    ok "FT41 a register commit made this turn on a branch, not main, counts"
else
    bad "FT41 branch commit" "rc $SRC missing '$(missing_set)'"
fi

# From here on a commit made this turn exists (on the branch), so (c) holds.

# FT42 — a turn that did not start from the words: silent, whatever the reply.
mk_transcript "$T" "[[\"human\", \"prompt-0042\", \"$TURN_START\", \"carry on with the build\"]]"
stop_run "$T" "prompt-0042" "Carrying on." false "c0420000-0000-4000-8000-000000000000"
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "FT42 a turn that did not start from the words: exit 0, nothing said"
else
    bad "FT42 untriggered is silent" "rc $SRC stderr: $SERR out: $SOUT"
fi

# FT43 — the re-fire. The engine's usual Stop pattern stands down when
# stop_hook_active is true; this gate deliberately does NOT, because every one of
# its three steps is one the lead can take in the same turn. The host's own
# consecutive-block cap is the bound.
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"]]"
stop_run "$T" "$P" "a landing failure" true "c0430000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ]; then
    ok "FT43 the re-fire (stop_hook_active) with the work still undone is REFUSED again, not waved through"
else
    bad "FT43 no stand-down on the re-fire" "rc $SRC"
fi

# FT44 — the re-fire after the work was done ends the turn.
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], $READ_OK, [\"say\", \"$(iso_at -30)\", \"$NAMED\"]]"
stop_run "$T" "$P" "Done: recorded." true "c0440000-0000-4000-8000-000000000000"
if [ "$SRC" = "0" ]; then
    ok "FT44 the re-fire after reading, naming (in an EARLIER text block of the turn) and committing ends the turn"
else
    bad "FT44 re-fire after the work" "rc $SRC missing '$(missing_set)'"
fi

# FT45 — carry-over. A refused turn that ended anyway (the host's cap) leaves
# the obligation; the NEXT turn is refused for it even though his next message
# does not carry the words; the turn that pays it clears it.
S45="c0450000-0000-4000-8000-000000000000"
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"]]"
stop_run "$T" "$P" "a landing failure" false "$S45"
R1="$SRC"
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"human\", \"prompt-0046\", \"$(iso_at -20)\", \"so?\"]]"
stop_run "$T" "prompt-0046" "Still thinking." false "$S45"
R2="$SRC"
mk_transcript "$T" "[[\"human\", \"prompt-0046\", \"$(iso_at -20)\", \"so?\"], [\"read\", \"prompt-0046\", \"$(iso_at -10)\", \"$REG\", false]]"
git -C "$REC" checkout -q cc/record-branch
write_register 4 5.5 "### 5.2 Type 3, again — earlier — before this turn" \
    "### 5.3 Type 3, again — today — on a branch" "### 5.4 Type 3, again — today — carried over"
commit_register "register 5.4 carried over" >/dev/null
git -C "$REC" checkout -q main
stop_run "$T" "prompt-0046" "$NAMED" false "$S45"
R3="$SRC"
mk_transcript "$T" "[[\"human\", \"prompt-0047\", \"$(iso_at -5)\", \"good\"]]"
stop_run "$T" "prompt-0047" "Next." false "$S45"
R4="$SRC"
if [ "$R1$R2$R3$R4" = "2200" ]; then
    ok "FT45 carry-over: refused, then refused again on his next message, then paid and CLEARED (2,2,0,0)"
else
    bad "FT45 carry-over" "exits $R1,$R2,$R3,$R4 (want 2,2,0,0)"
fi

# FT46 — a pending obligation from the UserPromptSubmit hook, for a message the
# transcript positively shows was NOT his, is dropped.
S46="c0460000-0000-4000-8000-000000000000"
UPS_SESSION="$S46" ups_run "what type of failure is this?"
mk_transcript "$T" "[[\"machine\", \"prompt-0048\", \"$TURN_START\", \"what type of failure is this?\"]]"
stop_run "$T" "prompt-0048" "ok" false "$S46"
R46a="$SRC"
UPS_SESSION="c0461000-0000-4000-8000-000000000000" ups_run "what type of failure is this?"
mk_transcript "$T" "[[\"human\", \"prompt-0049\", \"$TURN_START\", \"hello\"]]"
stop_run "$T" "prompt-0049" "ok" false "c0461000-0000-4000-8000-000000000000"
R46b="$SRC"
if [ "$R46a" = "0" ] && [ "$R46b" = "2" ]; then
    ok "FT46 an obligation raised at UserPromptSubmit is dropped only when the transcript shows the text was machine-written, and held otherwise"
else
    bad "FT46 pending obligations" "machine-shown: $R46a (want 0), not shown: $R46b (want 2)"
fi

# FT47 — reading: grep on the register counts; `git add` of it does not; an
# errored Read does not.
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"bash\", \"$P\", \"$(iso_at -50)\", \"grep -n 'Type 3' $REG\", false]]"
stop_run "$T" "$P" "$NAMED" false "c0470000-0000-4000-8000-000000000000"
G1="$SRC"
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"bash\", \"$P\", \"$(iso_at -50)\", \"git -C $REC add $REL\", false]]"
stop_run "$T" "$P" "$NAMED" false "c0471000-0000-4000-8000-000000000000"
G2="$SRC$(missing_set)"
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"read\", \"$P\", \"$(iso_at -50)\", \"$REG\", true]]"
stop_run "$T" "$P" "$NAMED" false "c0472000-0000-4000-8000-000000000000"
G3="$SRC$(missing_set)"
if [ "$G1" = "0" ] && [ "$G2" = "2a" ] && [ "$G3" = "2a" ]; then
    ok "FT47 grep of the register is a read; git add of it is not; a Read that errored is not"
else
    bad "FT47 what counts as reading" "grep: $G1 (want 0), git add: $G2 (want 2a), errored Read: $G3 (want 2a)"
fi

# FT48 — a copy of the register in a worktree is the register.
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"read\", \"$P\", \"$(iso_at -50)\", \"/elsewhere/rec-wt/agent/$REL\", false]]"
stop_run "$T" "$P" "$NAMED" false "c0480000-0000-4000-8000-000000000000"
if [ "$SRC" = "0" ]; then
    ok "FT48 reading the register's copy in a worktree counts"
else
    bad "FT48 worktree copy" "rc $SRC missing '$(missing_set)'"
fi

# FT49 — a message he queued mid-turn carries the words: the turn owes the answer.
mk_transcript "$T" "[[\"human\", \"prompt-0050\", \"$TURN_START\", \"land it\"], [\"queued\", \"prompt-0050\", \"$(iso_at -30)\", \"and what type of failure was that?\"]]"
stop_run "$T" "prompt-0050" "Landed." false "c0490000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ]; then
    ok "FT49 a message he queued while the turn was running carries the words: refused until answered"
else
    bad "FT49 queued message" "rc $SRC"
fi

# FT50 — a teammate's Stop is not the lead's.
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"]]"
stop_run "$T" "$P" "x" false "c0500000-0000-4000-8000-000000000000" '{"agent_id": "a-worker"}'
if [ "$SRC" = "0" ]; then
    ok "FT50 a teammate's payload (agent_id) is never refused by this gate"
else
    bad "FT50 teammates are exempt" "rc $SRC"
fi

# FT51 — the register cannot be read: the turn ends, and the person is TOLD.
mv "$REG" "$REG.away"
stop_run "$T" "$P" "x" false "c0510000-0000-4000-8000-000000000000"
mv "$REG.away" "$REG"
if [ "$SRC" = "0" ] && printf '%s' "$(sys_of "$SOUT")" | grep -q "NOT CHECKED"; then
    ok "FT51 a triggered turn whose register cannot be read ends, with a notice to the person that it was NOT CHECKED"
else
    bad "FT51 cannot-read is loud" "rc $SRC out: $SOUT"
fi

# FT52 — stood down by config: announced.
printf 'CHECK_FAILURE_TYPE=0\n' >> "$SEAT/orchestration.config"
stop_run "$T" "$P" "x" false "c0520000-0000-4000-8000-000000000000"
cp "$SANDBOX/oc.keep" "$SEAT/orchestration.config"
if [ "$SRC" = "0" ] && printf '%s' "$(sys_of "$SOUT")" | grep -q "STOOD DOWN"; then
    ok "FT52 CHECK_FAILURE_TYPE=0 lets the turn end and says so to the person"
else
    bad "FT52 stand-down announced" "rc $SRC out: $SOUT"
fi

# FT53 — no prompt_id: the turn cannot be scoped, and that is said, not guessed.
stop_run "$T" "" "x" false "c0530000-0000-4000-8000-000000000000"
if [ "$SRC" = "0" ] && printf '%s' "$(sys_of "$SOUT")" | grep -q "NOT CHECKED" \
   && printf '%s' "$(sys_of "$SOUT")" | grep -q "no prompt_id"; then
    ok "FT53 a payload with no prompt_id is announced as NOT CHECKED, for that reason, rather than judged on the whole session"
else
    bad "FT53 unscopable turn" "rc $SRC out: $SOUT"
fi

# FT54 — the words in the ASSISTANT's reply or a tool result never trigger.
mk_transcript "$T" "[[\"human\", \"prompt-0054\", \"$TURN_START\", \"status?\"], [\"say\", \"$(iso_at -30)\", \"No new type of failure today.\"], [\"bash\", \"prompt-0054\", \"$(iso_at -20)\", \"echo failure type\", false]]"
stop_run "$T" "prompt-0054" "No failure type to report." false "c0540000-0000-4000-8000-000000000000"
if [ "$SRC" = "0" ] && [ -z "$SERR" ]; then
    ok "FT54 the words in the lead's own reply or in tool traffic never trigger"
else
    bad "FT54 only his words trigger" "rc $SRC stderr: $SERR"
fi

# FT55 — a command that merely PRINTS the words "cat" and the register's name
# has read nothing: echo is not a reader, so (a) is still missing.
REGBASE="$(basename "$REG")"
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"bash\", \"$P\", \"$(iso_at -50)\", \"echo cat $REGBASE\", false]]"
stop_run "$T" "$P" "It looks like a landing failure." false "c0550000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "ab" ]; then
    ok "FT55 echoing 'cat <register name>' is not reading the register: refused for (a) and (b) ((c) was committed above)"
else
    bad "FT55 echo is not a read" "rc $SRC missing '$(missing_set)'"
fi

# FT57 — a reader run on some OTHER file is not the lookup either.
mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"bash\", \"$P\", \"$(iso_at -50)\", \"cat $SANDBOX/notes.md | head -5\", false]]"
stop_run "$T" "$P" "It looks like a landing failure." false "c0570000-0000-4000-8000-000000000000"
if [ "$SRC" = "2" ] && [ "$(missing_set)" = "ab" ]; then
    ok "FT57 a reader run on a different file is not reading the register: refused for (a) and (b)"
else
    bad "FT57 another file is not the register" "rc $SRC missing '$(missing_set)'"
fi

# FT56 — the opposite, same path: a real reader run on the register IS a read,
# alone, in a pipeline, and as the argument of a search.
for CMD56 in "cat $REG" "cat $REG | head -20" "grep -n 'Type 3' $REG" "sed -n 1,20p $REG" "cd $REC && cat $REL" "cat < $REG"; do
    mk_transcript "$T" "[[\"human\", \"$P\", \"$TURN_START\", \"$ASK\"], [\"bash\", \"$P\", \"$(iso_at -50)\", \"$CMD56\", false]]"
    stop_run "$T" "$P" "It looks like a landing failure." false "c0560000-0000-4000-8000-000000000000"
    if [ "$SRC" = "2" ] && [ "$(missing_set)" = "b" ]; then
        ok "FT56 a real reader on the register counts as reading it (a satisfied): $CMD56"
    else
        bad "FT56 a real read is credited: $CMD56" "rc $SRC missing '$(missing_set)'"
    fi
done

# FT58 — P5-57: printing the register's name, matching it as a pattern or feeding it
# as here-string text is not reading it; a real file operand or an interpreter
# program that reads its contents still is. Direct on the predicate, both directions.
FT58_OUT="$(python3 - "$LIB" "$REG" "$REL" <<'PYEOF'
import importlib.util, os, shlex, subprocess, sys
spec = importlib.util.spec_from_file_location("ft", sys.argv[1])
ft = importlib.util.module_from_spec(spec); spec.loader.exec_module(ft)
reg, rel = sys.argv[2], sys.argv[3]
base = os.path.basename(reg)
def reads(cmd):
    return ft.bash_reads_register(cmd, reg, rel, base)
not_reads = [
    "python3 -c \"print('%s')\"" % base,
    "grep %s other-file.md" % base,
    "grep -n %s other-file.md" % base,
    "wc <<< %s" % base,
    "cat <<< %s" % base,
    "sed -n 1,3p other.md <<< %s" % base,
    "perl -e 'print \"%s\"'" % base,
    "awk '/%s/' other.md" % base,
]
reads_it = [
    "cat %s" % reg,
    "grep -n 'Type 3' %s" % reg,
    "grep -e foo %s" % reg,
    "sed -n 1,20p %s" % reg,
    "awk '{print}' %s" % reg,
    "wc -l < %s" % reg,
    "python3 -c \"print(open('%s').read())\"" % reg,
    "perl -ne 'print' %s" % reg,
]
# An inline Python program needs an executed content read. A tool result is
# also required, even when the program visibly performs the read.
def python(program):
    return "python3 -c " + shlex.quote(program)
not_reads.extend(map(python, [
    f"def later():\n    open({reg!r}).read()",
    f"if False:\n    open({reg!r}).read()",
    f"lambda: open({reg!r}).read()",
    f"def lazy():\n    yield open({reg!r}).read()\nlazy()",
    f"open({reg!r})",
    f"open({reg!r}).read(0)",
    f"exit(0); open({reg!r}).read()",
    f"print(exit(0), open({reg!r}).read())",
    f"def read_file():\n    return None\n    open({reg!r}).read()\nread_file()",
]))
helper_program = f"def read_file(path):\n    return open(path).read()\nprint(read_file({reg!r}))"
helper = python(helper_program)
completed = subprocess.run([sys.executable, "-c", helper_program], capture_output=True, text=True, check=True)
with open(reg) as register:
    assert completed.stdout == register.read() + "\n"
reads_it.extend([
    helper,
    python(f"with open({reg!r}) as register:\n    print(register.read())"),
    python(f"from pathlib import Path; print(Path({reg!r}).read_text())"),
    python("import sys; print(open(sys.argv[1]).read())") + " " + shlex.quote(reg),
])
for result, expected in [({"content": completed.stdout}, True), ({"is_error": True}, False), (None, False)]:
    facts = {"calls": [{"id": "call", "name": "Bash", "input": {"command": helper}}],
             "results": {"call": result}}
    if ft.register_read(facts, reg, rel) != expected:
        print("Incorrect credit for helper result:", result)
bad = ["MISCREDITED: " + c for c in not_reads if reads(c)] + ["MISSED: " + c for c in reads_it if not reads(c)]
print("\n".join(bad))
PYEOF
)"
FT58_RC=$?
if [ "$FT58_RC" -eq 0 ] && [ -z "$FT58_OUT" ]; then
    ok "FT58 a printed, pattern or here-string mention of the register is not a read; real file operands and executed content reads are"
else
    bad "FT58 register-read proof" "$FT58_OUT"
fi

# ===========================================================================
# 4. BOTH HOOKS, IN ORDER — the replay shape
# ===========================================================================
echo ""
echo "--- both hooks, in order ---"
S70="c0700000-0000-4000-8000-000000000000"
UPS_SESSION="$S70" ups_run "WHAT MUST HAPPEN WHENEVER I MENTION THE WORDS \"TYPE\" AND \"FAILURE\" CLOSE TO EACH OTHER???"
CTX="$(ctx_of "$UOUT")"
mk_transcript "$T" "[[\"human\", \"prompt-0070\", \"$TURN_START\", \"WHAT MUST HAPPEN WHENEVER I MENTION THE WORDS \\\"TYPE\\\" AND \\\"FAILURE\\\" CLOSE TO EACH OTHER???\"]]"
stop_run "$T" "prompt-0070" "What must always happen: I read the register and name Type <N>." false "$S70"
if printf '%s' "$CTX" | grep -qF "Type 3: $HEAD3" && [ "$SRC" = "2" ] && [ "$(missing_set)" = "ab" ]; then
    ok "FT70 his capitalized, quoted words: the list is injected, and a reply naming only 'Type <N>' is refused for (a) and (b)"
else
    bad "FT70 both hooks in order" "ctx has Type 3: $(printf '%s' "$CTX" | grep -c 'Type 3:'); rc $SRC missing '$(missing_set)'"
fi

LEDGER_DIR="$SEAT/.claude/state/failure-type"
if [ -s "$LEDGER_DIR/events.jsonl" ] && grep -q '"blocked"' "$LEDGER_DIR/events.jsonl" \
   && grep -q '"satisfied"' "$LEDGER_DIR/events.jsonl" && grep -q '"injected"' "$LEDGER_DIR/events.jsonl"; then
    ok "FT71 every verdict is written to the entity's event log (injected, blocked, satisfied all present)"
else
    bad "FT71 the event log" "$(ls -la "$LEDGER_DIR" 2>&1 | head -5)"
fi

# ===========================================================================
# 5. THE MUTATION HARNESS
# ===========================================================================
echo ""
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$SCRIPT_DIR/failure-type.mutation.sh" ]; then
    echo "=== running the mutation harness ==="
    if bash "$SCRIPT_DIR/failure-type.mutation.sh"; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        echo "  FAIL  M. the mutation harness found a property this suite does not actually prove"
    fi
    echo ""
fi

if [ "$FAIL" -eq 0 ]; then
    printf '  %s/%s cases passed\n' "$PASS" "$PASS"
    exit 0
fi
printf '  %s passed, %s FAILED\n' "$PASS" "$FAIL"
exit 1
