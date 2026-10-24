#!/usr/bin/env bash
#
# unasked-deferral.test.sh — the unasked-deferral notice, end to end, in a
#                            sandbox, driven through the SHIPPED hook.
#
# WHAT IS ASSERTED, AND WHY EACH CASE EXISTS
#   1. THE SPECIMEN. The verbatim 2026-08-31 turn — the one the CEO had to ask a
#      third time about — produces a notice that QUOTES the construction.
#   2. THE THREE DISCHARGES. AskUserQuestion in the turn; the CEO's hand visible
#      in the text; an Agent spawn in the turn. Each silences the SAME specimen
#      text, so the silence is provably the discharge and not a hook that never
#      ran (case 2d re-runs the specimen bare and demands the notice back).
#   3. ORDINARY SEQUENCING PROSE IS SILENT. Real sentences from the corpus that
#      describe a PIPELINE — "the deploy runs after the merge", "Ray goes after
#      Tom", "I'll report when it's done" — must produce nothing. This is the
#      precision half and it is the reason the family list is short.
#   4. QUOTED IS NOT USED. A turn that QUOTES the trigger phrases — which is
#      what a turn describing this guard does — stays silent, with a positive
#      control proving the same words unquoted still fire.
#   5. IT REPORTS, NEVER BLOCKS. Exit 0 in every case above.
#   6. IT CAN BE SEEN GOING QUIET. Stood down, no analyzer, unresolvable root —
#      each says so on the systemMessage channel rather than dying silently.
#   7. STATE-CHANGE DE-DUPLICATION. The same deferral restated is announced
#      once; a DIFFERENT one speaks again.
#
# NEGATIVE CONTROLS ARE MANDATORY HERE. A suite for a regex guard passes
# trivially if the matcher is broken — everything is silent and every silence
# case is green. So every silence assertion in section 3 is paired with the
# specimen firing in the same run, and section 1 runs first.
#
# Run directly:  scripts/hooks/unasked-deferral.test.sh [--verbose]
# Exit 0 = all green.

set -uo pipefail

VERBOSE=0
[ "${1:-}" = "--verbose" ] && VERBOSE=1

SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SRC_DIR/../.." && pwd)"

PASS=0
FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '         %s\n' "$2"; FAIL=$((FAIL + 1)); }
say() { [ "$VERBOSE" -eq 1 ] && printf '\n----- %s -----\n%s\n' "$1" "$2"; return 0; }

command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

SANDBOX="$(cd "$(mktemp -d -t unasked-deferral.XXXXXX)" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT

REPO="$SANDBOX/repo"
mkdir -p "$REPO/scripts/hooks" "$REPO/scripts/lib"

# A copy, not a symlink: the mutation suite rewrites one of these.
cp "$SRC_DIR/notice-unasked-deferral.sh" "$REPO/scripts/hooks/"
cp "$SRC_DIR/guard-unasked-deferral.py" "$REPO/scripts/hooks/"
chmod +x "$REPO/scripts/hooks/notice-unasked-deferral.sh"
for l in resolve-roots.sh resolve-main-checkout.sh seat-jurisdiction.sh \
         stop-hook-notice.sh hold_words.py; do
    cp "$SRC_DIR/../lib/$l" "$REPO/scripts/lib/$l" 2>/dev/null || true
done
# A finding records nothing (R1). The registry library sits where the old hook
# found it, so a regression that opens a hold again shows up in the sandbox's own
# registry, which must stay empty.
mkdir -p "$REPO/mega-lander"
cp "$SRC_DIR/../../mega-lander/workspaces.py" "$REPO/mega-lander/"
export RICHOS_WORKSPACES_DIR="$SANDBOX/workspaces"

HOOK="$REPO/scripts/hooks/notice-unasked-deferral.sh"

printf 'PROTECTED_PATHS="wiki"\n' > "$REPO/orchestration.config"
echo "# sandbox" > "$REPO/README.md"

git -C "$REPO" init -q -b main
git -C "$REPO" config user.email "$(git config user.email 2>/dev/null || echo tester@example.invalid)"
git -C "$REPO" config user.name "$(git config user.name 2>/dev/null || echo tester)"
git -C "$REPO" add -A >/dev/null 2>&1
git -C "$REPO" commit -q -m "base" >/dev/null 2>&1

export RICHOS_ENTITY_ROOT="$REPO"

# ===========================================================================
# THE TRANSCRIPT. The hook reads the TURN'S TOOL USE from the transcript file
# named in the payload, exactly as the shipping binary supplies it. So the
# sandbox writes real transcript records rather than stubbing the reader —
# otherwise the three discharges would be tested against a fiction.
# ===========================================================================
write_transcript() { # <tool-name>...
    python3 - "$SANDBOX/transcript.jsonl" "$@" <<'PYEOF'
import json, sys
path, tools = sys.argv[1], sys.argv[2:]
recs = [
    {"type": "user", "isSidechain": False,
     "message": {"role": "user", "content": "do the thing"}},
]
for n, t in enumerate(tools):
    # "!Name" is a call the host REFUSED: its result is an error.
    refused = t.startswith("!")
    cid = "x%d" % n
    recs.append({"type": "assistant", "isSidechain": False,
                 "message": {"role": "assistant", "content": [
                     {"type": "tool_use", "name": t.lstrip("!"), "id": cid, "input": {}}]}})
    result = {"type": "tool_result", "tool_use_id": cid, "content": "ok"}
    if refused:
        result.update({"is_error": True, "content": "Refused before dispatch"})
    recs.append({"type": "user", "isSidechain": False,
                 "message": {"role": "user", "content": [result]}})
with open(path, "w", encoding="utf-8") as fh:
    for r in recs:
        fh.write(json.dumps(r) + "\n")
PYEOF
}

stop_payload() { # <message>
    python3 -c '
import json, sys
print(json.dumps({"hook_event_name": "Stop",
                  "session_id": "deadbeef-1111-4000-8000-000000000000",
                  "cwd": "", "stop_hook_active": False,
                  "transcript_path": sys.argv[1],
                  "last_assistant_message": sys.argv[2],
                  "background_tasks": [], "session_crons": []}))' \
        "$SANDBOX/transcript.jsonl" "$1"
}

forget() { rm -rf "$REPO/.claude/state/stop-hook-notices"; }

run_hook() { # <message> [--remember]
    local msg="$1"
    [ "${2:-}" = "--remember" ] || forget
    HOUT="$(stop_payload "$msg" | bash "$HOOK" 2>&1)"
    HRC=$?
}

spoke()  { printf '%s' "$HOUT" | grep -q '"additionalContext":"DEFERRAL CHECK (unverified'; }
quiet()  { ! spoke; }
names()  { printf '%s' "$HOUT" | grep -q "$1"; }

# --- THE SPECIMEN, verbatim from transcript 042f3850 -----------------------
SPECIMEN="**When:** I'm deliberately not spawning a fifth agent for it right now. Four are running, you're waiting to restart, and adding a fifth pushes that further out for a guard that isn't the thing you're waiting on. It goes in with the dialect guard's land — next spawn after these finish."

echo "=== the unasked deferral: the specimen, the discharges, and the silences ==="

# ===========================================================================
# 1. THE SPECIMEN FIRES, AND QUOTES ITSELF
# ===========================================================================
write_transcript Bash
run_hook "$SPECIMEN"

if spoke; then ok "1a  the 2026-08-31 specimen turn ends with a notice"
else bad "1a  the 2026-08-31 specimen turn ends with a notice" "the hook said nothing: $HOUT"; fi
say "1a" "$HOUT"

if names "deliberately not spawning"; then
    ok "1b  the notice QUOTES the construction it matched, not a count"
else bad "1b  the notice QUOTES the construction it matched" "$HOUT"; fi

if names "check his original request"; then
    ok "1c  the notice tells the lead what to do: check his original request, then act or drop it"
else bad "1c  the notice names the lead's next action" "$HOUT"; fi

if [ "$HRC" -eq 0 ]; then ok "1d  it REPORTS, it does not block — exit 0"
else bad "1d  it REPORTS, it does not block — exit 0" "rc=$HRC"; fi

# The other two families, each on its own, so a single broken pattern cannot
# hide behind the specimen matching four at once.
write_transcript Bash
run_hook "I'm queueing the fix rather than dispatching it now: it lives in intake_sweep, which the Eric Masi work currently owns."
if spoke; then ok "1e  family C alone fires — 'rather than dispatching it now'"
else bad "1e  family C alone fires" "$HOUT"; fi

write_transcript Bash
run_hook "Nothing else is outstanding. It goes in with the dialect guard's land."
if spoke; then ok "1f  family B alone fires — bundling into a later land"
else bad "1f  family B alone fires" "$HOUT"; fi

# T1 (2026-10-24, orchestration reliability R1, richos-hq docs/prds/2026-10-23-
# orchestration-reliability.md): a match is an unverified diagnostic for the LEAD,
# never a work item. The 2026-09-27 15:30Z reply ("Stream B is on hold,
# deliberately.") still reaches the lead, beside a spawn, on the model channel,
# and nothing is recorded anywhere: no hold file, no person-facing message.
no_record() { [ -z "$(ls -A "$RICHOS_WORKSPACES_DIR" 2>/dev/null)" ]; }
write_transcript Agent Bash
run_hook "Stream A is running. Stream B is on hold, deliberately."
if spoke && names "Stream B is on hold, deliberately." && no_record \
        && ! printf '%s' "$HOUT" | grep -q systemMessage; then
    ok "T1a 'Stream B is on hold, deliberately.' reaches the lead (additionalContext), beside a spawn, and records nothing"
else bad "T1a the 2026-09-27 reply reaches the lead and records nothing" "out=$HOUT ws=$(ls -R "$RICHOS_WORKSPACES_DIR" 2>/dev/null)"; fi
# The reply that answers the diagnostic is its continuation, not fresh input: the
# host appends our additionalContext as a hook_additional_context attachment.
python3 - "$SANDBOX/transcript.jsonl" <<'PYEOF'
import json, sys
with open(sys.argv[1], "a", encoding="utf-8") as fh:
    fh.write(json.dumps({"type": "attachment", "attachment": {"type": "hook_additional_context",
             "hookName": "Stop", "content": ["DEFERRAL CHECK (unverified, report only): this reply says ..."]}}) + "\n")
PYEOF
run_hook "Checked: Stream B is parked by his own order of 09-27, so nothing to record."
if quiet && no_record; then ok "T1b the lead's answer to the diagnostic is not read again: no second diagnostic, no recursion"
else bad "T1b the continuation is never fresh detector input" "$HOUT"; fi
# A choice put to him is his: no diagnostic and nothing recorded.
write_transcript Bash
run_hook "Spelling work is on hold, deliberately. Your call: run it now or keep holding?"
if quiet && no_record; then ok "T1c a hold handed back to him as his choice: no diagnostic, nothing recorded"
else bad "T1c a handed-back hold is quiet" "out=$HOUT"; fi
write_transcript Bash
run_hook "Spelling work is on hold, deliberately. The sign-in fix is parked."
if spoke && no_record; then ok "T1d two held pieces in one reply: one diagnostic, nothing recorded"
else bad "T1d two held pieces: one diagnostic, nothing recorded" "out=$HOUT"; fi
write_transcript Bash
run_hook "You killed Stream B, so Stream B is on hold."
if quiet && no_record; then ok "T1e his own order, cited: quiet, nothing recorded"
else bad "T1e a cited order of his is quiet" "out=$HOUT"; fi
# 2026-10-23 14:57:12Z: a retrospective explanation opened held-643a9400.
write_transcript Bash
run_hook "- Then I parked everything behind the nightly and the stable."
if no_record; then ok "T1f a retrospective explanation creates no work record (held-643a9400 specimen)"
else bad "T1f a retrospective explanation creates no work record" "ws=$(ls -R "$RICHOS_WORKSPACES_DIR")"; fi

# T1g (orchestration reliability R7): only the current turn is read. A 12 MB history
# whose earlier turn spawned an Agent, then this turn (Bash only): the spawn of an
# earlier turn discharges nothing, and well under 1 MB of the file is read.
python3 - "$SANDBOX/transcript.jsonl" <<'PYEOF'
import json, sys
pad = "x" * 2000
with open(sys.argv[1], "w", encoding="utf-8") as fh:
    fh.write(json.dumps({"type": "user", "message": {"role": "user", "content": "start the stream"}}) + "\n")
    fh.write(json.dumps({"type": "assistant", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "name": "Agent", "id": "old1", "input": {}}]}}) + "\n")
    for n in range(6000):
        fh.write(json.dumps({"type": "assistant", "message": {"role": "assistant", "content": [
            {"type": "text", "text": "%d %s" % (n, pad)}]}}) + "\n")
    fh.write(json.dumps({"type": "user", "message": {"role": "user", "content": "and the other stream?"}}) + "\n")
    fh.write(json.dumps({"type": "assistant", "message": {"role": "assistant", "content": [
        {"type": "tool_use", "name": "Bash", "id": "now1", "input": {}}]}}) + "\n")
    fh.write(json.dumps({"type": "user", "message": {"role": "user", "content": [
        {"type": "tool_result", "tool_use_id": "now1", "content": "ok"}]}}) + "\n")
PYEOF
TAIL_OUT="$(python3 - "$REPO/scripts/hooks/guard-unasked-deferral.py" "$SANDBOX/transcript.jsonl" <<'PYEOF'
import importlib.util, os, sys
spec = importlib.util.spec_from_file_location("deferral", sys.argv[1])
d = importlib.util.module_from_spec(spec); spec.loader.exec_module(d)
tools = d._tools_of_turn({"transcript_path": sys.argv[2]})
print(sorted(tools), d._READ["bytes"] < 1024 * 1024, os.path.getsize(sys.argv[2]) > 10 * 1024 * 1024)
PYEOF
)"
run_hook "Stream B is on hold, deliberately."
if [ "$TAIL_OUT" = "['Bash'] True True" ] && spoke; then
    ok "T1g only the current turn of a 12 MB transcript is read; an earlier turn's spawn discharges nothing"
else bad "T1g the current turn alone is read" "tail=$TAIL_OUT out=$HOUT"; fi

# T2 (2026-10-23): sentences that say nothing is held each opened a hold only his words
# could end (held-3c51602a, held-77933bd6, held-a2aa88fd). Verbatim from the lead's replies.
no_hold_case() { # <label> <text> <why>
    write_transcript Bash
    run_hook "$2"
    if quiet && no_record; then ok "$1 is quiet and records nothing: $3"
    else bad "$1 is quiet and records nothing: $3" "out=$HOUT"; fi
}
no_hold_case T2a "Nothing of yours is on hold." "a denial of a hold"
no_hold_case T2b 'The tracker repeated the "hold" it logged from my first message today; none of your work is on hold.' \
    "'none of your work is on hold' denies one"
no_hold_case T2c "Codex is still looking into why the end-of-turn output got so long, and I'm not starting anything alongside that." \
    "'not starting anything alongside' names no work held"
# The positive control: a denial beside a real hold still reaches the lead, and
# work named after "anything" is still work.
write_transcript Bash
run_hook "Nothing new: the latest notice is a repeat about a branch that's on hold. Nothing of yours is on hold."
if spoke && names "a branch that's on hold" && no_record; then
    ok "T2d POSITIVE CONTROL: beside a denial, a real hold still reaches the lead"
else bad "T2d a real hold beside a denial still reaches the lead" "out=$HOUT"; fi
write_transcript Bash
run_hook "I'm not spawning anything for it right now."
if spoke; then ok "T2e POSITIVE CONTROL: 'not spawning anything for it right now' still fires"
else bad "T2e work named after 'anything' still fires" "$HOUT"; fi
# Second review rv-20261023T204623Z-591cbe58-c2b4: #1, work named after "anyone else" /
# "anything new" is still a deferral; #2, denials written the normal ways open nothing.
for t in "T2f|I'm not spawning anyone else for the sign-in review right now." \
         "T2g|I'm not starting anything new on your requested sign-in fix today."; do
    write_transcript Bash
    run_hook "${t#*|}"
    if spoke && no_record; then ok "${t%%|*} work named after the qualifier still reaches the lead: ${t#*|}"
    else bad "${t%%|*} work named after the qualifier still reaches the lead: ${t#*|}" "out=$HOUT"; fi
done
no_hold_case T2h "Nothing you asked for is on hold." "a relative clause before the verb"
no_hold_case T2i "Nothing of yours is **on hold**." "markdown emphasis on the hold word"
no_hold_case T2j "None of your work is being put on hold." "the passive"

# ===========================================================================
# 2. THE THREE DISCHARGES — same text, silenced three different ways
# ===========================================================================
write_transcript Bash AskUserQuestion
run_hook "$SPECIMEN"
if quiet; then ok "2a  AskUserQuestion in the same turn discharges it — the choice was put to him"
else bad "2a  AskUserQuestion discharges the deferral" "$HOUT"; fi

write_transcript Bash
run_hook "$SPECIMEN Your call — say the word and it goes first."
if quiet; then ok "2b  the CEO's hand visible in the text discharges it — the turn hands the decision back"
else bad "2b  a hand-back in the text discharges the deferral" "$HOUT"; fi

write_transcript Bash
run_hook "You killed the fifth spawn, so I'm not spawning anything further."
if quiet; then ok "2c  a cited CEO order discharges it — 'you killed', 'those are orders'"
else bad "2c  a cited CEO order discharges the deferral" "$HOUT"; fi

write_transcript Agent Bash
run_hook "$SPECIMEN"
if quiet; then ok "2d  an Agent spawn in the same turn discharges it (the KNOWN BLIND SPOT, per the header)"
else bad "2d  an Agent spawn discharges the deferral" "$HOUT"; fi

# Hunt part 3 v3, finding 19: a REFUSED Agent call started nobody, so it
# discharges nothing; the specimen still speaks.
write_transcript '!Agent' Bash
run_hook "$SPECIMEN"
if spoke; then ok "2f  a REFUSED Agent call does not discharge it (P3-19)"
else bad "2f  a REFUSED Agent call does not discharge it (P3-19)" "$HOUT"; fi

# THE POSITIVE CONTROL for all four silences above. Without this, a hook that
# never ran would score 4/4 on section 2.
write_transcript Bash
run_hook "$SPECIMEN"
if spoke; then ok "2e  POSITIVE CONTROL — the same specimen, no discharge, speaks again"
else bad "2e  POSITIVE CONTROL — the specimen still fires without a discharge" \
        "every silence in section 2 is worthless: the hook is not matching at all"; fi

# ===========================================================================
# 3. ORDINARY SEQUENCING PROSE IS SILENT
#    Every line here is a real sentence from the 2,198-turn corpus. They
#    describe a PIPELINE or report PROGRESS; none postpones something the CEO
#    asked for, and a guard that fires on them is noise that gets switched off.
# ===========================================================================
sequencing_case() { # <label> <text>
    write_transcript Bash
    run_hook "$2"
    if quiet; then ok "3$1  silent: $3"
    else bad "3$1  silent: $3" "fired on ordinary prose: $HOUT"; fi
}

sequencing_case a "I'll land Mark's branch once it comes back green. Everything else is independently verified." \
    "progress reporting — 'I'll X once Y comes back green'"
sequencing_case b "From here only landings remain. I'll post the full pause-state summary when it's done." \
    "'when it's done' is a report promise, not a postponement"
sequencing_case c "After those land: wave 2 is Mark aligning the validators to the widened schema, plus the denominator reporting." \
    "'after those land' as roadmap narration"
sequencing_case d "The deploy runs after the merge, and Ray goes after Tom. This lands before that." \
    "the brief's own examples of pipeline prose"
sequencing_case e "The design was right — extending the CSV with a change_type column rather than adding a second ledger." \
    "'rather than adding' as a design choice, with no now-marker"
sequencing_case f "Ends with an apply-on-revival checklist so the fix goes in with the next pilot restart." \
    "'goes in with' whose object is not a unit of orchestration work"
sequencing_case g "That is recorded under the record's Deliberately NOT open section so nobody re-files it." \
    "'deliberately not' as a wiki heading — 23 corpus hits, the reason bare matching was refused"
sequencing_case h "I'm holding the merge until all three report, then landing them as one sequence with a single deploy." \
    "a pipeline hold — the 44% family that was measured and then deleted"

# ===========================================================================
# 4. QUOTED IS NOT USED
# ===========================================================================
write_transcript Bash
run_hook 'It keys on my own language — "I'"'"'m deliberately not spawning", "next spawn after these finish", "that goes in with" — a closed set of constructions.'
if quiet; then ok "4a  a turn QUOTING the trigger phrases is silent — a guard must not fire on its own documentation"
else bad "4a  quoted constructions are not used constructions" "$HOUT"; fi

write_transcript Bash
run_hook "It keys on my own language. I'm deliberately not spawning a fifth agent right now."
if spoke; then ok "4b  POSITIVE CONTROL — the same words UNQUOTED still fire"
else bad "4b  POSITIVE CONTROL — unquoted constructions still fire" \
        "4a is worthless: the matcher is not seeing this text at all"; fi

write_transcript Bash
run_hook 'The header says `I am not spawning it right now` is the shape to look for.'
if quiet; then ok "4c  a backtick span is stripped too — code and quoted spec are not speech"
else bad "4c  backtick spans are stripped" "$HOUT"; fi

# ===========================================================================
# 5. THE STAND-DOWNS ARE VISIBLE
#    A guard that switches itself off silently is the defect this engine keeps
#    re-learning. Each of these must reach the operator's channel.
# ===========================================================================
printf 'PROTECTED_PATHS="wiki"\nCHECK_UNASKED_DEFERRAL=0\n' > "$REPO/orchestration.config"
write_transcript Bash
run_hook "$SPECIMEN"
if printf '%s' "$HOUT" | grep -q 'STOOD DOWN by CHECK_UNASKED_DEFERRAL=0'; then
    ok "5a  stood down by config SAYS SO on the systemMessage channel"
else bad "5a  stood down by config announces itself" "$HOUT"; fi
if [ "$HRC" -eq 0 ]; then ok "5b  a stood-down hook still ends the turn cleanly"
else bad "5b  a stood-down hook still exits 0" "rc=$HRC"; fi
printf 'PROTECTED_PATHS="wiki"\n' > "$REPO/orchestration.config"

mv "$REPO/scripts/hooks/guard-unasked-deferral.py" "$SANDBOX/analyzer.hidden"
write_transcript Bash
run_hook "$SPECIMEN"
if printf '%s' "$HOUT" | grep -q 'analyzer is missing'; then
    ok "5c  a missing analyzer SAYS SO rather than passing the turn quietly"
else bad "5c  a missing analyzer announces itself" "$HOUT"; fi
if [ "$HRC" -eq 0 ]; then ok "5d  a missing analyzer still exits 0 — never a wedged session"
else bad "5d  a missing analyzer still exits 0" "rc=$HRC"; fi
mv "$SANDBOX/analyzer.hidden" "$REPO/scripts/hooks/guard-unasked-deferral.py"

# ROOT FAILURE. RICHOS_ENTITY_ROOT pointing at a path that is not a repository
# is the shape resolve-roots.sh reports as broken rather than not-adopted.
(
    export RICHOS_ENTITY_ROOT="$SANDBOX/no-such-repo"
    forget
    OUT="$(stop_payload "$SPECIMEN" | bash "$HOOK" 2>/dev/null)"
    RC=$?
    if printf '%s' "$OUT" | grep -q 'DEFERRAL WATCH IS OFF'; then
        printf '  PASS  %s\n' "5e  an unresolvable root SAYS SO — a defense that cannot tell which repo it governs is not entitled to be quiet"
    else
        printf '  FAIL  %s\n         %s\n' "5e  an unresolvable root announces itself" "$OUT"
        exit 1
    fi
    [ "$RC" -eq 0 ] || { printf '  FAIL  %s\n' "5f  an unresolvable root still exits 0"; exit 1; }
    printf '  PASS  %s\n' "5f  an unresolvable root still exits 0"
)
if [ $? -eq 0 ]; then PASS=$((PASS + 2)); else FAIL=$((FAIL + 1)); fi

# The positive control for section 5: the hook is healthy again and still fires.
write_transcript Bash
run_hook "$SPECIMEN"
if spoke; then ok "5g  POSITIVE CONTROL — after the stand-downs are undone the notice comes back"
else bad "5g  POSITIVE CONTROL — the hook recovers" "$HOUT"; fi

# ===========================================================================
# 6. STATE-CHANGE DE-DUPLICATION
# ===========================================================================
write_transcript Bash
run_hook "$SPECIMEN"
FIRST="$HOUT"
run_hook "$SPECIMEN" --remember
if quiet; then ok "6a  the SAME deferral restated is announced once, not every turn"
else bad "6a  the same deferral is de-duplicated" "spoke twice: $HOUT"; fi

run_hook "I'm queueing the fix rather than dispatching it now, because two agents in that file is how conflicts start." --remember
if spoke; then ok "6b  a DIFFERENT deferral speaks again — de-duplication is per-deferral, not a mute"
else bad "6b  a different deferral speaks again" "$HOUT"; fi
say "6b" "$HOUT"

# ===========================================================================
# 7. THE TRANSCRIPT IS UNREADABLE -> NO ACCUSATION, AND NO CLAIM TO HAVE LOOKED
#    Two of the three discharges depend on the transcript. If it cannot be
#    read, firing would blame a turn that may well have asked. Staying silent
#    about it is the other error: the turn's text DOES carry a deferral, and
#    nothing was able to decide whether the choice was put to him.
# ===========================================================================
rm -f "$SANDBOX/transcript.jsonl"
run_hook "$SPECIMEN"
if quiet; then ok "7a  an unreadable transcript does NOT accuse a turn whose discharges it cannot see"
else bad "7a  an unreadable transcript does not accuse" "$HOUT"; fi
if printf '%s' "$HOUT" | grep -q 'NOT RUN this turn'; then
    ok "7a2 and it says the check did not run, instead of ending the turn as though it had"
else bad "7a2 an unreadable transcript reports NOT RUN" "$HOUT"; fi
write_transcript Bash
run_hook "$SPECIMEN"
if spoke; then ok "7b  POSITIVE CONTROL — with the transcript back it fires again"
else bad "7b  POSITIVE CONTROL — the transcript path is what 7a turned off" "$HOUT"; fi

# 7c. stop_hook_active — a sibling already blocked this turn; do not pile on.
forget
HOUT="$(python3 -c '
import json, sys
print(json.dumps({"hook_event_name": "Stop", "session_id": "deadbeef-1111-4000-8000-000000000000",
                  "cwd": "", "stop_hook_active": True,
                  "transcript_path": sys.argv[1], "last_assistant_message": sys.argv[2]}))' \
    "$SANDBOX/transcript.jsonl" "$SPECIMEN" | bash "$HOOK" 2>&1)"
if quiet; then ok "7c  stop_hook_active is silent — it does not re-accuse on a blocked turn's re-fire"
else bad "7c  stop_hook_active is silent" "$HOUT"; fi

# ===========================================================================
# 8. THE RECOVERY LINE MUST NEVER CLAIM A CHECK THAT DID NOT HAPPEN
#
#    THE DEFECT, VERBATIM. Given a payload with no `last_assistant_message`,
#    this hook announced:
#
#      "DEFERRAL WATCH — RUNNING AGAIN. This turn's text was checked for work
#       postponed without putting the choice to you."
#
#    It was not checked. There was no turn text to check. Of the forty
#    PreToolUse and Stop hooks driven with empty, truncated and non-JSON
#    payloads on 2026-09-05, this was the ONLY affirmatively false statement
#    found — everywhere else a degraded payload produced silence.
#
#    Every case here is driven through the SAME recovery path the defect used:
#    the hook is first put into an announced stand-down, so that
#    stop_notice_normal's state-change condition is satisfied and the recovery
#    line is the thing under test. Without that setup these cases pass
#    vacuously, because a healthy hook is silent either way — the same
#    "passes for the wrong reason" this suite's positive controls exist for.
# ===========================================================================
stand_down_then() { # <payload-json>  -> HOUT
    # 1. A stand-down the operator is told about, which records a non-ok state.
    forget
    printf 'PROTECTED_PATHS="wiki"\nCHECK_UNASKED_DEFERRAL=0\n' > "$REPO/orchestration.config"
    stop_payload "ordinary prose with nothing deferred in it" | bash "$HOOK" >/dev/null 2>&1
    # 2. The hook is healthy again, and THIS payload is the one under test.
    printf 'PROTECTED_PATHS="wiki"\n' > "$REPO/orchestration.config"
    HOUT="$(printf '%s' "$1" | bash "$HOOK" 2>&1)"
    HRC=$?
}

claims_checked() { printf '%s' "$HOUT" | grep -q "turn's text was checked"; }
says_not_run()   { printf '%s' "$HOUT" | grep -q 'NOT RUN this turn'; }

write_transcript Bash

# 8a. THE POSITIVE CONTROL FIRST. A real turn, with real text, that genuinely
#     defers nothing: the recovery line is TRUE here and must still be said.
#     Without this case, 8b/8c/8d could be satisfied by a hook that had simply
#     stopped emitting the recovery line at all.
stand_down_then "$(stop_payload 'Landed the branch and deployed. Nothing outstanding.')"
if claims_checked; then ok "8a  POSITIVE CONTROL — a turn WITH text still gets the recovery line, because there it is true"
else bad "8a  POSITIVE CONTROL — a real turn still reports RUNNING AGAIN" "$HOUT"; fi

# 8b. THE SPECIMEN OF THE DEFECT: a payload that parses and carries no turn text.
NOTEXT="$(python3 -c '
import json, sys
print(json.dumps({"hook_event_name": "Stop", "session_id": "deadbeef-1111-4000-8000-000000000000",
                  "cwd": "", "stop_hook_active": False, "transcript_path": sys.argv[1]}))' \
    "$SANDBOX/transcript.jsonl")"
stand_down_then "$NOTEXT"
if claims_checked; then bad "8b  a payload with no turn text must NOT say the text was checked" "$HOUT"
else ok "8b  a payload with no turn text does not claim the text was checked"; fi
if says_not_run; then ok "8c  it says the check did NOT RUN, so an unexamined turn cannot read as a clean one"
else bad "8c  a payload with no turn text reports NOT RUN" "$HOUT"; fi

# 8d. An unreadable payload — the other two degraded shapes from the survey.
#     THESE TWO PASS AT THE PREVIOUS COMMIT AS WELL, and the reason is recorded
#     here rather than left to look like evidence it is not: a payload with no
#     readable session_id leaves the notice ledger unset, and stop_notice_normal
#     is silent with no prior state to recover from. They are a regression fence
#     around that structural silence, not proof of the fix. 7a2, 8b and 8c are
#     the cases that are red at the previous commit and green at this one.
for shape in "" '{"hook_event_name":"Stop","last_assis'; do
    stand_down_then "$shape"
    label="$([ -z "$shape" ] && echo 'an EMPTY payload' || echo 'a TRUNCATED payload')"
    if claims_checked; then bad "8d  $label must not claim the text was checked" "$HOUT"
    else ok "8d  $label does not claim the text was checked"; fi
    [ "$HRC" -eq 0 ] || bad "8e  $label still exits 0 — this hook never wedges a session" "rc=$HRC"
done
ok "8e  every degraded payload still exits 0 — the verdict is unchanged, only the silence is"

# --- THE MUTATION HARNESS RUNS FROM THE SUITE IT MUTATES -------------------
# Until 2026-09-05 it ran from NOTHING. run-all-tests.sh discovers *.test.sh;
# contract-integrity.test.sh names a hand-typed set of harnesses that never
# included this one; and the only other mention of the file anywhere in the
# repository was a comment. So every property it proves load-bearing was
# proven exactly when somebody remembered to type its path. Recorded as row
# 3.27 of wiki/open-items.md.
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
    echo "=== running the mutation harness: unasked-deferral.mutation.sh ==="
    if [ -x "$SRC_DIR/unasked-deferral.mutation.sh" ]; then
        "$SRC_DIR/unasked-deferral.mutation.sh" || FAIL=$((FAIL + 1))
    else
        echo "  FAIL  MUT. unasked-deferral.mutation.sh is missing or not executable — IT DID NOT RUN"
        FAIL=$((FAIL + 1))
    fi
fi

echo ""
echo "=== $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
