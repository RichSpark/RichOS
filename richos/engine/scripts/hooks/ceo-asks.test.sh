#!/usr/bin/env bash
#
# ceo-asks.test.sh — the CEO-ask gate, end to end, in a sandbox.
#
# Builds two real git repositories — a SEAT that has adopted the engine and a
# separate TODOREPO repository that declares the CEO's TODOs — because that
# cross-repository shape is the normal one for this operation (a femcboost seat,
# a richos-hq list) and a suite that only ever tested the single-repo case
# would pass over the configuration that actually ships.
#
# The SHIPPED hooks are driven with synthetic payloads. Nothing is stubbed
# except the entity root (RICHOS_ENTITY_ROOT), which is the one thing a test
# must not share with the live session.
#
# THE PROPERTY THIS SUITE EXISTS TO PIN, above all the others:
#
#   A QUESTION THAT MATCHES NOTHING DISCHARGES NOTHING.
#
# Without it, one junk question per session clears the gate forever and the
# whole mechanism becomes a formality with a log. Case C3 fires a junk question
# through the real witness and then asserts the real gate is STILL closed.
#
# Run directly:  scripts/hooks/ceo-asks.test.sh [--verbose]

set -uo pipefail

VERBOSE=0
[ "${1:-}" = "--verbose" ] && VERBOSE=1

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_SRC="$(cd "$SRC_DIR/../.." && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }
say() { [ "$VERBOSE" -eq 1 ] && printf '\n----- %s -----\n%s\n' "$1" "$2"; return 0; }

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d -t ceo-asks.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

ENGINE="$SANDBOX/engine"
mkdir -p "$ENGINE/scripts/hooks" "$ENGINE/scripts/lib"
for h in notice-ceo-asks.sh guard-ceo-ask-first.sh notice-ceo-unasked.sh session-start-ceo-ask.sh; do
    cp "$SRC_DIR/$h" "$ENGINE/scripts/hooks/$h"
    chmod +x "$ENGINE/scripts/hooks/$h"
done
for l in ceo-asks.sh ceo-asks.py ceo-todos.sh ceo-todos.py resolve-roots.sh \
         resolve-main-checkout.sh stop-hook-notice.sh cold-open-prompt.md; do
    cp "$SRC_DIR/../lib/$l" "$ENGINE/scripts/lib/$l" 2>/dev/null || true
done
cp "$ENGINE_SRC/scripts/ceo-asks-status.sh" "$ENGINE/scripts/"
chmod +x "$ENGINE/scripts/ceo-asks-status.sh"

WITNESS="$ENGINE/scripts/hooks/notice-ceo-asks.sh"
GATE="$ENGINE/scripts/hooks/guard-ceo-ask-first.sh"
STOPN="$ENGINE/scripts/hooks/notice-ceo-unasked.sh"
SSTART="$ENGINE/scripts/hooks/session-start-ceo-ask.sh"
STATUS="$ENGINE/scripts/ceo-asks-status.sh"

# --- the TODOREPO repository --------------------------------------------------
TODOREPO="$SANDBOX/todorepo"
mkdir -p "$TODOREPO/wiki"
git -C "$SANDBOX" init -q todorepo 2>/dev/null || true
cat > "$TODOREPO/.ceo-todos" <<'EOF'
TODO_RECORD="wiki/open-items.md"
TODO_VIEW="CEO-TODOs.md"
ROOT_README="README.md"
CEO_SECTIONS="1 2"
PREPARER_SECTION="3"
ARTIFACT_ROOTS="q=."
EOF
echo "# todorepo" > "$TODOREPO/README.md"
echo "# view" > "$TODOREPO/CEO-TODOs.md"

# THE RECORD. Three prepared items and one that is NOT prepared, because
# "prepared" is the predicate's own definition and a suite that never showed it
# an unprepared item could not tell the definition from a count of headings.
write_record() { # [state-of-2.2]
    cat > "$TODOREPO/wiki/open-items.md" <<EOF
# Open items

## 1. Waiting on the CEO — a decision

### 1.1 READY-FOR-CEO — Apple signing and developer enrollment

- **Open:** \`q/wiki/packaging-and-signing.md\`
- **Time:** 15 minutes
- **Done:** a ruling recorded in ceo-decisions.md — individual enrollment, company enrollment, or an explicit not yet
- **Unblocks:** stable microphone grants across rebuilds

Prose that is not a field.

### 1.2 READY-FOR-CEO — The call-transcription default

- **Open:** \`q/wiki/call-transcription-approach.md\`
- **Time:** 15 minutes
- **Done:** a ruling recorded in ceo-decisions.md — switch the default now, or hold it
- **Unblocks:** the shipping default

## 2. Waiting on the CEO — his hands

### 2.1 READY-FOR-CEO — Verify the podcast reference transcript

- **Open:** \`q/docs/worksheet.md\`
- **Time:** 30 minutes
- **Done:** every one of the 30 windows carries OK or a written correction
- **Unblocks:** the first computable word error rate

### 2.2 ${1:-BLOCKED-ON-RICH} — Windows real-capture test

- **Open:** \`q/docs/windows-protocol.md\`
- **Time:** 20 minutes
- **Done:** every check in the protocol marked pass or fail
- **Unblocks:** the Windows capture path

## 3. Buildable now — nobody blocked

Nothing here.
EOF
}
write_record

# --- the SEAT repository ---------------------------------------------------
SEAT="$SANDBOX/seat"
mkdir -p "$SEAT/.claude/state"
git -C "$SANDBOX" init -q seat 2>/dev/null || true
write_seat_config() { # <CEO_TODOS_REPOS value, or empty to omit the key>
    {
        echo 'PROTECTED_PATHS="src"'
        [ -n "${1:-}" ] && printf 'CEO_TODOS_REPOS="%s"\n' "$1"
    } > "$SEAT/orchestration.config"
}
write_seat_config "$TODOREPO"

LEDGER="$SEAT/.claude/state/ceo-asks.jsonl"

# --- payload builders ------------------------------------------------------
# The tool_input shape here is COPIED from a real AskUserQuestion call recovered
# from a session transcript on 2026-08-31, fields and all (header, question,
# multiSelect, options[].label, options[].description). A synthetic payload that
# is merely plausible would test the parser against the test author's memory of
# the schema, which is the one reader whose agreement proves nothing.
ask_payload() { # <session> <agent_id|-> <question> <optlabel> <optdesc>
    CA_S="$1" CA_A="$2" CA_Q="$3" CA_L="$4" CA_D="$5" CA_CWD="$SEAT" python3 -c '
import json, os
p = {
    "hook_event_name": "PostToolUse",
    "tool_name": "AskUserQuestion",
    "session_id": os.environ["CA_S"],
    "cwd": os.environ["CA_CWD"],
    "tool_input": {"questions": [{
        "header": "Decision",
        "question": os.environ["CA_Q"],
        "multiSelect": False,
        "options": [
            {"label": os.environ["CA_L"], "description": os.environ["CA_D"]},
            {"label": "Not yet", "description": "hold it"},
        ],
    }]},
}
a = os.environ["CA_A"]
if a != "-":
    p["agent_id"] = a
print(json.dumps(p))
'
}

agent_payload() { # <session> <prompt>
    CA_S="$1" CA_P="$2" CA_CWD="$SEAT" python3 -c '
import json, os
print(json.dumps({
    "hook_event_name": "PreToolUse",
    "tool_name": "Agent",
    "session_id": os.environ["CA_S"],
    "cwd": os.environ["CA_CWD"],
    "tool_input": {
        "subagent_type": "dev",
        "name": "dev-sonnet-t1",
        "isolation": "worktree",
        "prompt": os.environ["CA_P"],
    },
}))
'
}

stop_payload() { # <session>
    CA_S="$1" CA_CWD="$SEAT" python3 -c '
import json, os
print(json.dumps({
    "hook_event_name": "Stop",
    "session_id": os.environ["CA_S"],
    "cwd": os.environ["CA_CWD"],
    "stop_hook_active": False,
}))
'
}

run_hook() { # <hook> <payload> -> RC / OUT / ERR
    local hook="$1" payload="$2"
    # Never an app run unless a case gives one (run_hook_in_run): a suite run
    # from inside an app lead must not inherit that lead's.
    OUT="$(RICHOS_ENTITY_ROOT="$SEAT" printf '%s' "$payload" | env -u RICHOS_OPERATOR_LEAD RICHOS_ENTITY_ROOT="$SEAT" bash "$hook" 2>"$SANDBOX/err")"
    RC=$?
    ERR="$(cat "$SANDBOX/err")"
    return 0
}

run_hook_in_run() { # <app run> <hook> <payload>: as one of the app's leads -> RC / OUT / ERR
    local run="$1" hook="$2" payload="$3"
    OUT="$(printf '%s' "$payload" | RICHOS_OPERATOR_LEAD="$run" RICHOS_ENTITY_ROOT="$SEAT" bash "$hook" 2>"$SANDBOX/err")"
    RC=$?
    ERR="$(cat "$SANDBOX/err")"
    return 0
}

app_ask_payload() { # <session> <app run> <app question id>: a question the APP witnessed on display
    CA_S="$1" CA_R="$2" CA_QID="$3" CA_Q="$Q_SIGNING" CA_L="$L_SIGNING" CA_D="$D_SIGNING" CA_CWD="$SEAT" python3 -c '
import json, os
print(json.dumps({
    "hook_event_name": "PostToolUse", "tool_name": "AskUserQuestion",
    "session_id": os.environ["CA_S"], "cwd": os.environ["CA_CWD"],
    "app_question_id": os.environ["CA_QID"], "app_run": os.environ["CA_R"],
    "tool_input": {"questions": [{"question": os.environ["CA_Q"], "multiSelect": False,
                                   "options": [{"label": os.environ["CA_L"], "description": os.environ["CA_D"]}]}]},
}))
'
}

reset_ledger() { rm -f "$LEDGER"; rm -rf "$SEAT/.claude/state/stop-hook-notices"; }

# A real signing question — the words a person would actually put on screen.
Q_SIGNING="How should we sign RichOS? Apple Developer enrollment is the open question."
L_SIGNING="Enroll as an individual"
D_SIGNING="99 dollars a year, no company and no D-U-N-S; developer enrollment done today"

echo ""
echo "=== A. the predicate: which item was a question about? ==="

items_json() { # -> path
    local out="$SANDBOX/items.json"
    ( . "$ENGINE/scripts/lib/ceo-asks.sh"
      ca_resolve "$SEAT" >/dev/null 2>&1 || true
      ca_items_json "$out" >/dev/null 2>&1 ) || true
    printf '%s' "$out"
}
ITEMS="$(items_json)"

match_of() { # <text> -> the MATCH/UNMATCHED line
    printf '%s' "$1" > "$SANDBOX/q.txt"
    ( . "$ENGINE/scripts/lib/ceo-asks.sh"
      ca_match "$SANDBOX/q.txt" "$ITEMS" 2>/dev/null ) || true
}

M="$(match_of "$Q_SIGNING
$L_SIGNING
$D_SIGNING")"
say "A1" "$M"
case "$M" in
    MATCH*1.1*) ok "A1. a real Apple-signing question matches item 1.1" ;;
    *) bad "A1. a real Apple-signing question matches item 1.1" "$M" ;;
esac

M="$(match_of "What should we work on next?
Ship it
Wait")"
say "A2" "$M"
case "$M" in
    UNMATCHED*) ok "A2. 'what should we work on next' matches NOTHING" ;;
    *) bad "A2. 'what should we work on next' matches NOTHING" "$M" ;;
esac

M="$(match_of "Should I dispatch an engineer to the transcript work now?
Yes
No")"
say "A3" "$M"
case "$M" in
    UNMATCHED*) ok "A3. a near miss that borrows two of an item's words still matches NOTHING" ;;
    *) bad "A3. a near miss that borrows two of an item's words still matches NOTHING" "$M" ;;
esac

M="$(match_of "Item 2.1 — shall we do this one now?
Yes
No")"
say "A4" "$M"
case "$M" in
    MATCH*2.1*) ok "A4. naming an item by its id matches it" ;;
    *) bad "A4. naming an item by its id matches it" "$M" ;;
esac

# THE DEFINITION OF PREPARED, shown rather than assumed: 4 items are in CEO
# sections and only 3 are READY-FOR-CEO.
PREP="$( . "$ENGINE/scripts/lib/ceo-asks.sh"
         ca_resolve "$SEAT" >/dev/null 2>&1
         ca_assess "$SEAT" "NOSESSION" >/dev/null 2>&1
         printf '%s/%s' "$CA_PREPARED" "$CA_UNASKED" )"
if [ "$PREP" = "3/3" ]; then
    ok "A5. an item in a CEO section in the BLOCKED-ON-RICH state is NOT prepared (3 of 4 count)"
else
    bad "A5. an item in a CEO section in the BLOCKED-ON-RICH state is NOT prepared (3 of 4 count)" "prepared/unasked = $PREP"
fi

echo ""
echo "=== B. the witness: only a real AskUserQuestion call writes a record ==="

reset_ledger
run_hook "$WITNESS" "$(ask_payload S1 - "$Q_SIGNING" "$L_SIGNING" "$D_SIGNING")"
if [ "$RC" -eq 0 ] && [ -f "$LEDGER" ] \
   && grep -q '"matched_item": "1.1"' "$LEDGER" && grep -q '"discharges": true' "$LEDGER"; then
    ok "B1. a real question about 1.1 is witnessed as 1.1 and discharges"
else
    bad "B1. a real question about 1.1 is witnessed as 1.1 and discharges" "rc=$RC $(cat "$LEDGER" 2>/dev/null | head -1)"
fi

reset_ledger
run_hook "$WITNESS" "$(ask_payload S1 - "What should we work on next?" "Ship it" "get moving")"
if [ -f "$LEDGER" ] && grep -q '"matched_item": "UNMATCHED"' "$LEDGER" \
   && grep -q '"discharges": false' "$LEDGER"; then
    ok "B2. a junk question is witnessed as UNMATCHED and discharges nothing"
else
    bad "B2. a junk question is witnessed as UNMATCHED and discharges nothing" "$(cat "$LEDGER" 2>/dev/null | head -1)"
fi

reset_ledger
run_hook "$WITNESS" "$(ask_payload S1 agent-7 "$Q_SIGNING" "$L_SIGNING" "$D_SIGNING")"
if [ -f "$LEDGER" ] && grep -q '"agent_id": "agent-7"' "$LEDGER" \
   && grep -q '"discharges": false' "$LEDGER"; then
    ok "B3. a WORKER's matching question is recorded and discharges nothing"
else
    bad "B3. a WORKER's matching question is recorded and discharges nothing" "$(cat "$LEDGER" 2>/dev/null | head -1)"
fi

reset_ledger
run_hook "$WITNESS" "$(agent_payload S1 "spawn something")"
if [ ! -f "$LEDGER" ]; then
    ok "B4. a payload for another tool writes nothing at all"
else
    bad "B4. a payload for another tool writes nothing at all" "$(cat "$LEDGER")"
fi

reset_ledger
MULTI="$(CA_CWD="$SEAT" python3 -c '
import json, os
print(json.dumps({
    "hook_event_name": "PostToolUse", "tool_name": "AskUserQuestion",
    "session_id": "S1", "cwd": os.environ["CA_CWD"],
    "tool_input": {"questions": [
        {"question": "Apple signing and developer enrollment: which way?",
         "options": [{"label": "Individual", "description": "enroll now"}]},
        {"question": "The call-transcription default: switch or hold?",
         "options": [{"label": "Switch", "description": "change the default"}]},
    ]},
}))
')"
run_hook "$WITNESS" "$MULTI"
if [ "$(grep -c '"event": "CeoAsk"' "$LEDGER" 2>/dev/null || echo 0)" -eq 2 ] \
   && grep -q '"matched_item": "1.1"' "$LEDGER" && grep -q '"matched_item": "1.2"' "$LEDGER"; then
    ok "B5. a two-question call is witnessed as two records, one per item"
else
    bad "B5. a two-question call is witnessed as two records, one per item" "$(cat "$LEDGER" 2>/dev/null)"
fi

echo ""
echo "=== C. nothing refused; whether he was asked is the session's verdict ==="

# asked <session> [app run] -> RC 0 when this session (or app run) has put an item to him
asked() {
    OUT="$(env -u RICHOS_OPERATOR_LEAD ${2:+RICHOS_OPERATOR_LEAD=$2} RICHOS_ENTITY_ROOT="$SEAT" bash "$STATUS" "$SEAT" --session "$1" 2>&1)"
    RC=$?
    return 0
}

reset_ledger
run_hook "$GATE" "$(agent_payload S1 "do the thing")"
say "C1" "$ERR"
if [ "$RC" -eq 0 ] && [ -z "$ERR" ]; then
    ok "C1. an unasked prepared item refuses NO dispatch (orchestration reliability R3) and adds nothing to it"
else
    bad "C1. an unasked prepared item refuses NO dispatch" "rc=$RC $ERR"
fi

reset_ledger
run_hook "$WITNESS" "$(ask_payload S1 - "$Q_SIGNING" "$L_SIGNING" "$D_SIGNING")"
asked S1
if [ "$RC" -eq 0 ]; then
    ok "C2. one prepared item actually asked makes the session's verdict SATISFIED"
else
    bad "C2. one prepared item actually asked makes the session's verdict SATISFIED" "rc=$RC $OUT"
fi

# ===========================================================================
# C3 — THE ANTI-GAMING CASE. Everything else is plumbing next to this one.
# ===========================================================================
reset_ledger
run_hook "$WITNESS" "$(ask_payload S1 - "What should we work on next?" "Ship it" "get moving")"
asked S1
if [ "$RC" -eq 1 ]; then
    ok "C3. A JUNK QUESTION DISCHARGES NOTHING — the session is still OPEN after it"
else
    bad "C3. A JUNK QUESTION DISCHARGES NOTHING — the session is still OPEN after it" "rc=$RC"
fi

reset_ledger
run_hook "$WITNESS" "$(ask_payload S1 agent-7 "$Q_SIGNING" "$L_SIGNING" "$D_SIGNING")"
asked S1
if [ "$RC" -eq 1 ]; then
    ok "C4. a WORKER's matching question does not count as the lead asking him"
else
    bad "C4. a WORKER's matching question does not count as the lead asking him" "rc=$RC"
fi

reset_ledger
run_hook "$WITNESS" "$(ask_payload S0 - "$Q_SIGNING" "$L_SIGNING" "$D_SIGNING")"
asked S1
if [ "$RC" -eq 1 ]; then
    ok "C5. a question asked in ANOTHER session does not count for this one"
else
    bad "C5. a question asked in ANOTHER session does not count for this one" "rc=$RC"
fi

# C5b-C5g: THE APP RUN (operator contract notes §4 item 2, Frank's F10). In the
# RichOS app each conversation is its own lead and session. S0 below is one
# conversation's lead, S1 another's, in the same app run app-run-1.
reset_ledger
run_hook "$WITNESS" "$(app_ask_payload S0 app-run-1 q-1)"
if grep -q '"app_run": "app-run-1"' "$LEDGER" 2>/dev/null && grep -q '"discharges": true' "$LEDGER"; then
    ok "C5b. the app's display witness records the app run beside the session"
else
    bad "C5b. the app's display witness records the app run beside the session" "$(cat "$LEDGER" 2>/dev/null)"
fi
asked S1 app-run-1
if [ "$RC" -eq 0 ]; then
    ok "C5c. an ask made once in an app run counts for ANOTHER conversation's lead in that run"
else
    bad "C5c. an ask made once in an app run counts for ANOTHER conversation's lead in that run" "rc=$RC $OUT"
fi
asked S1 app-run-2
if [ "$RC" -eq 1 ]; then
    ok "C5d. an ask from an EARLIER app run counts for nothing in this one"
else
    bad "C5d. an ask from an EARLIER app run counts for nothing in this one" "rc=$RC"
fi
asked S1
if [ "$RC" -eq 1 ]; then
    ok "C5e. a session outside the app (no app run) is still keyed to its own session"
else
    bad "C5e. a session outside the app (no app run) is still keyed to its own session" "rc=$RC"
fi
reset_ledger
run_hook_in_run app-run-1 "$WITNESS" "$(app_ask_payload S0 app-run-1 q-2)"
OUT="$(cd "$SEAT" && RICHOS_OPERATOR_LEAD=app-run-1 RICHOS_ENTITY_ROOT="$SEAT" bash "$SSTART" </dev/null 2>/dev/null)"
if [ -z "$OUT" ]; then
    ok "C5g. a new conversation's lead in the same run is not told to put the same item to him again"
else
    bad "C5g. a new conversation's lead in the same run is not told to put the same item to him again" "$OUT"
fi

reset_ledger
run_hook "$GATE" "$(CA_CWD="$SEAT" python3 -c '
import json, os
print(json.dumps({"hook_event_name":"PreToolUse","tool_name":"Bash","session_id":"S1",
                  "cwd": os.environ["CA_CWD"], "tool_input":{"command":"ls"}}))')"
if [ "$RC" -eq 0 ]; then
    ok "C8. a payload for another tool passes untouched"
else
    bad "C8. a payload for another tool passes untouched" "rc=$RC $ERR"
fi

# --- the three states, and the two of them that must NOT block --------------
write_seat_config ""
reset_ledger
run_hook "$GATE" "$(agent_payload S1 "do the thing")"
if [ "$RC" -eq 0 ] && [ -z "$ERR" ]; then
    ok "C9. NOT-DECLARED: a seat with no CEO list stands down, silently"
else
    bad "C9. NOT-DECLARED: a seat with no CEO list stands down, silently" "rc=$RC err=$ERR"
fi

write_seat_config "$SANDBOX/no-such-repo"
run_hook "$GATE" "$(agent_payload S1 "do the thing")"
say "C10" "$ERR$OUT"
if [ "$RC" -eq 0 ] && printf '%s%s' "$ERR" "$OUT" | grep -q 'UNGATED'; then
    ok "C10. BROKEN: a declared list that is not on disk FAILS OPEN and says so"
else
    bad "C10. BROKEN: a declared list that is not on disk FAILS OPEN and says so" "rc=$RC err=$ERR out=$OUT"
fi

# P5-50: an UNREADABLE ledger is not an EMPTY one. A ledger path that cannot be
# opened as a file (here a directory) used to read as "no question was ever asked",
# turning every prepared item into a fresh obligation; it must be BROKEN instead,
# which fails open and says so, like a list that is not on disk.
write_seat_config "$TODOREPO"
write_record
rm -rf "$LEDGER"; mkdir -p "$LEDGER"
run_hook "$GATE" "$(agent_payload S1 "do the thing")"
say "C10b" "$ERR$OUT"
if [ "$RC" -eq 0 ] && printf '%s%s' "$ERR" "$OUT" | grep -q 'UNGATED'; then
    ok "C10b. BROKEN: an unreadable ledger FAILS OPEN and says so, it is not read as an empty ledger (P5-50)"
else
    bad "C10b. BROKEN: an unreadable ledger FAILS OPEN and says so" "rc=$RC err=$ERR out=$OUT"
fi
rm -rf "$LEDGER"
reset_ledger

write_seat_config "$TODOREPO"
write_record "BLOCKED-ON-RICH"
# every item unprepared -> nothing to ask about
python3 - "$TODOREPO/wiki/open-items.md" <<'PY'
import re, sys
p = sys.argv[1]
s = open(p, encoding="utf-8").read()
s = s.replace("READY-FOR-CEO", "BLOCKED-ON-RICH")
open(p, "w", encoding="utf-8").write(s)
PY
reset_ledger
run_hook "$GATE" "$(agent_payload S1 "do the thing")"
asked S1
if [ "$RC" -eq 0 ]; then
    ok "C11. NOTHING PREPARED: a list of unprepared items asks for nothing"
else
    bad "C11. NOTHING PREPARED: a list of unprepared items asks for nothing" "rc=$RC $OUT"
fi
write_record

echo ""
echo "=== D. the Stop notice: a turn does not end quietly ==="

reset_ledger
run_hook "$STOPN" "$(stop_payload S1)"
say "D1" "$OUT"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | python3 -c 'import json,sys
d = json.loads(sys.stdin.readline() or "{}")
h = d.get("hookSpecificOutput") or {}
sys.exit(0 if h.get("hookEventName") == "Stop" and "systemMessage" not in d
         and "has not been put to him" in h.get("additionalContext", "")
         and "1.1" in h.get("additionalContext", "")
         and "next reply" in h.get("additionalContext", "") else 1)'; then
    ok "D1. an unasked item reaches the LEAD's model input at turn end, named, to ask in its next reply (R3); nothing for the person"
else
    bad "D1. an unasked item reaches the LEAD's model input at turn end, named" "rc=$RC $OUT"
fi
if printf '%s' "$OUT" | grep -q '?'; then
    ok "D1b. it is rendered as a QUESTION, not as a count"
else
    bad "D1b. it is rendered as a QUESTION, not as a count" "$OUT"
fi

run_hook "$STOPN" "$(stop_payload S1)"
if [ -z "$OUT" ]; then
    ok "D2. the same unchanged set is announced ONCE, not under every turn"
else
    bad "D2. the same unchanged set is announced ONCE, not under every turn" "$OUT"
fi

run_hook "$WITNESS" "$(ask_payload S1 - "$Q_SIGNING" "$L_SIGNING" "$D_SIGNING")"
run_hook "$STOPN" "$(stop_payload S1)"
if printf '%s' "$OUT" | grep -q '1\.2'; then
    ok "D3. asking one item changes the set, so the notice speaks again with the next one"
else
    bad "D3. asking one item changes the set, so the notice speaks again with the next one" "$OUT"
fi

# D5-D6 (orchestration reliability R3, A1): a question put in the REPLY is an ask
# (CLAUDE.md §125), and an item already asked stays quiet.
reset_ledger
REPLY_PAYLOAD="$(CA_CWD="$SEAT" CA_Q="$Q_SIGNING $L_SIGNING: $D_SIGNING. Default meanwhile: I keep signing as it is." python3 -c '
import json, os
print(json.dumps({"hook_event_name": "Stop", "session_id": "S7", "cwd": os.environ["CA_CWD"],
                  "stop_hook_active": False, "last_assistant_message": os.environ["CA_Q"]}))')"
run_hook "$STOPN" "$REPLY_PAYLOAD"
asked S7
if [ "$RC" -eq 0 ] && grep -q '"source_hook": "Stop\[reply\]"' "$LEDGER" 2>/dev/null \
   && grep -q '"matched_item": "1.1"' "$LEDGER"; then
    ok "D5. his prepared question asked in the reply, with its default, is witnessed as the ask (§125)"
else
    bad "D5. his prepared question asked in the reply is witnessed as the ask" "rc=$RC $(cat "$LEDGER" 2>/dev/null)"
fi
run_hook "$STOPN" "$(stop_payload S7)"
if [ "$RC" -eq 0 ] && ! printf '%s' "$OUT" | grep -q '1\.1'; then
    ok "D6. an item already asked is not asked again"
else
    bad "D6. an item already asked is not asked again" "rc=$RC $OUT"
fi
reset_ledger
JUNK_REPLY="$(CA_CWD="$SEAT" python3 -c '
import json, os
print(json.dumps({"hook_event_name": "Stop", "session_id": "S8", "cwd": os.environ["CA_CWD"],
                  "stop_hook_active": False, "last_assistant_message": "What should we work on next?"}))')"
run_hook "$STOPN" "$JUNK_REPLY"
asked S8
if [ "$RC" -eq 1 ]; then
    ok "D7. POSITIVE CONTROL: a reply asking an unrelated question asks nothing of his list"
else
    bad "D7. a reply asking an unrelated question asks nothing" "rc=$RC"
fi

write_seat_config "$SANDBOX/no-such-repo"
reset_ledger
run_hook "$STOPN" "$(stop_payload S1)"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q 'UNGATED'; then
    ok "D4. a declared-but-unreadable list is announced LOUDLY at turn end, and never blocks"
else
    bad "D4. a declared-but-unreadable list is announced LOUDLY at turn end, and never blocks" "rc=$RC $OUT"
fi
write_seat_config "$TODOREPO"

echo ""
echo "=== E. session start: his question, not a count of his questions ==="

reset_ledger
OUT="$(cd "$SEAT" && RICHOS_ENTITY_ROOT="$SEAT" bash "$SSTART" </dev/null 2>"$SANDBOX/err")"
RC=$?
say "E1" "$OUT"
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q 'systemMessage' \
   && printf '%s' "$OUT" | grep -q 'additionalContext'; then
    ok "E1. it announces on BOTH channels — the operator's and the model's"
else
    bad "E1. it announces on BOTH channels — the operator's and the model's" "rc=$RC $OUT"
fi
if printf '%s' "$OUT" | grep -q '1\.1' && printf '%s' "$OUT" | grep -q 'Apple signing'; then
    ok "E2. it names the top item rather than counting the items"
else
    bad "E2. it names the top item rather than counting the items" "$OUT"
fi

if printf '%s' "$OUT" | grep -q 'PUT THIS TO THE CEO IN YOUR NEXT REPLY' \
   && printf '%s' "$OUT" | grep -q 'still current' \
   && printf '%s' "$OUT" | grep -q 'Nothing is refused meanwhile' \
   && printf '%s' "$OUT" | grep -q 'cancellation as an answer' \
   && printf '%s' "$OUT" | grep -q 'complete current work queue' \
   && printf '%s' "$OUT" | grep -q 'on 2026-08-31' \
   && ! printf '%s' "$OUT" | grep -q 'ceo-todos-deferred'; then
    ok "E4. startup asks for the question in the next reply, checked current first, and refuses nothing (R3)"
else
    bad "E4. startup ask contract" "$OUT"
fi

write_seat_config ""
OUT="$(cd "$SEAT" && RICHOS_ENTITY_ROOT="$SEAT" bash "$SSTART" </dev/null 2>/dev/null)"
if [ -z "$OUT" ]; then
    ok "E3. a repository with no CEO list is announced not at all"
else
    bad "E3. a repository with no CEO list is announced not at all" "$OUT"
fi
write_seat_config "$TODOREPO"

echo ""
echo "=== F. the CLI face gives the same verdict the hooks give ==="

reset_ledger
OUT="$(bash "$STATUS" "$SEAT" 2>&1)"; RC=$?
if [ "$RC" -eq 1 ] && printf '%s' "$OUT" | grep -q 'verdict    : OPEN' \
   && printf '%s' "$OUT" | grep -q 'refuses nothing' && ! printf '%s' "$OUT" | grep -q 'ceo-todos-deferred:'; then
    ok "F1. ceo-asks-status.sh exits 1 and reports OPEN when he has not been asked"
else
    bad "F1. ceo-asks-status.sh exits 1 and reports OPEN when he has not been asked" "rc=$RC $OUT"
fi
run_hook "$WITNESS" "$(ask_payload S1 - "$Q_SIGNING" "$L_SIGNING" "$D_SIGNING")"
OUT="$(bash "$STATUS" "$SEAT" --session S1 2>&1)"; RC=$?
if [ "$RC" -eq 0 ] && printf '%s' "$OUT" | grep -q 'verdict    : SATISFIED'; then
    ok "F2. ...and 0/SATISFIED for the session in which he was"
else
    bad "F2. ...and 0/SATISFIED for the session in which he was" "rc=$RC $OUT"
fi

echo ""
echo "=== G. the wiring: registered, hashed, and rooted like its siblings ==="

for h in notice-ceo-asks.sh guard-ceo-ask-first.sh notice-ceo-unasked.sh session-start-ceo-ask.sh; do
    # `|| echo 0` would be wrong here: grep -c already PRINTS 0 before exiting 1,
    # so the fallback appends a second line and `[` sees "0\n0".
    N="$(grep -c "scripts/hooks/$h" "$ENGINE_SRC/hooks/hooks.json" 2>/dev/null || true)"
    N="${N:-0}"
    if [ "$N" -eq 1 ]; then
        ok "G1. $h is registered exactly once in hooks/hooks.json"
    else
        bad "G1. $h is registered exactly once in hooks/hooks.json" "found $N"
    fi
done

for h in notice-ceo-asks guard-ceo-ask-first notice-ceo-unasked session-start-ceo-ask; do
    if grep -q "$h" "$SRC_DIR/contract-integrity-probe.sh" 2>/dev/null; then
        ok "G2. $h is declared in contract-integrity-probe.sh"
    else
        bad "G2. $h is declared in contract-integrity-probe.sh" "absent from the probe's inventory"
    fi
done

# The bootstrap block, byte-identical modulo the hook name and the exit code —
# the same normalization Layer R applies. Asserted here too, so a divergence is
# caught by the suite that owns these files and not only by the probe.
REF="$(sed -n '/^# --- ROOT RESOLUTION ---/,/^ENGINE_ROOT="\$(resolve_engine_root/p' \
        "$SRC_DIR/notice-unstarted-rows.sh" \
        | sed -e 's|scripts/hooks/[a-z-]*\.sh|<HOOK>|' -e 's|^    exit [0-9]*$|    exit <RC>|')"
for h in notice-ceo-asks.sh guard-ceo-ask-first.sh notice-ceo-unasked.sh session-start-ceo-ask.sh; do
    BLK="$(sed -n '/^# --- ROOT RESOLUTION ---/,/^ENGINE_ROOT="\$(resolve_engine_root/p' "$SRC_DIR/$h" \
            | sed -e 's|scripts/hooks/[a-z-]*\.sh|<HOOK>|' -e 's|^    exit [0-9]*$|    exit <RC>|')"
    if [ "$BLK" = "$REF" ]; then
        ok "G3. $h carries the byte-identical root-resolution bootstrap"
    else
        bad "G3. $h carries the byte-identical root-resolution bootstrap" "diverged"
    fi
done

for f in scripts/lib/ceo-asks.sh scripts/lib/ceo-asks.py; do
    if grep -q "$(basename "$f")" "$SRC_DIR/install.sh" 2>/dev/null; then
        ok "G4. $f is sidecar-hashed by install.sh"
    else
        bad "G4. $f is sidecar-hashed by install.sh" "install.sh does not name it, so the integrity probe cannot notice it changing"
    fi
done

# --- THE MUTATION HARNESS RUNS FROM THE SUITE IT MUTATES -------------------
# Until 2026-09-05 it ran from NOTHING. run-all-tests.sh discovers *.test.sh;
# contract-integrity.test.sh names a hand-typed set of harnesses that never
# included this one; and the only other mention of the file anywhere in the
# repository was a comment. So every property it proves load-bearing was
# proven exactly when somebody remembered to type its path. Recorded as row
# 3.22 of wiki/open-items.md.
#
# ITS FAILURE IS THIS SUITE'S FAILURE, not a warning. A harness whose result is
# advisory is a harness nobody has to fix.
#
# RICHOS_MUTATION_INNER is the only thing standing between this and an infinite
# regress: the harness EXPORTS it before running any copy of this suite, so the
# inner run skips this block. Never remove one half without the other.
# A MISSING HARNESS IS A FAILURE, NOT A SKIP, and that sentence was earned
# forty minutes ago rather than reasoned. The first draft of this block wrote
# the directory variable with one dollar sign too many, so the path expanded to
# the shell PID followed by the variable name, the test was false, the harness was
# skipped in silence and the suite printed a full green tally in 3.7 seconds.
# That is the exact defect this whole row is about, rebuilt inside the fix for
# it, and only the WALL CLOCK gave it away. So the guard now says so out loud.
if [ -z "${RICHOS_MUTATION_INNER:-}" ]; then
    echo ""
    echo "=== running the mutation harness: ceo-asks.mutation.sh ==="
    if [ -x "$SRC_DIR/ceo-asks.mutation.sh" ]; then
        "$SRC_DIR/ceo-asks.mutation.sh" || FAIL=$((FAIL + 1))
    else
        echo "  FAIL  MUT. ceo-asks.mutation.sh is missing or not executable — IT DID NOT RUN"
        FAIL=$((FAIL + 1))
    fi
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    echo "  $PASS/$PASS cases passed"
    exit 0
fi
echo "  $PASS passed, $FAIL FAILED"
exit 1
