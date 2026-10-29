#!/usr/bin/env bash
#
# record-canary.test.sh — PROVE THE RECORD CANARY IN BOTH DIRECTIONS.
#
# scripts/lib/record-canary.sh watches the operator's record — the ownership
# ledger, the platform's fallback event log, the team directories — per suite.
# A canary that cannot go red is a green tick printed over the defect, and the
# defect here is dated: on 2026-09-11 a green suite wrote a false `terminated`
# witness for a running agent into the operator's real ledger and nothing said
# so. So this suite drives the detector onto each of the three shapes that
# class has taken and off them again:
#
#   a `terminated` row appended to the ledger          (9b, 2026-09-11)
#   fixture rows appended to the fallback event log     (feedbeef, 2026-09-10)
#   a fixture team directory created                    (session-deadbeef, 2026-09-10)
#
# No event label exempts a write. Removal and restoration must also be visible.
#
# Everything happens under a throwaway CLAUDE_CONFIG_DIR. This suite reads
# nothing under the operator's real ~/.claude and writes nothing there —
# which, given what it is about, is the least it can do.
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PASS=0
FAIL=0
SANDBOX="$(cd "$(mktemp -d "${TMPDIR:?}/record-canary-test.XXXXXX")" && pwd -P)"
trap 'rm -rf "$SANDBOX"' EXIT
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '          %s\n' "$2"; FAIL=$((FAIL + 1)); }

CFG="$SANDBOX/cfg"
mkdir -p "$CFG/state" "$CFG/teams/session-aaaaaaaa"
export CLAUDE_CONFIG_DIR="$CFG"
# shellcheck source=record-canary.sh
. "$SCRIPT_DIR/record-canary.sh"

echo "=== record canary: red on changed records, green on unchanged fixtures ==="

# ===========================================================================
# 1. THE PATHS ARE CAPTURED AT SOURCE TIME, NOT RE-READ
# ===========================================================================
if [ "$RC_LEDGER" = "$CFG/state/worktree-ledger.jsonl" ] && [ "$RC_FALLBACK" = "$CFG/worker-events.jsonl" ] && [ "$RC_TEAMS" = "$CFG/teams" ]; then
    ok "1a  the three watched paths resolve under CLAUDE_CONFIG_DIR as it was when the library was sourced"
else
    bad "1a  watched paths" "ledger=$RC_LEDGER fallback=$RC_FALLBACK teams=$RC_TEAMS"
fi
CLAUDE_CONFIG_DIR="$SANDBOX/elsewhere"
if [ "$RC_LEDGER" = "$CFG/state/worktree-ledger.jsonl" ]; then
    ok "1b  and moving CLAUDE_CONFIG_DIR afterwards does not move them — a suite that relocates its config halfway through is still compared against the record it started with"
else
    bad "1b  paths captured, not re-read" "ledger=$RC_LEDGER"
fi
CLAUDE_CONFIG_DIR="$CFG"

# ===========================================================================
# 2. GREEN WHEN NOTHING HAPPENS — including with every path ABSENT (CI's shape)
# ===========================================================================
B0="$SANDBOX/b0.txt"
rc_baseline "$B0"
if [ "$RC_HEALTHY" -eq 1 ] && [ -z "$(rc_escaped "$B0")" ]; then
    ok "2a  with the ledger and the fallback absent and one empty team directory, the baseline is healthy and nothing has escaped"
else
    bad "2a  quiet before any write" "healthy=$RC_HEALTHY escaped=[$(rc_escaped "$B0")]"
fi
printf '{"event": "finished", "agent_id": "a1", "signal": "SubagentStop", "source": "worker-ended-handoff.sh", "ts": "2026-09-11T00:00:00+00:00"}\n' >>"$CFG/state/worktree-ledger.jsonl"
B1="$SANDBOX/b1.txt"
rc_baseline "$B1"
printf '{"event": "finished", "agent_id": "a2", "signal": "SubagentStop", "source": "worker-ended-handoff.sh", "ts": "2026-09-11T00:00:01+00:00"}\n' >>"$CFG/state/worktree-ledger.jsonl"
E="$(rc_escaped "$B1")"
if printf '%s' "$E" | grep -q 'event=finished'; then
    ok "2b  a finished label cannot exempt a write from the record canary"
else
    bad "2b  every ledger event is witnessed" "got: $E"
fi

# ===========================================================================
# 3. RED ON EACH SHAPE THE CLASS HAS TAKEN
# ===========================================================================
B2="$SANDBOX/b2.txt"
rc_baseline "$B2"
printf '{"event": "terminated", "agent_id": "ae904aac1949e5696", "teammate": "sage-fable-cert3", "witness": "platform-terminal-record", "ts": "2026-09-11T00:02:34.984941+00:00"}\n' >>"$CFG/state/worktree-ledger.jsonl"
E="$(rc_escaped "$B2")"
case "$E" in
    *"ledger row APPEARED"*"event=terminated"*"witness=platform-terminal-record"*"teammate=sage-fable-cert3"*)
        ok "3a  RED: a 'terminated' row appended to the ledger is caught and NAMED (event, witness, teammate) — the 2026-09-11 shape" ;;
    "") bad "3a  RED on a terminated row" "a terminated row appeared in the watched ledger and the canary reported nothing" ;;
    *)  bad "3a  the row is named" "got: $E" ;;
esac
B3="$SANDBOX/b3.txt"
rc_baseline "$B3"
printf '{"event": "registered", "agent_id": "x", "teammate": "dev-1", "source": "detect-nonnative-worktree.sh", "ts": "t"}\n{"event": "prepared", "teammate": "dev-2", "source": "create-teammate-worktree.sh", "ts": "t"}\n{"event": "retracted", "agent_id": "x", "retracts_ts": "t", "ts": "t"}\n' >>"$CFG/state/worktree-ledger.jsonl"
E="$(rc_escaped "$B3")"
if [ "$(printf '%s\n' "$E" | grep -c 'ledger row APPEARED')" -eq 3 ] \
   && printf '%s' "$E" | grep -q 'event=registered' && printf '%s' "$E" | grep -q 'event=prepared' && printf '%s' "$E" | grep -q 'event=retracted'; then
    ok "3b  registered, prepared and retracted rows are witnessed, one line each"
else
    bad "3b  every other event is witnessed" "got: $E"
fi
B4="$SANDBOX/b4.txt"
rc_baseline "$B4"
printf '{"event": "WorkerRunEnded", "agent_id": "feedbeef", "session_id": "feedbeef-0000", "timestamp": "2026-09-10T00:00:00+00:00"}\n' >>"$CFG/worker-events.jsonl"
E="$(rc_escaped "$B4")"
case "$E" in
    *"fallback: was ABSENT and now EXISTS"*|*"fallback event log line APPEARED"*"session=feedbeef-0000"*)
        ok "3c  RED: a fixture row appended to the fallback event log is caught and named — the feedbeef shape (Sage D4)" ;;
    "") bad "3c  RED on the fallback log" "a row appeared in the fallback event log and the canary reported nothing" ;;
    *)  bad "3c  the fallback row is named" "got: $E" ;;
esac
B5="$SANDBOX/b5.txt"
rc_baseline "$B5"
mkdir -p "$CFG/teams/session-deadbeef"
printf 'dev-1\n' >"$CFG/teams/session-deadbeef/spawned-names.log"
E="$(rc_escaped "$B5")"
case "$E" in
    *"team directory entry APPEARED: session-deadbeef/"*)
        ok "3d  RED: a fixture team directory is caught and named — the session-deadbeef shape (round 14, D4's sibling)" ;;
    "") bad "3d  RED on a team directory" "a directory appeared under teams/ and the canary reported nothing" ;;
    *)  bad "3d  the directory is named" "got: $E" ;;
esac
case "$E" in
    *"session-deadbeef/spawned-names.log"*) ok "3d' and the file inside it, by name" ;;
    *) bad "3d' the file inside the new directory is named" "got: $E" ;;
esac

# 3e. THE WORKSPACE REGISTRY — the 2026-09-11 shape: a suite registering a test
#     spawn in the operator's registry, the record the Stop gate decides from.
B3E="$SANDBOX/b3e.txt"
rc_baseline "$B3E"
mkdir -p "$CFG/state/workspaces/agents"
printf '{"key": "deadbeef--dev-sonnet-q1"}\n' >"$CFG/state/workspaces/agents/deadbeef--dev-sonnet-q1.json"
printf '{"event": "registered-spawn", "key": "deadbeef--dev-sonnet-q1", "ts": "2026-09-11T09:52:00Z"}\n' >"$CFG/state/workspaces/events.jsonl"
E="$(rc_escaped "$B3E")"
case "$E" in
    *"workspace registry entry APPEARED: agents/deadbeef--dev-sonnet-q1.json"*"event=registered-spawn"*|*"event=registered-spawn"*"agents/deadbeef--dev-sonnet-q1.json"*)
        ok "3e  RED: a registration written into the workspace registry is caught and named, record and event" ;;
    "") bad "3e  RED on the workspace registry" "a registration appeared in the registry and the canary reported nothing" ;;
    *)  bad "3e  the registration is named" "got: $E" ;;
esac
rm -rf "$CFG/state/workspaces"

# ===========================================================================
# 4. THE WITNESS IS CONTENTS, NOT COUNTS — an overwrite in place is a change
# ===========================================================================
B6="$SANDBOX/b6.txt"
rc_baseline "$B6"
python3 - "$CFG/state/worktree-ledger.jsonl" <<'PY'
import sys
p = sys.argv[1]
lines = open(p, encoding="utf-8").read().splitlines(True)
# rewrite the terminated row's witness in place: same row count, different bytes
lines = [l.replace('"witness": "platform-terminal-record"', '"witness": "edited-by-hand"') for l in lines]
open(p, "w", encoding="utf-8").write("".join(lines))
PY
E="$(rc_escaped "$B6")"
case "$E" in
    *"witness=edited-by-hand"*) ok "4a  a row rewritten IN PLACE (same count, different bytes) is caught — the witness is contents, not a line count" ;;
    "") bad "4a  in-place edits are caught" "the ledger changed bytes without changing its row count and the canary reported nothing" ;;
    *)  bad "4a  the edited row is named" "got: $E" ;;
esac

# ===========================================================================
# 5. A PATH IT CANNOT READ IS A FAILURE, NEVER A QUIET PASS
# ===========================================================================
chmod 000 "$CFG/state/worktree-ledger.jsonl"
if [ ! -r "$CFG/state/worktree-ledger.jsonl" ]; then
    B7="$SANDBOX/b7.txt"
    rc_baseline "$B7"
    if [ "$RC_HEALTHY" -eq 0 ]; then
        ok "5a  an unreadable ledger makes the baseline UNHEALTHY, so a runner can refuse to report a pass over it"
    else
        bad "5a  unreadable ledger is unhealthy" "RC_HEALTHY=$RC_HEALTHY with a mode-000 ledger"
    fi
    E="$(rc_escaped "$B7")"
    case "$E" in
        *UNREADABLE*) ok "5b  and the diff says UNREADABLE by name, never 'nothing escaped'" ;;
        *) bad "5b  unreadable is reported as unreadable" "got: [$E]" ;;
    esac
else
    ok "5a  (not testable here: this user can read a mode-000 file — running as root)"
    ok "5b  (same)"
fi
chmod 644 "$CFG/state/worktree-ledger.jsonl"

# Deletion of one row cannot disappear behind surviving rows. Restoring bytes
# and mtime cannot restore the kernel's change time. All targets are private.
for mutation in remove-row truncate restore detached-restore; do
    SNAPSHOT="$SANDBOX/$mutation.txt"
    rc_baseline "$SNAPSHOT"
    python3 - "$CFG/state/worktree-ledger.jsonl" "$mutation" <<'PY'
import os, pathlib, subprocess, sys
p = pathlib.Path(sys.argv[1])
kind = sys.argv[2]
if kind == 'detached-restore':
    subprocess.run([sys.executable, '-c', 'import pathlib,sys; p=pathlib.Path(sys.argv[1]); old=p.read_bytes(); p.write_bytes(b"escape"); p.write_bytes(old)', str(p)], start_new_session=True, check=True)
elif kind == 'restore':
    old, before = p.read_bytes(), p.stat()
    p.write_bytes(b'escape'); p.write_bytes(old)
    os.utime(p, ns=(before.st_atime_ns, before.st_mtime_ns))
elif kind == 'truncate':
    p.write_bytes(b'')
else:
    p.write_bytes(b''.join(p.read_bytes().splitlines(keepends=True)[1:]))
PY
    E="$(rc_escaped "$SNAPSHOT")"
    if printf '%s' "$E" | grep -q 'record file CHANGED:'; then
        ok "5c $mutation remains non-green"
    else
        bad "5c $mutation was missed" "$E"
    fi
done

mkdir -p "$CFG/state/workspaces/agents"
printf '{"state":"active"}\n' > "$CFG/state/workspaces/agents/fixture.json"
rc_baseline "$SANDBOX/registry-edit.txt"
printf '{"state":"finished"}\n' > "$CFG/state/workspaces/agents/fixture.json"
if printf '%s' "$(rc_escaped "$SANDBOX/registry-edit.txt")" | grep -q 'record file CHANGED:.*fixture.json'; then
    ok "5d registry content changes are visible without adding a filename or event"
else
    bad "5d registry content edit was missed"
fi
rc_baseline "$SANDBOX/registry-delete.txt"
rm "$CFG/state/workspaces/agents/fixture.json"
if printf '%s' "$(rc_escaped "$SANDBOX/registry-delete.txt")" | grep -q 'record file REMOVED:.*fixture.json'; then
    ok "5e removal of an individual registry record is visible"
else
    bad "5e registry record removal was missed"
fi

# ===========================================================================
# 7. rc_sandbox — A RECORD OF THE UNIT'S OWN (2026-09-27). The runners hand each
#    unit a throwaway home and watch the record inside it, so a concurrent
#    writer to the live record is never charged to the unit. The runner-level
#    proof, concurrent writer included, is ci-shard.test.sh S15b-S15h; these
#    pin the library's half.
# ===========================================================================
export RC_TEST_INSIDE="$CFG/state/test-devices"
export RC_TEST_OUTSIDE="$SANDBOX/not-the-record"
UH="$SANDBOX/unit-home"
if rc_sandbox "$UH"; then
    UHP="$(cd "$UH" && pwd -P)"
    if [ "$RC_CFG" = "$UHP/.claude" ] && [ "$RC_LEDGER" = "$UHP/.claude/state/worktree-ledger.jsonl" ] && [ "$RC_LIVE_CFG" = "$CFG" ]; then
        ok "7a  rc_sandbox points the canary at the record inside the unit's home, and still names the operator's as RC_LIVE_CFG"
    else
        bad "7a  rc_sandbox repoints the canary" "RC_CFG=$RC_CFG RC_LIVE_CFG=$RC_LIVE_CFG"
    fi
    ENVARGS=" ${RC_SANDBOX_ENV[*]} "
    case "$ENVARGS" in
        *" -u CLAUDE_CONFIG_DIR "*"-u RC_TEST_INSIDE "*"HOME=$UHP "*)
            ok "7b  the unit's env sets HOME to its home and REMOVES every variable pointing into the operator's record, CLAUDE_CONFIG_DIR included" ;;
        *) bad "7b  the unit's env" "got:$ENVARGS" ;;
    esac
    case "$ENVARGS" in
        *"CLAUDE_CONFIG_DIR="*|*"RICHOS_WORKSPACES_DIR="*)
            bad "7b' CLAUDE_CONFIG_DIR and RICHOS_WORKSPACES_DIR are never SET" "got:$ENVARGS" ;;
        *) ok "7b' and never SETS CLAUDE_CONFIG_DIR or RICHOS_WORKSPACES_DIR, so a unit that builds a world of its own keeps the resolution it chose" ;;
    esac
    case "$ENVARGS" in
        *RC_TEST_OUTSIDE*) bad "7c  a variable pointing elsewhere is left alone" "got:$ENVARGS" ;;
        *) ok "7c  and a variable pointing anywhere else is passed through untouched" ;;
    esac
    # 7g (2026-10-28): the app's session is removed BY NAME, whatever its value,
    # and a machine setting that is not the session passes through.
    APPENV="$(RICHOS_ENGINE_ROOT=/installed/engine RICHOS_APP_THREAD=thr_fixture RICHOS_APP_STATE=/app/state \
        RICHOS_WORKSPACES_DIR=/app/state/workspaces/x RICHOS_AGENT_HOLD_DIR=/app/state/agent-hold \
        STALL_WATCH_STATE_DIR=/app/state/stall-watch RICHOS_ENTITY_ROOT=/app/coordination \
        RICHOS_SESSION_ID=fixture-session RICHOS_BULK_ROOT=/bulk CLAUDE_PROJECT_DIR=/app/coordination \
        bash -c '. "$1"; rc_sandbox "$2" && printf " %s " "${RC_SANDBOX_ENV[*]}"' _ "$SCRIPT_DIR/record-canary.sh" "$SANDBOX/unit-home-app")"
    G7=1
    for name in RICHOS_ENGINE_ROOT RICHOS_APP_THREAD RICHOS_APP_STATE RICHOS_WORKSPACES_DIR RICHOS_AGENT_HOLD_DIR \
            STALL_WATCH_STATE_DIR RICHOS_ENTITY_ROOT RICHOS_SESSION_ID CLAUDE_PROJECT_DIR; do
        case "$APPENV" in *" -u $name "*) ;; *) G7=0 ;; esac
    done
    case "$APPENV" in *RICHOS_BULK_ROOT*|*"RICHOS_ENGINE_ROOT="*|*"RICHOS_APP_THREAD="*) G7=0 ;; esac
    if [ "$G7" -eq 1 ]; then
        ok "7g  the app's session (installed engine, live records, conversation) is REMOVED by name and never set; a machine setting passes"
    else
        bad "7g  the app's session is handed to the unit" "got:$APPENV"
    fi
    B8="$SANDBOX/b8.txt"
    rc_baseline "$B8"
    printf '{"event": "terminated", "agent_id": "z", "ts": "t"}\n' >>"$CFG/state/worktree-ledger.jsonl"
    if [ -z "$(rc_escaped "$B8")" ]; then
        ok "7d  a row written to the LIVE record while a unit runs is not charged to the unit"
    else
        bad "7d  the live record is out of the unit's diff" "got: $(rc_escaped "$B8")"
    fi
    # Exact lead-side incident: workspaces.sh wait changes both an agent record
    # and events.jsonl while an isolated canary unit is running. No event-name
    # exemption: the same write inside the unit must still be detected.
    WAIT_WRITER="$SANDBOX/wait-writer.py"
    cat > "$WAIT_WRITER" <<'PY'
import importlib.util, sys
spec = importlib.util.spec_from_file_location("workspaces", sys.argv[1])
ws = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ws)
ws.save_agent({"key": "wait-regression", "name": "lead-fixture", "session_id": "fixture"})
ws.wait("wait-regression", "started", "its merge check is running", session_id="fixture", until="at:2099-01-01T00:00:00Z")
PY
    WS_LIB="$SCRIPT_DIR/../../mega-lander/workspaces.py"
    if RICHOS_WORKSPACES_DIR="$CFG/state/workspaces" python3 -B "$WAIT_WRITER" "$WS_LIB" \
            && [ -z "$(rc_escaped "$B8")" ]; then
        ok "7d' the lead's real wait record during a unit does not cancel that isolated unit"
    else
        bad "7d' a live wait record was charged to the unit" "$(rc_escaped "$B8")"
    fi
    if env "${RC_SANDBOX_ENV[@]}" python3 -B "$WAIT_WRITER" "$WS_LIB" \
            && [ -n "$(rc_escaped "$B8")" ]; then
        ok "7d'' POSITIVE CONTROL: the same wait write inside the unit's record is still contamination"
    else
        bad "7d'' the canary failed to detect the unit's own wait write"
    fi
    env "${RC_SANDBOX_ENV[@]}" bash -c 'mkdir -p "$HOME/.claude/state" && printf "{\"event\": \"terminated\", \"agent_id\": \"own\", \"ts\": \"t\"}\n" >> "$HOME/.claude/state/worktree-ledger.jsonl"'
    case "$(rc_escaped "$B8")" in
        *"event=terminated"*"agent=own"*) ok "7e  POSITIVE CONTROL: a row the unit writes through its own \$HOME is caught and named" ;;
        *) bad "7e  the unit's own write is caught" "got: $(rc_escaped "$B8")" ;;
    esac
else
    bad "7a  rc_sandbox could not build a home in a writable directory"
fi
: >"$SANDBOX/a-file"
if rc_sandbox "$SANDBOX/a-file/home"; then
    bad "7f  a home that cannot be built is reported" "rc_sandbox returned 0 under a regular file"
else
    ok "7f  a home that cannot be built returns 1, so the runner reports a blind canary and never runs the unit against the operator's record"
fi
unset RC_TEST_INSIDE RC_TEST_OUTSIDE
rc_use_config "$CFG"

# ===========================================================================
# 6. NOTHING HERE TOUCHED THE OPERATOR'S REAL RECORD — the library's own promise,
#    checked against the real paths rather than assumed from the sandbox.
# ===========================================================================
if printf '%s' "$(rc_escaped "$SANDBOX/missing-baseline")" | grep -q UNREADABLE; then
    ok "5f missing baseline remains unresolved"
else
    bad "5f missing baseline was treated as unchanged"
fi
ln -s "$CFG/state/worktree-ledger.jsonl" "$CFG/state/workspaces/agents/linked.json"
rc_baseline "$SANDBOX/symlink.txt"
if [ "$RC_HEALTHY" -eq 0 ] && printf '%s' "$(rc_escaped "$SANDBOX/symlink.txt")" | grep -q UNREADABLE; then
    ok "5g symbolic-link record boundaries remain unresolved"
else
    bad "5g symbolic-link boundary claimed complete observation"
fi
rm "$CFG/state/workspaces/agents/linked.json"

rm "$CFG/worker-events.jsonl"
rc_baseline "$SANDBOX/create-delete.txt"
printf 'temporary escape\n' > "$CFG/worker-events.jsonl"
rm "$CFG/worker-events.jsonl"
if printf '%s' "$(rc_escaped "$SANDBOX/create-delete.txt")" | grep -q 'record file CHANGED:'; then
    ok "5h creating then removing an initially absent record remains visible"
else
    bad "5h create-delete restoration was missed"
fi

if [ -e "$SANDBOX/elsewhere" ]; then
    bad "6a  the suite wrote under the CLAUDE_CONFIG_DIR it merely pointed at" "$SANDBOX/elsewhere exists"
else
    ok "6a  the suite created nothing under a config directory it only named"
fi

printf '\n  %d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
