#!/usr/bin/env bash
#
# appinstances.test.sh — TESTED BY DEFEAT. A collector is judged on what it
# leaves running and on what it refuses to touch, never on a happy path.
#
# Every case launches a REAL process from a REAL executable named richos-tauri,
# with REAL open files, and asks the shipped module about it. Nothing here
# stubs the process table or lsof: the whole reason this module is built on
# lsof rather than on `ps eww` is a measurement of the real machine, and a
# suite that mocked the machine would have passed against the design that
# cannot work here.
#
#   A1  an instance under a SCRATCH HOME is COLLECT, and quitting it works.
#       The §54 addendum-4 case: the candidate-.7 instance left on his screen.
#   A2  THE NEGATIVE, WITH A POSITIVE CONTROL. An instance whose app state is
#       the operator's (a declared stand-in outside every scratch root, never
#       the real one; the module's default list is checked on its own) is
#       LEAVE — and the control is that the same
#       binary, same name, same argv, under a scratch home, is COLLECT in the
#       same run. Without the control this case passes against a module that
#       collects nothing at all.
#   A3  an instance that IGNORES the quit request and SIGTERM is still ended,
#       and the module says how it ended rather than claiming it quit.
#   A4  an instance that survives EVERYTHING is a SURVIVOR, reported, never
#       silently dropped — the §54 "clean-up failed" branch that must alert.
#   A5  a process merely NAMED richos-tauri with no scratch evidence is LEAVE.
#       Deny-by-default: nothing is collected for failing to be on a list.
#   A6  scratch evidence AND real-home evidence is INDETERMINATE, and is left
#       RUNNING. Three states, never two.
#   A7  the executable's own location does not decide it: the SAME binary under
#       the operator's home, run against a scratch home, is COLLECT (this is
#       how ~/Applications/RichOS.app launched for a test must behave).
#   A8  an unreadable process table is INDETERMINATE, never "nothing running".
#   A9  the quit is addressed BY PID: the osascript the module emits names the
#       unix id and never `application id com.richos.app` / `application
#       "RichOS"`, either of which would reach the CEO's own window.
#   A10 the root set is DERIVED from the reaper's keys — changing
#       SCRATCH_TMP_PATTERNS changes what is collected, with no second edit.
#   A11 a directory under $TMPDIR matching NO declared family is not a root.
#   A12 the collector never reports itself or its own shell.
#
# Exit 0 = every case passed; exit 1 = at least one failed.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$SCRIPT_DIR/../.." && pwd)"
MOD="$SCRIPT_DIR/appinstances.py"

PASS=0; FAIL=0
ok()  { printf '  PASS  %s\n' "$1"; PASS=$((PASS + 1)); }
bad() { printf '  FAIL  %s\n' "$1"; FAIL=$((FAIL + 1)); }

[ -f "$MOD" ] || { echo "FATAL: missing $MOD" >&2; exit 1; }
command -v lsof >/dev/null 2>&1 || {
    echo "FATAL: lsof is required — it is the whole substrate of this module" >&2
    exit 1; }

# The allocator, never a bare mktemp -d: scratch-allocation-lint.sh exists
# precisely so a cleanup harness cannot be the thing that leaves garbage.
. "$ENGINE/scripts/lib/scratch.sh"
SANDBOX="$(scratch_new appinstances-test --ttl 60)"

# PIDS is a FILE, one pid per line: launch() runs inside `$(launch ...)`, a
# subshell, so a shell variable appended to there never reaches this parent.
PIDS_FILE="$SANDBOX/launched.pids"
: > "$PIDS_FILE"
OUTSIDE=""     # anything this suite creates outside $SANDBOX, so the trap can
               # remove it even when a case fails half way through
cleanup() {
    # Belt and braces, and deliberately NOT the mechanism under test: every
    # case verifies its own process is gone. This is here so a case that fails
    # half way cannot leave one of these fake apps running — which would be
    # this suite committing the exact fault it tests for.
    #
    # SIGKILL, NOT SIGTERM: one variant here traps TERM on purpose, so a polite
    # teardown would leave exactly the deaf process it was built to model.
    local p
    while IFS= read -r p; do
        [ -n "$p" ] && kill -9 "$p" 2>/dev/null || true
    done < "$PIDS_FILE"
    for d in $OUTSIDE; do
        rm -rf "$d" 2>/dev/null || true
    done
    scratch_release "$SANDBOX" 2>/dev/null || true
}
trap cleanup EXIT

echo "=== appinstances tests ==="
echo "    sandbox: $SANDBOX"

# ---------------------------------------------------------------------------
# the fake app
# ---------------------------------------------------------------------------
# A real executable, really named richos-tauri, which really opens a file under
# whatever state path it is given and then waits. That open file is the ONLY way
# a scratch HOME is observable on this OS (ps exposes no environment), so a fake
# that did not open it would be testing a signal the module does not use.
# IT IS A COMPILED BINARY, NOT A SHELL SCRIPT, AND THAT IS A CORRECTION THE
# SUITE FORCED ON ITSELF. The first version was a bash script named
# richos-tauri, and three cases failed against a module that was already
# behaving correctly: `bash` keeps the script it is running open on file
# descriptor 255, a NUMERIC descriptor, so the fixture's own path showed up as
# an ordinary open data file and every instance looked as though its data lived
# wherever its executable did. The real app is a Mach-O binary whose image
# appears only under lsof's `txt`, which the module excludes by design.
#
# A fixture that carries a signal the real subject does not have tests the
# fixture. Compiling it removes the difference instead of teaching the module to
# tolerate it.
CC_SRC="$SANDBOX/fakeapp.c"
cat > "$CC_SRC" <<'CSRC'
/* A stand-in for the app: open a state file, hold the descriptor, and wait.
 *
 * THE SELF-IMPOSED DEADLINE IS NOT DECORATION. A `trap ... EXIT` in the harness
 * does not survive kill -9, an OOM kill or a test runner's own timeout — and the
 * first run of this suite proved it, leaving one of these processes running
 * after the runner killed the harness at 300 s. A fixture for the
 * garbage-collection rule must not be able to become the garbage, so it also
 * ends by itself. APP_DEAF=1 ignores SIGTERM, for the case that must escalate.
 */
#include <stdio.h>
#include <stdlib.h>
#include <signal.h>
#include <unistd.h>
#include <string.h>

int main(void) {
    const char *state = getenv("APP_STATE");
    const char *deaf  = getenv("APP_DEAF");
    const char *ttl   = getenv("APP_TTL_SECONDS");
    if (deaf && strcmp(deaf, "1") == 0) {
        signal(SIGTERM, SIG_IGN);
        signal(SIGINT,  SIG_IGN);
    }
    if (state) {
        FILE *f = fopen(state, "a+");
        if (!f) { return 2; }
        fprintf(f, "x\n");
        fflush(f);
        /* deliberately left open: this descriptor IS the evidence */
    }
    long seconds = ttl ? atol(ttl) : 120;
    if (seconds <= 0 || seconds > 600) { seconds = 120; }
    for (long i = 0; i < seconds * 5; i++) { usleep(200000); }
    return 0;
}
CSRC
if ! cc -o "$SANDBOX/richos-tauri.bin" "$CC_SRC" 2>"$SANDBOX/cc.err"; then
    echo "FATAL: could not compile the fixture:" >&2
    cat "$SANDBOX/cc.err" >&2
    exit 1
fi

make_app() {
    local dir="$1"
    mkdir -p "$dir"
    cp "$SANDBOX/richos-tauri.bin" "$dir/richos-tauri"
    chmod +x "$dir/richos-tauri"
}

# launch <exe-dir> <state-file> [deaf] -> echoes the pid
#
# THE REDIRECTION IS LOAD-BEARING. Without `>/dev/null 2>&1` the backgrounded
# app inherits this function's stdout, and `$(launch ...)` — a command
# substitution — blocks until every writer closes that pipe, i.e. until the app
# exits. The first run of this suite hung for 300 s for exactly that reason and
# produced no output at all, which is also why the header never printed.
launch() {
    local exedir="$1" state="$2" deaf="${3:-0}"
    mkdir -p "$(dirname "$state")"
    APP_STATE="$state" APP_DEAF="$deaf" APP_TTL_SECONDS="${APP_TTL_SECONDS:-120}" \
        "$exedir/richos-tauri" >/dev/null 2>&1 &
    local pid=$!
    printf '%s\n' "$pid" >> "$PIDS_FILE"
    # Wait until the descriptor is actually open, so no case races the app's
    # own startup and then reports a signal the module never had a chance to
    # see. A test that races is a test that is flaky and then deleted.
    local i=0
    while [ $i -lt 100 ]; do
        if lsof -p "$pid" 2>/dev/null | grep -q "$(basename "$state")"; then
            break
        fi
        sleep 0.05; i=$((i + 1))
    done
    echo "$pid"
}

# classify_pid <pid> [extra-roots...] -> the Instance as JSON.
# `--root` is not used for the declared cases: the point is that the DECLARED
# roots decide, with no help from the caller.
classify_pid() {
    local pid="$1"; shift
    python3 - "$pid" "$@" <<'PY'
import json, os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
pid = int(sys.argv[1])
roots = A.RootSet(extra=sys.argv[2:])
seen = A._lsof_for([pid])
paths = None if seen is None else seen.get(pid, [])
inst = A.classify(pid, "richos-tauri", paths, roots, A.real_home_prefixes())
print(json.dumps(inst.as_dict()))
PY
}

export APPINST_LIB="$SCRIPT_DIR"

verdict_of() { python3 -c "import json,sys;print(json.load(sys.stdin)['verdict'])"; }

SCRATCH_HOME="$SANDBOX/scratch-home"
EXE_SCRATCH="$SANDBOX/bundle"
make_app "$EXE_SCRATCH"

# ---------------------------------------------------------------------------
# A1 — the addendum-4 case: a test instance under a scratch HOME
# ---------------------------------------------------------------------------
P1="$(launch "$EXE_SCRATCH" "$SCRATCH_HOME/Library/state.db")"
V1="$(classify_pid "$P1" | verdict_of)"
if [ "$V1" = "COLLECT" ]; then
    ok "A1 an instance holding files under a scratch root is COLLECT"
else
    bad "A1 expected COLLECT, got $V1"
fi

QOUT="$(python3 - "$P1" <<'PY'
import os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
gone, how = A.quit_instance(int(sys.argv[1]), grace=2, kill_grace=2)
print("%s|%s" % (gone, how))
PY
)"
if [ "${QOUT%%|*}" = "True" ] && ! kill -0 "$P1" 2>/dev/null; then
    ok "A1 it is quit, and the pid is verified gone (${QOUT#*|})"
else
    bad "A1 quit failed or pid survived: $QOUT"
fi

# ---------------------------------------------------------------------------
# A2 — the negative, with its positive control IN THE SAME RUN
# ---------------------------------------------------------------------------
# THE OPERATOR'S APP STATE IS A FIXTURE, declared to the module through its own key
# (APP_TEST_INSTANCE_REAL_HOME_PATHS), and never the real one. Until 2026-10-11 this case
# opened a probe inside $HOME/Library/Application Support/com.richos.app: on a direct run
# that is the CEO's own app state, and under ci-shard, which gives every unit a private HOME
# inside its log directory (a scratch root), that path is scratch, so A2 and A6 failed
# there (COLLECT) against a module that was right. The fixture sits outside every scratch
# root (checked below, so a machine where it is not refuses instead of passing on a
# coincidence), and the module's own default list is checked separately (A2 default).
REAL_ROOT="/tmp/appinstances-real-state-$$"
OUTSIDE="$OUTSIDE $REAL_ROOT"
REAL_STATE="$REAL_ROOT/com.richos.app"
mkdir -p "$REAL_STATE"
export APP_TEST_INSTANCE_REAL_HOME_PATHS="$REAL_STATE"
REAL_CHECK="$(python3 - "$REAL_STATE" <<'PY'
import os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
state = os.path.realpath(sys.argv[1])
hit = A.RootSet().match(state)
print("scratch:%s" % hit if hit else ("ok" if state in A.real_home_prefixes() else "undeclared"))
PY
)"
if [ "$REAL_CHECK" != "ok" ]; then
    echo "FATAL: the stand-in for the operator's app state is not usable here ($REAL_CHECK): $REAL_STATE" >&2
    exit 1
fi
DEFAULT_CHECK="$(python3 - <<'PY'
import os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
os.environ.pop("APP_TEST_INSTANCE_REAL_HOME_PATHS", None)
import appinstances as A
want = os.path.realpath(os.path.join(os.path.expanduser("~"), "Library", "Application Support", "com.richos.app"))
print("yes" if want in A.real_home_prefixes() else "no")
PY
)"
if [ "$DEFAULT_CHECK" = "yes" ]; then
    ok "A2 default: the app's own state under \$HOME/Library/Application Support is operator evidence with nothing declared"
else
    bad "A2 default: \$HOME/Library/Application Support/com.richos.app is missing from the module's own list"
fi
P2="$(launch "$EXE_SCRATCH" "$REAL_STATE/appinstances-test-probe.tmp")"
V2="$(classify_pid "$P2" | verdict_of)"
if [ "$V2" = "LEAVE" ]; then
    ok "A2 an instance on the operator's own app state is LEAVE"
else
    bad "A2 expected LEAVE, got $V2 — this module could quit the CEO's app"
fi
# THE POSITIVE CONTROL: same binary, same name, scratch state -> COLLECT.
P2C="$(launch "$EXE_SCRATCH" "$SCRATCH_HOME/control/state.db")"
V2C="$(classify_pid "$P2C" | verdict_of)"
if [ "$V2C" = "COLLECT" ]; then
    ok "A2 positive control: the same binary under a scratch home is COLLECT"
else
    bad "A2 CONTROL FAILED ($V2C) — A2's LEAVE proves nothing"
fi
kill -9 "$P2" "$P2C" 2>/dev/null || true
rm -f "$REAL_STATE/appinstances-test-probe.tmp" 2>/dev/null || true

# ---------------------------------------------------------------------------
# A3 — an instance that ignores the quit request and SIGTERM
# ---------------------------------------------------------------------------
P3="$(launch "$EXE_SCRATCH" "$SCRATCH_HOME/deaf/state.db" 1)"
Q3="$(python3 - "$P3" <<'PY'
import os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
gone, how = A.quit_instance(int(sys.argv[1]), grace=1, kill_grace=1)
print("%s|%s" % (gone, how))
PY
)"
if [ "${Q3%%|*}" = "True" ] && ! kill -0 "$P3" 2>/dev/null; then
    case "${Q3#*|}" in
        *SIGTERM*|*killed*) ok "A3 a deaf instance is still ended, and said how: ${Q3#*|}" ;;
        *) bad "A3 ended but described it wrongly as: ${Q3#*|}" ;;
    esac
else
    bad "A3 a deaf instance survived: $Q3"
fi

# ---------------------------------------------------------------------------
# A4 — the survivor branch: clean-up FAILED, and it must say so
# ---------------------------------------------------------------------------
# SIGKILL cannot be ignored, so an unkillable process cannot be manufactured
# here. The branch is driven instead by making `alive()` answer "still there",
# which is what the code sees for a process wedged in an uninterruptible state.
# What is proven is the REPORTING: a failed collection returns False and a
# message naming what was tried, so collect() can put it in `survivors` and the
# §54 alert can fire. A branch nothing exercises is a branch that is wrong.
S4="$(python3 - <<'PY'
import os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
A.alive = lambda pid: True          # the wedged process
A._graceful = lambda pid, t: False
gone, how = A.quit_instance(999999, grace=0.2, kill_grace=0.2)
print("%s|%s" % (gone, how))
PY
)"
if [ "${S4%%|*}" = "False" ]; then
    case "${S4#*|}" in
        *SURVIVED*) ok "A4 an uncollectable instance is a reported failure: ${S4#*|}" ;;
        *) bad "A4 reported failure without naming it: ${S4#*|}" ;;
    esac
else
    bad "A4 a wedged instance was reported as collected: $S4"
fi
S4B="$(python3 - <<'PY'
import json, os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
A.find = lambda extra_roots=(), roots=None: [
    A.Instance(999999, "richos-tauri", A.COLLECT, "test", "/scratch", ["/scratch/x"])]
A.quit_instance = lambda pid, grace=None, kill_grace=None: (False, "SURVIVED")
r = A.collect()
print(json.dumps({"c": len(r["collected"]), "s": len(r["survivors"])}))
PY
)"
if [ "$S4B" = '{"c": 0, "s": 1}' ]; then
    ok "A4 collect() puts it in survivors, where the §54 alert can see it"
else
    bad "A4 collect() misrouted an uncollectable instance: $S4B"
fi

# ---------------------------------------------------------------------------
# A5 — deny by default
# ---------------------------------------------------------------------------
NEUTRAL="$SANDBOX/neutral-exe"
make_app "$NEUTRAL"
# State under /tmp directly: a real path, under no declared family at all.
P5="$(launch "$NEUTRAL" "/tmp/appinstances-neutral-$$.tmp")"
V5="$(classify_pid "$P5" | verdict_of)"
if [ "$V5" = "LEAVE" ]; then
    ok "A5 a process merely NAMED richos-tauri, with no scratch evidence, is LEAVE"
else
    bad "A5 deny-by-default breached: got $V5"
fi
kill -9 "$P5" 2>/dev/null || true
rm -f "/tmp/appinstances-neutral-$$.tmp" 2>/dev/null || true

# ---------------------------------------------------------------------------
# A6 — both kinds of evidence is INDETERMINATE, and is LEFT RUNNING
# ---------------------------------------------------------------------------
V6="$(python3 - <<'PY'
import os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
roots = A.RootSet()
home = A.real_home_prefixes()
# The declared stand-in for the operator's app state (A2), not home[0]: under ci-shard's
# private HOME, home[0] is itself scratch and the case would test nothing.
state = os.path.realpath(os.environ["APP_TEST_INSTANCE_REAL_HOME_PATHS"])
paths = [("3", "/private/tmp/claude-%d/slug/uuid/scratchpad/x" % os.getuid()),
         ("4", state + "/y")]
print(A.classify(1, "richos-tauri", paths, roots, home).verdict)
PY
)"
if [ "$V6" = "INDETERMINATE" ]; then
    ok "A6 scratch AND real-home evidence together is INDETERMINATE"
else
    bad "A6 expected INDETERMINATE, got $V6 — a guess about whose window it is"
fi
U6="$(python3 - <<'PY'
import json, os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
A.find = lambda extra_roots=(), roots=None: [
    A.Instance(1, "richos-tauri", A.INDETERMINATE, "test")]
called = []
A.quit_instance = lambda *a, **k: called.append(1) or (True, "x")
r = A.collect()
print(json.dumps({"q": len(called), "u": len(r["undecided"])}))
PY
)"
if [ "$U6" = '{"q": 0, "u": 1}' ]; then
    ok "A6 an INDETERMINATE instance is never quit, only reported"
else
    bad "A6 an undecided instance was acted on: $U6"
fi

# ---------------------------------------------------------------------------
# A7 — the binary's location does not decide it; the STATE does
# ---------------------------------------------------------------------------
# The realistic case this protects: ~/Applications/RichOS.app launched for a
# test against a scratch HOME must still be collectable, or every QA run that
# uses the installed bundle leaves garbage behind for ever.
# REGISTERED WITH THE TRAP BEFORE IT IS CREATED, and the reason is a real
# incident rather than tidiness. This path is under $HOME/.cache, which is
# OUTSIDE every root the reaper sweeps and outside the collector's root set too,
# so nothing on this machine would ever clean it up. The first run of this suite
# was killed by a harness timeout with the old removal sitting on the success
# path only; the fixture survived under ~/.cache for two hours and made
# `pgrep -fl richos-tauri` answer yes for every other agent's "is the app
# running?" check, costing a teammate a minute proving the process was not his.
#
# The case still needs a binary under $HOME — that is the whole property it
# tests — so the fix is a teardown that runs on EVERY exit, plus the fixture's
# own deadline, not a different location.
EXE_HOME="$HOME/.cache/appinstances-test-$$"
OUTSIDE="$OUTSIDE $EXE_HOME"
make_app "$EXE_HOME"
P7="$(launch "$EXE_HOME" "$SCRATCH_HOME/fromhome/state.db")"
V7="$(classify_pid "$P7" | verdict_of)"
if [ "$V7" = "COLLECT" ]; then
    ok "A7 a binary under \$HOME serving a scratch home is still COLLECT"
else
    bad "A7 expected COLLECT, got $V7 — installed-bundle test runs would leak"
fi
kill -9 "$P7" 2>/dev/null || true
rm -rf "$EXE_HOME" 2>/dev/null || true

# ---------------------------------------------------------------------------
# A8 — an unreadable process table is INDETERMINATE, not "nothing running"
# ---------------------------------------------------------------------------
R8="$(python3 - <<'PY'
import json, os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
A.candidate_pids = lambda: None
r = A.collect()
print(json.dumps({"unreadable": r["unreadable"], "c": len(r["collected"])}))
PY
)"
if [ "$R8" = '{"unreadable": true, "c": 0}' ]; then
    ok "A8 an unreadable process table is reported, never read as 'none running'"
else
    bad "A8 absence was treated as proof: $R8"
fi

# ---------------------------------------------------------------------------
# A9 — the quit is addressed BY PID and never by app name or bundle id
# ---------------------------------------------------------------------------
# This is the case that protects the CEO's own window. Both the bundled app and
# a test build are named richos-tauri and both claim com.richos.app, so a quit
# addressed by either reaches whichever one the window server registered.
# TESTED ON WHAT IT EXECUTES, NOT ON WHAT IT CONTAINS. The first version of this
# case grepped the module and failed it — because the module's own header
# DOCUMENTS the two unsafe forms in order to explain why they are not used. A
# grep cannot tell a warning from an instruction. So the osascript call is
# intercepted and the actual argument vector inspected.
A9="$(python3 - <<'PY'
import os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
seen = []
class R:
    returncode = 0
    stdout = ""
    stderr = ""
A.subprocess.run = lambda args, **kw: seen.append(args) or R()
A.alive = lambda pid: False          # so _graceful returns immediately
A._graceful(4242, 0.1)
if not seen:
    print("NO-CALL")
else:
    argv = " ".join(seen[0])
    bad = []
    if "unix id is 4242" not in argv:
        bad.append("does not address the pid")
    for form in ('application id "com.richos.app"', 'application "RichOS"',
                 'application id "com.richos'):
        if form in argv:
            bad.append("addresses by " + form)
    print("OK" if not bad else "; ".join(bad))
PY
)"
if [ "$A9" = "OK" ]; then
    ok "A9 the osascript it actually runs addresses the unix id, never a name or id"
else
    bad "A9 the quit it actually runs is unsafe: $A9"
fi
# THE POSITIVE CONTROL: the same inspection must REJECT a bundle-id address.
# Without it, A9 passes against a module whose _graceful does nothing at all.
A9C="$(python3 - <<'PY'
import os, sys
sys.path.insert(0, os.environ["APPINST_LIB"])
import appinstances as A
seen = []
class R:
    returncode = 0
    stdout = ""
    stderr = ""
A.subprocess.run = lambda args, **kw: seen.append(args) or R()
A.alive = lambda pid: False
# a _graceful that addresses the app the dangerous way
def unsafe(pid, timeout):
    A.subprocess.run(["osascript", "-e",
                      'tell application id "com.richos.app" to quit'])
    return True
A._graceful = unsafe
A._graceful(4242, 0.1)
argv = " ".join(seen[0]) if seen else ""
caught = ("unix id is 4242" not in argv) or ('application id "com.richos.app"' in argv)
print("REJECTED" if caught else "ACCEPTED")
PY
)"
if [ "$A9C" = "REJECTED" ]; then
    ok "A9 positive control: the same inspection rejects a bundle-id address"
else
    bad "A9 CONTROL FAILED — the inspection accepts a bundle-id address"
fi

# ---------------------------------------------------------------------------
# A10 — the root set is DERIVED from the reaper's own keys
# ---------------------------------------------------------------------------
# Defeat: invent a $TMPDIR family, declare it the way the REAPER declares one,
# and the collector must follow with no edit of its own. This is the property
# that makes "one definition, not two" true rather than asserted.
TMPREAL="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
FAMILY="appinst-invented-$$"
mkdir -p "$TMPREAL/$FAMILY.AAA/state"
P10="$(launch "$EXE_SCRATCH" "$TMPREAL/$FAMILY.AAA/state/db")"
V10_BEFORE="$(classify_pid "$P10" | verdict_of)"
V10_AFTER="$(SCRATCH_LEGACY_TMP_PATTERNS="$FAMILY.*" classify_pid "$P10" | verdict_of)"
if [ "$V10_BEFORE" = "LEAVE" ] && [ "$V10_AFTER" = "COLLECT" ]; then
    ok "A10 declaring a family to the REAPER's key changes what is collected"
else
    bad "A10 not derived from the reaper's keys (before=$V10_BEFORE after=$V10_AFTER)"
fi
kill -9 "$P10" 2>/dev/null || true
rm -rf "$TMPREAL/$FAMILY.AAA" 2>/dev/null || true

# ---------------------------------------------------------------------------
# A11 — an undeclared $TMPDIR directory is not a root
# ---------------------------------------------------------------------------
# The mirror of A10, and the reason the tmp arm is deny-by-default rather than
# "everything under $TMPDIR": a real checkout parked under $TMPDIR is not
# garbage because of where it sits.
A11="$(python3 -c "
import os,sys; sys.path.insert(0,os.environ['APPINST_LIB'])
import appinstances as A
t=os.path.realpath(os.environ.get('TMPDIR') or '/tmp')
print(A.RootSet().match(t+'/some-unknown-name-nobody-declared/x'))")"
if [ "$A11" = "None" ]; then
    ok "A11 a \$TMPDIR directory matching no declared family is not a root"
else
    bad "A11 an undeclared \$TMPDIR name was treated as scratch: $A11"
fi

# ---------------------------------------------------------------------------
# A12 — the collector never reports itself
# ---------------------------------------------------------------------------
A12="$(python3 -c "
import os,sys; sys.path.insert(0,os.environ['APPINST_LIB'])
import appinstances as A
c=A.candidate_pids()
print('self' if c is not None and os.getpid() in c else 'clean')")"
if [ "$A12" = "clean" ]; then
    ok "A12 the collector's own pid is never a candidate"
else
    bad "A12 the collector listed itself"
fi

echo
echo "=== appinstances: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ] || exit 1
