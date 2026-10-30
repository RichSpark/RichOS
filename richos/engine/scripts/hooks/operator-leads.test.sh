#!/usr/bin/env bash
#
# operator-leads.test.sh: the engine side of several operator leads on one Mac
# (richos-hq spec r3 (e) the claim, e3, e5; r4 §2.4; Frank G11).
#
#   The claim (operator-claim.sh, SessionStart; guard-operator-claim.sh, PreToolUse)
#     C0   OFF: no claim written, nothing said, nothing refused, even with a live app claim;
#          no hook starts operator_leads.py (C0c), and asked directly it decides
#          nothing (C0d): the bash gate and the Python gate are each proven alone
#     C1   a terminal session in the entity claims at startup
#     C2   idempotent: `compact` and `resume` rewrite nothing (r4 §2.4)
#     C3   a print-mode (`sdk-cli`) session never claims as the terminal
#     C4   a session seated in .claude/worktrees/ never claims
#     C5   a live APP claim: the terminal claims nothing and says so
#     C6   ... and the terminal is refused Agent, SendMessage, the land
#          lease, a memory write and a record write; other calls pass
#     C7   the app's own lead (RICHOS_OPERATOR_LEAD matching the live claim) passes
#          and its SessionStart writes nothing
#     C8   an unreadable claim is held: refused, naming the way through
#     C9   a dead app claim is replaced by the terminal's
#     C10  G11: an IDE entrypoint (claude-vscode) counts as the terminal
#     C11  a second terminal session joins the terminal's claim
#     C12  land-lease.sh acquire itself refuses the terminal while the app's claim is
#          live (so commit-ceo-inputs.py, which takes the lease as a program, is
#          barred too); a print-mode session and a claim-free terminal take it
#   e3 (guard-shared-writes.sh, PreToolUse; release-shared-writes.sh, PostToolUse[Failure])
#     S0   OFF: no lease taken, nothing refused
#     S1   a memory write takes the lease; PostToolUse releases it
#     S2   PostToolUseFailure releases it too
#     S3   another session's fresh lease is waited for, then taken once released
#     S4   a lease whose session has ended, or older than 5 s, is taken at once
#     S5   the same tool_use_id delivered again passes
#     S6   after the wait, a held lease refuses naming the holder (unit)
#     S9   a stale lease that cannot be removed is refused at the deadline (unit)
#     S7   a record write without the land lease is refused; with it, it passes
#     S8   a gitignored path, a native worktree and an unfenced repository pass
#   e5 (guard-live-names.sh, PreToolUse[Agent])
#     N0   OFF: never refused
#     N1   a name a live agent of ANOTHER session holds is refused
#     N2   the same session's own name is left to clause 3 (passes here)
#     N3   a finished agent's name, and an ended session's, are free
#
# Every case runs in scratch under the engine's allocator, with its own
# CLAUDE_CONFIG_DIR, lease home, workspace registry and repositories. Sessions
# are the fence fixture's servers (scripts/lib/operator-fences-fixture.sh):
# processes with their own session records, ended by the pid captured at spawn.
#
# Usage: scripts/hooks/operator-leads.test.sh

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LIB="$ENGINE_ROOT/scripts/lib/operator_leads.py"
H_CLAIM="$SCRIPT_DIR/operator-claim.sh"
H_GUARD="$SCRIPT_DIR/guard-operator-claim.sh"
H_SW="$SCRIPT_DIR/guard-shared-writes.sh"
H_REL="$SCRIPT_DIR/release-shared-writes.sh"
H_NAMES="$SCRIPT_DIR/guard-live-names.sh"
# shellcheck source=../lib/operator-fences-fixture.sh
. "$ENGINE_ROOT/scripts/lib/operator-fences-fixture.sh"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; [ -n "${2:-}" ] && printf '        %s\n' "$2" | head -8; FAIL=$((FAIL + 1)); return 0; }
check() { # <case> <condition-rc> [detail]
    if [ "$2" = 0 ]; then ok "$1"; else bad "$1" "${3:-}"; fi
}

ofx_init || exit 1
trap ofx_cleanup EXIT
# A folder with a space leads PATH for the whole run, as the app's installed runtime folder does in
# an app shell, so the quoting C0c depends on is exercised in every environment and not only there.
mkdir -p "$OFX/bin with space"
PATH="$OFX/bin with space:$PATH"
export RICHOS_WORKSPACES_DIR="$OFX/workspaces"
unset RICHOS_OPERATOR_LEAD CLAUDE_PROJECT_DIR 2>/dev/null
CLAIM="$OFX/claude/state/operator-lead.json"
MEM="$OFX/claude/projects/-fixture-entity/memory"
mkdir -p "$MEM" "$OFX/p"

# The entity: a main checkout carrying the adoption marker, with the launcher
# installed (off). The record: a second fenced repository. A third repository
# never gets a launcher.
ofx_repo ent; E="$OFX_R"
printf 'OPERATOR_FENCES="on"\n' > "$E/orchestration.config"
printf 'ignored/\n' > "$E/.gitignore"
git -C "$E" add orchestration.config .gitignore && git -C "$E" commit -q -m marker && git -C "$E" push -q origin main
ofx_repo rec; R="$OFX_R"
ofx_repo plain; P="$OFX_R"
ofx_install "$E" >/dev/null 2>&1 || { echo "fixture: install failed"; exit 1; }
ofx_install "$R" >/dev/null 2>&1 || { echo "fixture: install failed"; exit 1; }
switch() { # on|off, both fenced repositories
    if [ "$1" = on ]; then ofx_on "$E" >/dev/null 2>&1 && ofx_on "$R" >/dev/null 2>&1
    else ofx_off "$E" >/dev/null 2>&1 && ofx_off "$R" >/dev/null 2>&1; fi
}

# A payload file: pay <name> <python-literal dict>
pay() { python3 -c 'import json,sys; print(json.dumps(eval(sys.argv[2])))' "$1" "$2" > "$OFX/p/$1.json"; }
# Rewrite a fixture session's record: sess <name> <entrypoint> <cwd>
sess() {
    local pid; pid="$(cat "$OFX/s/$1.pid")"
    python3 - "$CLAUDE_CONFIG_DIR/sessions/$pid.json" "$2" "$3" <<'P'
import json, sys
path, entry, cwd = sys.argv[1:4]
rec = json.load(open(path)); rec["entrypoint"] = entry; rec["cwd"] = cwd
json.dump(rec, open(path, "w"))
P
}
# Run a hook inside a session: hook <session> <hook> <payload> [ENV=VAL ...]
hook() {
    local s="$1" h="$2" p="$3"; shift 3
    ofx_in "$s" "cd '$E' && env CLAUDE_PROJECT_DIR='$E' $* bash '$h' < '$OFX/p/$p.json'"
}
ident() { # <session> -> "pid start" from the kernel
    python3 -c "import sys; sys.path.insert(0,'$ENGINE_ROOT/scripts/lib'); import operator_fences as O; p=O.proc(int(open('$OFX/s/$1.pid').read())); print(p['pid'], p['start'])"
}
plant_claim() { # <owner> <claim_id> <session>... (a live process list), or "dead"
    local owner="$1" cid="$2"; shift 2
    local procs="" s
    for s in "$@"; do
        if [ "$s" = dead ]; then procs="$procs {\"role\":\"app\",\"pid\":999999,\"start\":1},"
        else set -- $(ident "$s"); procs="$procs {\"role\":\"lead\",\"pid\":$1,\"start\":$2},"; fi
    done
    mkdir -p "$(dirname "$CLAIM")"
    printf '{"schema":1,"owner":"%s","claim_id":"%s","processes":[%s],"leads":[{"pid":0,"start":0,"title":"Fixture conversation"}]}\n' \
        "$owner" "$cid" "${procs%,}" > "$CLAIM"
}
mtime() { python3 -c 'import os,sys; print(os.stat(sys.argv[1]).st_mtime_ns)' "$1" 2>/dev/null || echo none; }

ofx_session T; sess T cli "$E"            # the terminal
ofx_session D; sess D sdk-cli "$E"        # a print-mode session in the entity
ofx_session W; sess W cli "$E/.claude/worktrees/agent-x"
ofx_session V; sess V claude-vscode "$E"
ofx_session APP                            # stands in for the app and its lead
ofx_session U; sess U cli "$E"            # a second terminal-shaped session

pay start  "{'hook_event_name':'SessionStart','source':'startup','session_id':'T','cwd':'$E'}"
pay compact "{'hook_event_name':'SessionStart','source':'compact','session_id':'T','cwd':'$E'}"
pay resume "{'hook_event_name':'SessionStart','source':'resume','session_id':'T','cwd':'$E'}"
pay agent  "{'tool_name':'Agent','session_id':'T','cwd':'$E','tool_input':{'name':'mark-sonnet-x1','prompt':'p'}}"
pay stop   "{'tool_name':'TaskStop','session_id':'T','cwd':'$E','tool_input':{'task_id':'t1'}}"
pay msg    "{'tool_name':'SendMessage','session_id':'T','cwd':'$E','tool_input':{'to':'x','message':'m'}}"
pay lease  "{'tool_name':'Bash','session_id':'T','cwd':'$E','tool_input':{'command':'bash $ENGINE_ROOT/scripts/land-lease.sh acquire --repo $R'}}"
pay ls     "{'tool_name':'Bash','session_id':'T','cwd':'$E','tool_input':{'command':'ls -la'}}"
pay memw   "{'tool_name':'Write','session_id':'T','tool_use_id':'tu-1','cwd':'$E','tool_input':{'file_path':'$MEM/MEMORY.md','content':'x'}}"
pay recw   "{'tool_name':'Edit','session_id':'T','tool_use_id':'tu-2','cwd':'$E','tool_input':{'file_path':'$R/f.txt','old_string':'a','new_string':'b'}}"
pay elsew  "{'tool_name':'Write','session_id':'T','tool_use_id':'tu-3','cwd':'$E','tool_input':{'file_path':'$P/new.txt','content':'x'}}"

echo "=== operator-leads: the claim ==="

# C0: the switch is off (installed off). With a LIVE app claim planted, nothing refuses.
rm -f "$CLAIM"
hook T "$H_CLAIM" start; rc=$?
{ [ $rc = 0 ] && [ -z "$OFX_OUT" ] && [ ! -e "$CLAIM" ]; }; check "C0a OFF: SessionStart writes no claim and says nothing" $? "rc=$rc out=$OFX_OUT"
plant_claim app app-1 APP
r=0; for p in agent stop msg lease memw recw; do hook T "$H_GUARD" "$p" || r=1; [ -z "$OFX_OUT" ] || r=1; done
check "C0b OFF: with a live app claim, nothing is refused and nothing is said" $r "$OFX_OUT"

# C0c: the bash layer. A python3 first on PATH records every start of
# operator_leads.py and then runs the real interpreter. OFF: no hook starts it.
REALPY="$(command -v python3)"
mkdir -p "$OFX/fakepy"
printf '#!/bin/bash\ncase "$*" in *operator_leads.py*) echo started >> "%s/leads-started" ;; esac\nexec "%s" "$@"\n' \
    "$OFX" "$REALPY" > "$OFX/fakepy/python3"
chmod +x "$OFX/fakepy/python3"
# `hook` puts its extra arguments into a command line that is parsed again, so the value is quoted
# for that parse. In an app shell PATH begins with the installed engine's runtime folder, whose
# path has a space ("Application Support"); unquoted, `env` took the first half as PATH and the
# second half as the command, no hook ran, and C0c failed on a clean main (off_started=0 only
# because nothing ran, then no start once the fences were back on).
FP="PATH=$(printf '%q' "$OFX/fakepy:$PATH")"
hook T "$H_CLAIM" start "$FP"; hook T "$H_GUARD" agent "$FP"; hook T "$H_NAMES" agent "$FP"
hook T "$H_SW" memw "$FP"; hook T "$H_REL" memw "$FP"
off_started=0; [ -f "$OFX/leads-started" ] && off_started=1

# C0d: the Python layer, asked directly (no wrapper): OFF decides nothing either.
ofx_in T "cd '$E' && OPERATOR_ENTITY_ROOT='$E' python3 '$LIB' claim-guard < '$OFX/p/agent.json'"; d1=$?; o1="$OFX_OUT"
ofx_in T "cd '$E' && OPERATOR_ENTITY_ROOT='$E' python3 '$LIB' shared-writes-pre < '$OFX/p/memw.json'"; d2=$?
{ [ $d1 = 0 ] && [ -z "$o1" ] && [ $d2 = 0 ] && [ ! -d "$OFX/claude/state/shared-writes" ]; }
check "C0d OFF: operator_leads.py itself decides nothing when asked directly" $? "d1=$d1 d2=$d2 $o1"
rm -f "$CLAIM"

switch on
hook T "$H_GUARD" agent "$FP"
{ [ "$off_started" = 0 ] && [ -f "$OFX/leads-started" ]; }
check "C0c OFF: no hook starts operator_leads.py (and ON, the same call does)" $? "off_started=$off_started"
rm -f "$OFX/leads-started"
hook T "$H_CLAIM" start; rc=$?
python3 - "$CLAIM" "$(cat "$OFX/s/T.pid")" <<'P'; c1=$?
import json, sys
rec = json.load(open(sys.argv[1]))
assert rec["owner"] == "terminal" and rec["schema"] == 1
assert [p["pid"] for p in rec["processes"]] == [int(sys.argv[2])]
P
{ [ $rc = 0 ] && [ $c1 = 0 ] && [ -z "$OFX_OUT" ]; }; check "C1 a terminal session claims at startup, silently" $? "rc=$rc $OFX_OUT"

before="$(mtime "$CLAIM")"; sum1="$(shasum "$CLAIM")"
hook T "$H_CLAIM" compact; hook T "$H_CLAIM" resume; hook T "$H_CLAIM" start
{ [ "$(mtime "$CLAIM")" = "$before" ] && [ "$(shasum "$CLAIM")" = "$sum1" ]; }
check "C2 idempotent: compact, resume and a repeat startup rewrite nothing" $? "mtime $before -> $(mtime "$CLAIM")"

rm -f "$CLAIM"
hook D "$H_CLAIM" start; hook W "$H_CLAIM" start
[ ! -e "$CLAIM" ]; check "C3 a print-mode (sdk-cli) session never claims as the terminal" $?
[ ! -e "$CLAIM" ]; check "C4 a session seated in .claude/worktrees/ never claims" $?

plant_claim app app-1 APP
sum_app="$(shasum "$CLAIM")"
hook T "$H_CLAIM" start; rc=$?
{ [ $rc = 0 ] && [ "$(shasum "$CLAIM")" = "$sum_app" ] && printf '%s' "$OFX_OUT" | python3 -c 'import json,sys; m=json.load(sys.stdin)["systemMessage"]; sys.exit(0 if "running in the RichOS app (\"Fixture conversation\")" in m else 1)'; }
check "C5 a live app claim: the terminal claims nothing and says so" $? "$OFX_OUT"

r=0; for p in agent msg lease memw recw; do
    hook T "$H_GUARD" "$p"; rc=$?
    { [ $rc = 2 ] && printf '%s' "$OFX_OUT" | grep -q 'running in the RichOS app'; } || { r=1; echo "        $p rc=$rc"; }
done
check "C6a with the app claim live, the terminal is refused Agent, SendMessage, the lease, memory and record writes" $r
r=0; for p in ls elsew; do hook T "$H_GUARD" "$p" || r=1; [ -z "$OFX_OUT" ] || r=1; done
check "C6b ... and every other call passes, silently" $r "$OFX_OUT"
hook D "$H_GUARD" agent; rc=$?
check "C6c ... and a non-terminal session is never refused by the claim" $rc "$OFX_OUT"
hook T "$H_GUARD" stop; rc=$?
{ [ $rc = 0 ] && [ -z "$OFX_OUT" ]; }; check "C6d ... and a TaskStop is never refused: a stop only removes (§67)" $? "rc=$rc $OFX_OUT"

hook APP "$H_GUARD" agent RICHOS_OPERATOR_LEAD=app-1; r1=$?
sess APP cli "$E"
hook APP "$H_GUARD" agent RICHOS_OPERATOR_LEAD=app-1; r2=$?
hook APP "$H_CLAIM" start RICHOS_OPERATOR_LEAD=app-1; r3=$?
hook APP "$H_GUARD" agent RICHOS_OPERATOR_LEAD=wrong-id; r4=$?
{ [ $r1 = 0 ] && [ $r2 = 0 ] && [ $r3 = 0 ] && [ "$(shasum "$CLAIM")" = "$sum_app" ] && [ $r4 = 2 ]; }
check "C7 the app's own lead passes and claims nothing; a wrong claim id does not" $? "r1=$r1 r2=$r2 r3=$r3 r4=$r4"
sess APP sdk-cli "$E"

printf 'not json' > "$CLAIM"
hook T "$H_GUARD" agent; rc=$?
hook T "$H_CLAIM" start; rc2=$?
{ [ $rc = 2 ] && printf '%s' "$OFX_OUT" | grep -q "rm '$CLAIM'" && [ "$(cat "$CLAIM")" = "not json" ]; }
check "C8 an unreadable claim is held: refused, and the way through is named" $? "rc=$rc $OFX_OUT"

plant_claim app app-9 dead
hook T "$H_CLAIM" start
python3 -c "import json,sys; r=json.load(open('$CLAIM')); sys.exit(0 if r['owner']=='terminal' else 1)"
check "C9 a dead app claim is replaced by the terminal's" $?

rm -f "$CLAIM"
hook V "$H_CLAIM" start
python3 -c "import json,sys; r=json.load(open('$CLAIM')); sys.exit(0 if r['owner']=='terminal' else 1)" 2>/dev/null
check "C10 G11: an IDE entrypoint (claude-vscode) counts as the terminal" $?
hook U "$H_CLAIM" start
python3 -c "import json,sys; r=json.load(open('$CLAIM')); sys.exit(0 if len(r['processes'])==2 else 1)" 2>/dev/null
check "C11 a second terminal session joins the terminal's claim" $?
rm -f "$CLAIM"

# C12: the lease itself refuses the terminal while the app runs his team, so a
# program that takes it (commit-ceo-inputs.py) is barred as surely as a typed
# command; a print-mode session still takes it, and so does the terminal once
# the claim is gone.
LL="bash '$ENGINE_ROOT/scripts/land-lease.sh'"
plant_claim app app-1 APP
ofx_in T "$LL acquire --repo '$R' --wait 0"; r1=$?; o1="$OFX_OUT"
n1="$(ls "$OFX/locks/"*.lease 2>/dev/null | grep -c .)"
ofx_in D "$LL acquire --repo '$R' --wait 0"; r2=$?
ofx_in D "$LL release --repo '$R'"
rm -f "$CLAIM"
ofx_in T "$LL acquire --repo '$R' --wait 0"; r3=$?
ofx_in T "$LL release --repo '$R'"
{ [ $r1 = 2 ] && printf '%s' "$o1" | grep -q "running in the RichOS app" && [ "$n1" = 0 ] && [ $r2 = 0 ] && [ $r3 = 0 ]; }
check "C12 land-lease.sh acquire refuses the terminal while the app's claim is live; sdk-cli and a claim-free terminal take it" $? "r1=$r1 n1=$n1 r2=$r2 r3=$r3 $o1"

echo "=== operator-leads: e3, shared writes ==="
LEASES="$OFX/claude/state/shared-writes"
lease_files() { ls "$LEASES"/*.lease 2>/dev/null | grep -c . ; }
switch off
hook T "$H_SW" memw; r1=$?; hook T "$H_SW" recw; r2=$?
{ [ $r1 = 0 ] && [ $r2 = 0 ] && [ "$(lease_files)" = 0 ] && [ -z "$OFX_OUT" ]; }
check "S0 OFF: no lease is taken and nothing is refused" $? "r1=$r1 r2=$r2 leases=$(lease_files)"
switch on

hook T "$H_SW" memw; rc=$?
n1="$(lease_files)"
pay post "{'hook_event_name':'PostToolUse','tool_name':'Write','session_id':'T','tool_use_id':'tu-1','cwd':'$E','tool_input':{'file_path':'$MEM/MEMORY.md'}}"
hook T "$H_REL" post
{ [ $rc = 0 ] && [ "$n1" = 1 ] && [ "$(lease_files)" = 0 ]; }
check "S1 a memory write takes the lease and PostToolUse releases it" $? "rc=$rc n1=$n1 after=$(lease_files)"

hook T "$H_SW" memw
pay fail "{'hook_event_name':'PostToolUseFailure','tool_name':'Write','session_id':'T','tool_use_id':'tu-1','cwd':'$E','tool_input':{'file_path':'$MEM/MEMORY.md'},'error':'refused'}"
hook T "$H_REL" fail
[ "$(lease_files)" = 0 ]; check "S2 PostToolUseFailure releases it too" $?

pay memw2 "{'tool_name':'Edit','session_id':'U','tool_use_id':'tu-9','cwd':'$E','tool_input':{'file_path':'$MEM/other.md','old_string':'a','new_string':'b'}}"
hook T "$H_SW" memw
( sleep 1.5; rm -f "$LEASES"/*.lease ) &
releaser=$!
t0=$(date +%s)
hook U "$H_SW" memw2; rc=$?
waited=$(( $(date +%s) - t0 )); wait "$releaser" 2>/dev/null
{ [ $rc = 0 ] && [ "$waited" -ge 1 ] && [ "$(lease_files)" = 1 ]; }
check "S3 another session's fresh lease is waited for, then taken once released" $? "rc=$rc waited=${waited}s $OFX_OUT"
rm -f "$LEASES"/*.lease

python3 - "$LEASES" "$MEM" "$LIB" <<'P'
import json, os, sys, time
sys.path.insert(0, os.path.dirname(sys.argv[3]))
import operator_leads as L
path = L.memory_lease_path(sys.argv[2])
json.dump({"pid": 999999, "start": 1, "session_id": "gone", "tool_use_id": "old", "at": time.time()}, open(path, "w"))
P
t0=$(date +%s); hook U "$H_SW" memw2; r1=$?; w1=$(( $(date +%s) - t0 ))
rm -f "$LEASES"/*.lease
python3 - "$MEM" "$LIB" "$(ident T)" <<'P'
import json, os, sys, time
sys.path.insert(0, os.path.dirname(sys.argv[2]))
import operator_leads as L
pid, start = map(int, sys.argv[3].split())
json.dump({"pid": pid, "start": start, "session_id": "T", "tool_use_id": "stuck", "at": time.time() - 6},
          open(L.memory_lease_path(sys.argv[1]), "w"))
P
t0=$(date +%s); hook U "$H_SW" memw2; r2=$?; w2=$(( $(date +%s) - t0 ))
{ [ $r1 = 0 ] && [ $r2 = 0 ] && [ $w1 -le 3 ] && [ $w2 -le 3 ]; }
check "S4 a lease whose session has ended, or older than 5 s, is taken at once" $? "r1=$r1 w1=$w1 r2=$r2 w2=$w2"

t0=$(date +%s); hook U "$H_SW" memw2; rc=$?; w=$(( $(date +%s) - t0 ))
{ [ $rc = 0 ] && [ $w -le 2 ]; }; check "S5 the same tool_use_id delivered again passes at once" $? "rc=$rc waited=${w}s $OFX_OUT"
rm -f "$LEASES"/*.lease

python3 - "$MEM" "$LIB" "$(ident T)" <<'P'; rc=$?
import json, os, sys, time
sys.path.insert(0, os.path.dirname(sys.argv[2]))
import operator_leads as L
pid, start = map(int, sys.argv[3].split())
path = L.memory_lease_path(sys.argv[1])
ok, _ = L.take_memory_lease(path, {"pid": pid, "start": start, "session_id": "T", "tool_use_id": "a"})
assert ok
got, holder = L.take_memory_lease(path, {"pid": pid, "start": start, "session_id": "U", "tool_use_id": "b"}, wait=0.3)
assert not got and holder["tool_use_id"] == "a", (got, holder)
os.unlink(path)
P
check "S6 after the wait, a held lease is refused naming its holder (unit)" $rc

# S9 (hunt 2026-09-29 part 5, finding 61): a stale lease that cannot be removed
# used to `continue` straight past the deadline, so the O_EXCL create failed and
# retried forever and the worker's tool call never came back. Each attempt runs
# in a child with its own timeout, because on the defect the call never returns.
python3 - "$OFX/s9" "$LIB" <<'P'; rc=$?
import json, os, subprocess, sys, time
base, lib = sys.argv[1], os.path.dirname(sys.argv[2])
child = r'''
import json, os, sys, time
sys.path.insert(0, sys.argv[1])
import operator_leads as L
t0 = time.monotonic()
got, holder = L.take_memory_lease(sys.argv[2], {"pid": os.getpid(), "start": 0, "session_id": "U",
                                                "tool_use_id": "s9"}, wait=0.3)
print(json.dumps({"got": got, "holder": holder, "took": time.monotonic() - t0}))
'''
where = os.path.join(base, "dir", "memory-s9.lease")      # a directory where the lease file belongs
os.makedirs(where)
ro = os.path.join(base, "ro")                             # a stale lease whose directory refuses deletion
os.makedirs(ro)
stale = os.path.join(ro, "memory-s9.lease")
with open(stale, "w") as fh:
    json.dump({"pid": 999999, "start": 1, "session_id": "gone", "tool_use_id": "old", "at": time.time() - 60}, fh)
os.chmod(ro, 0o500)
bad = []
try:
    for name, path in (("a directory", where), ("a stale file that cannot be deleted", stale)):
        try:
            r = subprocess.run([sys.executable, "-c", child, lib, path], capture_output=True, text=True, timeout=5)
        except subprocess.TimeoutExpired:
            bad.append("%s: still spinning 5 s after a 0.3 s wait" % name)
            continue
        if r.returncode != 0:
            bad.append("%s: exit %d %s" % (name, r.returncode, r.stderr.strip()[-300:]))
            continue
        got = json.loads(r.stdout)
        if got["got"] or got["took"] > 2 or not (got["holder"] or {}).get("unremovable"):
            bad.append("%s: %r" % (name, got))
finally:
    os.chmod(ro, 0o700)
if bad:
    print("\n".join(bad))
    sys.exit(1)
P
check "S9 a stale lease that cannot be removed is refused at the deadline, saying so (unit)" $rc

hook T "$H_SW" recw; r1=$?; out1="$OFX_OUT"
ofx_in T "bash '$ENGINE_ROOT/scripts/land-lease.sh' acquire --repo '$R'"
hook T "$H_SW" recw; r2=$?
ofx_in T "bash '$ENGINE_ROOT/scripts/land-lease.sh' release --repo '$R'"
{ [ $r1 = 2 ] && printf '%s' "$out1" | grep -q "land-lease.sh acquire --repo $R" && [ $r2 = 0 ]; }
check "S7 a record write without the land lease is refused; with it, it passes" $? "r1=$r1 r2=$r2 $out1"

git -C "$R" -c core.hooksPath=/dev/null worktree add -q "$R/.claude/worktrees/agent-y" -b wt-y >/dev/null 2>&1
pay ign  "{'tool_name':'Write','session_id':'T','tool_use_id':'tu-4','cwd':'$E','tool_input':{'file_path':'$E/ignored/x.txt','content':'x'}}"
pay wtw  "{'tool_name':'Write','session_id':'T','tool_use_id':'tu-5','cwd':'$E','tool_input':{'file_path':'$R/.claude/worktrees/agent-y/f.txt','content':'x'}}"
r=0; for p in ign wtw elsew; do hook T "$H_SW" "$p" || { r=1; echo "        $p: $OFX_OUT"; }; done
check "S8 a gitignored path, a native worktree and an unfenced repository pass" $r

DISPATCH="$SCRIPT_DIR/dispatch-pretooluse.sh"
ofx_in T "cd '$E' && env CLAUDE_PROJECT_DIR='$E' bash '$DISPATCH' Write < '$OFX/p/recw.json'"; r1=$?; o1="$OFX_OUT"
plant_claim app app-1 APP
ofx_in T "cd '$E' && env CLAUDE_PROJECT_DIR='$E' bash '$DISPATCH' Bash < '$OFX/p/lease.json'"; r2=$?; o2="$OFX_OUT"
rm -f "$CLAIM"
{ [ $r1 = 2 ] && printf '%s' "$o1" | grep -q "OPERATOR SHARED WRITES" && [ $r2 = 2 ] && printf '%s' "$o2" | grep -q "running in the RichOS app"; }
check "D1 through the dispatcher: a lease-less record write and, with the app claim live, a terminal's lease command are refused" $? "r1=$r1 r2=$r2 $o1 $o2"

python3 - "$ENGINE_ROOT" <<'P'; rc=$?
import json, os, sys
root = sys.argv[1]
hooks = json.load(open(os.path.join(root, "hooks", "hooks.json")))["hooks"]
def where(script):
    return sorted({(event, group.get("matcher") or "") for event, groups in hooks.items() for group in groups
                   for h in group["hooks"] if h.get("command", "").endswith("/scripts/hooks/" + script)})
manifest = [l.strip() for l in open(os.path.join(root, "scripts", "hooks", "dispatch-pretooluse.manifest"))
            if l.strip() and not l.startswith("#")]
W = "Write|Edit|MultiEdit|NotebookEdit"
want = {
    "operator-claim.sh": [("SessionStart", "")],
    "guard-operator-claim.sh": [("PreToolUse", "Agent"), ("PreToolUse", "SendMessage")],
    "guard-live-names.sh": [("PreToolUse", "Agent")],
    "release-shared-writes.sh": [("PostToolUse", W), ("PostToolUseFailure", W)],
    "guard-shared-writes.sh": [],
}
bad = [s for s, w in want.items() if where(s) != sorted(w)]
for line in ("Bash|guard-operator-claim.sh", "Write|guard-operator-claim.sh", "Write|guard-shared-writes.sh"):
    if line not in manifest:
        bad.append(line)
if bad:
    print(bad)
    sys.exit(1)
P
check "R1 every piece is registered where it must fire (PostToolUseFailure included)" $rc

echo "=== operator-leads: e5, live names ==="
python3 - "$ENGINE_ROOT" "$(cat "$OFX/s/APP.pid")" <<'P'
import os, sys
sys.path.insert(0, os.path.join(sys.argv[1], "mega-lander"))
import workspaces as W
W._ensure_dirs()
pid = int(sys.argv[2])
st, text = W.process_start(pid)
assert st == "ok", (st, text)
live = W.new_record(W.named_key("other-session", "mark-sonnet-x1"), name="mark-sonnet-x1",
                    session_id="other-session", agent_id="a-live",
                    session_identity={"pid": pid, "pid_start": text})
W.save_agent(live)
done = W.new_record(W.named_key("other-session", "mark-sonnet-x2"), name="mark-sonnet-x2",
                    session_id="other-session", agent_id="a-done",
                    session_identity={"pid": pid, "pid_start": text},
                    end={"signal": "SubagentStop", "at": W.now(), "detail": ""}, handed_in={"at": W.now()})
W.save_agent(done)
gone = W.new_record(W.named_key("gone-session", "mark-sonnet-x3"), name="mark-sonnet-x3",
                    session_id="gone-session", agent_id="a-gone",
                    session_identity={"pid": 999999, "pid_start": "Thu Jan  1 00:00:01 1970"})
W.save_agent(gone)
P
pay spawn1 "{'tool_name':'Agent','session_id':'T','cwd':'$E','tool_input':{'name':'mark-sonnet-x1','prompt':'p'}}"
pay spawn1same "{'tool_name':'Agent','session_id':'other-session','cwd':'$E','tool_input':{'name':'mark-sonnet-x1','prompt':'p'}}"
pay spawn2 "{'tool_name':'Agent','session_id':'T','cwd':'$E','tool_input':{'name':'mark-sonnet-x2','prompt':'p'}}"
pay spawn3 "{'tool_name':'Agent','session_id':'T','cwd':'$E','tool_input':{'name':'mark-sonnet-x3','prompt':'p'}}"

switch off
hook T "$H_NAMES" spawn1; rc=$?
check "N0 OFF: a live name of another session is not refused" $rc "$OFX_OUT"
switch on
hook T "$H_NAMES" spawn1; rc=$?
{ [ $rc = 2 ] && printf '%s' "$OFX_OUT" | grep -q "mark-sonnet-x1 is held by a live agent of another session"; }
check "N1 a name a live agent of ANOTHER session holds is refused" $? "rc=$rc $OFX_OUT"
hook T "$H_NAMES" spawn1same; rc=$?
check "N2 the same session's own name is left to clause 3" $rc "$OFX_OUT"
hook T "$H_NAMES" spawn2; r1=$?; hook T "$H_NAMES" spawn3; r2=$?
{ [ $r1 = 0 ] && [ $r2 = 0 ]; }; check "N3 a finished agent's name, and an ended session's, are free" $? "r1=$r1 r2=$r2 $OFX_OUT"

switch off
for s in T D W V APP U; do ofx_end "$s"; done
if [ -z "${RICHOS_MUTATION_INNER:-}" ] && [ -f "$SCRIPT_DIR/operator-leads.mutation.sh" ]; then
    echo "=== running the mutation harness ==="
    if bash "$SCRIPT_DIR/operator-leads.mutation.sh"; then
        ok "M. every rule above has been watched fail"
    else
        bad "M. the mutation harness found a property this suite does not actually prove"
    fi
fi

printf 'operator-leads: %d passed, %d FAILED\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
