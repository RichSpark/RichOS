#!/usr/bin/env bash
#
# scratch-land-scope.test.sh — LANDING ONE AGENT DELETES ONLY THAT AGENT'S SCRATCH.
#
# 2026-10-01: `workspaces.sh land` ran the whole machine's scratch sweep, so two
# lands deleted 141 entries (scratch-reaper.log, 21:35:46Z and 22:31:39Z), none of
# them the landed agent's. Since this change a land passes the agent's name,
# session and workspaces, and the reaper plans only what provably belongs to it:
# an allocation made from one of its workspaces, or a directory named for it in an
# agent root or in its session's scratchpad. Everything else is counted and left.
#
# The sandbox: one Claude scratch root holding the session's SHARED scratchpad
# (the lead's briefs/, a live teammate's loose file and named directory, and the
# landed agent's named directory), one agent root with both agents' named
# directories, and an allocator root with allocations made from each agent's
# workspace (one of the landed agent's still owned by a live pid). HOME,
# CLAUDE_CONFIG_DIR, TMPDIR, the reaper's config, ledger, log and state all point
# into the sandbox, so nothing outside it is read for a decision or deleted.
#
#   L1  the plan (dry run) deletes exactly the landed agent's three items
#   L2  the shared scratchpad, the lead's briefs and the live agent's files are
#       never planned
#   L3  the allocation still owned by a live pid is KEPT
#   L4  everything else looked at is counted as unattributed
#   L5  the land's own path (workspaces.py land_sweep_scopes, then
#       sweep_scratch_after_land) deletes exactly those three; every other file
#       is still there; the printed line names the agent and the unattributed count
#   L6  another LIVE agent with the same name holds the named directories
#   L7  no scope, no sweep: an unknown agent deletes nothing
#
# ON AN ENGINE WITHOUT THE SCOPE (main before this change) L1 fails because the
# reaper refuses --agent, and every APPLY step below is skipped: the old land path
# is the whole-machine sweep, and this suite never runs that.
#
# Exit 0 = all cases pass; exit 1 = at least one failure.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$SCRIPT_DIR/.." && pwd)"
REAPER="$ENGINE/scripts/scratch-reaper.sh"
WS_PY="$ENGINE/mega-lander/workspaces.py"

PASS=0; FAIL=0
ok()  { PASS=$((PASS + 1)); printf '  PASS  %s\n' "$1"; }
bad() { FAIL=$((FAIL + 1)); printf '  FAIL  %s\n' "$1"; }

SANDBOX="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/scratch-land-scope.XXXXXX")" && pwd -P)"
LIVE_PID=""
cleanup() {
    if [ -n "$LIVE_PID" ]; then kill "$LIVE_PID" 2>/dev/null; wait "$LIVE_PID" 2>/dev/null; fi
    rm -rf "$SANDBOX"
}
trap cleanup EXIT

export HOME="$SANDBOX/home" CLAUDE_CONFIG_DIR="$SANDBOX/home/.claude"
export TMPDIR="$SANDBOX/tmp"
export RICHOS_WORKSPACES_DIR="$CLAUDE_CONFIG_DIR/state/workspaces"
export SCRATCH_LEDGER="$SANDBOX/scratch-ledger.jsonl"
export SCRATCH_REAPER_LOG="$SANDBOX/scratch-reaper.log"
export SCRATCH_REAPER_STATE="$SANDBOX/scratch-reaper-state.json"
unset CLAUDE_PROJECT_DIR RICHOS_SESSION_ID
mkdir -p "$HOME" "$CLAUDE_CONFIG_DIR/state" "$RICHOS_WORKSPACES_DIR/agents" "$TMPDIR"

SID="58a70f1b-5fb0-437f-b6a7-146d5090a3df"
LANDED="reed-sonnet-v2open1"
LIVE="echo-opus-secalerts1"
ROOT="$SANDBOX/claude-501"
PAD="$ROOT/-Users-alex-ab-femcboost/$SID/scratchpad"
export RICHOS_BULK_ROOT="$SANDBOX/E1TB"
AROOT="$RICHOS_BULK_ROOT/tmp/claude"
ALLOC="$TMPDIR/richos-scratch"
WS_LANDED="$SANDBOX/wt/$LANDED"
WS_LIVE="$SANDBOX/wt/$LIVE"
mkdir -p "$PAD/briefs" "$PAD/$LIVE" "$PAD/$LANDED" "$AROOT/$LANDED" "$AROOT/$LIVE" \
         "$ALLOC" "$WS_LANDED/richos/engine" "$WS_LIVE/richos/app"

# The sandbox's own LITERAL config: every root the reaper reads points into the
# sandbox, and every arm that could reach outside it is off. Nothing is read from
# the engine's orchestration.config.
CFG="$SANDBOX/orchestration.config"
cat >"$CFG" <<EOF
SCRATCH_REAPER_ENABLE="1"
SCRATCH_CLAUDE_ROOTS="$ROOT"
SCRATCH_SESSION_PROCESS_NAMES="claude"
SCRATCH_TMP_PATTERNS="richos-*-workspace"
SCRATCH_AGE_FLOOR_MINUTES="60"
SCRATCH_NIGHTLY_DIR="$SANDBOX/nightly"
SCRATCH_NIGHTLY_KEEP="3"
SCRATCH_NOTICE_BYTES="1073741824"
SCRATCH_SHARED_TMP_ROOTS=""
SCRATCH_CAMPAIGN_PARENT=""
SCRATCH_CAMPAIGN_ROOTS=""
SCRATCH_DOCKER_PRUNE="0"
EOF
export SCRATCH_REAPER_CONFIG="$CFG"

# A live owner, started BEFORE its allocation exists (a process that started
# after a directory was made cannot have made it).
sleep 600 >/dev/null 2>&1 &
LIVE_PID=$!
DEAD_PID="$(sh -c 'echo $$')"
sleep 1

printf 'lead brief\n'   >"$PAD/briefs/isaac-r3floor.md"
printf 'helper\n'       >"$PAD/rs_mutant.py"
printf 'backup\n'       >"$PAD/$LIVE/tailnet.rs.orig"
printf 'landed notes\n' >"$PAD/$LANDED/notes.md"
printf 'landed log\n'   >"$AROOT/$LANDED/run.log"
printf 'live log\n'     >"$AROOT/$LIVE/run.log"

A_LANDED="$ALLOC/$DEAD_PID-mutation-aaaa1111"
A_LANDED_LIVE="$ALLOC/$LIVE_PID-pool-bbbb2222"
A_LIVE="$ALLOC/$DEAD_PID-harness-cccc3333"
A_NOROW="$ALLOC/$DEAD_PID-unrecorded-dddd4444"
mkdir -p "$A_LANDED" "$A_LANDED_LIVE" "$A_LIVE" "$A_NOROW"
for d in "$A_LANDED" "$A_LANDED_LIVE" "$A_LIVE" "$A_NOROW"; do printf 'x\n' >"$d/f"; done
row() { # <path> <pid> <cwd>
    printf '{"path":"%s","label":"t","pid":%s,"ppid":1,"session":"%s","cwd":"%s","created":"2026-10-01T21:00:00Z","ttl_minutes":360,"event":"new"}\n' \
        "$(cd "$1" && pwd -P)" "$2" "$SID" "$3" >>"$SCRATCH_LEDGER"
}
row "$A_LANDED" "$DEAD_PID" "$WS_LANDED/richos/engine"
row "$A_LANDED_LIVE" "$LIVE_PID" "$WS_LANDED/richos/engine"
row "$A_LIVE" "$DEAD_PID" "$WS_LIVE/richos/app"

# The registry, as a land leaves it: the landed agent's record carries its
# session and its (now deleted) workspaces.
python3 - "$RICHOS_WORKSPACES_DIR/agents" "$SID" "$LANDED" "$WS_LANDED" <<'PY'
import json, os, sys
d, sid, name, ws = sys.argv[1:5]
key = "%s--%s" % (sid, name)
rec = {"key": key, "name": name, "session_id": sid, "agent_id": "a1",
       "workspaces": [{"path": ws, "repo": ws, "deleted_at": 1}],
       "disposition": {"kind": "landed", "at": 1}, "continues": []}
json.dump(rec, open(os.path.join(d, key + ".json"), "w"))
PY

PLAN="$SANDBOX/plan.json"
SCOPE=(--agent "$LANDED" --session "$SID" --workspace "$WS_LANDED")
echo "=== scratch land scope tests ==="

# --- L1-L4: the plan -------------------------------------------------------
if bash "$REAPER" --json "${SCOPE[@]}" >"$PLAN" 2>"$SANDBOX/plan.err" || [ $? -eq 3 ]; then
    PLANNED=1
else
    PLANNED=0
fi
python3 - "$PLAN" "$PLANNED" "$PAD" "$AROOT" "$LANDED" "$A_LANDED" "$A_LANDED_LIVE" "$LIVE" >"$SANDBOX/plan.verdict" <<'PY'
import json, os, sys
plan, planned, pad, aroot, landed, a_landed, a_landed_live, live = sys.argv[1:9]
real = os.path.realpath
try:
    d = json.load(open(plan)) if planned == "1" else None
except ValueError:
    d = None
if not d:
    print("L1 FAIL the reaper has no scoped plan (it refused --agent)")
    print("L2 FAIL no plan"); print("L3 FAIL no plan"); print("L4 FAIL no plan")
    raise SystemExit(0)
dele = {real(e["path"]) for e in d["entries"] if e["action"] == "DELETE"}
want = {real(os.path.join(pad, landed)), real(os.path.join(aroot, landed)), real(a_landed)}
print("L1 %s planned DELETE %s" % ("PASS" if dele == want else "FAIL", sorted(dele)))
others = [real(os.path.join(pad, n)) for n in ("briefs", "rs_mutant.py", live)]
named = [e for e in d["entries"]
         if real(e["path"]) == real(pad)
         or any(real(e["path"]) == s or real(e["path"]).startswith(s + "/") for s in others)]
print("L2 %s shared scratchpad entries planned: %s" % ("PASS" if not named else "FAIL", named))
keep = [e for e in d["entries"] if real(e["path"]) == real(a_landed_live)]
print("L3 %s live-owned allocation: %s" % ("PASS" if keep and keep[0]["action"] == "KEEP" else "FAIL", keep))
# unattributed: allocator (the live agent's row + the row-less one), the shared
# scratchpad (briefs, rs_mutant.py, the live agent's dir), the agent root (the
# live agent's dir) = 2 + 3 + 1
print("L4 %s unattributed=%s (want 6)" % ("PASS" if d.get("unattributed") == 6 else "FAIL", d.get("unattributed")))
PY
while IFS= read -r line; do
    case "$line" in
        *" PASS "*) ok "$line" ;;
        *) bad "$line" ;;
    esac
done <"$SANDBOX/plan.verdict"

# --- L5-L7: the land's own path, only on an engine that has the scope -------
if ! grep -q '^L1 PASS' "$SANDBOX/plan.verdict"; then
    bad "L5  skipped: this engine has no scoped land sweep, and its land path is the whole-machine sweep, which this suite never runs"
    bad "L6  skipped (no scope)"
    bad "L7  skipped (no scope)"
else
    python3 - "$WS_PY" "$SID" "$LANDED" "$RICHOS_WORKSPACES_DIR/agents" >"$SANDBOX/land.out" 2>"$SANDBOX/land.err" <<'PY'
import contextlib, importlib.util, io, json, os, sys
ws_py, sid, landed, agents = sys.argv[1:5]
spec = importlib.util.spec_from_file_location("ws", ws_py)
ws = importlib.util.module_from_spec(spec); spec.loader.exec_module(ws)
print("HAS", hasattr(ws, "land_sweep_scopes"))
scopes = ws.land_sweep_scopes(landed, sid)
print("SCOPES", json.dumps(scopes))
out, err = io.StringIO(), io.StringIO()
with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
    ws.sweep_scratch_after_land(scopes)
print("PRINTED", out.getvalue().strip().replace("\n", " | "))
print("STDERR", err.getvalue().strip().replace("\n", " | "))
print("NOSCOPE", json.dumps(ws.land_sweep_scopes("nobody-opus-x9", sid)))
PY
    GONE=1
    for p in "$PAD/$LANDED" "$AROOT/$LANDED" "$A_LANDED"; do [ -e "$p" ] && GONE=0; done
    KEPT=1
    for p in "$PAD" "$PAD/briefs/isaac-r3floor.md" "$PAD/rs_mutant.py" "$PAD/$LIVE/tailnet.rs.orig" \
             "$AROOT/$LIVE/run.log" "$A_LANDED_LIVE/f" "$A_LIVE/f" "$A_NOROW/f"; do
        [ -e "$p" ] || { KEPT=0; echo "    missing: $p"; }
    done
    PRINTED="$(sed -n 's/^PRINTED //p' "$SANDBOX/land.out")"
    if [ "$GONE" = 1 ] && [ "$KEPT" = 1 ] \
       && printf '%s' "$PRINTED" | grep -q "scratch swept for $LANDED" \
       && printf '%s' "$PRINTED" | grep -q "6 entries not provably its own, left alone"; then
        ok "L5  the land path deleted only $LANDED's three items; the lead's briefs, the live agent's files and other allocations remain; it said: $PRINTED"
    else
        bad "L5  gone=$GONE kept=$KEPT printed='$PRINTED' out=$(tr '\n' ' ' <"$SANDBOX/land.out") err=$(tr '\n' ' ' <"$SANDBOX/land.err")"
    fi

    # L6: a live namesake (paused, so not finished) holds the named directories.
    mkdir -p "$PAD/$LANDED" "$AROOT/$LANDED"
    printf 'again\n' >"$PAD/$LANDED/n2"; printf 'again\n' >"$AROOT/$LANDED/n2"
    python3 - "$RICHOS_WORKSPACES_DIR/agents" "$LANDED" <<'PY'
import json, os, sys
d, name = sys.argv[1:3]
rec = {"key": "other--" + name, "name": name, "session_id": "", "agent_id": "a2",
       "workspaces": [], "pause": {"at": 1, "until": "x"}, "continues": []}
json.dump(rec, open(os.path.join(d, "other--" + name + ".json"), "w"))
PY
    python3 - "$WS_PY" "$SID" "$LANDED" >"$SANDBOX/hold.out" 2>&1 <<'PY'
import importlib.util, json, sys
ws_py, sid, landed = sys.argv[1:4]
spec = importlib.util.spec_from_file_location("ws", ws_py)
ws = importlib.util.module_from_spec(spec); spec.loader.exec_module(ws)
scopes = ws.land_sweep_scopes(landed, sid)
print("HOLD", json.dumps([s.get("hold_named") for s in scopes]))
ws.sweep_scratch_after_land(scopes)
PY
    if [ -e "$PAD/$LANDED/n2" ] && [ -e "$AROOT/$LANDED/n2" ] && grep -q 'HOLD \[true\]' "$SANDBOX/hold.out"; then
        ok "L6  a LIVE agent with the same name holds the named directories"
    else
        bad "L6  hold: $(tr '\n' ' ' <"$SANDBOX/hold.out")"
    fi

    if grep -q '^NOSCOPE \[\]$' "$SANDBOX/land.out"; then
        ok "L7  an agent with no record gets no scope, so its land sweeps nothing"
    else
        bad "L7  $(grep NOSCOPE "$SANDBOX/land.out")"
    fi
fi

echo ""
echo "=== scratch land scope tests: $FAIL FAILED, $PASS passed ==="
[ "$FAIL" -eq 0 ]
