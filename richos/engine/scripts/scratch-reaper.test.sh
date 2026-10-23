#!/usr/bin/env bash
#
# merge-gate: mutation-pass
# (Declared 2026-10-18. This suite and its 53-mutant harness run as ONE unit and took 2238 s
# in a land check on a 91% busy host, past the merge gate's 600 s cap. A header line is the one
# way autocheck.py's mutation_unit lets a unit say "the nightly engine run, not the merge",
# so the merge names it NOT RUN and nightly-engine.py runs it with every other unit.)
#
# scratch-reaper.test.sh — A DELETER IS JUDGED ON WHAT IT LEAVES ALONE.
#
# ===========================================================================
# WHAT THIS SUITE IS FOR
# ===========================================================================
# Proving that a reaper deletes is one case and it is the easy one. Every
# other case here asks the only question that matters about a program with
# rm -rf in it:
#
#     WHEN IT IS WRONG, IS IT WRONG IN THE DIRECTION THAT COSTS NOTHING?
#
# The liveness cases use a REAL SESSION PROCESS — /bin/sleep started as
# `exec -a "$SUITE_PROC"` — with a REAL entry in the process table and a REAL session
# file naming its pid and its start time. Nothing about liveness is faked or
# injected, because the defect this suite exists to catch is a liveness check
# that has quietly stopped working, and a faked process table cannot catch it.
#
#   S1  a LIVE session's scratch survives.
#   S1b THE POSITIVE CONTROL for S1 — the same directory, same contents, same
#       everything, with the process KILLED, is deleted. Without this, S1
#       passes just as well against a reaper that deletes nothing at all,
#       which is the way a safety test rots.
#   S2  a REGISTERED WORKSPACE inside a scratch root survives (wall 3).
#   S3  INDETERMINATE survives AND SAYS SO: a running claude process that no
#       session file names makes every unattributed scratch undecidable, and
#       the verdict line carries the count.
#   S4  a tree containing a .git survives (wall 2).
#   S5  the LOG carries every deleted path, with bytes and a reason.
#   S6  --apply prints, byte for byte, what the dry run printed, plus one
#       final `applied:` line. A plan that changes when you agree to it is not
#       a plan.
#   S7  a file in a scratch root that PREDATES every running claude process is
#       deleted; one written after the oldest running process started is kept.
#   S8  nightly retention keeps the newest N and deletes the rest, and never
#       touches the source worktree or the runtime.
#   S9  AN UNDECLARED THRESHOLD IS A REFUSAL, not a default. Exit 2, nothing
#       deleted.
#   S10 a $TMPDIR harness workspace that no process holds open is deleted; one
#       held open is kept.
#   S11 the verdict line exits 3 while anything is undecidable — undecidable is
#       a failure, never a footnote beside a success-shaped count.
#
# --- added 2026-09-18 with the allocator root and the legacy sweep -----------
# The 105 GB night: a sandbox named by a bare mktemp sat under $TMPDIR for five
# hours while this reaper ran twice, because SCRATCH_TMP_PATTERNS was an
# allowlist of two globs and nothing matched it.
#
#   S12  THE ALLOCATOR ROOT, DENY-BY-DEFAULT — the arm that would have caught
#        it. A dead owner goes (S12), a LIVE owner stays (S12b, the control), an
#        unrecorded directory goes because the only way to get one there is to
#        ask the allocator (S12c), and with the ledger DELETED a live owner is
#        still protected by the pid in the directory name (S12d).
#   S13  a RELEASED ledger row whose tree survived the rm is deleted.
#   S14  the legacy family sweep: abandoned goes (S14), a fixture containing a
#        .git goes (S14b), A REGISTERED WORKSPACE WEARING A LEGACY NAME IS
#        STILL REFUSED (S14c — wall 3 survived the wall-2 narrowing, which is
#        the case that decides whether that narrowing was safe), and with the
#        narrowing declared OFF the same fixture goes back to undecidable
#        (S14d, the control that makes S14b a decision rather than an oversight).
#   S15  a legacy directory with an open file descriptor inside is kept, and
#        the same directory with its holder gone is deleted (S15b, the control).
#   S16  a STOPPED Docker daemon is skipped silently and is never a failure
#        (S16/S16b); a daemon that ANSWERS is actually asked to prune, both
#        arms, and the bytes it reports reach the log (S16c/S16d, the controls
#        that stop S16 passing against a docker arm that is dead code).
#   S17  A FAILED DELETION MUST NOT HIDE ITSELF. Measured live: five deletions
#        failed with EPERM and the next run said failures=0 with all five still
#        on disk, because rmtree's partial progress bumped each directory's
#        mtime and the age floor then KEPT it — silently, for two hours. The
#        reaper's own failure erased its own evidence. A failure is now durable
#        and reconsidered regardless of age (S17), the age floor still works
#        when nothing failed (S17b), and a resolved failure leaves the books so
#        the alert can clear (S17c).
#   S18  an obstacle to removal is CLEARED in machine-made harness scratch
#        (S18/S18b, logged because it is a power) and REPORTED rather than
#        overridden in a session's scratchpad (S18c, the bound).
#   S19  THE ALLOCATOR ARM CONSULTS THE OPEN-FILE TABLE. A dead owner pid with a
#        live holder inside is KEPT (S19) and the reason names the holder
#        (S19b); with the holder gone the same tree is deleted (S19c, the
#        control). Reproduced as a real deletion before it was fixed.
#   S20  a tree held ONLY by a test instance of the app is collected rather than
#        kept for ever: the instance is quit (S20b) and the act is logged
#        (S20c). §54 addendum 4 — the instance IS the garbage, and it pins the
#        rest. Any other holder, or a mixed set, still means KEEP (that is S19).
#        A second richos-tauri runs OUTSIDE this suite's tree throughout, as
#        a peer's `cargo test --bin richos-tauri` does on a busy Mac, and is
#        left running (S20d).
#
#   S21  DENY-BY-DEFAULT OVER $TMPDIR AND THE SHARED TEMP ROOTS. Frank's D1/D2,
#        and they were the absence of a check rather than the defeat of one: a
#        name matching neither declared pattern list was not kept and not
#        deleted, it was never looked at — 56,770 of 60,942 entries. An
#        undeclared name is now swept (S21/S21a) and the CONTROL that matters is
#        S21b/S21c: with the switch declared off the same tree survives and is
#        not even mentioned, which is the old behavior exactly. The keep-list of
#        FOREIGN owners is kept and counted (S21d/S21f) against a non-foreign
#        sibling deleted in the same run (S21e). The PROOF is the process table
#        and not the age (S21g/S21h). A declared shared root is swept too
#        (S21i, which is /private/tmp). Wall 3 stands over the new arm
#        (S21j/S21k) and so does the open-file wall (S21l/S21m). The declared age
#        floor is the SECOND condition and has its own case (S21n-S21p), which is
#        also the case that found a one-hour DST error in lstart_epoch.
#
#   S22  THE GARBAGE ALARM, and a FAILED DELETION THAT REACHES THE EXIT CODE.
#        Frank's verdict was that the mechanism had "a disk-space alarm and a
#        delete-failure alarm" and "no garbage alarm", so garbage nothing would
#        ever collect fell between the two halves of §54 in silence. The verdict
#        line now carries skipped=/skipped_bytes= (S22, control S22b); a failed
#        deletion exits 4 and says failures=N on stdout instead of only in a log
#        (S22c/S22d, control S22e); --apply publishes the numbers the watchdog
#        reads (S22f); and the banner raises the alarm FROM THE LAST FULL PASS,
#        dated, without paying for the expensive arm (S22g-S22i2, control S22j).
#
#   S23  A NAME-DERIVED PID IS ATTRIBUTION, NOT LIVENESS (Frank's D11). He put
#        `1-frank-immortal-b` in the allocator root and the reaper answered "pid 1
#        is ALIVE ... a live owner is kept whatever its TTL says" — forever, at any
#        size, with no alert, and silently by construction because a KEEP is the
#        reaper working as designed. A name-derived pid must now be OURS, above a
#        declared floor, and have started no later than the directory was created
#        (S23/S23c/S23d), while a live pid of ours still keeps its sandbox
#        (S23b, the control that matters) and a LEDGER row stays exempt from the
#        two name-shape tests (S23e). S23f is the second hole in the same family,
#        found by mutant M37: a REUSED LEDGER pid was immortal too, and the code's
#        own comment claimed TTL covered it when TTL never runs after a live KEEP.
#
#   S24  DECLARED CAMPAIGN ROOTS UNDER ~/ab ARE REPORTED, NEVER DELETED (D3). An
#        enumeration rather than a sweep, because the census says 24 of 29
#        directories under ~/ab are in no ledger and most of them are the
#        operator's projects — `fitapp`, `prospects`, `deeply`, `saferecord`. S24e
#        is the case that matters: an UNDECLARED sibling is never nominated.
#   S25  THE VOLUME STAGE (D14), with the age applied by us because
#        `docker volume prune` has no `until` filter — checked against the vendor's
#        reference before it was built. Anonymous only (S25c) and past the declared
#        age only (S25b), which are the two things making it safe.
#   S26  A RUNNING CONTAINER on a declared throwaway image is REPORTED and never
#        stopped (D14). S26d is the control that protects the Buzz production
#        stack: an undeclared image is never nominated, however old.
#
# --- added 2026-09-19, after the CEO read the alert his own mechanism made ---
# "And what is this all about: MASSIVE ALERT — DISK: 218 path(s) could not be
# deleted (delete BY HAND) | 13.9 GB in 4122 place(s) NOTHING WILL EVER COLLECT |
# 11.4 GB in 2 place(s) UNDECIDABLE — no run will clear it". Every one of those
# three numbers was wrong in a different way, and the alert had been repeating
# them every six hours for a day.
#
#   S28  A SOCKET IS NOT A DIRECTORY. 200 of the 218 were unix sockets and FIFOs
#        that apply() sent to rmtree because the branch asked what they were NOT
#        (`islink or isfile -> unlink, ELSE rmtree`). They had passed every wall
#        and the open-handle check; one missing `os.unlink` turned two hundred
#        correct decisions into two hundred permanent alerts. S28b asserts
#        `failures=0`, because "the file is gone" also passes against a reaper
#        that never planned it, and S28c is the control that it still deletes.
#   S29  WHAT THE KERNEL OWNS IS NOT GARBAGE. The other 18 were the directories
#        macOS makes for its own agents, marked `sunlnk`, retried 52 times each,
#        each asking a person to delete something no person can. Now classified
#        once (S29), never counted as a failure (S29a), with the unflagged
#        sibling still deleted in the same run (S29b, the control), and the rows
#        already in the ledger WITHDRAWN (S29c) without touching the path (S29d).
#   S30  A FINISHED SESSION'S CLOCK. 11 GB of an ended session's scratchpad was
#        kept for 22 hours because an orphaned writer kept its mtime at "0 min
#        ago" and the floor never expired. The floor now runs from the recorded
#        end (S30); the control is that a just-ended session is still kept
#        (S30b) and that the reason names the clock (S30c).
#   S31  ...AND THE ONE THING IN THERE THAT IS NOT OURS: a repository an app
#        instance has registered survives (S31/S31a), against the control that
#        the same scratchpad with nothing registered is collected (S31b).
#   S32  THE SKIPPED PILE IS SPLIT BY WHO CAN ACT ON IT. The 13.9 GB was 12.8 GB
#        of our own campaign roots — each printing the command that reclaims it —
#        summed with 1.15 GB of other programs' temp that nobody should touch.
#        Two opposite responses under one heading is a heading nobody can act on.
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REAPER="$SCRIPT_DIR/scratch-reaper.sh"

PASS=0; FAIL=0
SANDBOX="$(cd "$(mktemp -d "${TMPDIR:-/tmp}/scratch-reaper-test.XXXXXX")" && pwd -P)"
# --apply also collects test simulators and emulators (scripts/lib/testdevices.py),
# and most worlds here keep the real HOME. So every run is handed a simulator
# list with nothing in it and an empty Android cache root: this suite must never
# read, let alone delete, a simulator on the machine it runs on.
NO_DEVICES="$SANDBOX/no-devices"
mkdir -p "$NO_DEVICES/android"
printf '#!/bin/sh\necho %s\n' "'{\"devices\": {}}'" >"$NO_DEVICES/simctl"
chmod +x "$NO_DEVICES/simctl"

# ---------------------------------------------------------------------------
# THE OPEN-FILE TABLE THIS SUITE READS IS ITS OWN, NEVER THE WHOLE MAC'S.
# ---------------------------------------------------------------------------
# The reaper asks `lsof -n -P -F n` for EVERY open file on the machine, once
# per run, and `lsof +D <tree>` per held candidate — which also enumerates every
# process before it filters. Measured on 2026-09-30 with a logging wrapper: one
# run of this suite made 42 whole-machine reads and 7 `+D` reads at about
# 0.25 s of kernel time each, and the mutation harness runs this suite once
# per mutant, 51 mutants, eight at a time. That is roughly 2,500 whole-machine
# scans in a quarter of an hour, and it is what held the Mac at about 60%
# SYSTEM CPU during the 2026-09-30 merge checks while the CPU admission rule
# refused every other job on the machine. The SCR layer reached its 900 s
# deadline and was killed.
#
# NOTHING A CASE HERE PROVES NEEDS ANOTHER PROGRAM'S FILES. Every holder a case
# asserts about — the `sleep` sitting in a workspace, the `tail -f` in a legacy
# tree, the fake app instance — is a process THIS SUITE started. So the suite
# puts an `lsof` first on PATH (the reaper runs `lsof` by name, the same seam
# the Docker and simctl stubs already use) that runs the REAL lsof with the
# query unchanged and one restriction added: `-a -p <this suite's process
# tree>`. The table is real — real processes, real descriptors, read by the
# real tool — and it is only this suite's part of it. A query that already
# names its pids (appinstances asks `lsof -p <candidates>`) is passed through
# unchanged, because it was never a whole-machine scan.
#
# THE TREE IS COMPUTED AT EACH CALL from the process table, rooted at this
# suite's own shell, so a holder started by any case is inside it with no
# bookkeeping to forget. It also removes cross-talk: eight copies of this suite
# run at once under the mutation harness, and none of them can now see the
# others' holders.
#
# A stub that INVENTED the table would prove nothing about the reaper reading a
# real one; that is why this wrapper narrows the question and never answers it.
# S27's deliberately blind `lsof` is still put in FRONT of this one on PATH, so
# that case is unchanged. S33 at the end proves the reaper's queries actually
# came through here, so a reaper that started calling lsof by absolute path —
# and went back to scanning the whole Mac — would turn this suite red.
SCOPED_LSOF_LOG=""
if command -v lsof >/dev/null 2>&1; then
    # INHERITED WHEN NESTED: a copy of this suite run by the mutation harness
    # finds the OUTER copy's wrapper first on PATH, and must not wrap a wrapper.
    SRT_REAL_LSOF="${SRT_REAL_LSOF:-$(command -v lsof)}"
    SRT_SUITE_PID="$$"
    SCOPED_LSOF_LOG="$SANDBOX/scoped-lsof.log"
    : >"$SCOPED_LSOF_LOG"
    SRT_LSOF_LOG="$SCOPED_LSOF_LOG"
    export SRT_REAL_LSOF SRT_SUITE_PID SRT_LSOF_LOG
    mkdir -p "$SANDBOX/scoped-bin"
    cat >"$SANDBOX/scoped-bin/lsof" <<'PYEOF'
#!/usr/bin/env python3
"""The real lsof, asked only about the process tree of the suite that made it."""
import os
import subprocess
import sys

real = os.environ.get("SRT_REAL_LSOF", "")
root = os.environ.get("SRT_SUITE_PID", "")
args = sys.argv[1:]
if not real or not root or os.path.basename(os.path.dirname(real)) == "scoped-bin":
    sys.stderr.write("scoped lsof: no real lsof or no suite root; refusing to "
                     "scan the whole machine\n")
    sys.exit(97)
ps = subprocess.run(["ps", "-Ao", "pid=,ppid="], capture_output=True, text=True)
if ps.returncode != 0:
    sys.stderr.write("scoped lsof: ps failed; refusing to guess the tree\n")
    sys.exit(98)
children = {}
for line in ps.stdout.splitlines():
    parts = line.split()
    if len(parts) == 2:
        children.setdefault(parts[1], []).append(parts[0])
tree, stack = [], [root]
while stack:
    pid = stack.pop()
    tree.append(pid)
    stack.extend(children.get(pid, []))
if "-p" in args:
    at = args.index("-p") + 1
    requested = args[at].split(",")
    if any(not pid.isdecimal() for pid in requested):
        sys.stderr.write("scoped lsof: refusing a PID list it cannot read\n")
        sys.exit(99)
    # A LIVE PID OUTSIDE THIS SUITE'S TREE IS DROPPED, NOT A REASON TO REFUSE
    # THE REST. appinstances shortlists every process on the Mac whose argv
    # carries `richos-tauri`, so a peer's `cargo test --bin richos-tauri` lands
    # in the same `-p` list as this suite's fake instance. Refusing the whole
    # query answered nothing about the fake either, and S20 failed on any busy
    # Mac (the 2026-09-30 merge of echo-sonnet-app1). Dropping it is the same
    # narrowing the scoped form makes with `-a -p <tree>`: the foreign process
    # is never read, and the reaper sees it holding nothing, which is LEAVE.
    # A pid in no process table at all is passed through as before; lsof
    # reports nothing for it. Only a list with nothing of ours left is refused.
    live = {p for values in children.values() for p in values}
    foreign = [pid for pid in requested if pid not in tree and pid in live]
    kept = [pid for pid in requested if pid not in foreign]
    if not kept:
        sys.stderr.write("scoped lsof: refusing a PID outside the fixture\n")
        sys.exit(99)
    args[at] = ",".join(kept)
    mode = "pid-list" if not foreign else "pid-list-narrowed"
else:
    args = ["-a", "-p", ",".join(tree)] + args
    mode = "scoped"
log = os.environ.get("SRT_LSOF_LOG", "")
if log:
    try:
        with open(log, "a", encoding="utf-8") as fh:
            fh.write("%s\t%s\n" % (mode, " ".join(sys.argv[1:])))
    except OSError:
        pass
os.execv(real, [real] + args)
PYEOF
    chmod +x "$SANDBOX/scoped-bin/lsof"
    # NO TEST ASKS SYSTEM EVENTS TO QUIT AN APP (2026-10-18). S20 makes the reaper end a
    # fake app instance, and appinstances._graceful runs an `osascript` that tells System
    # Events to quit it. On a host with a shield that call is blocked and logged as an
    # attack on the person's session. The fake never answers a quit request anyway, so an
    # `osascript` that records the call and fails sends the reaper on to SIGTERM, the path
    # S20b verifies.
    SRT_OSASCRIPT_LOG="$SANDBOX/osascript-calls.log"
    : >"$SRT_OSASCRIPT_LOG"
    export SRT_OSASCRIPT_LOG
    cat >"$SANDBOX/scoped-bin/osascript" <<'OSAEOF'
#!/bin/sh
printf '%s\n' "$*" >>"$SRT_OSASCRIPT_LOG"
exit 1
OSAEOF
    chmod +x "$SANDBOX/scoped-bin/osascript"
    PATH="$SANDBOX/scoped-bin:$PATH"
    export PATH

    # Observe requests before executable lookup. Counting the wrapper's own log
    # cannot catch a second call that bypasses it by absolute path. Reject that
    # call before it can scan the host, including calls made by imported helpers.
    SRT_SCANNER_AUDIT="$SANDBOX/scanner-audit.log"
    SRT_SCOPED_LSOF="$SANDBOX/scoped-bin/lsof"
    export SRT_SCANNER_AUDIT SRT_SCOPED_LSOF
    : >"$SRT_SCANNER_AUDIT"
    mkdir -p "$SANDBOX/scoped-python"
    cat >"$SANDBOX/scoped-python/sitecustomize.py" <<'PYEOF'
import os
from pathlib import Path
import shutil
import sys

def audit(event, arguments):
    if event not in ("subprocess.Popen", "os.exec", "os.posix_spawn"):
        return
    executable, argv = arguments[:2]
    name = Path(os.fsdecode(executable)).name
    if name not in ("lsof", "du"):
        return
    resolved = os.path.realpath(shutil.which(os.fsdecode(executable)) or os.fsdecode(executable))
    scoped = os.path.realpath(os.environ["SRT_SCOPED_LSOF"])
    blind = os.environ.get("SRT_BLIND_LSOF", "")
    permitted = name == "lsof" and (resolved == scoped or (blind and resolved == os.path.realpath(blind)))
    # The wrapper's exec of the real tool must carry its PID restriction.
    if event == "os.exec" and name == "lsof":
        permitted = (resolved == os.path.realpath(os.environ["SRT_REAL_LSOF"]) and "-p" in argv
                     and os.path.realpath(sys.argv[0]) == scoped)
    outcome = "allowed" if permitted else "denied"
    role = "control" if os.environ.get("SRT_SCANNER_CONTROL") == "1" else "suite"
    with open(os.environ["SRT_SCANNER_AUDIT"], "a", encoding="utf-8") as stream:
        stream.write("%s\t%s\t%s\t%s\n" % (outcome, role, event, resolved))
    if not permitted:
        raise RuntimeError("fixture refused an unscoped scanner: " + resolved)

sys.addaudithook(audit)
PYEOF
    export PYTHONPATH="$SANDBOX/scoped-python${PYTHONPATH:+:$PYTHONPATH}"
fi
KILL_LIST=""
cleanup() {
    # Reaped as well as killed: an unreaped job prints "Terminated: 15" AFTER
    # the summary line, which is the last thing a reader should see.
    for p in $KILL_LIST; do
        kill "$p" >/dev/null 2>&1 || true
        wait "$p" >/dev/null 2>&1 || true
    done
    rm -rf "$SANDBOX"
}
trap cleanup EXIT
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -f "$REAPER" ] || { echo "FATAL: missing $REAPER" >&2; exit 1; }
command -v python3 >/dev/null 2>&1 || { echo "FATAL: python3 required" >&2; exit 1; }

# Settings a caller exported (a proof run exports the bulk root) would override each
# world's own settings file, so none of them reaches a world.
for _k in RICHOS_MACHINE_SETTINGS_RESOLVED $(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import machine_settings; print(" ".join(machine_settings.KEYS))' "$SCRIPT_DIR/lib"); do
    unset "$_k"
done
unset _k

echo "=== scratch-reaper tests ==="

# ---------------------------------------------------------------------------
# THE PROCESS NAME IS UNIQUE TO THIS RUN, and that is what makes this suite
# hermetic. The reaper reads the MACHINE-WIDE process table, which is shared
# state: the mutation harness runs eight copies of this suite at once, each
# starting session processes of its own, and under the shared name `claude`
# every run saw the others' processes as unattributed and correctly answered
# INDETERMINATE for everything. Three mutants then scored green against a
# reaper that was deleting nothing — the exact green-for-the-wrong-reason this
# suite exists to refuse, arriving through the suite itself.
#
# SCRATCH_SESSION_PROCESS_NAMES is declared per world, so the shipped code path
# is byte-for-byte the production one and only this run's processes are in it.
#
# A REAL PROCESS — `exec -a "$SUITE_PROC" /bin/sleep`. It is /bin/sleep,
# it is in the process table under that name, it has a genuine start time, and
# it can be killed to make a genuinely dead one. The reaper reads it with the
# same ps call it uses in production and cannot tell it from a session.
#
# TWO EARLIER SHAPES WERE MEASURED AND BOTH LIED:
#   * `cp /bin/sleep $sandbox/claude` — macOS SIGKILLs the copy of a signed
#     platform binary. The process was dead before the reaper looked, which
#     made a liveness test pass for the wrong reason.
#   * `PID=$(start_claude)` — the background job starts inside the command
#     substitution's SUBSHELL, and every KILL_LIST append made there is thrown
#     away with it. The pid came back; the bookkeeping did not.
# The pid is therefore returned in a variable set by the CURRENT shell.
# ---------------------------------------------------------------------------
SUITE_PROC="zsr${$}claude"
LAST_CLAUDE=""
start_claude() {           # -> LAST_CLAUDE
    ( exec -a "$SUITE_PROC" /bin/sleep 600 ) >/dev/null 2>&1 &
    LAST_CLAUDE=$!
    KILL_LIST="$KILL_LIST $LAST_CLAUDE"
}

register() {               # <sessions-dir> <pid> <session-id>
    local sd="$1" pid="$2" sid="$3" start
    mkdir -p "$sd"
    start="$(LC_ALL=C LANG=C TZ=UTC0 ps -o lstart= -p "$pid" | tr -s ' ' | sed 's/^ *//;s/ *$//')"
    python3 - "$sd/$pid.json" "$pid" "$sid" "$start" <<'PY'
import json, sys
path, pid, sid, start = sys.argv[1:5]
with open(path, "w", encoding="utf-8") as fh:
    json.dump({"pid": int(pid), "sessionId": sid, "procStart": start,
               "cwd": "/", "kind": "interactive"}, fh)
PY
}

# ---------------------------------------------------------------------------
# A SANDBOX WORLD: its own scratch root, its own sessions registry, its own
# config file. The config is a FILE and not exported variables, because the
# shipped script sources its declarations from a file and a test that bypassed
# that would not be testing the shipped path.
# ---------------------------------------------------------------------------
# EVERY SESSION PROCESS THIS SUITE HAS ALREADY STARTED IS REGISTERED IN EACH
# NEW WORLD, with a filler session id: an earlier case's process is still
# running, and an unregistered running process is a correct INDETERMINATE and a
# suite that has stopped testing anything. Measured on the first run, where the
# operator's own live session did exactly that to all 22 cases.
attribute_running_claudes() {   # <sessions-dir>
    local sd="$1" pid i=0
    for pid in $(LC_ALL=C LANG=C TZ=UTC0 ps -Ao pid=,comm= \
                 | awk -v want="$SUITE_PROC" \
                       '{ n=split($2, p, "/"); if (p[n] == want) print $1 }'); do
        i=$((i + 1))
        register "$sd" "$pid" "$(printf 'f0000000-0000-0000-0000-%012d' "$i")"
    done
}

world() {                  # <name> -> exports W_*
    W_ROOT="$SANDBOX/$1"
    W_SCRATCH="$W_ROOT/scratch"
    W_SESSIONS="$W_ROOT/sessions"
    W_CFG="$W_ROOT/orchestration.config"
    W_HOME="$W_ROOT/claude"
    W_TMP="$W_ROOT/tmp"
    W_NIGHTLY="$W_ROOT/nightly"
    W_FAKE_HOME=""
    # THE WORLD'S OWN MACHINE SETTINGS. Without them the reaper read the operator's
    # private settings file, so every --apply here swept the REAL build caches (and,
    # once producer temp roots were added, would have swept Codex's and the gates'
    # temp roots with this suite's scoped lsof, which cannot see their holders).
    W_BULK="$W_ROOT/bulk"
    mkdir -p "$W_BULK"
    printf 'RICHOS_BULK_ROOT=%s\n' "$W_BULK" >"$W_ROOT/machine.env"
    mkdir -p "$W_SCRATCH" "$W_SESSIONS" "$W_HOME/state" "$W_TMP" "$W_NIGHTLY"
    {
        echo 'SCRATCH_REAPER_ENABLE="1"'
        echo "SCRATCH_SESSION_PROCESS_NAMES=\"$SUITE_PROC\""
        echo "SCRATCH_CLAUDE_ROOTS=\"$W_SCRATCH\""
        echo 'SCRATCH_TMP_PATTERNS="richos-*-workspace"'
        echo 'SCRATCH_AGE_FLOOR_MINUTES="0"'
        echo "SCRATCH_NIGHTLY_DIR=\"$W_NIGHTLY\""
        echo 'SCRATCH_NIGHTLY_KEEP="3"'
        echo 'SCRATCH_NOTICE_BYTES="1"'
        echo 'SCRATCH_REAPER_HOURS="4"'
        echo 'SCRATCH_REAPER_MINUTE="40"'
        # --- added 2026-09-18 with the allocator root and the legacy sweep ---
        echo 'SCRATCH_ROOT_NAME="richos-scratch"'
        echo 'SCRATCH_DEFAULT_TTL_MINUTES="360"'
        # A family name NOTHING ELSE IN THIS SUITE USES, so adding the legacy
        # arm cannot change the verdict of a case written before it existed.
        echo 'SCRATCH_LEGACY_TMP_PATTERNS="zlegacy-*"'
        echo 'SCRATCH_LEGACY_AGE_HOURS="0"'
        echo 'SCRATCH_LEGACY_GIT_IS_FIXTURE="1"'
        # OFF by default in every world. A suite that shelled out to the
        # operator's real Docker daemon would be pruning their images, and S16
        # turns it on against a deliberately broken `docker` instead.
        echo 'SCRATCH_DOCKER_PRUNE="0"'
        echo 'SCRATCH_DOCKER_KEEP_STORAGE="20GB"'
    } >"$W_CFG"
    attribute_running_claudes "$W_SESSIONS"
}

# alloc <name> [pid] — a directory in the allocator root, with a ledger row.
# The ledger lives in the world's own CLAUDE_CONFIG_DIR, so nothing here can
# see or damage the real one.
alloc() {                  # <dir-name> <pid> <label>
    local name="$1" pid="$2" label="${3:-fixture}"
    local root="$W_TMP/richos-scratch"
    mkdir -p "$root/$name"
    printf '%s\n' "payload" >"$root/$name/payload.txt"
    mkdir -p "$W_HOME/state"
    printf '{"path":"%s","label":"%s","pid":%d,"ppid":1,"session":"","created":"%s","ttl_minutes":360,"event":"new"}\n' \
        "$root/$name" "$label" "$pid" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        >>"$W_HOME/state/scratch-ledger.jsonl"
    printf '%s\n' "$root/$name"
}

run() {                    # run the reaper in the current world
    RICHOS_ENV_FILE="$W_ROOT/machine.env" \
    SCRATCH_REAPER_CONFIG="$W_CFG" \
    RICHOS_SESSIONS_DIR="$W_SESSIONS" \
    CLAUDE_CONFIG_DIR="$W_HOME" \
    TMPDIR="$W_TMP" \
    HOME="${W_FAKE_HOME:-$HOME}" \
    RICHOS_SIMCTL="$NO_DEVICES/simctl" \
    RICHOS_ANDROID_CACHES_ROOT="$NO_DEVICES/android" \
    bash "$REAPER" "$@" 2>&1
}
# W_FAKE_HOME IS EMPTY IN EVERY WORLD BUT S31's, and it has to be RESET by
# world() rather than merely set by the case that wants it: a shell variable set
# in one case is still set in the next, and a suite where case 30 changes what
# case 31 measures is a suite that cannot be read in isolation.
W_FAKE_HOME=""

SID_LIVE="11111111-1111-1111-1111-111111111111"
SID_DEAD="22222222-2222-2222-2222-222222222222"

# ===========================================================================
# S1 / S1b — the live session survives, and the same thing dead does not
# ===========================================================================
world live
start_claude; PID_LIVE="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_LIVE" "$SID_LIVE"
mkdir -p "$W_SCRATCH/-a-project/$SID_LIVE/scratchpad"
echo payload >"$W_SCRATCH/-a-project/$SID_LIVE/scratchpad/work.txt"

OUT="$(run --apply)"
if [ -f "$W_SCRATCH/-a-project/$SID_LIVE/scratchpad/work.txt" ]; then
    ok "S1  a live session's scratch survives --apply"
else
    bad "S1  a live session's scratch was DELETED while its process was running"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# The positive control. Same directory, same contents; the process is killed
# and its registration left behind exactly as a crashed session leaves it.
kill "$PID_LIVE" >/dev/null 2>&1 || true
wait "$PID_LIVE" 2>/dev/null || true
mkdir -p "$W_SCRATCH/-a-project/$SID_LIVE/scratchpad"
echo payload >"$W_SCRATCH/-a-project/$SID_LIVE/scratchpad/work.txt"
OUT="$(run --apply)"
if [ ! -e "$W_SCRATCH/-a-project/$SID_LIVE" ]; then
    ok "S1b POSITIVE CONTROL: the same scratch with the process gone IS deleted"
else
    bad "S1b the dead session's scratch survived — S1 proves nothing"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# ===========================================================================
# S2 — a registered workspace survives
# ===========================================================================
world registered
mkdir -p "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/workspace"
echo work >"$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/workspace/file.txt"
mkdir -p "$W_HOME/state/workspaces/agents"
python3 - "$W_HOME/state/workspaces/agents/a.json" \
          "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/workspace" <<'PY'
import json, sys
path, wt = sys.argv[1:3]
with open(path, "w", encoding="utf-8") as fh:
    json.dump({"name": "someone", "worktree": wt, "workspaces": [wt]}, fh)
PY
OUT="$(run --apply)"
if [ -f "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/workspace/file.txt" ]; then
    ok "S2  a REGISTERED workspace inside dead scratch survives (wall 3)"
else
    bad "S2  a registered workspace was deleted"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
case "$OUT" in
    *"REGISTERED workspace"*) ok "S2b the refusal NAMES the registration" ;;
    *) bad "S2b the refusal did not name the registration"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac

# ===========================================================================
# S3 / S11 — INDETERMINATE survives, is named, and costs exit 3
# ===========================================================================
world indeterminate
mkdir -p "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad"
echo payload >"$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/work.txt"
start_claude; PID_UNATTRIBUTED="$LAST_CLAUDE"     # running, named by NO session file
OUT="$(run --apply)"; RC=$?
if [ -f "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/work.txt" ]; then
    ok "S3  scratch an unattributed claude process could own is NOT deleted"
else
    bad "S3  INDETERMINATE was collapsed into dead and the scratch was deleted"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
case "$OUT" in
    *"named by no session file"*|*"named by NO session file"*)
        ok "S3b the reason NAMES the unattributed process" ;;
    *) bad "S3b the undecidable entry carried no named reason"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac
case "$OUT" in
    *"undecidable=0"*) bad "S11 undecidable was reported as zero while one stood" ;;
    *undecidable=*)    ok "S11 the verdict line carries the undecidable count" ;;
    *) bad "S11 no verdict line" ;;
esac
run --apply >/dev/null 2>&1; RC=$?
if [ "$RC" = "3" ]; then
    ok "S11b exit 3 while anything is undecidable"
else
    bad "S11b exit was $RC, not 3, with an undecidable entry standing"
fi
kill "$PID_UNATTRIBUTED" >/dev/null 2>&1 || true
wait "$PID_UNATTRIBUTED" 2>/dev/null || true

# ===========================================================================
# S4 — a .git in the tree survives
# ===========================================================================
world repo
mkdir -p "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/checkout/.git"
echo ref >"$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/checkout/.git/HEAD"
OUT="$(run --apply)"
if [ -f "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/checkout/.git/HEAD" ]; then
    ok "S4  a tree containing a .git survives (wall 2)"
else
    bad "S4  a checkout inside dead scratch was deleted"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# ===========================================================================
# S5 — the log carries every deleted path
# ===========================================================================
world logged
for n in 1 2 3; do
    sid="3333333$n-3333-3333-3333-333333333333"
    mkdir -p "$W_SCRATCH/-a-project/$sid/scratchpad"
    echo x >"$W_SCRATCH/-a-project/$sid/scratchpad/f"
done
run --apply >/dev/null
LOG="$W_HOME/state/scratch-reaper.log"
MISSING=0
for n in 1 2 3; do
    sid="3333333$n-3333-3333-3333-333333333333"
    grep -q "$W_SCRATCH/-a-project/$sid" "$LOG" 2>/dev/null || MISSING=$((MISSING + 1))
done
if [ "$MISSING" = "0" ]; then
    ok "S5  every deleted path is named in $LOG"
else
    bad "S5  $MISSING deleted path(s) never reached the log"
fi
if grep -q "bytes=" "$LOG" 2>/dev/null && grep -q "why=" "$LOG" 2>/dev/null; then
    ok "S5b each log line carries bytes and the reason it qualified"
else
    bad "S5b log lines carry no bytes/why"
fi
if grep -q "verdict:" "$LOG" 2>/dev/null; then
    ok "S5c the run ends with a verdict line in the log"
else
    bad "S5c no verdict line in the log"
fi

# ===========================================================================
# S6 — the dry run IS the plan
# ===========================================================================
world identical
for n in 1 2; do
    sid="4444444$n-4444-4444-4444-444444444444"
    mkdir -p "$W_SCRATCH/-a-project/$sid/scratchpad"
    echo x >"$W_SCRATCH/-a-project/$sid/scratchpad/f"
done
DRY="$(run --dry-run)"
APPLIED="$(run --apply)"
# --apply adds exactly one line, at the end.
if [ "$DRY" = "$(printf '%s\n' "$APPLIED" | sed '$d')" ]; then
    ok "S6  --apply prints the dry run's plan byte for byte, plus one line"
else
    bad "S6  the applied plan differs from the plan that was shown"
    diff <(printf '%s\n' "$DRY") <(printf '%s\n' "$APPLIED" | sed '$d') \
        | sed 's/^/        /' | head -20
fi
case "$APPLIED" in
    *"applied: deleted="*) ok "S6b the added line is the applied summary" ;;
    *) bad "S6b --apply did not report what it did" ;;
esac

# ===========================================================================
# S7 — the orphan rule, both ways
# ===========================================================================
world orphans
touch -t 202001010000 "$W_SCRATCH/ancient.out"        # before any process
echo fresh >"$W_SCRATCH/fresh.out"                    # after every process
start_claude; PID_KEEP="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_KEEP" "$SID_LIVE"
sleep 1
echo newer >"$W_SCRATCH/newer.out"
OUT="$(run --apply)"
if [ ! -e "$W_SCRATCH/ancient.out" ]; then
    ok "S7  a scratch-root file older than every running claude process is deleted"
else
    bad "S7  the ancient orphan survived"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
if [ -f "$W_SCRATCH/newer.out" ]; then
    ok "S7b a file written AFTER a running process started is kept"
else
    bad "S7b a file a running session could own was deleted"
fi
kill "$PID_KEEP" >/dev/null 2>&1 || true
wait "$PID_KEEP" 2>/dev/null || true

# ===========================================================================
# S8 — nightly retention
# ===========================================================================
world nightly
mkdir -p "$W_NIGHTLY/releases" "$W_NIGHTLY/logs" "$W_NIGHTLY/source" "$W_NIGHTLY/runtime"
echo src >"$W_NIGHTLY/source/keep.txt"
echo rt  >"$W_NIGHTLY/runtime/keep.txt"
for n in 1 2 3 4 5; do
    mkdir -p "$W_NIGHTLY/releases/v$n"
    echo build >"$W_NIGHTLY/releases/v$n/app"
    touch -t "2026090${n}0000" "$W_NIGHTLY/releases/v$n" "$W_NIGHTLY/releases/v$n/app"
done
OUT="$(run --apply)"
KEPT=0
for n in 3 4 5; do [ -d "$W_NIGHTLY/releases/v$n" ] && KEPT=$((KEPT + 1)); done
GONE=0
for n in 1 2; do [ ! -e "$W_NIGHTLY/releases/v$n" ] && GONE=$((GONE + 1)); done
if [ "$KEPT" = "3" ] && [ "$GONE" = "2" ]; then
    ok "S8  the newest 3 releases are kept and the older 2 are deleted"
else
    bad "S8  retention kept=$KEPT of 3 newest, deleted=$GONE of 2 oldest"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
if [ -f "$W_NIGHTLY/source/keep.txt" ] && [ -f "$W_NIGHTLY/runtime/keep.txt" ]; then
    ok "S8b the nightly source checkout and runtime are never touched"
else
    bad "S8b the reaper reached into the nightly source or runtime"
fi

# ===========================================================================
# S9 — an undeclared threshold refuses
# ===========================================================================
world undeclared
mkdir -p "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad"
echo x >"$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/f"
grep -v SCRATCH_AGE_FLOOR_MINUTES "$W_CFG" >"$W_CFG.tmp" && mv "$W_CFG.tmp" "$W_CFG"
OUT="$(run --apply)"; RC=$?
if [ "$RC" = "2" ] && [ -f "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/f" ]; then
    ok "S9  an undeclared threshold is exit 2 and nothing is deleted"
else
    bad "S9  exit was $RC with a threshold undeclared"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
case "$OUT" in
    *SCRATCH_AGE_FLOOR_MINUTES*) ok "S9b the refusal names the undeclared value" ;;
    *) bad "S9b the refusal did not name the missing declaration" ;;
esac

# ===========================================================================
# S10 — a temp workspace nobody holds open
# ===========================================================================
world tmpworkspaces
mkdir -p "$W_TMP/richos-abandoned-workspace" "$W_TMP/richos-held-workspace"
echo x >"$W_TMP/richos-abandoned-workspace/f"
echo x >"$W_TMP/richos-held-workspace/f"
if command -v lsof >/dev/null 2>&1; then
    # `exec` inside the backgrounded subshell, so $! is the pid of the process
    # that ends up SITTING IN the directory. Backgrounding inside the subshell
    # instead leaves the child orphaned the moment the subshell exits, and it
    # was measured dying before lsof ever saw it.
    ( cd "$W_TMP/richos-held-workspace" && exec -a "$SUITE_PROC" /bin/sleep 600 ) >/dev/null 2>&1 &
    HELD=$!
    KILL_LIST="$KILL_LIST $HELD"
    sleep 1
    OUT="$(run --apply)"
    if [ ! -e "$W_TMP/richos-abandoned-workspace" ]; then
        ok "S10  an abandoned harness workspace is deleted"
    else
        bad "S10  the abandoned harness workspace survived"
        printf '%s\n' "$OUT" | sed 's/^/        /'
    fi
    if [ -f "$W_TMP/richos-held-workspace/f" ]; then
        ok "S10b a workspace a process is sitting in is kept"
    else
        bad "S10b a held workspace was deleted out from under a process"
    fi
    kill "$HELD" >/dev/null 2>&1 || true
    wait "$HELD" >/dev/null 2>&1 || true
else
    ok "S10  SKIPPED — lsof is not on this host, and without it the reaper is
        required to answer INDETERMINATE rather than delete"
    OUT="$(run)"
    case "$OUT" in
        *"lsof could not answer"*) ok "S10b no lsof means INDETERMINATE, not deletion" ;;
        *) bad "S10b without lsof the temp-workspace class did not say so" ;;
    esac
fi


# ===========================================================================
# S12 — THE ALLOCATOR ROOT, SWEPT DENY-BY-DEFAULT
# ===========================================================================
# This is the arm that would have caught the 105 GB, so it gets the fullest
# treatment: a dead owner goes, a LIVE owner stays, and an entry that no ledger
# row mentions at all goes because the only way to get a directory under that
# root is to ask the allocator for one.
world alloc
start_claude; PID_ALIVE="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_ALIVE" "$SID_LIVE"

D_DEAD="$(alloc "99999-mutation-deadowner" 99999 mutation)"
D_LIVE="$(alloc "$PID_ALIVE-mutation-liveowner" "$PID_ALIVE" mutation)"
# No ledger row at all, and no pid in the name either.
mkdir -p "$W_TMP/richos-scratch/unrecorded-junk"
echo payload >"$W_TMP/richos-scratch/unrecorded-junk/payload.txt"

OUT="$(run --apply)"

if [ ! -d "$D_DEAD" ]; then
    ok "S12  an allocation whose owner pid is ENDED is deleted"
else
    bad "S12  a dead owner's allocation survived"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

if [ -f "$D_LIVE/payload.txt" ]; then
    ok "S12b a LIVE owner's allocation survives — the positive control for S12"
else
    bad "S12b a live owner's allocation was deleted from under it"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

if [ ! -d "$W_TMP/richos-scratch/unrecorded-junk" ]; then
    ok "S12c an unrecorded directory under the allocator root is deleted"
else
    bad "S12c an unrecorded directory under the allocator root survived — the"
    bad "     root is supposed to be deny-by-default"
fi

# THE NAME IS THE SECOND RECORD. With the ledger destroyed entirely, a live
# owner must STILL be protected, because the pid is on the front of the
# directory name. This is the case that decides whether losing $HOME turns the
# reaper into something that deletes a running harness's sandbox.
D_LIVE2="$(alloc "$PID_ALIVE-mutation-ledgerless" "$PID_ALIVE" mutation)"
rm -f "$W_HOME/state/scratch-ledger.jsonl"
OUT="$(run --apply)"
if [ -f "$D_LIVE2/payload.txt" ]; then
    ok "S12d with the ledger DELETED, a live owner is still protected by the"
    ok "     pid in the directory name"
else
    bad "S12d losing the ledger made the reaper delete a LIVE owner's sandbox"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
kill "$PID_ALIVE" >/dev/null 2>&1 || true
wait "$PID_ALIVE" 2>/dev/null || true

# ===========================================================================
# S13 — A RELEASED ROW WHOSE TREE IS STILL THERE
# ===========================================================================
# scratch_release ran, said so in the ledger, and the rm did not complete.
# Nothing owns it and its maker has said as much.
world released
mkdir -p "$W_TMP/richos-scratch/4242-mutation-halfreleased"
echo payload >"$W_TMP/richos-scratch/4242-mutation-halfreleased/payload.txt"
mkdir -p "$W_HOME/state"
{
    printf '{"path":"%s","label":"mutation","pid":4242,"ppid":1,"session":"","created":"%s","ttl_minutes":360,"event":"new"}\n' \
        "$W_TMP/richos-scratch/4242-mutation-halfreleased" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    printf '{"path":"%s","pid":4242,"released":"%s","event":"release"}\n' \
        "$W_TMP/richos-scratch/4242-mutation-halfreleased" "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
} >>"$W_HOME/state/scratch-ledger.jsonl"
OUT="$(run --apply)"
if [ ! -d "$W_TMP/richos-scratch/4242-mutation-halfreleased" ]; then
    ok "S13  a RELEASED row whose tree survived the rm is deleted"
else
    bad "S13  a released-but-present allocation survived"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# ===========================================================================
# S14 — THE LEGACY FAMILY SWEEP, AND WALL 3 STILL STANDING OVER IT
# ===========================================================================
# The legacy arm is what reclaims the names that 171 engine files still create
# with a bare mktemp. S14b is the case that matters most: narrowing wall 2 for
# these families must NOT have opened a path to a registered worktree.
world legacy
mkdir -p "$W_TMP/zlegacy-abandoned"
echo payload >"$W_TMP/zlegacy-abandoned/payload.txt"

# A fixture repository — exactly the 2,800 entries that forced the narrowing.
mkdir -p "$W_TMP/zlegacy-fixture-repo/.git"
echo payload >"$W_TMP/zlegacy-fixture-repo/payload.txt"

# A REGISTERED workspace wearing a legacy family name. Wall 3 must refuse it.
mkdir -p "$W_TMP/zlegacy-registered/.git"
echo payload >"$W_TMP/zlegacy-registered/payload.txt"
mkdir -p "$W_HOME/state"
printf '{"path":"%s","branch":"b","repo":"r"}\n' "$W_TMP/zlegacy-registered" \
    >>"$W_HOME/state/worktree-ledger.jsonl"

OUT="$(run --apply)"

if [ ! -d "$W_TMP/zlegacy-abandoned" ]; then
    ok "S14  an abandoned legacy-family directory is deleted"
else
    bad "S14  an abandoned legacy-family directory survived"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

if [ ! -d "$W_TMP/zlegacy-fixture-repo" ]; then
    ok "S14b a legacy-family fixture containing a .git is deleted"
else
    bad "S14b the 2,800-fixture case is still undecidable"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

if [ -f "$W_TMP/zlegacy-registered/payload.txt" ]; then
    ok "S14c A REGISTERED WORKSPACE with a legacy name is still refused — wall 3"
    ok "     survived the wall-2 narrowing"
else
    bad "S14c THE WALL-2 NARROWING REACHED A REGISTERED WORKSPACE. This is the"
    bad "     failure that narrowing was supposed not to have."
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# THE POSITIVE CONTROL FOR THE NARROWING ITSELF: with it declared OFF, the very
# same fixture goes back to being undecidable. Without this, S14b passes just as
# well against a reaper that ignores .git entirely.
world legacyoff
python3 - "$W_CFG" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read().replace('SCRATCH_LEGACY_GIT_IS_FIXTURE="1"',
                           'SCRATCH_LEGACY_GIT_IS_FIXTURE="0"')
open(p, "w").write(s)
PY
mkdir -p "$W_TMP/zlegacy-fixture-repo/.git"
echo payload >"$W_TMP/zlegacy-fixture-repo/payload.txt"
OUT="$(run --apply)"; RC=$?
if [ -f "$W_TMP/zlegacy-fixture-repo/payload.txt" ] && [ "$RC" = "3" ]; then
    ok "S14d CONTROL: with the narrowing declared OFF the same fixture is kept"
    ok "     and the run exits 3 — so S14b is a decision, not an oversight"
else
    bad "S14d CONTROL FAILED: the narrowing declaration changes nothing (rc=$RC)"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# ===========================================================================
# S15 — A LEGACY DIRECTORY SOMETHING IS STILL WRITING INTO IS KEPT
# ===========================================================================
# The legacy age floor is taken from the NEWEST mtime, which is what lets a
# 181-minute mutation pass run to completion under a two-hour floor. With the
# floor declared as 0 hours the only thing standing between a running harness
# and deletion is the open-handle check, so this is the case that proves it.
world legacyheld
mkdir -p "$W_TMP/zlegacy-inuse"
echo payload >"$W_TMP/zlegacy-inuse/payload.txt"
if command -v lsof >/dev/null 2>&1; then
    # A real process with a real open file descriptor inside the directory.
    #
    # `tail -f`, NOT `( exec 9>file; sleep 30 ) &`. The first shape of this case
    # used the redirect-and-sleep form and S15b's control failed: `sleep`
    # INHERITS fd 9 from the subshell, so killing the subshell left the
    # descriptor open in an orphaned sleep and the directory was correctly kept
    # — a control that failed for a reason that had nothing to do with the
    # reaper. `tail -f` is a single process that holds the file itself, so
    # killing it actually releases it.
    : >"$W_TMP/zlegacy-inuse/held.lock"
    tail -f "$W_TMP/zlegacy-inuse/held.lock" >/dev/null 2>&1 &
    HELD_PID=$!
    KILL_LIST="$KILL_LIST $HELD_PID"
    sleep 1
    OUT="$(run --apply)"
    if [ -f "$W_TMP/zlegacy-inuse/payload.txt" ]; then
        ok "S15  a legacy directory with an open file descriptor inside is kept"
    else
        bad "S15  a legacy directory was deleted while a process held it open"
        printf '%s\n' "$OUT" | sed 's/^/        /'
    fi
    kill "$HELD_PID" >/dev/null 2>&1 || true
    wait "$HELD_PID" 2>/dev/null || true

    # The positive control: the same directory with the holder gone.
    OUT="$(run --apply)"
    if [ ! -d "$W_TMP/zlegacy-inuse" ]; then
        ok "S15b CONTROL: with the holder gone the same directory is deleted"
    else
        bad "S15b CONTROL FAILED: it is kept whether held or not, so S15 proves"
        bad "     nothing"
        printf '%s\n' "$OUT" | sed 's/^/        /'
    fi
else
    ok "S15  SKIPPED — lsof is not on this host"
    ok "S15b SKIPPED — lsof is not on this host"
fi

# ===========================================================================
# S16 — A STOPPED DOCKER DAEMON IS SKIPPED SILENTLY, NEVER A FAILURE
# ===========================================================================
# CEO, 2026-09-18: prune Docker "only when the Docker daemon is running", and
# a stopped daemon must "never be reported as a failure". Docker Desktop is
# shut down most of the time on a laptop, and a scheduled job that logged a
# failure every six hours for an optional tool would train its reader to skip
# the log — the same way a 2,800-fixture alert would have died.
#
# The `docker` on PATH here is a STUB that fails `docker info` exactly as the
# real client does with the daemon down. The operator's real daemon is never
# contacted by this suite.
world docker
python3 - "$W_CFG" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read().replace('SCRATCH_DOCKER_PRUNE="0"', 'SCRATCH_DOCKER_PRUNE="1"')
open(p, "w").write(s)
PY
STUBDIR="$W_ROOT/stubbin"
mkdir -p "$STUBDIR"
cat >"$STUBDIR/docker" <<'STUB'
#!/bin/sh
# The real client's behavior with Docker Desktop not running: the client
# answers, the daemon does not.
case "$1" in
  info) echo "Cannot connect to the Docker daemon. Is the docker daemon running?" >&2; exit 1 ;;
  *)    echo "docker: daemon not running" >&2; exit 1 ;;
esac
STUB
chmod +x "$STUBDIR/docker"

mkdir -p "$W_SCRATCH/-p/$SID_DEAD/scratchpad"
echo payload >"$W_SCRATCH/-p/$SID_DEAD/scratchpad/work.txt"
OUT="$(PATH="$STUBDIR:$PATH" run --apply)"; RC=$?
# THE LOG, NOT STDOUT, and M16 is why. The first shape of this case grepped
# `$OUT` for `failures=[1-9]` — but the failures count lives on the verdict line
# in the LOG FILE and never on stdout, so the assertion could not fail and
# M16.docker-daemon-probe-removed scored "the suite still PASSED without this
# property". The mutation harness caught a test that was checking a channel the
# evidence does not travel on.
DLOG="$W_HOME/state/scratch-reaper.log"
if [ "$RC" = "0" ] \
   && ! grep -q 'FAILED docker' "$DLOG" 2>/dev/null \
   && ! grep -q 'failures=[1-9]' "$DLOG" 2>/dev/null; then
    ok "S16  a stopped Docker daemon is skipped and is NOT a failure"
else
    bad "S16  a stopped Docker daemon was reported as a failure (rc=$RC)"
    sed 's/^/        /' "$DLOG" 2>/dev/null | tail -6
fi
if ! grep -q 'class=docker-' "$DLOG" 2>/dev/null; then
    ok "S16b a stopped daemon produces no docker log line at all"
else
    bad "S16b a stopped daemon still logged a docker action"
    sed 's/^/        /' "$DLOG" 2>/dev/null | tail -6
fi

# THE POSITIVE CONTROL. A daemon that ANSWERS must actually be asked to prune,
# all THREE stages, in the order that matters, and the log must carry the bytes
# and the names. Without this, S16 passes against a reaper whose docker arm is
# dead code.
#
# The stub RECORDS THE ARGUMENTS IT WAS CALLED WITH, because the CEO's rule is
# as much about the flags as about the fact of pruning: `-a`, `until=720h`, and
# containers before images. A stub that only echoed a total could not tell a
# correct implementation from one that prunes dangling images forever.
cat >"$STUBDIR/docker" <<STUB
#!/bin/sh
echo "\$@" >>"$W_ROOT/docker-calls.txt"
case "\$1" in
  info)      echo "27.0.0"; exit 0 ;;
  container) echo "Deleted Containers:"; echo "deleted: old-exited-container"
             echo "Total reclaimed space: 442.4kB"; exit 0 ;;
  image)     echo "Deleted Images:"; echo "untagged: stale/app:v1"
             echo "deleted: sha256:aaaa"
             echo "Total reclaimed space: 17.8GB"; exit 0 ;;
  builder)   echo "Deleted build cache objects:"
             echo "Total reclaimed space: 8.5GB"; exit 0 ;;
esac
exit 1
STUB
chmod +x "$STUBDIR/docker"
OUT="$(PATH="$STUBDIR:$PATH" run --apply)"; RC=$?
CALLS="$W_ROOT/docker-calls.txt"
LOG="$W_HOME/state/scratch-reaper.log"

if grep -q 'class=docker-container-prune ' "$LOG" 2>/dev/null \
   && grep -q 'class=docker-image-prune ' "$LOG" 2>/dev/null \
   && grep -q 'class=docker-builder-prune ' "$LOG" 2>/dev/null; then
    ok "S16c CONTROL: a daemon that answers IS asked to prune, all three stages"
else
    bad "S16c CONTROL FAILED: the docker arm is dead code, so S16 proves nothing"
    sed 's/^/        /' "$LOG" 2>/dev/null | tail -8
fi

# CONTAINERS BEFORE IMAGES. A stopped container pins its image, so the reverse
# order silently leaves every pinned image behind.
C_LINE="$(grep -n '^container prune' "$CALLS" 2>/dev/null | head -1 | cut -d: -f1)"
I_LINE="$(grep -n '^image prune' "$CALLS" 2>/dev/null | head -1 | cut -d: -f1)"
if [ -n "$C_LINE" ] && [ -n "$I_LINE" ] && [ "$C_LINE" -lt "$I_LINE" ]; then
    ok "S16d containers are pruned BEFORE images — a stopped container pins its image"
else
    bad "S16d the prune order is wrong (container at '$C_LINE', image at '$I_LINE')"
    sed 's/^/        /' "$CALLS" 2>/dev/null
fi

# THE CEO'S FLAGS, EXACTLY. `-a` is what reaches unused NAMED images, and
# until=720h is the 30 days that makes -a safe.
if grep -q 'image prune -a -f --filter until=720h' "$CALLS" 2>/dev/null \
   && grep -q 'container prune -f --filter until=720h' "$CALLS" 2>/dev/null; then
    ok "S16e the declared rule is on the wire: -a for named images, until=720h"
else
    bad "S16e the prune flags are not the declared standing rule"
    sed 's/^/        /' "$CALLS" 2>/dev/null
fi

if grep -q 'bytes=17800000000' "$LOG" 2>/dev/null \
   && grep -q 'stale/app:v1' "$LOG" 2>/dev/null; then
    ok "S16f the LOG carries the bytes the daemon reported AND the name removed"
else
    bad "S16f the docker log line is missing its bytes or the removed name"
    sed 's/^/        /' "$LOG" 2>/dev/null | tail -8
fi

# S16g — A FRESH IMAGE SURVIVES. The CEO asked for this control by name. The
# age filter is the ONLY thing making `-a` safe, so a stub whose daemon reports
# nothing removed (which is what a real daemon does when every image is inside
# the window) must produce no removal and no failure.
world dockerfresh
python3 - "$W_CFG" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read().replace('SCRATCH_DOCKER_PRUNE="0"', 'SCRATCH_DOCKER_PRUNE="1"')
open(p, "w").write(s)
PY
STUBDIR2="$W_ROOT/stubbin"
mkdir -p "$STUBDIR2"
cat >"$STUBDIR2/docker" <<'STUB'
#!/bin/sh
# A daemon whose every image is NEWER than the filter: docker prints the
# header and a zero total, and removes nothing.
case "$1" in
  info) echo "27.0.0"; exit 0 ;;
  *)    echo "Total reclaimed space: 0B"; exit 0 ;;
esac
STUB
chmod +x "$STUBDIR2/docker"
OUT="$(PATH="$STUBDIR2:$PATH" run --apply)"; RC=$?
LOG2="$W_HOME/state/scratch-reaper.log"
if [ "$RC" = "0" ] \
   && ! grep -q 'untagged\|deleted:' "$LOG2" 2>/dev/null \
   && ! printf '%s' "$OUT" | grep -q 'failures=[1-9]'; then
    ok "S16g CONTROL: a fresh (in-window) image is not removed and is no failure"
else
    bad "S16g a fresh image was removed or reported as a failure (rc=$RC)"
    sed 's/^/        /' "$LOG2" 2>/dev/null | tail -6
fi


# ===========================================================================
# S17 — A FAILED DELETION MUST NOT HIDE ITSELF
# ===========================================================================
# THE DEFECT THIS CASE EXISTS FOR, measured 2026-09-18 on the real machine: five
# of 37,146 deletions failed with EPERM. The NEXT run reported ok:true,
# failures:0 with all five directories still on disk — because rmtree had
# removed some children before it hit the unremovable one, THAT UPDATED THE
# DIRECTORY'S MTIME, and the tree came back "touched 1 min ago, inside the 2 h
# legacy floor" and was KEPT. Silently. For two hours.
#
# The reaper's own failure made its next attempt impossible and erased the
# evidence, which under the CEO's rule means garbage that cannot be removed
# produces no alert at all. So a failure is now durable and is retried
# regardless of age.
world standing
mkdir -p "$W_TMP/zlegacy-stuck/inner"
echo payload >"$W_TMP/zlegacy-stuck/inner/payload.txt"
chmod 500 "$W_TMP/zlegacy-stuck/inner" 2>/dev/null || true

OUT="$(run --apply)"
FSTATE="$W_HOME/state/scratch-failures.json"

# It may have succeeded via the retry, which is the desired outcome and not what
# this case is about. Force the durable path by recording a failure directly and
# proving the NEXT run reconsiders it despite a fresh mtime.
mkdir -p "$W_TMP/zlegacy-recent" "$W_HOME/state"
echo payload >"$W_TMP/zlegacy-recent/payload.txt"
touch "$W_TMP/zlegacy-recent"            # brand new mtime: inside every floor
python3 - "$FSTATE" "$W_TMP/zlegacy-recent" <<'PY'
import json, sys
path, target = sys.argv[1], sys.argv[2]
json.dump({target: {"first": "2026-09-18T07:40:05Z",
                    "last": "2026-09-18T07:40:05Z",
                    "error": "[Errno 1] Operation not permitted",
                    "attempts": 1}},
          open(path, "w"), indent=1)
PY
# THE AGE FLOOR MUST ACTUALLY BE IN FORCE FOR THIS CASE. The world's config sets
# SCRATCH_LEGACY_AGE_HOURS=0, which makes every legacy directory old enough and
# made S17b's "kept by the age floor" control untrue in this world while it
# reported success either way (P5-68). S17 and S17b run under the real 2 h floor.
cp "$W_CFG" "$W_CFG.floor0"
sed -i.bak 's/^SCRATCH_LEGACY_AGE_HOURS=.*/SCRATCH_LEGACY_AGE_HOURS="2"/' "$W_CFG"
OUT="$(run)"
if printf '%s' "$OUT" | grep -q 'A PREVIOUS RUN FAILED TO DELETE THIS'; then
    ok "S17  a recorded failure is reconsidered DESPITE a fresh mtime"
else
    bad "S17  a recorded failure was hidden behind the age floor — the 2026-09-18"
    bad "     defect, where the reaper's own failure erased its own evidence"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -12
fi

# THE POSITIVE CONTROL: the very same fresh directory, with NO failure recorded,
# is correctly KEPT by the age floor. Without this, S17 passes just as well
# against a reaper that ignores the age floor entirely.
rm -f "$FSTATE"
OUT="$(run --verbose)"
# The disposition, not the name: the directory must be listed as a KEEP and must
# not be listed as a DELETE. Its name appearing anywhere proves neither.
if printf '%s\n' "$OUT" | grep -E '(^|[[:space:]])KEEP[[:space:]].*zlegacy-recent' >/dev/null \
   && ! printf '%s\n' "$OUT" | grep -E '(^|[[:space:]])DELETE[[:space:]].*zlegacy-recent' >/dev/null \
   && ! printf '%s' "$OUT" | grep -q 'A PREVIOUS RUN FAILED'; then
    ok "S17b CONTROL: with no failure recorded the same fresh directory is kept"
else
    bad "S17b CONTROL: the fresh directory was not KEPT by the age floor, so S17 cannot tell a working floor from none"
    printf '%s\n' "$OUT" | grep 'zlegacy-recent' | sed 's/^/        /' | head -5
fi
cp "$W_CFG.floor0" "$W_CFG"

# AND THE FILE MUST EMPTY ITSELF. A failure record that only ever grew would be
# a permanent alert about paths cleaned up weeks ago, and a permanent alert is
# one nobody reads.
python3 - "$FSTATE" <<'PY'
import json, sys
json.dump({"/nonexistent/path/that/was/cleaned/up": {
    "first": "2026-09-01T00:00:00Z", "last": "2026-09-01T00:00:00Z",
    "error": "gone", "attempts": 3}}, open(sys.argv[1], "w"), indent=1)
PY
run --apply >/dev/null 2>&1
if [ ! -f "$FSTATE" ] || ! grep -q 'was/cleaned/up' "$FSTATE" 2>/dev/null; then
    ok "S17c a recorded failure whose path is GONE leaves the file"
else
    bad "S17c a resolved failure stayed on the books — a permanent false alert"
    sed 's/^/        /' "$FSTATE" 2>/dev/null
fi

# ===========================================================================
# S18 — AN OBSTACLE TO REMOVAL IS CLEARED IN SCRATCH, REPORTED ELSEWHERE
# ===========================================================================
# Measured 2026-09-18: two trees resisted both shutil.rmtree AND /bin/rm -rf
# with EPERM. The cause was one file, .../.parked-agent-*/pinned.txt, carrying
# chflags uchg (flag 0x2), left by a harness that tests pinned files. Mode 644,
# owner the invoking user; only the flag stood in the way.
world unstick
mkdir -p "$W_TMP/zlegacy-pinned"
echo payload >"$W_TMP/zlegacy-pinned/pinned.txt"
if chflags uchg "$W_TMP/zlegacy-pinned/pinned.txt" 2>/dev/null; then
    OUT="$(run --apply)"
    if [ ! -d "$W_TMP/zlegacy-pinned" ]; then
        ok "S18  a uchg-pinned file in a legacy family is cleared and removed"
    else
        bad "S18  a uchg-pinned legacy tree could not be reclaimed"
        printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
    fi
    if grep -q 'uchg flag' "$W_HOME/state/scratch-reaper.log" 2>/dev/null; then
        ok "S18b the clearing is LOGGED — it is a power, so it is auditable"
    else
        bad "S18b the flag was cleared without saying so in the log"
    fi

    # THE BOUND. A uchg file in a CLAUDE SESSION's scratch is NOT unstuck: a
    # person may have pinned that deliberately, and they get asked instead. This
    # is the case that proves the clearing is bounded rather than universal.
    world unstickbound
    mkdir -p "$W_SCRATCH/-proj/$SID_DEAD/scratchpad"
    echo payload >"$W_SCRATCH/-proj/$SID_DEAD/scratchpad/pinned.txt"
    chmod 500 "$W_SCRATCH/-proj/$SID_DEAD/scratchpad" 2>/dev/null || true
    chflags uchg "$W_SCRATCH/-proj/$SID_DEAD/scratchpad/pinned.txt" 2>/dev/null || true
    OUT="$(run --apply)"
    if [ -f "$W_SCRATCH/-proj/$SID_DEAD/scratchpad/pinned.txt" ]; then
        ok "S18c a uchg file in a SESSION's scratch is NOT unstuck — reported,"
        ok "     not overridden, because a person may have pinned it on purpose"
    else
        bad "S18c the flag-clearing reached a session scratchpad. It is supposed"
        bad "     to be bounded to machine-made harness scratch."
    fi
    chflags nouchg "$W_SCRATCH/-proj/$SID_DEAD/scratchpad/pinned.txt" 2>/dev/null || true
    chmod 700 "$W_SCRATCH/-proj/$SID_DEAD/scratchpad" 2>/dev/null || true
else
    ok "S18  SKIPPED — chflags is not available on this host"
    ok "S18b SKIPPED — chflags is not available on this host"
    ok "S18c SKIPPED — chflags is not available on this host"
fi

# ===========================================================================
# S19 — THE ALLOCATOR ARM DOES NOT DELETE A TREE A LIVE PROCESS IS READING
# ===========================================================================
# This was a real defect, found by Frank's attack pass and reproduced here on
# the operator's own machine before it was fixed: the allocator arm was the
# ONLY arm that went from "the owning pid is dead" straight to DELETE without
# consulting the open-file table. Both $TMPDIR arms already checked. Measured
# on the real allocator root, with a dead owner pid in the directory name and a
# live `tail -f` inside:
#
#   DELETE  2.0 MB  .../richos-scratch/99999997-zach-d5-repro
#     why: pid 99999997 is ENDED (owner of 'unrecorded', from the directory
#          name) and nothing has touched this for 374961 min
#
# That is data loss, not garbage collection. The owning pid being dead says the
# ALLOCATION is over; it says nothing about who is reading the tree now.
world allocheld
mkdir -p "$W_TMP/richos-scratch/99999997-held"
echo payload >"$W_TMP/richos-scratch/99999997-held/payload.txt"
if command -v lsof >/dev/null 2>&1; then
    # `tail -f` for the reason S15's comment gives: a single process that holds
    # the descriptor itself, so killing it actually releases it.
    : >"$W_TMP/richos-scratch/99999997-held/held.lock"
    tail -f "$W_TMP/richos-scratch/99999997-held/held.lock" >/dev/null 2>&1 &
    AHELD_PID=$!
    KILL_LIST="$KILL_LIST $AHELD_PID"
    sleep 1
    OUT="$(run --apply)"
    if [ -f "$W_TMP/richos-scratch/99999997-held/payload.txt" ]; then
        ok "S19  an allocation with a dead owner but a LIVE holder is kept"
    else
        bad "S19  a tree was deleted from under a live process (D5)"
        printf '%s\n' "$OUT" | sed 's/^/        /'
    fi
    # The reason must NAME the holder. A right verdict for an unstated reason is
    # a verdict nobody can check, and this arm's old reason said only that the
    # owner was dead.
    #
    # --verbose, because report() prints KEEP lines only under it; the first
    # version of this assertion read --apply's output, where the kept entry does
    # not appear at all, and failed for that reason rather than for the reaper's.
    OUT="$(run --dry-run --verbose)"
    if printf '%s\n' "$OUT" | grep -q "holds a file open inside this tree"; then
        ok "S19b the reason says a live process holds it, not just that the owner died"
    else
        bad "S19b kept, but the reason does not name the holder"
        printf '%s\n' "$OUT" | sed 's/^/        /'
    fi
    kill "$AHELD_PID" >/dev/null 2>&1 || true
    wait "$AHELD_PID" 2>/dev/null || true

    # THE POSITIVE CONTROL. Without it every assertion above passes against an
    # arm that has simply stopped deleting anything at all — which is exactly
    # what a too-eager wall would look like.
    OUT="$(run --apply)"
    if [ ! -d "$W_TMP/richos-scratch/99999997-held" ]; then
        ok "S19c CONTROL: with the holder gone the same allocation is deleted"
    else
        bad "S19c CONTROL FAILED: the allocator arm keeps it whether held or not,"
        bad "     so S19 proves nothing"
        printf '%s\n' "$OUT" | sed 's/^/        /'
    fi
else
    ok "S19  SKIPPED — lsof is not on this host"
    ok "S19b SKIPPED — lsof is not on this host"
    ok "S19c SKIPPED — lsof is not on this host"
fi

# ===========================================================================
# S20 — A TEST INSTANCE OF THE APP DOES NOT MAKE ITS OWN GARBAGE IMMORTAL
# ===========================================================================
# §54 addendum 4. The wall S19 adds is correct and it introduces a new way to
# lose: a stray app instance holds its own scratch open, so the tree is KEPT,
# on every run, for ever. That was observed for real while this was being
# written — a fixture app left behind by a killed harness kept its sandbox
# alive, and the sweeper's verdict was KEEP, "a process holds a file open
# inside it", which was correct by the rule and guaranteed the garbage stayed.
#
# So a holder that is POSITIVELY a collectable test instance is quit first and
# the tree removed; ANY other holder still means KEEP (S19 is that case).
world allocapp
mkdir -p "$W_TMP/richos-scratch/99999996-appheld"
echo payload >"$W_TMP/richos-scratch/99999996-appheld/payload.txt"
if command -v lsof >/dev/null 2>&1 && command -v cc >/dev/null 2>&1; then
    # A REAL binary really named richos-tauri, holding a file inside the tree.
    # Compiled, not a shell script: bash keeps its script open on a numeric
    # descriptor, which would put the fixture's own path into the evidence and
    # test the fixture instead of the reaper.
    cat > "$W_ROOT/fake.c" <<'CSRC'
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
int main(void) {
    const char *s = getenv("APP_STATE");
    if (s) { FILE *f = fopen(s, "a+"); if (!f) return 2; fprintf(f, "x\n"); fflush(f); }
    for (long i = 0; i < 600; i++) { usleep(200000); }
    return 0;
}
CSRC
    mkdir -p "$W_ROOT/appbin"
    if cc -o "$W_ROOT/appbin/richos-tauri" "$W_ROOT/fake.c" 2>/dev/null; then
        # A SECOND richos-tauri OUTSIDE THIS SUITE'S PROCESS TREE, holding
        # nothing. The Mac this suite runs on is never quiet: during the
        # 2026-09-30 merge of echo-sonnet-app1, `cargo test --bin richos-tauri`
        # ran in parallel with this case, appinstances shortlisted it by that
        # argv token and asked `lsof -p <fake>,<cargo>`, and the scoped lsof
        # REFUSED the whole query because one pid lay outside the fixture. No
        # answer made every instance INDETERMINATE, so S20, S20b and S20c all
        # failed for a reason that had nothing to do with the reaper. The
        # decoy makes that condition part of the case instead of an accident
        # of scheduling: it is reparented to launchd by the subshell exiting,
        # so it is outside the tree exactly as a peer's process is.
        DECOY_PIDFILE="$W_ROOT/decoy.pid"
        ( "$W_ROOT/appbin/richos-tauri" >/dev/null 2>&1 &
          echo $! >"$DECOY_PIDFILE" )
        DECOY_PID="$(cat "$DECOY_PIDFILE" 2>/dev/null || true)"
        [ -n "$DECOY_PID" ] && KILL_LIST="$KILL_LIST $DECOY_PID"
        APP_STATE="$W_TMP/richos-scratch/99999996-appheld/app.state" \
            "$W_ROOT/appbin/richos-tauri" >/dev/null 2>&1 &
        APP_PID=$!
        KILL_LIST="$KILL_LIST $APP_PID"
        # Wait for the descriptor, so the case cannot race the fixture's start.
        i=0
        while [ $i -lt 100 ]; do
            if lsof -p "$APP_PID" 2>/dev/null | grep -q "app.state"; then break; fi
            sleep 0.05; i=$((i + 1))
        done
        OUT="$(run --apply)"
        if [ ! -d "$W_TMP/richos-scratch/99999996-appheld" ]; then
            ok "S20  a tree held ONLY by a test instance is collected, not kept for ever"
        else
            bad "S20  a test instance made its own scratch immortal"
            printf '%s\n' "$OUT" | sed 's/^/        /'
        fi
        if ! kill -0 "$APP_PID" 2>/dev/null; then
            ok "S20b the instance itself was quit, and the pid is verified gone"
        else
            bad "S20b the tree went but the app instance is STILL RUNNING — the"
            bad "     window would still be on his screen (§54 addendum 4)"
            kill -9 "$APP_PID" 2>/dev/null || true
        fi
        if grep -q 'unix id is' "${SRT_OSASCRIPT_LOG:-/nonexistent}" 2>/dev/null; then
            ok "S20e the quit request went to this suite's osascript stand-in, never to System Events"
        else
            bad "S20e the reaper's quit of the fake instance did not go through the suite's osascript stand-in"
        fi
        # THE LOG FILE, not stdout. The first version of this assertion grepped
        # --apply's stdout and failed while S20/S20b passed: the quit lines go
        # where every other deletion line goes, which is the log. A case that
        # reads the wrong channel reports a defect that is not there — the same
        # mistake M16 caught in S16 when this suite was first written.
        if grep -q "QUIT test-instance" "$W_HOME/state/scratch-reaper.log" 2>/dev/null; then
            ok "S20c the log says which instance it quit, so the act is auditable"
        else
            bad "S20c nothing in the log names the instance that was quit"
            sed -n '1,20p' "$W_HOME/state/scratch-reaper.log" 2>/dev/null \
                | sed 's/^/        /'
        fi
        # The decoy holds nothing under any scratch root, so it is not a test
        # instance and must still be running: deny-by-default, whoever it is.
        if [ -n "$DECOY_PID" ] && kill -0 "$DECOY_PID" 2>/dev/null; then
            ok "S20d a richos-tauri outside the fixture holding no scratch was left running"
        else
            bad "S20d the decoy richos-tauri outside the fixture is gone (pid ${DECOY_PID:-none})"
        fi
        [ -n "$DECOY_PID" ] && kill -9 "$DECOY_PID" 2>/dev/null || true
    else
        ok "S20  SKIPPED — the fixture would not compile on this host"
        ok "S20b SKIPPED — the fixture would not compile on this host"
        ok "S20c SKIPPED — the fixture would not compile on this host"
        ok "S20d SKIPPED — the fixture would not compile on this host"
    fi
else
    ok "S20  SKIPPED — lsof or cc is not on this host"
    ok "S20b SKIPPED — lsof or cc is not on this host"
    ok "S20c SKIPPED — lsof or cc is not on this host"
    ok "S20d SKIPPED — lsof or cc is not on this host"
fi

# ===========================================================================
# S21 — DENY-BY-DEFAULT OVER $TMPDIR AND THE SHARED TEMP ROOTS
# ===========================================================================
# Frank's D1 and D2, and they were not defeats of a check — they were the
# absence of one. `scan_tmp` skipped past any name matching neither declared
# pattern list, so 56,770 of 60,942 entries under $TMPDIR (1.95 GB, measured on
# this machine 2026-09-18) were not kept and not deleted: they were never looked
# at, and no number in the verdict line could be used to notice them.
# /private/tmp outside the two claude roots was read by no arm at all.
#
# EVERY CASE BELOW HAS ITS CONTROL, because every one of them also passes
# against an arm that simply deletes nothing.
deny_world() {              # <name> — a world with the deny-by-default arm ON
    world "$1"
    mkdir -p "$W_ROOT/shared"
    {
        echo 'SCRATCH_TMP_DENY_BY_DEFAULT="1"'
        echo "SCRATCH_SHARED_TMP_ROOTS=\"$W_ROOT/shared\""
        # A foreign family NOTHING ELSE IN THIS SUITE USES.
        echo 'SCRATCH_FOREIGN_PATTERNS="zforeign.*"'
        echo 'SCRATCH_UNKNOWN_AGE_HOURS="1"'
        echo 'SCRATCH_UNKNOWN_GIT_IS_FIXTURE="0"'
    } >>"$W_CFG"
}

# backdate <dir> — every mtime in the tree, THE DIRECTORY ITSELF INCLUDED, set
# two days back.
#
# THE DIRECTORY ITSELF IS THE WHOLE POINT, and two cases in this block failed for
# want of it before it was a function. Creating a file inside a directory updates
# THAT DIRECTORY's mtime, so a fixture built as "backdate the tree, then add a
# lock file" leaves the candidate itself stamped now — and the arm correctly
# kept it, saying a running session might own it. The suite was wrong and the
# reaper was right, which is the only way round that is any use.
backdate() {
    find "$1" -exec touch -t "$(date -v-2d +%Y%m%d%H%M 2>/dev/null \
        || date -d '2 days ago' +%Y%m%d%H%M)" {} + 2>/dev/null || true
}

# stale <dir> — a two-day-old tree with a payload in it.
stale() {
    mkdir -p "$1"
    printf 'payload\n' >"$1/payload.txt"
    backdate "$1"
}

deny_world denyname
start_claude; PID_D1="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D1" "$SID_LIVE"
stale "$W_TMP/zzz-nobody-declares-this"
OUT="$(run --apply)"
if [ ! -d "$W_TMP/zzz-nobody-declares-this" ]; then
    ok "S21  a \$TMPDIR name NO declared pattern matches is swept (D1)"
else
    bad "S21  an undeclared name was skipped, exactly as it was before (D1)"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
case "$OUT" in
    *"no arm declares this name"*)
        ok "S21a the reason says it was decided, not merely matched" ;;
    *) bad "S21a deleted, but not by the deny-by-default arm"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac

# THE CONTROL THAT MATTERS MOST: with the switch declared OFF, the identical
# fixture is not deleted AND NOT EVEN MENTIONED. That is the old behavior, and it
# is what proves S21 is the switch doing the work rather than some other arm.
world denyoff
start_claude; PID_D2="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D2" "$SID_LIVE"
stale "$W_TMP/zzz-nobody-declares-this"
run --apply >/dev/null 2>&1
OUT="$(run --dry-run --verbose)"
if [ -d "$W_TMP/zzz-nobody-declares-this" ]; then
    ok "S21b CONTROL: with SCRATCH_TMP_DENY_BY_DEFAULT unset the same tree survives"
else
    bad "S21b the tree went with the switch off — S21 proves nothing about the switch"
fi
case "$OUT" in
    *zzz-nobody-declares-this*)
        bad "S21c with the switch off the entry was still considered" ;;
    *)  ok "S21c and with the switch off it is not even MENTIONED — which is D1" ;;
esac

# --- the keep-list -----------------------------------------------------------
deny_world denyforeign
start_claude; PID_D3="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D3" "$SID_LIVE"
stale "$W_TMP/zforeign.someone-elses-app"
stale "$W_TMP/zzz-ours"
OUT="$(run --apply)"
if [ -d "$W_TMP/zforeign.someone-elses-app" ]; then
    ok "S21d a declared FOREIGN owner is kept, never deleted by us"
else
    bad "S21d another program's temp directory was deleted"
    printf '%s\n' "$OUT" | sed 's/^/        /' ;
fi
if [ ! -d "$W_TMP/zzz-ours" ]; then
    ok "S21e CONTROL: the identical non-foreign sibling in the same run IS deleted"
else
    bad "S21e nothing was deleted in that run, so S21d proves nothing"
fi
OUT="$(run --dry-run --verbose)"
case "$OUT" in
    *"declared FOREIGN owner"*)
        ok "S21f the foreign entry is COUNTED and named, not silently skipped" ;;
    *) bad "S21f a foreign entry vanished from the report — that is D1 again"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac

# --- the proof is the session table, not the age -----------------------------
deny_world denyfresh
start_claude; PID_D4="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D4" "$SID_LIVE"
mkdir -p "$W_TMP/zzz-written-just-now"
echo payload >"$W_TMP/zzz-written-just-now/payload.txt"
OUT="$(run --apply)"
if [ -d "$W_TMP/zzz-written-just-now" ]; then
    ok "S21g a tree touched AFTER a running session started is kept"
else
    bad "S21g a tree a running session may own was deleted"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
OUT="$(run --dry-run --verbose)"
case "$OUT" in
    *"touched after the earliest running session process started"*)
        ok "S21h the reason is the PROOF (the process table), not the mtime" ;;
    *) bad "S21h kept, but not for the session-table reason"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac

# --- the shared root (D2) ----------------------------------------------------
deny_world denyshared
start_claude; PID_D5="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D5" "$SID_LIVE"
stale "$W_ROOT/shared/zzz-in-the-shared-root"
OUT="$(run --apply)"
if [ ! -d "$W_ROOT/shared/zzz-in-the-shared-root" ]; then
    ok "S21i a DECLARED SHARED temp root is swept too (D2 — /private/tmp)"
else
    bad "S21i the shared root was not swept; D2 is still open"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# --- the walls still stand over the new arm ----------------------------------
deny_world denywall3
start_claude; PID_D6="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D6" "$SID_LIVE"
stale "$W_TMP/zzz-holds-a-workspace/workspace"
mkdir -p "$W_HOME/state/workspaces/agents"
python3 - "$W_HOME/state/workspaces/agents/d.json" \
          "$W_TMP/zzz-holds-a-workspace/workspace" <<'PY'
import json, sys
path, wt = sys.argv[1:3]
with open(path, "w", encoding="utf-8") as fh:
    json.dump({"name": "someone", "worktree": wt, "workspaces": [wt]}, fh)
PY
# The CANDIDATE is the parent, and `mkdir -p .../workspace` stamped it now.
backdate "$W_TMP/zzz-holds-a-workspace"
OUT="$(run --apply)"; RC=$?
if [ -d "$W_TMP/zzz-holds-a-workspace/workspace" ]; then
    ok "S21j wall 3 stands over the new arm — a REGISTERED workspace survives"
else
    bad "S21j the new arm deleted a registered workspace"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
case "$OUT" in
    *"REGISTERED workspace"*) ok "S21k and the refusal NAMES the registration" ;;
    *) bad "S21k kept without saying why"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac

# --- the open-file wall, which is D5 one arm across --------------------------
if command -v lsof >/dev/null 2>&1; then
    deny_world denyheld
    start_claude; PID_D7="$LAST_CLAUDE"
    register "$W_SESSIONS" "$PID_D7" "$SID_LIVE"
    stale "$W_TMP/zzz-somebody-is-reading"
    : >"$W_TMP/zzz-somebody-is-reading/held.lock"
    # AFTER the lock file exists, not before: creating it stamped the directory.
    backdate "$W_TMP/zzz-somebody-is-reading"
    tail -f "$W_TMP/zzz-somebody-is-reading/held.lock" >/dev/null 2>&1 &
    UHELD_PID=$!
    KILL_LIST="$KILL_LIST $UHELD_PID"
    sleep 1
    OUT="$(run --apply)"
    if [ -d "$W_TMP/zzz-somebody-is-reading" ]; then
        ok "S21l an undeclared tree a LIVE process is reading is kept"
    else
        bad "S21l the new arm deleted a tree from under a live process"
        printf '%s\n' "$OUT" | sed 's/^/        /'
    fi
    kill "$UHELD_PID" >/dev/null 2>&1 || true
    wait "$UHELD_PID" 2>/dev/null || true
    OUT="$(run --apply)"
    if [ ! -d "$W_TMP/zzz-somebody-is-reading" ]; then
        ok "S21m CONTROL: with the holder gone the same tree IS deleted"
    else
        bad "S21m CONTROL FAILED: kept whether held or not, so S21l proves nothing"
        printf '%s\n' "$OUT" | sed 's/^/        /'
    fi
else
    ok "S21l SKIPPED — lsof is not on this host"
    ok "S21m SKIPPED — lsof is not on this host"
fi

# --- the declared age floor, which is the SECOND condition -------------------
# The session-table proof is the first one, and on its own it would delete a
# directory written five minutes before the only running session started. The
# declared floor is what stops that, and nothing else in this block exercises it:
# every case above is either fresh (caught by the proof) or two days old (past
# both). So this case has to make the process table say yes and the floor say no.
#
# EVERY SUITE PROCESS IS KILLED FIRST so the ONLY running session on the machine
# is the one this case starts — otherwise "the earliest running session" is some
# process an earlier case started minutes ago and the construction is a race
# rather than a test. It runs last in this block for that reason.
for p in $KILL_LIST; do
    kill "$p" >/dev/null 2>&1 || true
    wait "$p" >/dev/null 2>&1 || true
done
KILL_LIST=""
deny_world denyfloor
mkdir -p "$W_TMP/zzz-recent-but-orphaned"
echo payload >"$W_TMP/zzz-recent-but-orphaned/payload.txt"
find "$W_TMP/zzz-recent-but-orphaned" -exec touch -t \
    "$(date -v-5M +%Y%m%d%H%M 2>/dev/null || date -d '5 minutes ago' +%Y%m%d%H%M)" \
    {} + 2>/dev/null || true
start_claude; PID_D8="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D8" "$SID_LIVE"
OUT="$(run --apply)"
if [ -d "$W_TMP/zzz-recent-but-orphaned" ]; then
    ok "S21n nothing running can own it, but the declared floor still keeps it"
else
    bad "S21n the age floor was not applied — the proof alone deleted it"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
OUT="$(run --dry-run --verbose)"
case "$OUT" in
    *"floor for an undeclared temp family"*)
        ok "S21o and the reason is the floor, not the process table" ;;
    *) bad "S21o kept, but not by the floor"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac
backdate "$W_TMP/zzz-recent-but-orphaned"
OUT="$(run --apply)"
if [ ! -d "$W_TMP/zzz-recent-but-orphaned" ]; then
    ok "S21p CONTROL: the same tree past the floor IS deleted"
else
    bad "S21p CONTROL FAILED: kept at any age, so S21n proves nothing"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi

# ===========================================================================
# S22 — THE GARBAGE ALARM, AND A FAILED DELETION THAT REACHES THE EXIT CODE
# ===========================================================================
# frank-opus-garbage1's verdict on the whole mechanism: "Its alarm is a
# disk-space alarm and a delete-failure alarm. It has no garbage alarm." The
# CEO's rule has two halves — always cleaned up, OR Rich gets a MASSIVE ALERT —
# and garbage nothing would ever collect fell between them in silence.

deny_world skipcount
start_claude; PID_D9="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D9" "$SID_LIVE"
stale "$W_TMP/zforeign.not-ours-at-all"
OUT="$(run --dry-run)"
case "$OUT" in
    *"skipped=0 "*|*"skipped=0"*)
        bad "S22  the verdict line says skipped=0 with a foreign pile standing" ;;
    *skipped=*skipped_bytes=*)
        ok "S22  the verdict line carries skipped= AND skipped_bytes=" ;;
    *) bad "S22  the verdict line carries no skipped count at all"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac

# THE CONTROL. A world with nothing foreign in it must say skipped=0 — otherwise
# S22 is measuring a constant rather than the pile.
deny_world skipzero
start_claude; PID_D10="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D10" "$SID_LIVE"
stale "$W_TMP/zzz-ordinary"
OUT="$(run --dry-run)"
case "$OUT" in
    *"skipped=0"*) ok "S22b CONTROL: with nothing foreign the same line says skipped=0" ;;
    *) bad "S22b skipped= is not zero on a world with nothing to skip"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac

# --- a failed deletion reaches the exit code and stdout (D10) ----------------
# It used to be `return 3 if undecidable else 0` with the failure list never
# consulted, and stdout said `applied: deleted=0 freed=0 B` — byte-identical in
# shape to a run with nothing to do. launchd saw green on a run that could not
# delete a thing, and the word FAILED existed only inside the log.
world failexit
mkdir -p "$W_TMP/zlegacy-cannotgo/inner"
echo payload >"$W_TMP/zlegacy-cannotgo/inner/payload.txt"
# The PARENT's mode, not the tree's: _make_writable widens the tree it is handed
# and never its parent, which is the hole Frank went through to force a failure.
chmod 500 "$W_TMP" 2>/dev/null || true
OUT="$(run --apply)"; RC=$?
chmod 700 "$W_TMP" 2>/dev/null || true
if [ "$RC" = "4" ]; then
    ok "S22c a FAILED deletion exits 4 — launchd can no longer see green"
else
    bad "S22c a failed deletion exited $RC; a failure is not in the exit code (D10)"
    printf '%s\n' "$OUT" | sed 's/^/        /' | tail -6
fi
case "$OUT" in
    *"failures=1"*|*"failures=2"*|*"failures=3"*)
        ok "S22d and stdout says failures=N, not only the log file" ;;
    *) bad "S22d stdout gave no failure count — the run looks like a quiet one"
       printf '%s\n' "$OUT" | sed 's/^/        /' | tail -6 ;;
esac

# THE CONTROL: the same world with nothing unremovable exits 0 and says
# failures=0. Without it, S22c passes against a program that always exits 4.
world failexitok
mkdir -p "$W_TMP/zlegacy-cango"
echo payload >"$W_TMP/zlegacy-cango/payload.txt"
OUT="$(run --apply)"; RC=$?
if [ "$RC" = "0" ] && printf '%s' "$OUT" | grep -q 'failures=0'; then
    ok "S22e CONTROL: a clean run still exits 0 and says failures=0"
else
    bad "S22e a clean run exited $RC — exit 4 is not conditional on a failure"
    printf '%s\n' "$OUT" | sed 's/^/        /' | tail -6
fi

# --- the state file is the interface the watchdog reads ----------------------
deny_world skipstate
start_claude; PID_D11="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D11" "$SID_LIVE"
stale "$W_TMP/zforeign.published"
run --apply >/dev/null 2>&1
SFILE="$W_HOME/state/scratch-reaper-state.json"
if python3 - "$SFILE" <<'PY'
import json, sys
try:
    st = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    sys.exit(1)
sys.exit(0 if (st.get("skipped_bytes") or 0) > 0
         and "skipped" in st and "undecidable_bytes" in st else 1)
PY
then
    ok "S22f --apply PUBLISHES skipped/skipped_bytes/undecidable_bytes"
else
    bad "S22f the state file carries no garbage numbers, so the watchdog cannot"
    bad "     see them and the garbage alarm has nothing to read"
    cat "$SFILE" 2>/dev/null | sed 's/^/        /'
fi

# --- the banner quotes the last full pass, and does NOT re-derive it ---------
# The first version of this ran the expensive arm under a budget and printed
# "44,851 entries were NOT MEASURED" at every session start forever. A line that
# is always true is wallpaper, and wallpaper is how a real signal gets skipped.
deny_world noticegarbage
start_claude; PID_D12="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D12" "$SID_LIVE"
stale "$W_TMP/zzz-a-real-candidate"
mkdir -p "$W_HOME/state"
python3 - "$W_HOME/state/scratch-reaper-state.json" <<'PY'
import json, sys
json.dump({"last_apply": "2026-09-18T04:40:00Z", "deleted": 0, "freed": 0,
           "undecidable": 0, "undecidable_bytes": 0,
           "skipped": 7, "skipped_bytes": 5000000000,
           "failures": 0, "verdict": "decided"}, open(sys.argv[1], "w"))
PY
echo 'SCRATCH_SKIPPED_NOTICE_BYTES="1073741824"' >>"$W_CFG"
OUT="$(run --notice)"
case "$OUT" in
    *"NOTHING WILL EVER COLLECT"*)
        ok "S22g the banner raises the GARBAGE ALARM from the last full pass" ;;
    *) bad "S22g no garbage alarm with 5 GB of skipped garbage on the books"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac
case "$OUT" in
    *2026-09-18T04:40:00Z*)
        ok "S22h and it DATES the number, because it is not a live reading" ;;
    *) bad "S22h the banner presented a six-hour-old number as if it were live"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac
# THE OBSERVABLE FOR "IT DOES NOT ATTEMPT THE EXPENSIVE ARM" is that the banner
# cannot see what only that arm decides. This world holds one stale undeclared
# candidate and SCRATCH_NOTICE_BYTES is 1, so a banner that ran the arm would
# announce reclaimable dead scratch. A banner that skips it cannot.
#
# ASSERTING ON "NOT MEASURED" WAS NOT ENOUGH AND THE MUTATION HARNESS SAID SO:
# M33 scored green against a banner that re-ran the arm, because a small test
# world never exhausts a budget and so never prints that line at all. The
# assertion has to be about the arm's RESULTS, not about its failure mode.
case "$OUT" in
    *"of dead scratch"*)
        bad "S22i the banner is attempting the expensive arm again — it reported"
        bad "     scratch only that arm can find" ;;
    *)  ok "S22i and it does not attempt the expensive arm, so it stays cheap" ;;
esac
# THE CONTROL: the very same world, scanned fully, DOES find that candidate.
# Without it S22i passes against a banner that reports nothing ever.
OUT2="$(run --dry-run)"
case "$OUT2" in
    *zzz-a-real-candidate*)
        ok "S22i2 CONTROL: a full scan of the same world DOES find it" ;;
    *) bad "S22i2 the candidate is invisible to a full scan too, so S22i is vacuous"
       printf '%s\n' "$OUT2" | sed 's/^/        /' ;;
esac

# THE CONTROL: below the declared threshold the same banner is silent. Without
# it, S22g passes against a banner that shouts unconditionally.
echo 'SCRATCH_SKIPPED_NOTICE_BYTES="9999999999999"' >>"$W_CFG"
OUT="$(run --notice)"
case "$OUT" in
    *"NOTHING WILL EVER COLLECT"*)
        bad "S22j the garbage alarm ignores its own declared threshold" ;;
    *)  ok "S22j CONTROL: below the declared threshold the alarm is silent" ;;
esac

# ===========================================================================
# S23 — A NAME-DERIVED PID IS ATTRIBUTION, NOT LIVENESS (D11)
# ===========================================================================
# frank-opus-garbage1 put a directory called `1-frank-immortal-b` in the allocator
# root and the reaper answered:
#
#   KEEP  2.0 MB  .../richos-scratch/1-frank-immortal-b
#         why: pid 1 is ALIVE (owner of 'unrecorded', from the directory name)
#              — a live owner is kept whatever its TTL says
#
# Forever, at any size, with no alert and no TTL escape — AND SILENTLY BY
# CONSTRUCTION, because a KEEP is the reaper working as designed. pid_alive()
# returns True on PermissionError, which is what pid 1 gives, and macOS recycles
# pids at 99998, so a week-old name whose leading digits match a live pid is not
# exotic.
world immortal
echo 'SCRATCH_MIN_OWNER_PID="100"' >>"$W_CFG"
start_claude; PID_D13="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D13" "$SID_LIVE"
mkdir -p "$W_TMP/richos-scratch/1-immortal"
echo payload >"$W_TMP/richos-scratch/1-immortal/payload.txt"
OUT="$(run --apply)"
if [ ! -d "$W_TMP/richos-scratch/1-immortal" ]; then
    ok "S23  a directory named for pid 1 is no longer immortal (D11)"
else
    bad "S23  pid 1 still makes an allocation live forever"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# THE CONTROL THAT MATTERS: a directory named for a pid that IS a live process of
# OURS, above the floor, started before the directory existed, is still KEPT.
# Without it S23 passes against a reaper that has stopped believing any name.
world liveowner
echo 'SCRATCH_MIN_OWNER_PID="100"' >>"$W_CFG"
start_claude; PID_D14="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D14" "$SID_LIVE"
mkdir -p "$W_TMP/richos-scratch/$PID_D14-mine"
echo payload >"$W_TMP/richos-scratch/$PID_D14-mine/payload.txt"
OUT="$(run --apply)"
if [ -d "$W_TMP/richos-scratch/$PID_D14-mine" ]; then
    ok "S23b CONTROL: a live pid of OURS, above the floor, is still a live owner"
else
    bad "S23b a running owner's sandbox was deleted from under it — worse than D11"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# PID REUSE, CONSTRUCTED RATHER THAN ARGUED ABOUT. The directory is created FIRST,
# then a process is started, then the directory is RENAMED to carry that process's
# pid — and rename preserves st_birthtime because the inode does not change. So the
# name says a live pid owns it and the filesystem says that process did not exist
# when the directory was made.
world pidreuse
echo 'SCRATCH_MIN_OWNER_PID="100"' >>"$W_CFG"
mkdir -p "$W_TMP/richos-scratch/pending"
echo payload >"$W_TMP/richos-scratch/pending/payload.txt"
sleep 2
start_claude; PID_D15="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D15" "$SID_LIVE"
mv "$W_TMP/richos-scratch/pending" "$W_TMP/richos-scratch/$PID_D15-reused"
OUT="$(run --dry-run --verbose)"
if printf '%s' "$OUT" | grep -q 'this is pid reuse'; then
    ok "S23c a live pid that started AFTER the directory is called pid reuse"
else
    bad "S23c pid reuse was read as a live owner, so the KEEP is immortal again"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi
OUT="$(run --apply)"
if [ ! -d "$W_TMP/richos-scratch/$PID_D15-reused" ]; then
    ok "S23d ...and it is collected rather than kept for ever"
else
    bad "S23d named as reuse and still kept"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# AND A LEDGER ROW IS EXEMPT FROM ALL THREE TESTS, because it was written by the
# allocator AT allocation time rather than derived from a name anybody can choose.
# This is the case that keeps the narrowing narrow.
#
# THE FIXTURE IS DELIBERATELY EXTREME — a ledger row whose pid is 1 — because that
# is the only shape where the two branches disagree, and a case where they agree
# proves nothing about which one ran. The allocator writes `$$` and could never
# produce this row; the point is to show the ledger branch is the branch taken.
world ledgerpid
echo 'SCRATCH_MIN_OWNER_PID="100"' >>"$W_CFG"
start_claude; PID_D16="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D16" "$SID_LIVE"
alloc "recorded-by-pid-one" 1 "a-real-allocation" >/dev/null
OUT="$(run --apply)"
if [ -d "$W_TMP/richos-scratch/recorded-by-pid-one" ]; then
    ok "S23e a LEDGER row is exempt from the two NAME-shape tests"
else
    bad "S23e the name-shape tests reached a ledger row, which is a positive record"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# THE SECOND HOLE IN THE SAME FAMILY, AND THE MUTATION HARNESS IS WHY IT IS HERE.
# M37 came back "the suite still PASSED without this property", which sent me back
# to the code — where scan_scratch_root's own comment claimed TTL let a row "whose
# pid has been REUSED by an unrelated process still age out". It did not: a live
# pid returned KEEP two lines before TTL was ever consulted, so a ledger row from
# three days ago whose pid now belongs to an unrelated live process was immortal in
# exactly the way `1-frank-immortal-b` was. The comment described a safety the
# program did not have.
#
# So the REUSE test applies to a ledger row too — it is a fact about the filesystem
# and the process table, not a question of how much a record is trusted.
world ledgerreuse
echo 'SCRATCH_MIN_OWNER_PID="100"' >>"$W_CFG"
mkdir -p "$W_TMP/richos-scratch/pending2"
echo payload >"$W_TMP/richos-scratch/pending2/payload.txt"
sleep 2
start_claude; PID_D17="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D17" "$SID_LIVE"
mv "$W_TMP/richos-scratch/pending2" "$W_TMP/richos-scratch/ledger-reused"
mkdir -p "$W_HOME/state"
printf '{"path":"%s","label":"%s","pid":%d,"ppid":1,"session":"","created":"%s","ttl_minutes":360,"event":"new"}\n' \
    "$W_TMP/richos-scratch/ledger-reused" "stale-row" "$PID_D17" \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" >>"$W_HOME/state/scratch-ledger.jsonl"
OUT="$(run --dry-run --verbose)"
if printf '%s' "$OUT" | grep -q 'this is pid reuse'; then
    ok "S23f a LEDGER row whose pid was REUSED is not a live owner either"
else
    bad "S23f a reused ledger pid is still immortal — the hole the old comment"
    bad "     claimed TTL covered, and TTL never runs after a live KEEP"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
fi

# ===========================================================================
# S24 — DECLARED CAMPAIGN ROOTS ARE REPORTED, NEVER DELETED (D3)
# ===========================================================================
# Frank's D3: `richos-rechecks` (18.90 GB) and `richos-password-free-workspaces`
# (13.75 GB) sit under ~/ab in no ledger row and under no declared root, so
# neither reaper will ever see them.
#
# HIS PROPOSED SIGNAL — "not in the ledger" — DOES NOT SURVIVE THE CENSUS, and that
# is why this arm is an enumeration rather than a sweep. Over the whole of ~/ab on
# 2026-09-18, 24 of 29 directories are unnamed by any ledger, and they include
# `fitapp`, `prospects`, `li-profile-da""ta-grabber`, `deeply`, `saferecord` and
# `autocoder` — the operator's own projects. Under ~/ab the default is "this is
# somebody's work", the exact opposite of the default under $TMPDIR.
#
# So: nominated by declaration only, and NEVER deleted. Every one of these trees
# holds a checkout, which is wall 2 everywhere else in this program, so §54's second
# branch applies — Rich is told and removes it by hand.
world campaign
mkdir -p "$W_ROOT/ab/zcampaign-old" "$W_ROOT/ab/zcampaign-fresh" \
         "$W_ROOT/ab/zzz-a-real-project"
echo evidence >"$W_ROOT/ab/zcampaign-old/notes.txt"
echo evidence >"$W_ROOT/ab/zcampaign-fresh/notes.txt"
echo work >"$W_ROOT/ab/zzz-a-real-project/source.txt"
mkdir -p "$W_ROOT/ab/zcampaign-old/.git" "$W_ROOT/ab/zzz-a-real-project/.git"
backdate "$W_ROOT/ab/zcampaign-old"
backdate "$W_ROOT/ab/zzz-a-real-project"
{
    echo "SCRATCH_CAMPAIGN_PARENT=\"$W_ROOT/ab\""
    echo 'SCRATCH_CAMPAIGN_ROOTS="zcampaign-*"'
    echo 'SCRATCH_CAMPAIGN_RETENTION_DAYS="1"'
} >>"$W_CFG"
OUT="$(run --apply)"
if [ -f "$W_ROOT/ab/zcampaign-old/notes.txt" ]; then
    ok "S24  a campaign root past its retention is NOT deleted"
else
    bad "S24  a campaign root holding a checkout was DELETED"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi
case "$OUT" in
    *"PAST ITS RETENTION"*)
        ok "S24b ...it is REPORTED, which is §54's other branch" ;;
    *) bad "S24b the pile was neither deleted nor reported — the D3 state exactly"
       printf '%s\n' "$OUT" | sed 's/^/        /' | head -8 ;;
esac
case "$OUT" in
    *"rm -rf $W_ROOT/ab/zcampaign-old"*)
        ok "S24c and the reason carries the command a person runs" ;;
    *) bad "S24c reported without saying what to do about it"
       printf '%s\n' "$OUT" | sed 's/^/        /' | head -8 ;;
esac
# THE TWO CONTROLS, and the second is the one that matters most: an UNDECLARED
# directory beside it — the shape of `fitapp` and `prospects` — must not be
# nominated at all, however old it is.
OUT="$(run --dry-run --verbose)"
case "$OUT" in
    *zcampaign-fresh*"inside"*|*"zcampaign-fresh"*"retention is"*)
        ok "S24d CONTROL: one inside its retention is kept and says so" ;;
    *) bad "S24d a young campaign root was not reported at all"
       printf '%s\n' "$OUT" | sed 's/^/        /' | head -10 ;;
esac
if printf '%s' "$OUT" | grep -q 'zzz-a-real-project'; then
    bad "S24e AN UNDECLARED DIRECTORY UNDER THE PARENT WAS NOMINATED. That is the"
    bad "     shape of fitapp and prospects, and this arm must never reach them."
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -10
else
    ok "S24e CONTROL: an UNDECLARED sibling is not nominated, however old"
fi
# AND IT REACHES THE GARBAGE ALARM, because reporting that nobody reads is the
# same as not reporting.
if printf '%s' "$OUT" | grep -qE 'skipped=[1-9]'; then
    ok "S24f a reported campaign root is counted into skipped, so the alarm sees it"
else
    bad "S24f it is reported in the plan and invisible to the alarm"
    printf '%s\n' "$OUT" | grep verdict | sed 's/^/        /'
fi

# ===========================================================================
# S25 — THE VOLUME STAGE, AND THE AGE THIS PROGRAM HAS TO APPLY ITSELF
# ===========================================================================
# Frank's D14: 21 dangling volumes, 402.2 MB, and no arm of the sweep had an
# opinion about any of them.
#
# THE BRIEF PRESCRIBED `docker volume prune --filter until=...` AND THAT FILTER
# DOES NOT EXIST. Checked against this machine and the vendor's reference before
# anything was built: volume prune takes `label` and nothing else, while container,
# image and builder prune all take `until`. Copying the prescription would have
# shipped a command that errors on every run — or, worse, one whose unknown filter
# is ignored, which prunes with NO AGE LIMIT AT ALL. Every other stage of this
# sweep is safe BECAUSE of its age filter, so the age is applied here instead, from
# `docker volume inspect --format '{{.CreatedAt}}'`.
#
# The stub RECORDS ITS ARGUMENTS, for the reason S16c gives: a stub that only
# echoed a total could not tell a correct implementation from one that deletes
# every volume on the machine.
world dockervol
STUBV="$W_ROOT/stubbin"
mkdir -p "$STUBV"
OLD_TS="$(date -u -v-40d +%Y-%m-%dT%H:%M:%SZ 2>/dev/null \
    || date -u -d '40 days ago' +%Y-%m-%dT%H:%M:%SZ)"
NEW_TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat >"$STUBV/docker" <<STUB
#!/bin/sh
echo "\$@" >>"$W_ROOT/docker-calls.txt"
case "\$1 \$2" in
  "info --format")  echo "27.0.0"; exit 0 ;;
  "volume ls")      echo vol-old-anon; echo vol-new-anon; echo vol-old-named; exit 0 ;;
  "volume inspect")
      case "\$3" in
        vol-old-anon)  echo "$OLD_TS|"; exit 0 ;;
        vol-new-anon)  echo "$NEW_TS|"; exit 0 ;;
        vol-old-named) echo "$OLD_TS|<no value>"; exit 0 ;;
      esac
      exit 1 ;;
  "volume rm")      exit 0 ;;
esac
case "\$1" in
  info)      echo "27.0.0"; exit 0 ;;
  container) echo "Total reclaimed space: 0B"; exit 0 ;;
  image)     echo "Total reclaimed space: 0B"; exit 0 ;;
  builder)   echo "Total reclaimed space: 0B"; exit 0 ;;
  ps)        exit 0 ;;
esac
exit 1
STUB
chmod +x "$STUBV/docker"
sed -i.bak 's/^SCRATCH_DOCKER_PRUNE=.*/SCRATCH_DOCKER_PRUNE="1"/' "$W_CFG"
echo 'SCRATCH_DOCKER_UNTIL="720h"' >>"$W_CFG"
OUT="$(PATH="$STUBV:$PATH" run --apply)"
VLOG="$W_HOME/state/scratch-reaper.log"
VCALLS="$W_ROOT/docker-calls.txt"
if grep -q 'volume rm vol-old-anon' "$VCALLS" 2>/dev/null; then
    ok "S25  an ANONYMOUS volume past the declared age is removed"
else
    bad "S25  the volume stage did not run, so D14 is still open"
    sed 's/^/        /' "$VCALLS" 2>/dev/null | head -10
fi
if grep -q 'volume rm vol-new-anon' "$VCALLS" 2>/dev/null; then
    bad "S25b A VOLUME CREATED TODAY WAS REMOVED. The age is the only thing making"
    bad "     this safe, and Docker has no until= filter for volumes to do it."
    sed 's/^/        /' "$VCALLS" 2>/dev/null | head -10
else
    ok "S25b CONTROL: one created today is NOT removed — the age is applied"
fi
if grep -q 'volume rm vol-old-named' "$VCALLS" 2>/dev/null; then
    bad "S25c A NAMED VOLUME WAS REMOVED. A name is somebody having meant it, and"
    bad "     docker itself needs -a before it will touch one."
else
    ok "S25c CONTROL: a NAMED volume is left alone whatever its age"
fi
if grep -q 'class=docker-volume-prune-item' "$VLOG" 2>/dev/null; then
    ok "S25d the removal is on the record with its name and its age"
else
    bad "S25d a volume was removed and nothing says which"
    sed 's/^/        /' "$VLOG" 2>/dev/null | tail -6
fi

# ===========================================================================
# S26 — A RUNNING CONTAINER ON A THROWAWAY IMAGE IS REPORTED, NEVER STOPPED
# ===========================================================================
# `container prune` only ever considers STOPPED containers — correct as a deletion
# rule, and it leaves `rl55` and `rlx`, two `sleep infinity` dev shells eight days
# old, pinning 514 MB and 349 MB of image for ever with nothing alerting.
#
# NOTHING IS EVER STOPPED. Four of the six containers on this machine are the Buzz
# production stack. A match is REPORTED, and a person ends it.
world dockerps
STUBP="$W_ROOT/stubbin"
mkdir -p "$STUBP"
OLD_PS="$(date -v-9d '+%Y-%m-%d %H:%M:%S %z %Z' 2>/dev/null \
    || date -d '9 days ago' '+%Y-%m-%d %H:%M:%S %z %Z')"
NEW_PS="$(date '+%Y-%m-%d %H:%M:%S %z %Z')"
cat >"$STUBP/docker" <<STUB
#!/bin/sh
case "\$1" in
  info) echo "27.0.0"; exit 0 ;;
  ps)   printf 'zthrow-old\tzimg-throwaway:latest\t$OLD_PS\tsleep infinity\n'
        printf 'zthrow-new\tzimg-throwaway:latest\t$NEW_PS\tsleep infinity\n'
        printf 'zservice\tpostgres:17-alpine\t$OLD_PS\tdocker-entrypoint\n'
        exit 0 ;;
  volume) exit 0 ;;
  container|image|builder) echo "Total reclaimed space: 0B"; exit 0 ;;
esac
exit 1
STUB
chmod +x "$STUBP/docker"
sed -i.bak 's/^SCRATCH_DOCKER_PRUNE=.*/SCRATCH_DOCKER_PRUNE="1"/' "$W_CFG"
{
    echo 'SCRATCH_DOCKER_THROWAWAY_IMAGES="zimg-throwaway:*"'
    echo 'SCRATCH_DOCKER_CONTAINER_ALERT_DAYS="7"'
} >>"$W_CFG"
OUT="$(PATH="$STUBP:$PATH" run --dry-run)"
case "$OUT" in
    *"docker://container/zthrow-old"*)
        ok "S26  a running container on a declared throwaway image is REPORTED" ;;
    *) bad "S26  an eight-day-old dev shell is still invisible (D14)"
       printf '%s\n' "$OUT" | sed 's/^/        /' | tail -8 ;;
esac
case "$OUT" in
    *"docker rm -f zthrow-old"*)
        ok "S26b and the reason carries the command, because nothing is stopped here" ;;
    *) bad "S26b reported without saying what to do about it"
       printf '%s\n' "$OUT" | sed 's/^/        /' | tail -8 ;;
esac
case "$OUT" in
    *zthrow-new*) bad "S26c a container started today was reported — the age is not applied" ;;
    *)            ok "S26c CONTROL: one started today is not reported" ;;
esac
case "$OUT" in
    *zservice*)
        bad "S26d A SERVICE WAS NOMINATED. Only DECLARED throwaway images may be"
        bad "     named here; the Buzz production stack is the shape this protects." ;;
    *)  ok "S26d CONTROL: an UNDECLARED image is never nominated, however old" ;;
esac

# ===========================================================================
# S27 — AN UNREADABLE OPEN-FILE TABLE IS ONE LINE, NOT FORTY THOUSAND
# ===========================================================================
# THE MEASUREMENT THAT FORCED THIS, read off the operator's own launchd log on
# 2026-09-18 while this pass was being written:
#
#   $ grep -c 'verdict: undecided' ~/.claude/state/scratch-reaper-launchd.log  -> 1
#   $ grep -c 'verdict: decided'   ~/.claude/state/scratch-reaper-launchd.log  -> 4
#
# and the undecided one's reason is exactly this — "the open-file table could not be
# read". THAT ESTABLISHES THAT IT HAPPENS UNDER LAUNCHD, NOT HOW OFTEN: five verdicts
# is not a frequency and no rate is claimed from it.
#
# Happening at all is enough, because of what it would now cost. It cost ONE entry in
# that run, since the old $TMPDIR arm had almost no candidates; the deny-by-default
# arm has tens of thousands, so the same lsof timeout would produce ~40,000 identical
# INDETERMINATE entries, an enormous undecidable pile in the garbage alarm, and exit 3.
#
# The safety is unchanged either way — nothing under that root is deleted. What
# changes is whether the report is readable, and thousands of copies of one fact is
# the noise that makes a real signal get skipped.
deny_world blindlsof
start_claude; PID_D18="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_D18" "$SID_LIVE"
for i in 1 2 3 4 5 6; do
    stale "$W_TMP/zzz-blind-$i"
done
# An `lsof` that cannot answer. A STUB rather than a real failure, because making
# the real one time out would mean a two-minute case.
BLINDDIR="$W_ROOT/blindbin"
mkdir -p "$BLINDDIR"
printf '#!/bin/sh\nexit 9\n' >"$BLINDDIR/lsof"
chmod +x "$BLINDDIR/lsof"
OUT="$(SRT_BLIND_LSOF="$BLINDDIR/lsof" PATH="$BLINDDIR:$PATH" run --dry-run)"
N_UNDEC="$(printf '%s\n' "$OUT" | grep -c 'OPEN-FILE TABLE COULD NOT BE READ' || true)"
if [ "$N_UNDEC" = "1" ]; then
    ok "S27  an unreadable open-file table is reported ONCE for the whole root"
else
    bad "S27  it was reported $N_UNDEC times — one fact per entry is the noise"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi
if [ -d "$W_TMP/zzz-blind-1" ]; then
    ok "S27b and NOTHING under that root was deleted — the safety is unchanged"
else
    bad "S27b a tree was deleted with no open-file answer at all"
fi
case "$OUT" in
    *"6 undeclared entr"*)
        ok "S27c and the one line carries the COUNT, so the scale is not lost" ;;
    *) bad "S27c reported once, but without saying how many it covers"
       printf '%s\n' "$OUT" | sed 's/^/        /' | head -8 ;;
esac
# AND THE UNDECIDABLE COUNT ITSELF MUST BE 1, which is the assertion that actually
# measures the aggregation. S27 above counts one BANNER, and mutant M46 showed that
# is not enough: with the per-entry path restored the banner is still printed once
# and six MORE undecidable entries appear beside it, so the case stayed green
# against exactly the defect it was written for. The number in the verdict line is
# the thing that cannot be fooled.
if printf '%s' "$OUT" | grep -q 'undecidable=1 '; then
    ok "S27e the verdict line says undecidable=1, not one per entry"
else
    bad "S27e the undecidable count is per-entry: $(printf '%s' "$OUT" | grep -o 'undecidable=[0-9]*' | head -1)"
    printf '%s\n' "$OUT" | grep verdict | sed 's/^/        /'
fi
# THE CONTROL: with lsof working again, the same six are decided individually and
# deleted. Without it, S27 passes against an arm that has stopped working.
OUT="$(run --apply)"
if [ ! -d "$W_TMP/zzz-blind-1" ] && [ ! -d "$W_TMP/zzz-blind-6" ]; then
    ok "S27d CONTROL: with lsof answering, the same six are decided and deleted"
else
    bad "S27d CONTROL FAILED: the arm decides nothing either way"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -8
fi

# ===========================================================================
# S28 — A SOCKET AND A FIFO ARE UNLINKED, NOT rmtree'd
# ===========================================================================
# THE CEO, 2026-09-19: "And what is this all about: MASSIVE ALERT — DISK: 218
# path(s) could not be deleted (delete BY HAND)".
#
# TWO HUNDRED OF THOSE 218 WERE ONE MISSING BRANCH. apply() asked what the path
# was NOT — `islink or isfile -> unlink, ELSE rmtree` — and a unix socket is
# neither a link nor a REGULAR file, so every one of them went to rmtree, which
# cannot take a socket. Measured in the standing-failure ledger that morning:
#
#   198  [Errno 102] Operation not supported on socket  (197 srt-mux-*.sock)
#     2  [Errno 20]  Not a directory                    (clr-debug-pipe-*)
#
# Every one had ALREADY passed every wall, the age floor and the open-handle
# check. The reaper had decided correctly and then could not carry the decision
# out, and re-issued the same two hundred "delete this BY HAND" lines every six
# hours for a day — against paths a person would have deleted with one `rm`.
#
# THE ASSERTION IS `failures=0` AND NOT MERELY "the file is gone", because a
# reaper that never planned them at all would also leave nothing behind.
deny_world sockets
start_claude; PID_S28="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_S28" "$SID_LIVE"
# BOUND FROM INSIDE THE DIRECTORY, ON PURPOSE. macOS caps an AF_UNIX path at
# 104 bytes, and this suite's sandbox lives under
# /private/var/folders/mx/.../T/scratch-reaper-test.XXXXXX/sockets/tmp — which is
# already past it. The first version of this case bound an absolute path, the
# bind failed, the socket was never created, and `[ ! -e ]` then passed against
# the UNFIXED reaper: the negative test passed because the fixture was absent.
# Hence the chdir, and hence the precondition assertion below it.
python3 - "$W_TMP" <<'PY'
import os, socket, sys
os.chdir(sys.argv[1])
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind("zzz-dead.sock")
s.close()                      # bound, then abandoned: the file stays behind
PY
mkfifo "$W_TMP/zzz-dead.fifo"
stale "$W_TMP/zzz-ordinary-dir"
backdate "$W_TMP/zzz-dead.sock"
backdate "$W_TMP/zzz-dead.fifo"
# THE PRECONDITION IS AN ASSERTION, NOT A COMMENT. An absent fixture makes every
# "it is gone" test below pass for the one reason that proves nothing.
if [ -S "$W_TMP/zzz-dead.sock" ] && [ -p "$W_TMP/zzz-dead.fifo" ]; then
    ok "S28-pre the fixture really is a socket and a FIFO before the run"
else
    bad "S28-pre the fixture was never created — every S28 case below is vacuous"
    ls -l "$W_TMP" | sed 's/^/        /' | head -6
fi
OUT="$(run --apply)"
if [ ! -e "$W_TMP/zzz-dead.sock" ]; then
    ok "S28  an abandoned unix SOCKET is unlinked (198 of the 218)"
else
    bad "S28  the socket survived — rmtree cannot take one, and unlink was never tried"
    printf '%s\n' "$OUT" | grep -i sock | sed 's/^/        /' | head -4
fi
if [ ! -e "$W_TMP/zzz-dead.fifo" ]; then
    ok "S28a an abandoned FIFO is unlinked (2 of the 218)"
else
    bad "S28a the FIFO survived — [Errno 20] Not a directory, for ever"
    printf '%s\n' "$OUT" | grep -i fifo | sed 's/^/        /' | head -4
fi
case "$OUT" in
    *"failures=0"*)
        ok "S28b ...and the run reports failures=0, so no MASSIVE ALERT is raised" ;;
    *)  bad "S28b the run still reports a failure for a path it removed"
        printf '%s\n' "$OUT" | grep -E 'applied:|verdict:' | sed 's/^/        /' ;;
esac
# THE CONTROL: an ordinary directory in the SAME run is still deleted. Without
# it, S28 passes just as well against a reaper that has stopped planning
# anything at all under this root.
if [ ! -d "$W_TMP/zzz-ordinary-dir" ]; then
    ok "S28c CONTROL: an ordinary stale directory in the same run still goes"
else
    bad "S28c the arm deleted nothing at all — S28 proves nothing"
fi

# ===========================================================================
# S29 — WHAT THE KERNEL OWNS IS CLASSIFIED ONCE AND NEVER ATTEMPTED AGAIN
# ===========================================================================
# The other 18 of the 218. macOS creates a directory under the per-user temp
# root for each of its own agents — AudioComponentRegistrar, SandboxHelper,
# talagent, studentd, ${DaemonNameOrIdentifierHere} — and marks it `sunlnk`
# (SF_NOUNLINK) with the `com.apple.rootless` attribute. Every unlink returns
# EPERM. Measured 2026-09-19: 18 of 18 carried the flag, each had been retried 52
# times since the previous day, and each produced a line telling Rich to go and
# delete something nobody at this privilege can delete.
#
# WHY THE FIXTURE USES `nodump` AND NOT `sunlnk`: an SF_* flag CANNOT BE SET
# WITHOUT ROOT — `chflags sunlnk` from this account is "Operation not permitted",
# which is the same fact that makes the classification correct in production and
# would otherwise make it untestable. SCRATCH_SYSTEM_FLAGS is the declared mask,
# so this world declares a flag it can actually set and the shipped code path —
# a real chflags, a real st_flags, a real classification — runs byte for byte.
deny_world sysflags
{
    echo 'SCRATCH_SYSTEM_FLAGS="0x00000001"'   # UF_NODUMP, settable unprivileged
} >>"$W_CFG"
start_claude; PID_S29="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_S29" "$SID_LIVE"
stale "$W_TMP/zzz-the-os-owns-this"
stale "$W_TMP/zzz-ours-plainly"
chflags nodump "$W_TMP/zzz-the-os-owns-this"
OUT="$(run --apply)"
if [ -d "$W_TMP/zzz-the-os-owns-this" ]; then
    ok "S29  a kernel-flagged path is KEPT, not attempted"
else
    bad "S29  the flagged path was deleted — the mask is read by nothing"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -6
fi
case "$OUT" in
    *"failures=0"*) ok "S29a ...and it is not reported as a failed clean-up" ;;
    *) bad "S29a it still counts as a deletion that failed, which is the alert"
       printf '%s\n' "$OUT" | grep -E 'applied:' | sed 's/^/        /' ;;
esac
# THE CONTROL, and it is the one that decides whether S29 means anything: the
# IDENTICAL fixture without the flag, in the same run, is deleted.
if [ ! -d "$W_TMP/zzz-ours-plainly" ]; then
    ok "S29b CONTROL: the identical unflagged sibling IS deleted in the same run"
else
    bad "S29b nothing was deleted either way — S29 proves nothing about the flag"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -6
fi
# AND THE EIGHTEEN ALREADY IN THE LEDGER LEAVE IT. A failure row is a standing
# instruction to a person; one that nobody can carry out is withdrawn, not
# re-issued every six hours.
deny_world sysledger
{
    echo 'SCRATCH_SYSTEM_FLAGS="0x00000001"'
} >>"$W_CFG"
start_claude; PID_S29B="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_S29B" "$SID_LIVE"
stale "$W_TMP/zzz-already-blamed"
chflags nodump "$W_TMP/zzz-already-blamed"
python3 - "$W_HOME/state/scratch-failures.json" "$W_TMP/zzz-already-blamed" <<'PY'
import json, sys
path, target = sys.argv[1:3]
json.dump({target: {"first": "2026-09-18T12:15:05Z", "last": "2026-09-19T12:10:33Z",
                    "error": "[Errno 1] Operation not permitted", "attempts": 52}},
          open(path, "w"))
PY
OUT="$(run --apply)"
if [ ! -s "$W_HOME/state/scratch-failures.json" ]; then
    ok "S29c a standing failure the kernel owns LEAVES the ledger"
else
    bad "S29c the unclearable row survived — the alert is permanent by construction"
    cat "$W_HOME/state/scratch-failures.json" | sed 's/^/        /'
fi
if [ -d "$W_TMP/zzz-already-blamed" ]; then
    ok "S29d ...and withdrawing the claim did not delete the thing"
else
    bad "S29d it cleared the row by removing a path it does not own"
fi

# ===========================================================================
# S30 — A FINISHED SESSION'S AGE COMES OFF ITS RECORDED END, NOT ITS MTIME
# ===========================================================================
# MEASURED 2026-09-19. 11 GB of a finished session's scratchpad sat on this disk
# and every six-hourly pass reported it KEEP, with this reason:
#
#   session 5645c662 recorded its end at 2026-09-18T15:38:15Z, and no running
#   claude process is this session, but it was written 0 min ago and the floor
#   is 60 min
#
# — twenty-two hours after the session ended. The age was taken from the newest
# mtime in the tree, and an orphaned background child of the dead session kept
# touching it, so the tree was permanently "0 min old" and the 60-minute floor
# never expired. Rich deleted it by hand; free space went 198 -> 214 GB. That is
# §54's failure exactly: reclaiming it took a person.
#
# THE FLOOR'S OWN DECLARATION SAYS WHY THIS IS RIGHT: it "exists only to cover
# the seconds between a session starting and recording itself". A session that
# has written `ended_at` is not in that race. It is the same defect, in the same
# file, as the one scan_standing_failures exists for — a clock read off an mtime
# that something else keeps moving.
world endedclock
sed -i.bak 's/^SCRATCH_AGE_FLOOR_MINUTES=.*/SCRATCH_AGE_FLOOR_MINUTES="60"/' "$W_CFG"
start_claude; PID_S30="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_S30" "$SID_LIVE"
mkdir -p "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad" \
         "$W_HOME/state/workspaces/sessions"
echo payload >"$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/work.txt"
python3 - "$W_HOME/state/workspaces/sessions/$SID_DEAD.json" "$SID_DEAD" \
          "$(date -u -v-22H +%Y-%m-%dT%H:%M:%SZ 2>/dev/null \
             || date -u -d '22 hours ago' +%Y-%m-%dT%H:%M:%SZ)" <<'PY'
import json, sys
path, sid, ended = sys.argv[1:4]
json.dump({"session_id": sid, "ended_at": ended, "end_reason": "prompt_input_exit",
           "pid": 999999, "pid_start": "Wed Sep 16 23:31:27 2026",
           "cwd": "/", "repo": "/"}, open(path, "w"))
PY
# The orphaned writer, in one line: the tree is touched RIGHT NOW, which is what
# kept the real 11 GB alive for twenty-two hours.
touch "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/work.txt"
OUT="$(run --apply)"
if [ ! -e "$W_SCRATCH/-a-project/$SID_DEAD" ]; then
    ok "S30  a session that ENDED 22 h ago is collected though its tree was touched now"
else
    bad "S30  the floor read the mtime and kept it — the 11 GB case, unchanged"
    printf '%s\n' "$OUT" | grep -A1 "$SID_DEAD" | sed 's/^/        /' | head -4
fi
# THE CONTROL, and it is the safety half: a session that ended MOMENTS ago is
# still inside the floor and is still kept. Without this, S30 passes against a
# reaper that has simply stopped applying the floor to sessions at all.
world endedfloor
sed -i.bak 's/^SCRATCH_AGE_FLOOR_MINUTES=.*/SCRATCH_AGE_FLOOR_MINUTES="60"/' "$W_CFG"
start_claude; PID_S30B="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_S30B" "$SID_LIVE"
mkdir -p "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad" \
         "$W_HOME/state/workspaces/sessions"
echo payload >"$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/work.txt"
backdate "$W_SCRATCH/-a-project/$SID_DEAD"
python3 - "$W_HOME/state/workspaces/sessions/$SID_DEAD.json" "$SID_DEAD" \
          "$(date -u +%Y-%m-%dT%H:%M:%SZ)" <<'PY'
import json, sys
path, sid, ended = sys.argv[1:4]
json.dump({"session_id": sid, "ended_at": ended, "end_reason": "prompt_input_exit",
           "pid": 999999, "pid_start": "Wed Sep 16 23:31:27 2026",
           "cwd": "/", "repo": "/"}, open(path, "w"))
PY
OUT="$(run --apply)"
if [ -e "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/work.txt" ]; then
    ok "S30b CONTROL: a session that ended MOMENTS ago is still inside the floor"
else
    bad "S30b a just-ended session was collected — the floor no longer applies at all"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -6
fi
# --verbose, because a KEEP is only printed in the full report — and the REASON
# is the assertion here, not the survival. S21o is the case that established
# why: a test asserting only that the tree was still there would have passed
# against the wrong answer for ever (it hid a one-hour DST error for weeks).
OUT="$(run --dry-run --verbose)"
case "$OUT" in
    *"recorded its end"*)
        ok "S30c ...and the reason says WHICH clock it used" ;;
    *)  bad "S30c kept, but not by the recorded-end clock — the reason is the test"
        printf '%s\n' "$OUT" | grep -A1 "$SID_DEAD" | sed 's/^/        /' | head -4 ;;
esac

# ===========================================================================
# S31 — A REPOSITORY AN APP INSTANCE HAS REGISTERED IS NOT SCRATCH
# ===========================================================================
# The one thing inside a finished session's scratchpad that is NOT garbage. A QA
# fixture repository is built in there and its absolute path is written into the
# app's entity registry, so an instance that is still running is pointed at a
# directory this program is now entitled to remove. Deleting it is not
# reclaiming garbage; it is pulling a repository out from under a running
# program, and the running program is the only thing that knew it mattered.
world appregistry
sed -i.bak 's/^SCRATCH_AGE_FLOOR_MINUTES=.*/SCRATCH_AGE_FLOOR_MINUTES="0"/' "$W_CFG"
W_FAKE_HOME="$W_ROOT/fakehome"
mkdir -p "$W_FAKE_HOME/Library/Application Support/com.richos.app"
start_claude; PID_S31="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_S31" "$SID_LIVE"
mkdir -p "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/qa-fixture-repo"
echo payload >"$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/qa-fixture-repo/f.txt"
backdate "$W_SCRATCH/-a-project/$SID_DEAD"
python3 - "$W_FAKE_HOME/Library/Application Support/com.richos.app/entities.json" \
          "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/qa-fixture-repo" <<'PY'
import json, sys
path, repo = sys.argv[1:3]
json.dump({"version": 1, "entities": [
    {"id": "qa", "display_name": "QA", "roots": [repo]}]}, open(path, "w"))
PY
OUT="$(run --apply)"
if [ -f "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/qa-fixture-repo/f.txt" ]; then
    ok "S31  a dead session's scratchpad holding a REGISTERED repository survives"
else
    bad "S31  the registered repository went with the scratchpad"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -6
fi
# --verbose for the same reason S30c gives: the REASON is the assertion.
OUT="$(run --dry-run --verbose)"
case "$OUT" in
    *"registered as a repository"*)
        ok "S31a ...and the reason names the registration, not the age" ;;
    *)  bad "S31a kept, but for some other reason — S31 may be passing by luck"
        printf '%s\n' "$OUT" | grep -A1 "$SID_DEAD" | sed 's/^/        /' | head -4 ;;
esac
# THE CONTROL: the same dead session's scratchpad with NOTHING registered is
# collected. Without it, S31 passes against a reaper that keeps every scratchpad.
world appregistryoff
sed -i.bak 's/^SCRATCH_AGE_FLOOR_MINUTES=.*/SCRATCH_AGE_FLOOR_MINUTES="0"/' "$W_CFG"
W_FAKE_HOME="$W_ROOT/fakehome"
mkdir -p "$W_FAKE_HOME/Library/Application Support/com.richos.app"
start_claude; PID_S31B="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_S31B" "$SID_LIVE"
mkdir -p "$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/qa-fixture-repo"
echo payload >"$W_SCRATCH/-a-project/$SID_DEAD/scratchpad/qa-fixture-repo/f.txt"
backdate "$W_SCRATCH/-a-project/$SID_DEAD"
python3 - "$W_FAKE_HOME/Library/Application Support/com.richos.app/entities.json" <<'PY'
import json, sys
json.dump({"version": 1, "entities": []}, open(sys.argv[1], "w"))
PY
OUT="$(run --apply)"
if [ ! -e "$W_SCRATCH/-a-project/$SID_DEAD" ]; then
    ok "S31b CONTROL: with nothing registered the same scratchpad IS collected"
else
    bad "S31b it keeps every scratchpad regardless — S31 proves nothing"
    printf '%s\n' "$OUT" | sed 's/^/        /' | head -6
fi

# ===========================================================================
# S32 — THE SKIPPED PILE IS SPLIT BY WHO CAN ACT ON IT
# ===========================================================================
# CEO, 2026-09-19: "13.9 GB in 4122 place(s) NOTHING WILL EVER COLLECT". Measured
# the same hour, that number was 12.8 GiB of two DECLARED campaign roots past
# their retention — ours, each printing its own `rm -rf`, 12.8 GB a person gets
# back by running it — added to 1.15 GiB of Visual Studio Code's and Codex's
# temporary files, which nobody should ever touch. One heading, two opposite
# responses, and the actionable 12.8 GB invisible inside a number that is mostly
# not actionable.
deny_world skippedsplit
{
    echo "SCRATCH_CAMPAIGN_PARENT=\"$W_ROOT/campaigns\""
    echo 'SCRATCH_CAMPAIGN_ROOTS="zcampaign-done"'
    echo 'SCRATCH_CAMPAIGN_RETENTION_DAYS="1"'
} >>"$W_CFG"
mkdir -p "$W_ROOT/campaigns"
start_claude; PID_S32="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_S32" "$SID_LIVE"
stale "$W_ROOT/campaigns/zcampaign-done"
stale "$W_TMP/zforeign.someone-elses-app"
OUT="$(run --dry-run)"
case "$OUT" in
    *"by_hand=1"*) ok "S32  the verdict line counts OUR by-hand pile separately" ;;
    *) bad "S32  the by-hand pile is still summed into one skipped= number"
       printf '%s\n' "$OUT" | grep verdict | sed 's/^/        /' ;;
esac
case "$OUT" in
    *"foreign=1"*) ok "S32a ...and another program's temp is its own number" ;;
    *) bad "S32a the foreign pile is not separated from ours"
       printf '%s\n' "$OUT" | grep verdict | sed 's/^/        /' ;;
esac
# THE PUBLISHED RECORD CARRIES THE SPLIT AND THE PATHS, because the watchdog
# reads that file and cannot say WHICH 12.8 GB from a total.
run --apply >/dev/null 2>&1
if python3 - "$W_HOME/state/scratch-reaper-state.json" <<'PY'
import json, sys
try:
    d = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(1)
s = d.get("skipped_split") or {}
paths = (s.get("by_hand") or {}).get("paths") or []
sys.exit(0 if (s.get("by_hand", {}).get("count") == 1
               and s.get("foreign", {}).get("count") == 1
               and any("zcampaign-done" in (p.get("path") or "") for p in paths)
               and d.get("last_apply_epoch")) else 1)
PY
then
    ok "S32b --apply publishes the split, the paths and the measurement time"
else
    bad "S32b the watchdog cannot tell the two piles apart from what is published"
    cat "$W_HOME/state/scratch-reaper-state.json" 2>/dev/null | sed 's/^/        /' | head -20
fi

# ===========================================================================
# S34 — A CHECKOUT OF ANOTHER REPOSITORY IS NEVER A FIXTURE (cleanup PRD R1)
# ===========================================================================
# Wall 2 is narrowed for the deny-by-default arm because a harness's throwaway
# repository is scaffolding. A LINKED checkout of a repository elsewhere is not:
# measured 2026-10-23, Codex's temp root held two linked checkouts of the main
# repository, and wall 3 cannot see git's own worktree list. Controls: a
# self-contained repository, and a pointer to a path that does not exist on this
# machine (an extracted copy of another machine's checkout), are still collected.
deny_world linkedcheckout
echo 'SCRATCH_UNKNOWN_GIT_IS_FIXTURE="1"' >>"$W_CFG"
start_claude; PID_LC="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_LC" "$SID_LIVE"
mkdir -p "$W_ROOT/outside-repo/.git/worktrees/linked"
mkdir -p "$W_TMP/zzz-linked/checkout" "$W_TMP/zzz-selfcontained/repo/.git/objects" \
         "$W_TMP/zzz-dangling/checkout"
printf 'gitdir: %s/outside-repo/.git/worktrees/linked\n' "$W_ROOT" >"$W_TMP/zzz-linked/checkout/.git"
printf 'gitdir: %s/no-such-machine/repo/.git/worktrees/x\n' "$W_ROOT" >"$W_TMP/zzz-dangling/checkout/.git"
for _d in zzz-linked zzz-selfcontained zzz-dangling; do
    printf 'payload\n' >"$W_TMP/$_d/payload.txt"
    backdate "$W_TMP/$_d"
done
OUT="$(run --apply)"
if [ -f "$W_TMP/zzz-linked/checkout/.git" ]; then
    ok "S34  a linked checkout of a repository outside the tree is kept"
else
    bad "S34  a linked checkout of an outside repository was deleted as a fixture"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
case "$OUT" in
    *"linked checkout of a repository outside"*)
        ok "S34a and the reason names the outside repository" ;;
    *) bad "S34a kept, but not for the outside-repository reason"
       printf '%s\n' "$OUT" | sed 's/^/        /' ;;
esac
if [ ! -d "$W_TMP/zzz-selfcontained" ]; then
    ok "S34b CONTROL: a self-contained fixture repository is still collected"
else
    bad "S34b CONTROL FAILED: a self-contained fixture repository was kept"
fi
if [ ! -d "$W_TMP/zzz-dangling" ]; then
    ok "S34c CONTROL: a pointer to a path that does not exist here is collected"
else
    bad "S34c CONTROL FAILED: a dangling pointer kept its tree"
fi
kill "$PID_LC" >/dev/null 2>&1 || true
wait "$PID_LC" 2>/dev/null || true

# ===========================================================================
# S35 — THE PRODUCERS' OWN TEMP ROOTS ON THE BULK DRIVE ARE SWEPT
# ===========================================================================
# The nightly gates and Codex run with their own TMPDIR on the bulk drive, and the
# scheduled job's TMPDIR is launchd's: measured 2026-10-23, 13 GB of fixtures
# there had never been looked at. A declared producer root gets the shared-root
# rule for its children and the owner rule for its allocator root. Controls: a
# live allocation, a held child and an undeclared sibling root all survive.
deny_world bulkroots
echo 'SCRATCH_BULK_TMP_ROOTS="tmp/gates"' >>"$W_CFG"
start_claude; PID_BR="$LAST_CLAUDE"
register "$W_SESSIONS" "$PID_BR" "$SID_LIVE"
G="$W_BULK/tmp/gates"
stale "$G/zzz-gate-fixture"
stale "$G/zzz-gate-held"
stale "$W_BULK/tmp/undeclared/zzz-elsewhere"
for _a in "99999-gate-dead" "$PID_BR-gate-live"; do
    mkdir -p "$G/richos-scratch/$_a"
    printf 'payload\n' >"$G/richos-scratch/$_a/payload.txt"
    printf '{"path":"%s","label":"gate","pid":%d,"ppid":1,"session":"","created":"%s","ttl_minutes":360,"event":"new"}\n' \
        "$G/richos-scratch/$_a" "${_a%%-*}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
        >>"$W_HOME/state/scratch-ledger.jsonl"
done
HELD_BR=""
if command -v lsof >/dev/null 2>&1; then
    tail -f "$G/zzz-gate-held/payload.txt" >/dev/null 2>&1 &
    HELD_BR=$!
    KILL_LIST="$KILL_LIST $HELD_BR"
    # The fact, not a clock: lsof shows the holder has the file open.
    _n=0
    until lsof -p "$HELD_BR" 2>/dev/null | grep -q 'zzz-gate-held/payload.txt'; do
        _n=$((_n + 1))
        [ "$_n" -lt 300 ] || break
        sleep 0.1
    done
fi
OUT="$(run --apply)"
if [ ! -d "$G/zzz-gate-fixture" ]; then
    ok "S35  a stale child of a declared producer temp root is deleted"
else
    bad "S35  a declared producer temp root was not swept"
    printf '%s\n' "$OUT" | sed 's/^/        /'
fi
if [ ! -d "$G/richos-scratch/99999-gate-dead" ]; then
    ok "S35a an ended allocation in that root's allocator root is deleted"
else
    bad "S35a an ended allocation under the producer root's allocator root survived"
fi
if [ -f "$G/richos-scratch/$PID_BR-gate-live/payload.txt" ]; then
    ok "S35b CONTROL: a LIVE owner's allocation there is kept"
else
    bad "S35b a live owner's allocation under a producer root was deleted"
fi
if [ -n "$HELD_BR" ]; then
    if [ -f "$G/zzz-gate-held/payload.txt" ]; then
        ok "S35c CONTROL: a child something holds open is kept"
    else
        bad "S35c a held child of a producer root was deleted"
    fi
    kill "$HELD_BR" >/dev/null 2>&1 || true
    wait "$HELD_BR" 2>/dev/null || true
else
    ok "S35c SKIPPED — lsof is not on this host"
fi
if [ -d "$W_BULK/tmp/undeclared/zzz-elsewhere" ]; then
    ok "S35d CONTROL: a root nobody declared is not touched"
else
    bad "S35d a root nobody declared was swept"
fi
kill "$PID_BR" >/dev/null 2>&1 || true
wait "$PID_BR" 2>/dev/null || true

# ===========================================================================
# S33 — THIS SUITE NEVER SCANNED THE WHOLE MAC'S OPEN-FILE TABLE
# ===========================================================================
# The scoped lsof at the top of this file only helps if the reaper actually
# goes through it. A reaper that began calling lsof by absolute path would pass
# every case above AND put the 2026-09-30 kernel flood straight back, with
# nothing anywhere saying so. So: the reaper's whole-machine query must have
# arrived here at least once (it runs on every pass), and it must have been
# narrowed to this suite's tree when it did.
if [ -n "$SCOPED_LSOF_LOG" ]; then
    N_SCOPED="$(grep -c '^scoped' "$SCOPED_LSOF_LOG" 2>/dev/null || true)"
    N_WHOLE="$(grep -c $'^scoped\t-n -P -F n$' "$SCOPED_LSOF_LOG" 2>/dev/null || true)"
    N_DENIED="$(grep -c $'^denied\tsuite\t' "$SRT_SCANNER_AUDIT" || true)"
    N_REQUESTS="$(grep -c $'^allowed\tsuite\tsubprocess.Popen\t' "$SRT_SCANNER_AUDIT" || true)"
    if [ "${N_WHOLE:-0}" -gt 0 ] && [ "${N_REQUESTS:-0}" -ge "${N_SCOPED:-0}" ] && [ "${N_DENIED:-0}" -eq 0 ]; then
        ok "S33  scanner requests were audited before execution, with no escaping scan"
    else
        bad "S33  the reaper's whole-machine lsof never reached the scoped wrapper (scoped=$N_SCOPED whole-form=$N_WHOLE), so this suite scanned the whole Mac"
    fi

    # Partial bypass control: one real wrapped query followed by an absolute
    # query. A harmless fake stands in for the forbidden host scanner, so even
    # a broken audit hook cannot make this control scan the Mac.
    mkdir -p "$SANDBOX/escaping-bin"
    printf '#!/bin/sh\ntouch "%s"\n' "$SANDBOX/escaped-scan" > "$SANDBOX/escaping-bin/lsof"
    chmod +x "$SANDBOX/escaping-bin/lsof"
    if SRT_SCANNER_CONTROL=1 python3 - "$SANDBOX/escaping-bin/lsof" <<'PY' >"$SANDBOX/partial-bypass.log" 2>&1
import subprocess, sys
subprocess.run(["lsof", "-n", "-P", "-F", "n"], capture_output=True)
subprocess.run([sys.argv[1], "-n", "-P", "-F", "n"], check=True)
PY
    then
        bad "S33b the partial scanner bypass was accepted"
    elif [ ! -f "$SANDBOX/escaped-scan" ] && grep -q 'fixture refused an unscoped scanner' "$SANDBOX/partial-bypass.log"; then
        ok "S33b a partial absolute-path bypass is refused before its scanner starts"
    else
        bad "S33b the bypass control failed without proving pre-execution refusal"
    fi
    if SRT_REAL_LSOF="$SANDBOX/escaping-bin/lsof" SRT_SCANNER_CONTROL=1 python3 - <<'PY' >"$SANDBOX/pid-bypass.log" 2>&1
import subprocess
subprocess.run(["lsof", "-p", "1"], check=True)
PY
    then
        bad "S33c a scanner could ask about a process outside the fixture"
    elif [ ! -f "$SANDBOX/escaped-scan" ] && grep -q 'refusing a PID outside the fixture' "$SANDBOX/pid-bypass.log"; then
        ok "S33c an explicit PID outside the fixture is refused before its scanner starts"
    else
        bad "S33c the PID control did not prove refusal before execution"
    fi
else
    ok "S33  SKIPPED — lsof is not on this host"
fi

# --- THE MUTATION HARNESS RUNS FROM THE SUITE IT MUTATES -------------------
# run-all-tests.sh discovers *.test.sh; a *.mutation.sh is invisible to it, and
# eight harnesses in this engine were once run by nothing at all. It matters
# more here than usual: MOST OF THE CASES ABOVE ARE THINGS THAT MUST NOT
# HAPPEN, and every one of them passes against a reaper that deletes nothing.
# The harness is what proves they are not passing for that reason.
#
# ITS FAILURE IS THIS SUITE'S FAILURE, and a MISSING harness is a failure and
# never a skip. RICHOS_MUTATION_INNER is the only thing between this and an
# infinite regress — the harness exports it before running any copy of this
# suite, so never remove one half without the other.
if [ -z "${RICHOS_MUTATION_INNER:-}" ]; then
    echo ""
    echo "=== running the mutation harness: scratch-reaper.mutation.sh ==="
    if [ -x "$SCRIPT_DIR/scratch-reaper.mutation.sh" ]; then
        "$SCRIPT_DIR/scratch-reaper.mutation.sh" || FAIL=$((FAIL + 1))
    else
        echo "  FAIL  MUT. scratch-reaper.mutation.sh is missing or not executable — IT DID NOT RUN"
        FAIL=$((FAIL + 1))
    fi
fi

echo ""
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
exit 0
