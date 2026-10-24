#!/usr/bin/env bash
#
# turn-manifest.test.sh — regression tests for the Stop-time TURN MANIFEST
# (scripts/hooks/turn-manifest.sh + .py).
#
# WHAT THIS SUITE IS DEFENDING
#   The manifest's whole value is that its rows are GENERATED from what each
#   tool returned. Every way it could stop being that is a way it becomes a
#   second, more convincing version of the failure it was built to end — a
#   reassuring list that does not describe reality. So the cases below are
#   organized around the four ways that happens: wrong scope, silent
#   truncation, a gap that reads as an absence, and a fixture that proves
#   nothing.
#
#   richos is a public repository. Every fixture reproduces the SHAPE of a
#   real record with invented content; the one exception is the SendMessage
#   result string, which is the tool's own product text and is reproduced
#   because reproducing it exactly IS the test.
#
# Covers:
#   THE POSITIVE CONTROL — the actual failing case
#     (a)  SendMessage returned "Message queued …"  -> the row says queued
#     (a2) …and the manifest does NOT render its success flag as the status
#     (a3) the assistant's own words never enter the manifest
#   THE EMPTY TURN, AND THE GAP THAT MUST NOT LOOK LIKE ONE
#     (b)  zero tool calls -> an EXPLICIT "0 tool calls", never a blank render
#          (the "dispatching it" failure had zero calls; a blank would hide it)
#     (b2) unreadable transcript -> UNAVAILABLE, and NOT "0 tool calls"
#     (b3) absent prompt_id -> UNAVAILABLE, and NOT "0 tool calls"
#   PER-TURN SCOPING — pinned with ONLY the call's position moving
#     (c)  one fixture, one variable: the same Glob call before vs after the
#          turn's prompt record. Before -> absent. After -> present.
#     (c2) an earlier turn's result never pairs with this turn's call
#     (c3) subagent (isSidechain) traffic is never reported as the
#          orchestrator's own
#   STATUSES COME FROM THE RESULTS
#     (d)  is_error -> ERROR + the result's own first line
#     (d2) no matching tool_result -> NO RESULT
#     (d3) plain output -> ok + a measured size
#   TRUNCATION IS ANNOUNCED
#     (e)  40 calls -> 25 rows, header tallies all 40, omission line names the
#          count and the dropped tally
#     (e2) a truncated row says how many characters were removed
#     (e3) a SendMessage past the row cap is STILL visible in the header tally
#   THE NEGATIVE CONTROL
#     (f)  the observation record's records_examined equals the fixture's exact
#          record count — so this suite cannot pass by rendering convincingly
#          from an empty transcript. This session found a scanner reporting
#          CLEAN over an empty corpus and a reporting layer dead for weeks;
#          this case is why there will not be a third.
#     (f2) an empty transcript really does examine 0 records (proves f can
#          distinguish the two, rather than passing on any number)
#   IT NEVER BLOCKS, AND NEVER WEDGES A SESSION
#     (g)  every single case above exits 0 — asserted globally at the end
#     (g2) stop_hook_active -> exit 0, renders NOTHING (cannot loop on itself)
#     (g3) repository has not adopted the engine -> exit 0, renders nothing
#     (g4) unparseable payload -> exit 0
#     (g5) broken install (resolve-roots.sh absent) -> exit 0
#     (g6) SHOW_TURN_MANIFEST=0 -> exit 0 + a VISIBLE stand-down, because a
#          stand-down nobody can see is a defense that decays into a rumour
#   THE OUTPUT CONTRACT
#     (h)  whenever it renders, stdout is valid JSON carrying systemMessage
#          and suppressOutput — the only shape the host will show the operator
#
# Run directly: scripts/hooks/turn-manifest.test.sh
# Exit 0 = all pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOOK="$SCRIPT_DIR/turn-manifest.sh"
unset CLAUDE_PROJECT_DIR

PASS=0
FAIL=0

SANDBOX="$(mktemp -d -t turn-manifest.XXXXXX)"
trap 'rm -rf "$SANDBOX"' EXIT

PID="11111111-2222-3333-4444-555555555555"
EARLIER_PID="00000000-0000-4000-8000-000000000000"

# --- the governed repository ----------------------------------------------
# The adoption marker is all the resolver needs; unlike the sibling claim gate
# this hook never touches an object database, so no git repo is required.
ENTITY="$SANDBOX/entity"
mkdir -p "$ENTITY"
: > "$ENTITY/orchestration.config"

UNADOPTED="$SANDBOX/unadopted"
mkdir -p "$UNADOPTED"

# ===========================================================================
# fixture builder
# ===========================================================================
# make_transcript <outfile> <spec-name>
#
# The specs live in python because a transcript record is a nested JSON object
# and heredoc-ing those by hand is how a fixture ends up not carrying the field
# the test depends on — which is exactly how the sibling guard's turn-scoping
# bug survived for weeks behind a green suite.
make_transcript() {
    python3 "$SANDBOX/fixtures.py" "$1" "$2"
}

cat >"$SANDBOX/fixtures.py" <<'PYEOF'
import json
import sys

PID = "11111111-2222-3333-4444-555555555555"
EARLIER = "00000000-0000-4000-8000-000000000000"

# The tool's own product text, reproduced exactly. Rendering THIS sentence
# instead of the success flag beside it is the entire point of the manifest.
QUEUED = json.dumps({
    "success": True,
    "message": "Message queued for delivery to norm-opus-c1 at its next tool round.",
    "pin": {"id": "a43bfb5c049ccdaec", "name": "norm-opus-c1", "ref": "dcacad"},
})


def prompt(pid):
    return {"type": "user", "promptId": pid,
            "message": {"content": [{"type": "text", "text": "go"}]}}


def use(tid, name, sidechain=False):
    r = {"type": "assistant",
         "message": {"content": [{"type": "tool_use", "id": tid, "name": name, "input": {}}]}}
    if sidechain:
        r["isSidechain"] = True
    return r


def res(tid, content, pid=PID, is_error=False, sidechain=False):
    block = {"type": "tool_result", "tool_use_id": tid, "content": content}
    if is_error:
        block["is_error"] = True
    r = {"type": "user", "promptId": pid, "message": {"content": [block]}}
    if sidechain:
        r["isSidechain"] = True
    return r


def text_res(tid, s, **kw):
    return res(tid, [{"type": "text", "text": s}], **kw)


def spec(name):
    # ---- the motivating turn: the five statuses side by side --------------
    if name == "mixed":
        return [
            prompt(EARLIER),
            use("old1", "Glob"),
            res("old1", "a\nb\nc", pid=EARLIER),
            prompt(PID),
            use("t1", "Bash"),
            res("t1", "line1\nline2"),
            use("t2", "SendMessage"),
            text_res("t2", QUEUED),
            use("t3", "Edit"),
            res("t3", "String to replace not found in file.", is_error=True),
            use("t4", "Agent"),
        ]

    # ---- a turn that ran nothing at all ------------------------------------
    if name == "empty-turn":
        return [prompt(EARLIER), use("old1", "Bash"), res("old1", "x", pid=EARLIER),
                prompt(PID)]

    # ---- THE SCOPING PIN ---------------------------------------------------
    # These two differ in ONE way and one way only: whether the Glob call sits
    # BEFORE or AFTER the record that opens this turn. Same call, same id, same
    # name, same result, same everything else — only its position moves. That
    # is the fixture the sibling guard did not have, and its absence is why a
    # session-wide scope passed as per-turn for weeks.
    if name in ("scope-before", "scope-after"):
        glob_pair = [use("g1", "Glob"),
                     res("g1", "one\ntwo", pid=(EARLIER if name == "scope-before" else PID))]
        tail = [use("t1", "Bash"), res("t1", "ok")]
        if name == "scope-before":
            return [prompt(EARLIER)] + glob_pair + [prompt(PID)] + tail
        return [prompt(EARLIER), prompt(PID)] + glob_pair + tail

    # ---- an earlier turn's result must not pair with this turn's call ------
    # Same tool_use_id on both sides of the boundary. If the reader collected
    # results session-wide it would attach the earlier "ok" to this turn's
    # unanswered call and print a status that never happened.
    if name == "stale-pairing":
        return [
            prompt(EARLIER),
            use("dup", "Bash"),
            res("dup", "FROM-THE-EARLIER-TURN", pid=EARLIER),
            prompt(PID),
            use("dup", "Bash"),
        ]

    # ---- subagent traffic is not the orchestrator's own --------------------
    if name == "sidechain":
        return [
            prompt(PID),
            use("s1", "Bash", sidechain=True),
            res("s1", "subagent output", sidechain=True),
            use("t1", "Read"),
            res("t1", "mine"),
        ]

    # ---- forty calls, so truncation has to say what it dropped -------------
    # The SendMessage is placed LAST, well past the row cap, so the header
    # tally is the only thing that can keep it visible.
    if name == "forty":
        out = [prompt(PID)]
        for i in range(39):
            out.append(use("f%d" % i, "Bash"))
            out.append(res("f%d" % i, "out"))
        out.append(use("f39", "SendMessage"))
        out.append(text_res("f39", QUEUED))
        return out

    # ---- a result long enough to force an announced truncation ------------
    if name == "long-detail":
        return [
            prompt(PID),
            use("t1", "SendMessage"),
            text_res("t1", json.dumps({"success": True, "message": "Q" * 400})),
        ]

    raise SystemExit("unknown fixture spec: %s" % name)


out_path, spec_name = sys.argv[1], sys.argv[2]
records = spec(spec_name)
with open(out_path, "w", encoding="utf-8") as fh:
    for r in records:
        fh.write(json.dumps(r) + "\n")
# The exact record count, printed for the negative control to pin against.
print(len(records))
PYEOF

MIXED_TR="$SANDBOX/mixed.jsonl";            MIXED_N="$(make_transcript "$MIXED_TR" mixed)"
EMPTY_TURN_TR="$SANDBOX/empty-turn.jsonl";  make_transcript "$EMPTY_TURN_TR" empty-turn >/dev/null
BEFORE_TR="$SANDBOX/scope-before.jsonl";    make_transcript "$BEFORE_TR" scope-before >/dev/null
AFTER_TR="$SANDBOX/scope-after.jsonl";      make_transcript "$AFTER_TR" scope-after >/dev/null
STALE_TR="$SANDBOX/stale.jsonl";            make_transcript "$STALE_TR" stale-pairing >/dev/null
SIDE_TR="$SANDBOX/sidechain.jsonl";         make_transcript "$SIDE_TR" sidechain >/dev/null
FORTY_TR="$SANDBOX/forty.jsonl";            make_transcript "$FORTY_TR" forty >/dev/null
LONG_TR="$SANDBOX/long.jsonl";              make_transcript "$LONG_TR" long-detail >/dev/null
EMPTY_FILE_TR="$SANDBOX/empty-file.jsonl";  : > "$EMPTY_FILE_TR"

# ===========================================================================
# payload + runner
# ===========================================================================
payload() { # <transcript> [prompt_id] [stop_hook_active] [assistant-message]
    python3 - "$1" "${2:-$PID}" "${3:-false}" "${4:-I have told him.}" "$ENTITY" <<'PY'
import json, sys
tr, pid, active, msg, cwd = sys.argv[1:6]
p = {
    "hook_event_name": "Stop",
    "session_id": "feedface-0000-4000-8000-000000000000",
    "transcript_path": tr,
    "cwd": cwd,
    "permission_mode": "bypassPermissions",
    "stop_hook_active": active == "true",
    "last_assistant_message": msg,
    "background_tasks": [],
    "session_crons": [],
}
if pid != "NONE":
    p["prompt_id"] = pid
print(json.dumps(p))
PY
}

# run <payload-json> [entity-root]
# Sets LAST_RC, LAST_OUT (raw stdout) and LAST_MSG: the hook's own systemMessage when it said one
# (a stand-down or a root failure), else the manifest `turn-manifest.py --show` renders for the
# same payload. Since 2026-10-24 the hook shows no routine manifest (R6, case h); the
# rendering is still what every case below checks.
# NON_ZERO_EXITS accumulates any non-zero exit, because "it never blocks" is a
# property of the whole suite, not of one case (g).
#
# The payload is delivered through a FILE rather than a pipe. Under `set -o
# pipefail` a hook that exits before reading stdin gives the writing printf an
# EPIPE, and the pipeline's status then belongs to printf — so a perfectly
# well-behaved hook would be recorded here as having exited non-zero. A
# harness defect that reads as a hook defect is worse than no harness.
NON_ZERO_EXITS=""
run() {
    local json="$1" root="${2:-$ENTITY}"
    printf '%s' "$json" > "$SANDBOX/payload.json"
    LAST_OUT="$(RICHOS_ENTITY_ROOT="$root" "$HOOK" < "$SANDBOX/payload.json" 2>/dev/null)"
    LAST_RC=$?
    [ "$LAST_RC" -eq 0 ] || NON_ZERO_EXITS="$NON_ZERO_EXITS rc=$LAST_RC"
    if [ -n "$LAST_OUT" ]; then
        LAST_MSG="$(printf '%s' "$LAST_OUT" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("systemMessage",""))' 2>/dev/null)"
    elif grep -q '"stop_hook_active": true' "$SANDBOX/payload.json"; then
        LAST_MSG=""
    else
        LAST_MSG="$(RICHOS_MANIFEST_ENTITY_ROOT="$SANDBOX/show-log" python3 "${HOOK%.sh}.py" --show < "$SANDBOX/payload.json" 2>/dev/null)"
    fi
}

ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n         %s\n' "$1" "$2"; FAIL=$((FAIL + 1)); }

has()     { case "$LAST_MSG" in *"$1"*) ok "$2" ;; *) bad "$2" "manifest lacked '$1'. Got: ${LAST_MSG:-<empty>}" ;; esac; }
lacks()   { case "$LAST_MSG" in *"$1"*) bad "$2" "manifest contained '$1', which it must not. Got: $LAST_MSG" ;; *) ok "$2" ;; esac; }
is_empty() { if [ -z "$LAST_OUT" ]; then ok "$1"; else bad "$1" "expected NO output, got: $LAST_OUT"; fi; }

# record_field <json-key> — the last observation line's value for that key.
record_field() {
    python3 - "$ENTITY/.claude/state/turn-manifests.jsonl" "$1" <<'PY'
import json, sys
try:
    lines = [l for l in open(sys.argv[1], encoding="utf-8") if l.strip()]
    print(json.loads(lines[-1]).get(sys.argv[2], ""))
except Exception:
    print("")
PY
}

echo "=== turn-manifest.sh — statuses rendered from results, never written ==="

# ===========================================================================
# (a) THE POSITIVE CONTROL — the case that motivated the whole mechanism
# ===========================================================================
run "$(payload "$MIXED_TR")"
has "queued" "a.  a SendMessage that returned 'queued' renders QUEUED"
has "Message queued for delivery to norm-opus-c1 at its next tool round." \
    "a1. the tool's own sentence is reproduced verbatim, not summarized"
# The failure was reading success:true and reporting "I've told him". A
# manifest that rendered the flag would be committing the same error.
lacks "success" "a2. the success flag is NOT rendered as the status"
lacks "I have told him" "a3. the assistant's own words never enter the manifest"

# ===========================================================================
# (b) THE EMPTY TURN, AND THE GAP THAT MUST NOT RESEMBLE ONE
# ===========================================================================
run "$(payload "$EMPTY_TURN_TR")"
has "0 tool calls this turn" "b.  a turn with no tool calls renders an EXPLICIT empty manifest"
if [ -n "$LAST_MSG" ]; then ok "b1. …and renders something rather than nothing"
else bad "b1. empty turn renders something" "systemMessage was empty"; fi

# An unreadable transcript is a GAP IN THE RECORD. If it rendered "0 tool
# calls" it would assert something false about the turn, which is the defect
# this hook exists to remove — so the two must be distinguishable on sight.
run "$(payload "$SANDBOX/does-not-exist.jsonl")"
has "UNAVAILABLE" "b2. an unreadable transcript renders UNAVAILABLE"
lacks "0 tool calls" "b2a. …and never claims the turn ran nothing"

run "$(payload "$MIXED_TR" NONE)"
has "UNAVAILABLE" "b3. a payload with no prompt_id renders UNAVAILABLE"
lacks "0 tool calls" "b3a. …and never claims the turn ran nothing"
has "cannot be told apart" "b3b. …and names the reason: the turn cannot be scoped"

# ===========================================================================
# (c) PER-TURN SCOPING — only the call's POSITION moves
# ===========================================================================
# Two fixtures generated from one spec, differing solely in whether the Glob
# pair sits before or after the record that opens this turn.
run "$(payload "$BEFORE_TR")"
lacks "Glob" "c.  a call from the PREVIOUS turn is not listed"
has "1 tool call(s) this turn" "c1. …and the count is this turn's alone"

run "$(payload "$AFTER_TR")"
has "Glob" "c2. the SAME call, moved inside the turn, IS listed"
has "2 tool call(s) this turn" "c3. …and the count grows by exactly one"

# The earlier turn's result carries the same tool_use_id as this turn's call.
# A session-wide result map would attach it and print a status that did not
# happen in this turn.
run "$(payload "$STALE_TR")"
has "NO RESULT" "c4. an earlier turn's result never pairs with this turn's call"
lacks "FROM-THE-EARLIER-TURN" "c5. …and its content never leaks into this manifest"

run "$(payload "$SIDE_TR")"
has "1 tool call(s) this turn" "c6. subagent traffic is not reported as the orchestrator's own"
has "Read" "c7. …while the orchestrator's own call in the same turn still is"

# ===========================================================================
# (d) STATUSES ARE READ FROM THE RESULTS
# ===========================================================================
run "$(payload "$MIXED_TR")"
has "ERROR — String to replace not found in file." \
    "d.  is_error renders ERROR plus the result's own first line"
has "NO RESULT" "d1. a call with no tool_result says so, rather than 'ok'"
has "RETURNED — 2 line(s)" "d2. plain Bash output reports returned, not verification"
has "Bash×1, SendMessage×1, Edit×1, Agent×1" "d3. the header tallies every call by tool name"

# ===========================================================================
# (e) TRUNCATION IS ANNOUNCED — silent elision is the defect, rebuilt
# ===========================================================================
run "$(payload "$FORTY_TR")"
has "40 tool call(s) this turn" "e.  the header counts ALL calls, before any row is dropped"
has "Bash×39, SendMessage×1" "e1. …and tallies them all by name"
has "15 further call(s) not listed above" "e2. dropped rows are counted out loud"
has "Bash×14, SendMessage×1" "e3. …and the dropped rows are tallied BY NAME too"
ROWS="$(printf '%s\n' "$LAST_MSG" | grep -cE '^ *[0-9]+ ')"
if [ "$ROWS" -eq 25 ]; then ok "e4. exactly 25 rows are printed at the cap"
else bad "e4. row cap" "expected 25 rows, counted $ROWS"; fi
# The whole point of tallying before truncating: a SendMessage past the cap is
# still visible. If it were not, truncation would hide the exact class of call
# this mechanism exists to surface.
case "$LAST_MSG" in
    *"SendMessage×1"*) ok "e5. a SendMessage past the row cap stays visible in the header" ;;
    *) bad "e5. SendMessage past the cap" "it vanished: $LAST_MSG" ;;
esac

run "$(payload "$LONG_TR")"
has "chars)" "e6. a shortened row says how many characters were removed"

# ===========================================================================
# (f) THE NEGATIVE CONTROL — this suite cannot pass over an empty corpus
# ===========================================================================
run "$(payload "$MIXED_TR")"
EXAMINED="$(record_field records_examined)"
# The turn is located from the END of the file (no size cap), so only the records
# from this turn's opening prompt on are examined: the 3 records of the earlier
# turn in the "mixed" fixture are not read.
TURN_N=$((MIXED_N - 3))
if [ "$EXAMINED" = "$TURN_N" ] && [ "$TURN_N" -gt 0 ]; then
    ok "f.  the manifest examined all $TURN_N records of this turn (non-zero, exact)"
else
    bad "f.  records examined" "expected exactly $TURN_N, record says '${EXAMINED:-<none>}' — a manifest rendered from nothing would still look convincing, so this number is the proof it read something"
fi

# f2 proves f can tell the two apart, rather than passing on any number at all.
run "$(payload "$EMPTY_FILE_TR")"
EXAMINED0="$(record_field records_examined)"
if [ "$EXAMINED0" = "0" ]; then ok "f1. an empty transcript really does examine 0 records"
else bad "f1. empty corpus" "expected 0, got '${EXAMINED0:-<none>}'"; fi

# ===========================================================================
# (g) IT NEVER BLOCKS, AND NEVER WEDGES A SESSION
# ===========================================================================
run "$(payload "$MIXED_TR" "$PID" true)"
is_empty "g1. stop_hook_active renders NOTHING, so it cannot re-render on a retry"

# g2 WAS MISNAMED, AND THE MISNAMING HID A GAP.
#
# It set RICHOS_ENTITY_ROOT to an unadopted directory and expected silence,
# under the title "a repository that has not adopted the engine". The resolver
# does not agree: an EXPLICIT root override that carries no orchestration.config
# resolves to status `broken`, with the reason "an explicitly declared root MUST
# be an adopted engine root; the resolver will NOT quietly substitute a
# different one." So this case never exercised the not-adopted path at all — it
# exercised the hard-failure path, and asserted that a hard failure says nothing
# the operator can see. It passed only because the banner went to stderr and the
# harness discards stderr.
#
# Split in two, because the two paths must behave OPPOSITELY:
#   g2  a misconfigured root is a HARD FAILURE and is announced.
#   g2a a genuinely unadopted directory is NOT APPLICABLE and stays silent.
# Without g2a, "announce on root failure" could quietly start nagging every
# session opened outside an adopted repo, which is the noise trap.
run "$(payload "$MIXED_TR")" "$UNADOPTED"
has "UNAVAILABLE" "g2. a root override pointing at an unadopted directory is a hard failure, and says so where the operator reads"

# g2a — no override at all, and a payload cwd that carries no marker. This is
# the real not-adopted path: not applicable, therefore silent.
UNADOPTED_CWD="$SANDBOX/unadopted-cwd"
mkdir -p "$UNADOPTED_CWD"
printf '{"session_id":"feedface-0000-4000-8000-000000000000","transcript_path":"%s","cwd":"%s","prompt_id":"%s","hook_event_name":"Stop","stop_hook_active":false,"last_assistant_message":"done"}' \
    "$MIXED_TR" "$UNADOPTED_CWD" "$PID" > "$SANDBOX/g2a-payload.json"
# The hook's own working directory has to be unadopted too. `pwd` is the last
# candidate the resolver tries, and this suite runs from the engine checkout —
# which IS adopted, so leaving the cwd alone resolves successfully and the case
# would silently test something else entirely. Caught exactly that way on the
# first run: it reached the RENDERER and complained about a missing prompt_id.
LAST_OUT="$(cd "$UNADOPTED_CWD" && env -u RICHOS_ENTITY_ROOT -u CLAUDE_PROJECT_DIR \
    "$HOOK" < "$SANDBOX/g2a-payload.json" 2>/dev/null)"
LAST_RC=$?
[ "$LAST_RC" -eq 0 ] || NON_ZERO_EXITS="$NON_ZERO_EXITS rc=$LAST_RC"
is_empty "g2a. a genuinely unadopted directory stays silent — not-applicable is not a failure"

LAST_OUT="$(printf 'not json at all' | RICHOS_ENTITY_ROOT="$ENTITY" "$HOOK" 2>/dev/null)"
LAST_RC=$?
[ "$LAST_RC" -eq 0 ] || NON_ZERO_EXITS="$NON_ZERO_EXITS rc=$LAST_RC"
if [ "$LAST_RC" -eq 0 ]; then ok "g3. an unparseable payload ends the turn cleanly"
else bad "g3. unparseable payload" "exit $LAST_RC"; fi

# A broken install must not wedge turn-end. Every Stop hook here fails OPEN for
# the reason in the sibling's header: failing closed refuses to let the SESSION
# end, retrying to the block cap while the operator watches.
BROKEN="$SANDBOX/broken"
mkdir -p "$BROKEN"
cp "$HOOK" "$BROKEN/turn-manifest.sh"
cp "$SCRIPT_DIR/turn-manifest.py" "$BROKEN/turn-manifest.py"
# The payload reaches the hook through a FILE, not a pipe. The hook's
# broken-install branch exits before it ever reads stdin, so a pipe would give
# the writer EPIPE and — under `set -o pipefail` — hand this case a non-zero
# status belonging to printf rather than to the hook. That is a defect in the
# harness that would read as a defect in the hook.
printf '%s' "$(payload "$MIXED_TR")" > "$SANDBOX/g4-payload.json"
RICHOS_ENTITY_ROOT="$ENTITY" bash "$BROKEN/turn-manifest.sh" \
    < "$SANDBOX/g4-payload.json" >/dev/null 2>"$SANDBOX/g4-err.txt"
RC=$?
[ "$RC" -eq 0 ] || NON_ZERO_EXITS="$NON_ZERO_EXITS rc=$RC"
if [ "$RC" -eq 0 ]; then ok "g4. a broken install (no resolve-roots.sh) still lets the turn end"
else bad "g4. broken install" "exit $RC — a Stop hook that fails closed strands the session: $(cat "$SANDBOX/g4-err.txt")"; fi
if grep -q 'BROKEN INSTALL' "$SANDBOX/g4-err.txt"; then
    ok "g4a. …and says so, rather than passing the turn through in silence"
else
    bad "g4a. broken install is announced" "no banner: $(cat "$SANDBOX/g4-err.txt")"
fi

# The opt-out must be VISIBLE. On a zero exit the host files a Stop hook's
# stderr into the transcript and shows the operator nothing, so a stderr notice
# would be a stand-down nobody can see.
printf 'SHOW_TURN_MANIFEST=0\n' > "$ENTITY/orchestration.config"
run "$(payload "$MIXED_TR")"
has "STOOD DOWN" "g5. the opt-out announces itself through the channel the operator reads"
: > "$ENTITY/orchestration.config"

# ===========================================================================
# (h) THE OUTPUT CONTRACT — the only shape the host will show the operator
# ===========================================================================
# Orchestration reliability R6: "Normal successful turns emit no routine tool manifest." The
# turn's calls and statuses are in the log; --show renders the manifest on request.
LOG="$ENTITY/.claude/state/turn-manifests.jsonl"
BEFORE_ROWS="$(wc -l < "$LOG" 2>/dev/null || echo 0)"
run "$(payload "$MIXED_TR")"
AFTER_ROWS="$(wc -l < "$LOG" 2>/dev/null || echo 0)"
if [ -z "$LAST_OUT" ] && [ "$AFTER_ROWS" -gt "$BEFORE_ROWS" ] \
   && tail -1 "$LOG" | python3 -c 'import json,sys; d=json.loads(sys.stdin.readline()); sys.exit(0 if d.get("calls") and d.get("statuses") else 1)'; then
    ok "h.  a routine turn shows NO manifest (R6); its calls and statuses are logged, and --show renders it"
else bad "h.  no routine manifest" "stdout: ${LAST_OUT:-<empty>} rows: $BEFORE_ROWS -> $AFTER_ROWS"; fi
case "$LAST_MSG" in *"TURN MANIFEST"*) ok "h2. --show still renders the turn's manifest on request" ;;
    *) bad "h2. --show renders" "got: ${LAST_MSG:-<empty>}" ;; esac

# ===========================================================================
# (g) the global assertion: not one case above exited non-zero
# ===========================================================================
if [ -z "$NON_ZERO_EXITS" ]; then
    ok "g.  every case exited 0 — this hook has no path that can block a turn"
else
    bad "g.  never blocks" "non-zero exits observed:$NON_ZERO_EXITS"
fi

# --- THE MUTATION HARNESS RUNS FROM THE SUITE IT MUTATES -------------------
# Until 2026-09-05 it ran from NOTHING. run-all-tests.sh discovers *.test.sh;
# contract-integrity.test.sh names a hand-typed set of harnesses that never
# included this one; and the only other mention of the file anywhere in the
# repository was a comment. So every property it proves load-bearing was
# proven exactly when somebody remembered to type its path. Recorded as row
# 3.26 of wiki/open-items.md.
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
    echo "=== running the mutation harness: turn-manifest.mutation.sh ==="
    if [ -x "$SCRIPT_DIR/turn-manifest.mutation.sh" ]; then
        "$SCRIPT_DIR/turn-manifest.mutation.sh" || FAIL=$((FAIL + 1))
    else
        echo "  FAIL  MUT. turn-manifest.mutation.sh is missing or not executable — IT DID NOT RUN"
        FAIL=$((FAIL + 1))
    fi
fi

echo ""
if [ "$FAIL" -eq 0 ]; then
    printf '✓ %s/%s turn-manifest checks passed.\n' "$PASS" "$((PASS + FAIL))"
    exit 0
fi
printf '✗ %s/%s passed, %s FAILED.\n' "$PASS" "$((PASS + FAIL))" "$FAIL" >&2
exit 1
