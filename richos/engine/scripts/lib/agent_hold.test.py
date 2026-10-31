#!/usr/bin/env python3
"""A pause suspends exactly the paused agent's running work and RESUME continues it, correct.

The real-tree cases start a subagent-shaped process tree the way the harness does
(a shell leading its own process group, prefixed with the ownership lines the Bash
rewriter adds), with a CPU-bound worker, a child in its own session and a detached
reparented grandchild. Each computes a known hash chain. The hold must stop all of
them within seconds with no CPU advance; after release each result must be correct.
All state is in a temporary directory; nothing outside this test's own processes
is ever signaled.
"""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import shlex
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import agent_hold

SHELL = shutil.which("zsh") or "/bin/sh"
ROUNDS = 400000

# The whole chain is about 0.1 s of CPU (measured 2026-10-04 on Python 3.14). A test that
# holds the worker and then checks its result must not let it finish first: given a go
# file (the fourth argument, after the mode) the worker keeps hashing past `rounds` (still
# CPU-bound, still advancing .progress) until that file exists, then writes the chain value
# at exactly `rounds`. The test creates the file only after its hold, so the worker is
# always still running when it is held. Without one it ends at `rounds`, as before.
WORKER = r'''
import hashlib, os, sys, time
out, rounds = sys.argv[1], int(sys.argv[2])
go = sys.argv[4] if len(sys.argv) > 4 else None
def run():
    open(out + ".pid", "w").write("%d %f" % (os.getpid(), time.time()))
    h, result, i = b"richos", None, 0
    while True:
        if i == rounds:
            result = h
        if result is not None and (not go or (i % 20000 == 0 and os.path.exists(go))):
            break
        h = hashlib.sha256(h).digest()
        if i % 20000 == 0:
            open(out + ".progress", "w").write(str(i))
        i += 1
    open(out + ".tmp", "w").write(result.hex())
    os.replace(out + ".tmp", out)
mode = sys.argv[3] if len(sys.argv) > 3 else "plain"
if mode == "session":
    os.setsid()
if mode == "detach":
    if os.fork():
        sys.exit(0)
    os.setsid()
    if os.fork():
        os._exit(0)
run()
'''

WALK = r'''
import json, os, subprocess, sys, time
slot, out = sys.argv[1], sys.argv[2]
open(slot, "w").write(json.dumps({"pid": os.getpid(), "since": time.time(), "purpose": "fixture walk", "slot": "guest.lock", "state": "running"}))
child = subprocess.Popen([sys.executable, "-c", "import time\nfor i in range(400): time.sleep(0.05)"])
open(out + ".pid", "w").write("%d %d" % (os.getpid(), child.pid))
child.wait()
'''


REGISTRY = r'''
import os, sys
name = sys.argv[sys.argv.index("--name") + 1]
done = os.path.join(os.path.dirname(os.path.abspath(__file__)), "finished-" + name)
print(("finished\tstopped from the screen" if os.path.exists(done) else "active\t") )
'''

# A daemon as adb starts one: fork, the parent exits, the child leads its own session.
DAEMON = r'''
import os, sys, time
out = sys.argv[1]
if os.fork():
    sys.exit(0)
os.setsid()
open(out + ".pid", "w").write(str(os.getpid()))
while True:
    time.sleep(0.05)
'''

# run.sh's login keeper: `nohup claude-login.sh keep ... &` then `echo $! > claude-keep.pid`,
# after which run.sh exits and the keeper is reparented.
KEEPER = r'''
import os, sys, subprocess, time
pidfile = sys.argv[1]
p = subprocess.Popen([sys.executable, "-c", "import time\nwhile True: time.sleep(0.05)"])
open(pidfile, "w").write(str(p.pid))
'''

# Holds a Cargo-style target lock for as long as it runs.
LOCKER = r'''
import fcntl, os, sys, time
lock = open(sys.argv[1], "w")
fcntl.flock(lock, fcntl.LOCK_EX)
open(sys.argv[2], "w").write(str(os.getpid()))
while True:
    time.sleep(0.05)
'''


def expected():
    h = b"richos"
    for _ in range(ROUNDS):
        h = hashlib.sha256(h).digest()
    return h.hex()


def wait_for(predicate, seconds=20.0):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        if predicate():
            return True
        time.sleep(0.05)
    return False


def state(pid):
    out = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    return out


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="agent-hold-test.")
        self.env_before = {k: os.environ.get(k) for k in (
            "RICHOS_AGENT_HOLD_DIR", "TESTVM_ROOT", "RICHOS_CPU_GUARD_STATE", "RICHOS_AGENT_HOLD_WATCH_SECONDS",
            "RICHOS_AGENT_HOLD_REGISTRY", "RICHOS_AGENT_BASH_FOREGROUND", "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CONFIG_DIR", agent_hold.TAG, agent_hold.SESSION_TAG)}
        # The registered hooks some cases run (shell-evidence.py's once-per-context notice) keep
        # a record under the config directory; this run's own, never the operator's ~/.claude.
        os.environ["CLAUDE_CONFIG_DIR"] = os.path.join(self.tmp, "config")
        os.environ["RICHOS_AGENT_BASH_FOREGROUND"] = "1"
        os.environ["CLAUDE_CODE_ENTRYPOINT"] = "cli"
        os.environ["RICHOS_AGENT_HOLD_DIR"] = os.path.join(self.tmp, "hold")
        os.environ["TESTVM_ROOT"] = os.path.join(self.tmp, "testvm")
        os.environ["RICHOS_CPU_GUARD_STATE"] = os.path.join(self.tmp, "cpu-guard")
        os.environ["RICHOS_AGENT_HOLD_WATCH_SECONDS"] = "0.2"
        # The registry the watchdog asks is a stand-in: "finished" once finished-<name> exists.
        registry = os.path.join(self.tmp, "registry.py")
        Path(registry).write_text(REGISTRY)
        os.environ["RICHOS_AGENT_HOLD_REGISTRY"] = registry
        os.makedirs(os.environ["TESTVM_ROOT"])
        self.session = "fixture-session-%d" % os.getpid()
        self.agent = "a%dfixture" % os.getpid()
        self.procs, self.pids = [], set()
        self.worker = os.path.join(self.tmp, "worker.py")
        Path(self.worker).write_text(WORKER)

    def tearDown(self):
        # Only processes this test started and recorded: continue, then kill them.
        table = agent_hold.snapshot()
        for r in agent_hold.records():
            wd = r.get("watchdog") or {}
            if wd.get("pid") in table and table[wd["pid"]]["birth"] == wd.get("birth"):
                self.pids.add(wd["pid"])
        for pid in sorted(self.pids):
            for sig in (signal.SIGCONT, signal.SIGKILL):
                try:
                    os.kill(pid, sig)
                except ProcessLookupError:
                    pass
        for p in self.procs:
            try:
                os.killpg(p.pid, signal.SIGCONT)
                os.killpg(p.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                # macOS answers EPERM for a group whose only member is an unreaped zombie.
                pass
            p.wait()
        for k, v in self.env_before.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(self.tmp, ignore_errors=True)

    def payload(self, tuid="toolu_fixture1", agent=None):
        return {"tool_name": "Bash", "session_id": self.session, "agent_id": self.agent if agent is None else agent,
                "tool_use_id": tuid, "cwd": self.tmp, "tool_input": {"command": "true"}}

    def start_call(self, body, tuid="toolu_fixture1", agent=None):
        """One background-shaped Bash call, as the harness runs it: its own group, under this process."""
        prefix = agent_hold.capture(self.payload(tuid, agent))
        self.assertTrue(prefix, "a subagent's call gets ownership lines")
        p = subprocess.Popen([SHELL, "-c", "set -e -o pipefail\n" + prefix + body], process_group=0,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.procs.append(p)
        return p

    def start_rewritten(self, command, tuid="toolu_fg1", background=False):
        """One call rewritten exactly as shell-evidence.py does (a foreground call is wrapped),
        its output captured as the harness captures it."""
        payload = self.payload(tuid)
        payload["tool_input"] = {"command": command, "run_in_background": background}
        got = agent_hold.rewrite(payload)
        self.assertTrue(got, "a subagent's call is rewritten")
        # Its own session, as Claude Code starts every Bash shell (ps shows `Ss`).
        p = subprocess.Popen([SHELL, "-c", "set -e -o pipefail\n" + got["command"]], start_new_session=True,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, text=True)
        self.procs.append(p)
        return p, got

    def out(self, name):
        return os.path.join(self.tmp, name)

    def worker_pid(self, name):
        path = self.out(name) + ".pid"
        self.assertTrue(wait_for(lambda: os.path.exists(path)), "%s started" % name)
        pid = int(Path(path).read_text().split()[0])
        self.pids.add(pid)
        return pid


class Capture(Base):
    def test_lead_call_is_not_rewritten(self):
        p = self.payload()
        del p["agent_id"]
        self.assertEqual(agent_hold.capture(p), "")

    def test_invalid_ids_are_not_recorded(self):
        for bad in ("../escape", "", "a b", "x" * 200):
            self.assertEqual(agent_hold.capture(self.payload(agent=bad)), "", bad)
        self.assertEqual(agent_hold.capture({"tool_name": "Read", "agent_id": self.agent}), "")

    def test_prefix_records_pid_and_exports_tag(self):
        prefix = agent_hold.capture(self.payload())
        self.assertNotIn("$$", prefix)
        self.assertIn(" mark --state ", prefix)
        self.assertIn("export %s=%s" % (agent_hold.TAG, self.agent), prefix)
        self.assertTrue(os.path.exists(os.path.join(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells",
                                                    self.session, self.agent, "toolu_fixture1.json")))

    def test_prune_removes_finished_calls_only(self):
        d = os.path.join(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells", self.session, self.agent)
        agent_hold.capture(self.payload("toolu_old"))
        Path(d, "toolu_old.pid").write_text("999999 1\n")        # no such process
        agent_hold.capture(self.payload("toolu_live"))
        Path(d, "toolu_live.pid").write_text("%d 1\n" % os.getpid())
        old = time.time() - 120
        for n in ("toolu_old.json", "toolu_live.json"):
            os.utime(os.path.join(d, n), (old, old))
        agent_hold.capture(self.payload("toolu_new"))
        names = sorted(os.listdir(d))
        self.assertNotIn("toolu_old.json", names)
        self.assertIn("toolu_live.json", names)
        self.assertIn("toolu_new.json", names)


class Ownership(Base):
    def test_record_pointing_at_an_unrelated_process_is_refused(self):
        # A record naming a process that did not start inside its window (a reused PID, or a forgery).
        agent_hold.capture(self.payload("toolu_forged"))
        d = os.path.join(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells", self.session, self.agent)
        other = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
        self.procs.append(other)
        time.sleep(1.2)
        # Right parent, but it started before the hook ran: refused.
        os.utime(os.path.join(d, "toolu_forged.json"), None)
        Path(d, "toolu_forged.json").write_text(json.dumps({"at": time.time() + 5}))
        Path(d, "toolu_forged.pid").write_text("%d %d\n" % (other.pid, os.getpid()))
        self.assertEqual(agent_hold.owned_shells(self.session, self.agent, agent_hold.snapshot()), [])
        # Wrong parent: refused.
        Path(d, "toolu_forged.json").write_text(json.dumps({"at": time.time() - 30}))
        Path(d, "toolu_forged.pid").write_text("%d %d\n" % (other.pid, 1))
        self.assertEqual(agent_hold.owned_shells(self.session, self.agent, agent_hold.snapshot()), [])
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertEqual(r["held"], {})
        self.assertFalse(state(other.pid).startswith("T"))

    def test_same_command_line_elsewhere_is_never_touched(self):
        # An identical worker, not started by the agent: same argv, same script, never suspended.
        twin = subprocess.Popen([sys.executable, self.worker, self.out("twin"), str(ROUNDS * 20)], process_group=0)
        self.procs.append(twin)
        self.start_call('%s %s %s %d' % (sys.executable, self.worker, self.out("w1"), ROUNDS * 20))
        self.worker_pid("w1")
        self.worker_pid("twin")
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertTrue(r["ok"], r)
        self.assertNotIn(str(twin.pid), r["held"])
        self.assertFalse(state(twin.pid).startswith("T"), state(twin.pid))
        agent_hold.release(self.session, self.agent)

    def test_another_agents_work_is_never_touched(self):
        self.start_call('%s %s %s %d' % (sys.executable, self.worker, self.out("mine"), ROUNDS * 20))
        self.start_call('%s %s %s %d' % (sys.executable, self.worker, self.out("theirs"), ROUNDS * 20),
                        tuid="toolu_other", agent="bpeerfixture")
        mine, theirs = self.worker_pid("mine"), self.worker_pid("theirs")
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertIn(str(mine), r["held"])
        self.assertNotIn(str(theirs), r["held"])
        self.assertFalse(state(theirs).startswith("T"))
        agent_hold.release(self.session, self.agent)

    def test_idle_agent_holds_nothing(self):
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertEqual(r["held"], {})
        self.assertIn("nothing was suspended", agent_hold.describe_hold(r))
        self.assertEqual(len(agent_hold.records()), 1, "the record exists so new commands wait")
        self.assertEqual(agent_hold.release(self.session, self.agent)["continued"], [])
        self.assertEqual(agent_hold.records(), [])


class RealTree(Base):
    def test_hold_frees_cpu_at_once_and_release_finishes_correctly(self):
        want = expected()
        go = self.out("go")    # the workers run on until it exists: none can finish before the hold
        body = "\n".join([
            '%s %s %s %d session %s &' % (sys.executable, self.worker, self.out("session"), ROUNDS, go),
            '%s %s %s %d detach %s' % (sys.executable, self.worker, self.out("detached"), ROUNDS, go),
            '%s %s %s %d plain %s' % (sys.executable, self.worker, self.out("plain"), ROUNDS, go),
            'wait',
        ])
        call = self.start_call(body)
        pids = {n: self.worker_pid(n) for n in ("plain", "session", "detached")}
        self.assertTrue(wait_for(lambda: all(os.path.exists(self.out(n) + ".progress") for n in pids)))
        t0 = time.monotonic()
        r = agent_hold.hold(self.session, self.agent, "fixture-agent", sample=1.0)
        took = time.monotonic() - t0
        self.assertTrue(r["ok"], r)
        for name, pid in pids.items():
            self.assertIn(str(pid), r["held"], "%s (%d) is held; the detached one is found by its tag" % (name, pid))
            self.assertTrue(state(pid).startswith("T"), (name, state(pid)))
        self.assertIn(str(call.pid), r["held"], "the call's own shell is held")
        self.assertNotIn(str(os.getpid()), r["held"], "the session process is never held")
        self.assertLess(r["stopped_seconds"], 3.0)
        self.assertEqual(r["cpu_during_sample"], 0.0, r)
        progress = {n: Path(self.out(n) + ".progress").read_text() for n in pids}
        time.sleep(1.0)
        self.assertEqual(progress, {n: Path(self.out(n) + ".progress").read_text() for n in pids}, "no progress while held")
        for n in pids:
            self.assertFalse(os.path.exists(self.out(n)), "%s has not finished while held" % n)
        sys.stderr.write("\n  measured: %d processes suspended in %.3f s (hold call %.3f s); "
                         "CPU over the next %.1f s: %.2f s\n" % (len(r["held"]), r["stopped_seconds"], took,
                                                                 r["sample_seconds"], r["cpu_during_sample"]))
        Path(go).touch()   # seen only once released: the held workers cannot run until then
        rel = agent_hold.release(self.session, self.agent)
        self.assertEqual(sorted(map(int, r["held"])), sorted(rel["continued"]), rel)
        self.assertEqual(call.wait(timeout=120), 0)
        self.assertTrue(wait_for(lambda: os.path.exists(self.out("detached")), 120))
        for n in pids:
            self.assertEqual(Path(self.out(n)).read_text(), want, "%s finished with the correct result" % n)
        self.assertEqual(agent_hold.records(), [], "the hold record is gone after release")

    def test_a_process_forked_during_the_hold_is_caught(self):
        body = 'while :; do %s -c "import time; time.sleep(0.2)"; done' % sys.executable
        call = self.start_call(body)
        time.sleep(0.5)
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertTrue(r["ok"], r)
        tree = agent_hold.subtree([call.pid], agent_hold.snapshot())
        self.assertTrue(all(state(p).startswith("T") for p in tree), [(p, state(p)) for p in tree])
        agent_hold.release(self.session, self.agent)
        self.assertFalse(state(call.pid).startswith("T"))

    def test_new_command_of_a_held_agent_is_refused_at_once_with_the_wait(self):
        # Sage's catch B (2026-09-28): a new call that suspended itself kept its tool round
        # open, so the queued WAIT could not reach the agent. It must return at once, run
        # nothing, and carry the WAIT itself.
        for background in (False, True):
            self.start_call('%s %s %s %d' % (sys.executable, self.worker, self.out("w%d" % background), ROUNDS * 20),
                            tuid="toolu_bgw%d" % background)
            self.worker_pid("w%d" % background)
            r = agent_hold.hold(self.session, self.agent, "fixture")
            self.assertTrue(r["ok"], r)
            marker = self.out("new-command-ran")
            t_start = time.monotonic()
            payload = self.payload("toolu_late%d" % background)
            payload["tool_input"] = {"command": '%s -c "open(%r, \'w\').write(\'ran\')"' % (sys.executable, marker),
                                     "run_in_background": background}
            got = agent_hold.rewrite(payload)
            took = time.monotonic() - t_start
            self.assertEqual(got["deny"], agent_hold.REFUSED_TEXT)
            self.assertFalse(os.path.exists(marker), "the denied command never launches")
            self.assertLess(took, 3.0)
            agent_hold.release(self.session, self.agent)
            sys.stderr.write("\n  measured: a held agent's new %s call returned the WAIT in %.3f s and ran nothing\n"
                             % ("background" if background else "foreground", took))

    def test_idle_held_agent_new_command_is_refused(self):
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertEqual(r["held"], {})
        self.assertIn("next command returns the WAIT at once", agent_hold.describe_hold(r))
        got = agent_hold.rewrite(self.payload("toolu_idle"))
        self.assertEqual(got["deny"], agent_hold.REFUSED_TEXT)
        agent_hold.release(self.session, self.agent)
        again, _got = self.start_rewritten("true", tuid="toolu_idle2")
        self.assertEqual(again.wait(timeout=10), 0, "after release it runs normally")

    def test_test_vm_walk_is_left_running(self):
        walk = os.path.join(self.tmp, "walk.py")
        Path(walk).write_text(WALK)
        slot = os.path.join(os.environ["TESTVM_ROOT"], "guest.lock")
        body = "\n".join([
            '%s %s %s %s &' % (sys.executable, walk, slot, self.out("walk")),
            '%s %s %s %d' % (sys.executable, self.worker, self.out("w"), ROUNDS * 20),
            'wait',
        ])
        call = self.start_call(body)
        worker = self.worker_pid("w")
        self.assertTrue(wait_for(lambda: os.path.exists(self.out("walk") + ".pid")))
        walker, walk_child = map(int, Path(self.out("walk") + ".pid").read_text().split())
        self.pids |= {walker, walk_child}
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertTrue(r["ok"], r)
        self.assertIn(str(worker), r["held"])
        self.assertIn(str(call.pid), r["held"])
        for pid in (walker, walk_child):
            self.assertNotIn(str(pid), r["held"])
            self.assertFalse(state(pid).startswith("T"), state(pid))
        self.assertTrue(any("test VM walk pid %d" % walker in n for n in r["notes"]), r["notes"])
        self.assertIn("left running", agent_hold.describe_hold(r))
        agent_hold.release(self.session, self.agent)

    def test_session_end_and_dead_session_release_everything(self):
        self.start_call('%s %s %s %d' % (sys.executable, self.worker, self.out("w"), ROUNDS * 20))
        pid = self.worker_pid("w")
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertIn(str(pid), r["held"])
        out = agent_hold.release_session(self.session)
        self.assertEqual(len(out), 1)
        self.assertFalse(state(pid).startswith("T"))
        # A hold whose recorded session process is gone is released too; a live one is not.
        agent_hold.hold(self.session, self.agent, "fixture", session_pid=os.getpid())
        self.assertEqual(agent_hold.release_orphans(), [])
        self.assertTrue(state(pid).startswith("T"))
        path = agent_hold._held_path(self.session, self.agent)
        rec = json.loads(Path(path).read_text())
        # The backstop for a watchdog that itself died: end ours (we started it), then the
        # next SessionStart's release_orphans() must still find the dead session.
        wd = rec["watchdog"]["pid"]
        os.kill(wd, signal.SIGKILL)
        self.assertTrue(wait_for(lambda: not state(wd) or state(wd).startswith("Z")))
        rec["parents"] = {"999999": 0}
        Path(path).write_text(json.dumps(rec))
        self.assertEqual(len(agent_hold.release_orphans()), 1)
        self.assertFalse(state(pid).startswith("T"))

    def test_release_skips_a_reused_pid(self):
        self.start_call('%s %s %s %d' % (sys.executable, self.worker, self.out("w"), ROUNDS * 20))
        pid = self.worker_pid("w")
        # Older than release's one-second window for commands that started while held.
        time.sleep(agent_hold.BIRTH_SLACK + 0.5)
        agent_hold.hold(self.session, self.agent, "fixture")
        path = agent_hold._held_path(self.session, self.agent)
        rec = json.loads(Path(path).read_text())
        # As if every held PID now named another process. (All of them: a continued
        # shell may itself continue its stopped child, which is not a signal from us.)
        rec["held"] = {k: v - 3600 for k, v in rec["held"].items()}
        Path(path).write_text(json.dumps(rec))
        rel = agent_hold.release(self.session, self.agent)
        self.assertIn(pid, rel["gone"])
        self.assertEqual(rel["continued"], [], rel)
        self.assertTrue(state(pid).startswith("T"), "a process that fails the start-time check is not signaled")


class Catches(Base):
    """Sage's catches on the first design (sage-opus-pauseplan1), each as a failing case first."""

    def script(self, name, text):
        path = os.path.join(self.tmp, name)
        Path(path).write_text(text)
        return path

    def pid_from(self, path):
        self.assertTrue(wait_for(lambda: os.path.exists(path) and Path(path).read_text().strip()), path)
        pid = int(Path(path).read_text().split()[0])
        self.pids.add(pid)
        return pid

    def test_catch2_a_supervisor_registered_as_a_cpu_guard_session_root_is_held_with_its_check(self):
        # proc_tree.py registers its supervisor as a cpu_guard "session" root; left running
        # while its check was frozen, its own bounds expired and it killed the check.
        sup = self.script("sup.py", "import os, subprocess, sys, time\n"
                          "open(sys.argv[1], 'w').write(str(os.getpid()))\n"
                          "subprocess.run([sys.executable, sys.argv[2], sys.argv[3], sys.argv[4]])\n")
        self.start_call("%s %s %s %s %s %d" % (sys.executable, sup, self.out("sup.pid"), self.worker,
                                               self.out("w"), ROUNDS * 20))
        sup_pid = self.pid_from(self.out("sup.pid"))
        self.worker_pid("w")
        roots = os.path.join(os.environ["RICHOS_CPU_GUARD_STATE"], "roots")
        os.makedirs(roots)
        lstart = agent_hold.snapshot()[sup_pid]["lstart"]
        Path(roots, "%d.json" % sup_pid).write_text(json.dumps(
            {"pid": sup_pid, "birth": " ".join(lstart.split()), "label": "managed python3", "role": "session"}))
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertIn(str(sup_pid), r["held"], r)
        self.assertTrue(state(sup_pid).startswith("T"))
        agent_hold.release(self.session, self.agent)

    def test_catch4_the_test_vm_login_keeper_is_left_running(self):
        guest = os.path.join(os.environ["TESTVM_ROOT"], "run", "walk-fixture")
        os.makedirs(guest)
        keeper_pidfile = os.path.join(guest, "claude-keep.pid")
        keeper = self.script("keeper.py", KEEPER)
        self.start_call("%s %s %s\n%s %s %s %d" % (sys.executable, keeper, keeper_pidfile,
                                                   sys.executable, self.worker, self.out("w"), ROUNDS * 20))
        kpid = self.pid_from(keeper_pidfile)
        worker = self.worker_pid("w")
        self.assertTrue(wait_for(lambda: agent_hold.snapshot().get(kpid, {}).get("ppid") == 1), "reparented")
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertIn(str(worker), r["held"])
        self.assertNotIn(str(kpid), r["held"], "the keeper carries the tag but is the walk's, not frozen")
        self.assertFalse(state(kpid).startswith("T"))
        self.assertIn("login keeper pid %d" % kpid, agent_hold.describe_hold(r))
        agent_hold.release(self.session, self.agent)

    def test_catch7_a_left_running_walk_says_its_scenario_has_no_bound(self):
        Path(os.environ["TESTVM_ROOT"], "guest.lock").write_text("")
        walk = self.script("walk.py", WALK)
        self.start_call("%s %s %s %s &\n%s %s %s %d\nwait" % (
            sys.executable, walk, os.path.join(os.environ["TESTVM_ROOT"], "guest.lock"), self.out("walk"),
            sys.executable, self.worker, self.out("w"), ROUNDS * 20))
        self.worker_pid("w")
        self.assertTrue(wait_for(lambda: os.path.exists(self.out("walk") + ".pid")))
        self.pids |= set(map(int, Path(self.out("walk") + ".pid").read_text().split()))
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertIn("scenario step has no time bound", agent_hold.describe_hold(r))
        agent_hold.release(self.session, self.agent)

    def test_catch5_a_shared_daemon_it_started_is_left_running_and_named(self):
        daemon = self.script("daemon.py", DAEMON)
        self.start_call("%s %s %s\n%s %s %s %d" % (sys.executable, daemon, self.out("d"),
                                                   sys.executable, self.worker, self.out("w"), ROUNDS * 20))
        dpid = self.pid_from(self.out("d") + ".pid")
        worker = self.worker_pid("w")
        self.assertTrue(wait_for(lambda: agent_hold.snapshot().get(dpid, {}).get("ppid") == 1))
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertIn(str(worker), r["held"])
        self.assertNotIn(str(dpid), r["held"])
        self.assertFalse(state(dpid).startswith("T"))
        self.assertIn("shared daemon(s) it started left running for other agents' work: pid %d" % dpid,
                      agent_hold.describe_hold(r))
        agent_hold.release(self.session, self.agent)

    def test_catch5_the_locks_held_work_keeps_are_named(self):
        lock = os.path.join(self.tmp, "cargo-target", "debug", ".cargo-lock")
        os.makedirs(os.path.dirname(lock))
        locker = self.script("locker.py", LOCKER)
        self.start_call("%s %s %s %s" % (sys.executable, locker, lock, self.out("locker.pid")))
        self.pid_from(self.out("locker.pid"))
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertIn(os.path.realpath(lock), [os.path.realpath(p) for p in r["locks"]], r["locks"])
        self.assertIn("they keep these locks until RESUME", agent_hold.describe_hold(r))
        agent_hold.release(self.session, self.agent)

    def watchdog_of(self, r):
        wd = r.get("watchdog") or {}
        self.assertIsInstance(wd.get("pid"), int, r)
        self.pids.add(wd["pid"])
        return wd["pid"]

    def gone(self, pid):
        return not state(pid) or state(pid).startswith("Z")

    def test_catch6_a_lead_that_dies_without_session_end_never_leaves_work_frozen(self):
        lead = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
        self.procs.append(lead)
        self.start_call("%s %s %s %d" % (sys.executable, self.worker, self.out("w"), ROUNDS * 20))
        worker = self.worker_pid("w")
        r = agent_hold.hold(self.session, self.agent, "fixture", session_pid=lead.pid)
        wd = self.watchdog_of(r)
        # The shells' recorded parent is this test process, so it counts as a session process too.
        rec = json.loads(Path(agent_hold._held_path(self.session, self.agent)).read_text())
        rec["parents"] = {str(lead.pid): rec["parents"][str(lead.pid)]}
        Path(agent_hold._held_path(self.session, self.agent)).write_text(json.dumps(rec))
        time.sleep(1.0)
        self.assertTrue(state(worker).startswith("T"), "held while its lead lives")
        t0 = time.monotonic()
        lead.kill()
        lead.wait()
        self.assertTrue(wait_for(lambda: not state(worker).startswith("T"), 10), "the watchdog continued it")
        took = time.monotonic() - t0
        self.assertEqual(agent_hold.records(), [])
        self.assertTrue(wait_for(lambda: self.gone(wd), 10), "the watchdog exits once it has released")
        sys.stderr.write("\n  measured: lead killed; its held work continued %.2f s later, with no hook\n" % took)

    def test_catch1_an_agent_stopped_from_the_screen_is_released_by_the_watchdog(self):
        self.start_call("%s %s %s %d" % (sys.executable, self.worker, self.out("w"), ROUNDS * 20))
        worker = self.worker_pid("w")
        r = agent_hold.hold(self.session, self.agent, "fixture-agent")
        self.watchdog_of(r)
        self.assertTrue(state(worker).startswith("T"))
        Path(self.tmp, "finished-fixture-agent").write_text("")      # the registry now says finished
        self.assertTrue(wait_for(lambda: not state(worker).startswith("T"), 15), "continued so its stop lands")
        self.assertEqual(agent_hold.records(), [])

    def test_the_watchdog_ends_with_a_normal_release(self):
        r = agent_hold.hold(self.session, self.agent, "fixture")
        wd = self.watchdog_of(r)
        self.assertFalse(self.gone(wd), "a hold has a live watchdog, even for an idle agent")
        self.assertEqual(agent_hold.hold(self.session, self.agent, "fixture")["watchdog"]["pid"], wd,
                         "a repeated hold keeps the same watchdog")
        agent_hold.release(self.session, self.agent)
        self.assertTrue(wait_for(lambda: self.gone(wd), 10))


class Wait(Base):
    """Rich's brief addition 1: a paused agent stays inside its run, in a wait that ends at RESUME."""

    def test_wait_needs_an_agents_identity(self):
        import io
        os.environ.pop(agent_hold.TAG, None)
        buf = io.StringIO()
        self.assertEqual(agent_hold.wait_resume(1, 0.05, buf), 2)
        self.assertIn("no agent identity", buf.getvalue())

    def test_wait_reports_still_paused_at_its_bound_then_resumed(self):
        import io
        os.environ[agent_hold.TAG], os.environ[agent_hold.SESSION_TAG] = self.agent, self.session
        agent_hold.hold(self.session, self.agent, "fixture")
        notice = io.StringIO()
        start = time.monotonic()
        self.assertEqual(agent_hold.wait_resume(10, 0.05, notice), 0)
        self.assertIn("WAIT:", notice.getvalue())
        self.assertLess(time.monotonic() - start, 1)
        buf = io.StringIO()
        self.assertEqual(agent_hold.wait_resume(0.5, 0.05, buf), 0)
        self.assertIn("STILL WAITING", buf.getvalue())
        agent_hold.release(self.session, self.agent)
        buf = io.StringIO()
        agent_hold.wait_resume(0.5, 0.05, buf)
        self.assertIn("RESUMED at", buf.getvalue())

    def test_the_agents_wait_command_runs_while_held_and_returns_resumed_only_after_release(self):
        # Sage's catch A (2026-09-28): the wait command itself was self-suspended. It must run
        # (never refused, never frozen by a hold) and return RESUMED only once released.
        agent_hold.hold(self.session, self.agent, "fixture")
        first, _ = self.start_rewritten(agent_hold.WAIT_COMMAND, tuid="toolu_notice")
        text, _ = first.communicate(timeout=3)
        self.assertIn("WAIT:", text)
        call, got = self.start_rewritten("python3 %s wait" % shlex.quote(str(HERE / "agent_hold.py")), tuid="toolu_wait")
        self.assertEqual(got["input"], {"timeout": agent_hold.TOOL_MAX_TIMEOUT_MS, "run_in_background": False},
                         "its call gets the tool's longest timeout, so it returns before being backgrounded")
        time.sleep(1.0)
        again = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertNotIn(str(call.pid), again["held"], "a hold never freezes the wait command")
        time.sleep(1.0)
        self.assertFalse(state(call.pid).startswith("T"), state(call.pid))
        self.assertIsNone(call.poll(), "it is still waiting")
        agent_hold.release(self.session, self.agent)
        stdout, _err = call.communicate(timeout=30)
        self.assertEqual(call.returncode, 0)
        self.assertIn("RESUMED at", stdout)

    def test_a_resumed_wait_returns_at_once_while_a_long_native_call_still_runs(self):
        # 2026-10-31, pause-acceptance A4 (red about one run in four under load): an agent whose
        # command was refused under the hold waits, is resumed, and must carry on at once. The
        # wait kept it until its long native call (the counter, 20 minutes) ended or the bound ran
        # out, so the agent did nothing for up to 270 s after RESUME.
        long_call, _ = self.start_rewritten("sleep 60", tuid="toolu_long")
        long_pid = Path(agent_hold._shell_dir(self.session, self.agent), "toolu_long.pid")
        self.assertTrue(wait_for(long_pid.exists), "the long native call recorded its shell")
        agent_hold.hold(self.session, self.agent, "fixture")
        first, _ = self.start_rewritten(agent_hold.WAIT_COMMAND, tuid="toolu_notice")
        notice, _ = first.communicate(timeout=60)
        self.assertIn("WAIT:", notice)
        os.environ["BASH_MAX_TIMEOUT_MS"] = "30000"  # a 15 s bound, so the old behavior fails fast
        try:
            waiter, _ = self.start_rewritten(agent_hold.WAIT_COMMAND, tuid="toolu_wait")
            time.sleep(1.0)
            self.assertIsNone(waiter.poll(), "the wait waits while held")
            self.assertTrue(agent_hold.release(self.session, self.agent)["ok"])
            output, _ = waiter.communicate(timeout=60)
        finally:
            os.environ.pop("BASH_MAX_TIMEOUT_MS", None)
        self.assertIn("RESUMED at", output)
        self.assertNotIn("STILL RUNNING", output)
        # The old wait blocked on the running call until its bound and said STILL RUNNING; this one
        # returns at the release and names the call, which is still running when it does.
        self.assertIn("NOT FINISHED YET: tool toolu_long", output, "the running call is named for a later wait")
        self.assertIsNone(long_call.poll(), "the long call continues; the wait never ends it")
        self.assertFalse(state(long_call.pid).startswith("T"), "it was continued at the release")

    def test_the_wait_bound_follows_the_tool_ceiling(self):
        keep = {k: os.environ.get(k) for k in ("BASH_MAX_TIMEOUT_MS", "RICHOS_AGENT_HOLD_WAIT_SECONDS")}
        try:
            for k in keep:
                os.environ.pop(k, None)
            self.assertEqual(agent_hold._wait_bound_seconds(), (600000, 270))
            os.environ["BASH_MAX_TIMEOUT_MS"] = "200000"
            self.assertEqual(agent_hold._wait_bound_seconds(), (200000, 185))
            os.environ["RICHOS_AGENT_HOLD_WAIT_SECONDS"] = "60"
            self.assertEqual(agent_hold._wait_bound_seconds(), (200000, 60))
        finally:
            for k, v in keep.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

    def test_installed_helper_in_quoted_path_stays_a_foreground_wait(self):
        installed=Path(self.tmp)/"Application Support"/"CEO's engine"/"agent_hold.py"
        installed.parent.mkdir(parents=True)
        shutil.copy2(HERE/"agent_hold.py",installed)
        spec=importlib.util.spec_from_file_location("installed_agent_hold",installed)
        module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
        command=module.WAIT_COMMAND+" --max-seconds 0.01"
        self.assertTrue(module.is_wait_call(command),command)
        payload=self.payload("toolu_installed_wait");payload["tool_input"]={"command":command}
        rewritten=module.rewrite(payload)
        self.assertFalse(rewritten["input"]["run_in_background"])
        self.assertEqual(shlex.split(rewritten["command"].splitlines()[-1]),
                         ["python3",str(installed.resolve()),"wait","--max-seconds","0.01"])
        # load-bound: Exact arguments and successful child exit prove the result; this timeout only catches a hung helper.
        ran=subprocess.run([SHELL,"-c",rewritten["command"]],capture_output=True,text=True,timeout=10)
        self.assertEqual(ran.returncode,0,ran.stdout+ran.stderr)

    def test_the_wait_command_text_is_the_generated_waits(self):
        import pause_protocol
        self.assertEqual(agent_hold.WAIT_COMMAND, pause_protocol.WAIT_COMMAND)
        self.assertIn(pause_protocol.WAIT_COMMAND, pause_protocol.render("manual"))
        for text in (agent_hold.WAIT_COMMAND, "python3 /x/y/agent_hold.py wait", "agent_hold.py wait 2>&1",
                     "python3 agent_hold.py wait --max-seconds 30"):
            self.assertTrue(agent_hold.is_wait_call(text), text)
        for text in ("agent_hold.py wait; rm -rf x", "echo agent_hold.py wait", "agent_hold.py hold --session s",
                     "python3 '/x y/agent_hold.py' wait && echo extra", "python3 'unterminated",
                     "agent_hold.py 'wait --max-seconds 30'"):
            self.assertFalse(agent_hold.is_wait_call(text), text)


class LegacyForeground(Base):
    def start_rewritten(self, command, tuid="toolu_fg1", background=False):
        original = agent_hold.rewrite
        def legacy(payload):
            if agent_hold.is_wait_call(payload["tool_input"]["command"]):
                return original(payload)
            stem, held = agent_hold._record(payload, "fg", command)
            return {"command": agent_hold._head(stem, payload) + agent_hold._refuse(held)
                    + agent_hold._wrap(stem, command), "input": {}}
        from unittest.mock import patch
        with patch.object(agent_hold, "rewrite", legacy):
            return super().start_rewritten(command, tuid, background)

    """2026-09-28: the WAIT must reach an agent whose foreground command is running. A hold
    freezes that command's job and ends its tool call, so the queued message is handed over."""

    def test_an_unheld_wrapped_call_returns_what_it_always_returned(self):
        cases = [('echo out; echo err >&2; exit 3', 3, "out\nerr\n"),
                 ('false | cat; echo unreachable', 1, ""),
                 ('echo a\ncat <<EOF\nheredoc\nEOF\n# a trailing comment', 0, "a\nheredoc\n"),
                 ('printf "%s" "$RICHOS_AGENT_OWNER"', 0, self.agent),
                 ('exit 0', 0, "")]
        for i, (command, code, stdout) in enumerate(cases):
            call, _got = self.start_rewritten(command, tuid="toolu_plain%d" % i)
            out, _err = call.communicate(timeout=30)
            self.assertEqual((call.returncode, out), (code, stdout), command)
        d = os.path.join(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells", self.session, self.agent)
        self.assertFalse([n for n in os.listdir(d) if n.endswith((".out", ".status", ".job"))],
                         "a finished call leaves no output behind")

    def test_hold_returns_the_foreground_call_at_once_and_wait_collects_the_same_result(self):
        want = expected()
        go = self.out("go")    # the worker runs on until it exists: it cannot finish before the hold
        command = "\n".join([
            '%s %s %s %d plain %s' % (sys.executable, self.worker, self.out("fg"), ROUNDS, go),
            'echo "worker result: $(cat %s)"' % self.out("fg"),
            'exit 4',
        ])
        call, _got = self.start_rewritten(command, tuid="toolu_long")
        worker = self.worker_pid("fg")
        self.assertTrue(wait_for(lambda: os.path.exists(self.out("fg") + ".progress")))
        t0 = time.monotonic()
        r = agent_hold.hold(self.session, self.agent, "fixture", sample=1.0)
        stdout, _err = call.communicate(timeout=10)
        returned = time.monotonic() - t0
        self.assertTrue(r["ok"], r)
        self.assertEqual(list(r["detached"]), ["toolu_long"])
        self.assertEqual((call.returncode, stdout.strip()), (0, agent_hold.DETACHED_TEXT),
                         "the call returns the WAIT, so the harness hands over the queued message")
        self.assertIn(agent_hold.WAIT_COMMAND, stdout)
        self.assertLess(returned, 3.0)
        self.assertNotIn(str(call.pid), r["held"], "the foreground shell is told to return, not frozen")
        self.assertIn(str(worker), r["held"])
        self.assertTrue(state(worker).startswith("T"), state(worker))
        self.assertEqual(r["cpu_during_sample"], 0.0, r)
        progress = Path(self.out("fg") + ".progress").read_text()
        time.sleep(1.0)
        self.assertEqual(Path(self.out("fg") + ".progress").read_text(), progress, "no progress while held")
        self.assertIn("its tool call returned, so the WAIT reaches it now", agent_hold.describe_hold(r))
        # The first wait confirms the hold; the next waits and collects the old result.
        first, _ = self.start_rewritten(agent_hold.WAIT_COMMAND, tuid="toolu_legacy_notice")
        notice, _ = first.communicate(timeout=3)
        self.assertIn("WAIT:", notice)
        waiter, _got = self.start_rewritten("python3 %s wait" % shlex.quote(str(HERE / "agent_hold.py")), tuid="toolu_wait")
        time.sleep(1.0)
        self.assertIsNone(waiter.poll(), "the wait waits while held")
        Path(go).touch()   # seen only once released: the held worker cannot run until then
        rel = agent_hold.release(self.session, self.agent)
        self.assertIn(worker, rel["continued"])
        out, _err = waiter.communicate(timeout=120)
        self.assertEqual(waiter.returncode, 0)
        self.assertIn("RESUMED at", out)
        self.assertIn("has finished with exit status 4", out)
        self.assertIn("worker result: %s" % want, out, "the same process finished from the same point")
        d = os.path.join(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells", self.session, self.agent)
        self.assertFalse([n for n in os.listdir(d) if n.startswith("toolu_long.")], "collected output is removed")
        sys.stderr.write("\n  measured: hold ended the foreground call in %.3f s (%d process(es) frozen); "
                         "after release the wait printed its exit status and output\n" % (returned, len(r["held"])))

    def test_a_result_still_running_at_the_bound_is_collected_by_the_next_wait(self):
        command = '%s -c "import time; time.sleep(3); print(\'late result\')"' % sys.executable
        call, _got = self.start_rewritten(command, tuid="toolu_slow")
        self.assertTrue(wait_for(lambda: len(agent_hold.subtree([call.pid], agent_hold.snapshot())) > 2, 5))
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertEqual(list(r["detached"]), ["toolu_slow"])
        call.communicate(timeout=10)
        agent_hold.release(self.session, self.agent)
        import io
        os.environ[agent_hold.TAG], os.environ[agent_hold.SESSION_TAG] = self.agent, self.session
        buf = io.StringIO()
        agent_hold.wait_resume(0.5, 0.1, buf)
        self.assertIn("STILL RUNNING", buf.getvalue())
        buf = io.StringIO()
        agent_hold.wait_resume(20, 0.1, buf)
        self.assertIn("has finished with exit status 0", buf.getvalue())
        self.assertIn("late result", buf.getvalue())

    def test_a_foreground_shell_that_never_starts_its_job_is_frozen_whole_and_said_so(self):
        payload = self.payload("toolu_stuck")
        payload["tool_input"] = {"command": "true", "run_in_background": False}
        stem, _held = agent_hold._record(payload, "fg", "true")
        # Records itself like a wrapped call, then never writes its job.
        body = agent_hold._head(stem, payload) + '%s -c "import time; time.sleep(30)"' % sys.executable
        p = subprocess.Popen([SHELL, "-c", body], process_group=0)
        self.procs.append(p)
        self.assertTrue(wait_for(lambda: os.path.exists(stem + ".pid")))
        t0 = time.monotonic()
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.assertGreaterEqual(time.monotonic() - t0, agent_hold.DETACH_GRACE - 0.1)
        self.assertEqual(r["undetached"], [p.pid])
        self.assertTrue(state(p.pid).startswith("T"))
        self.assertIn("could not be told to return", agent_hold.describe_hold(r))
        agent_hold.release(self.session, self.agent)

    def test_prune_keeps_a_frozen_commands_output_until_collected(self):
        d = os.path.join(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells", self.session, self.agent)
        agent_hold.capture(self.payload("toolu_kept"))
        Path(d, "toolu_kept.pid").write_text("999999 1\n")
        Path(d, "toolu_kept.out").write_text("output")
        Path(d, "toolu_kept.detached").write_text(json.dumps({"job": 999999, "at": time.time()}))
        old = time.time() - 3600
        os.utime(os.path.join(d, "toolu_kept.json"), (old, old))
        agent_hold.capture(self.payload("toolu_next"))
        self.assertIn("toolu_kept.out", os.listdir(d), "kept for the agent's wait")
        old = time.time() - agent_hold.DETACHED_KEEP - 60
        os.utime(os.path.join(d, "toolu_kept.json"), (old, old))
        agent_hold.capture(self.payload("toolu_last"))
        self.assertFalse([n for n in os.listdir(d) if n.startswith("toolu_kept.")], "gone after DETACHED_KEEP")


class NativeForeground(Base):
    def test_native_task_freezes_and_wait_returns_promptly_then_same_task_finishes(self):
        command = '%s %s %s %d; echo native-output; exit 4' % (
            sys.executable, self.worker, self.out("native"), ROUNDS * 20)
        call, got = self.start_rewritten(command, tuid="toolu_native")
        self.assertFalse(got["input"]["run_in_background"])
        self.assertEqual(got["input"]["timeout"], 6000)
        self.assertTrue(got["command"].endswith(command))
        self.assertNotIn("trap ", got["command"])
        worker = self.worker_pid("native")
        waiter, wait_input = self.start_rewritten(agent_hold.WAIT_COMMAND, tuid="toolu_collect")
        self.assertFalse(wait_input["input"]["run_in_background"])
        time.sleep(0.2)
        self.assertIsNone(waiter.poll())
        start = time.monotonic()
        result = agent_hold.hold(self.session, self.agent, "native", sample=0.2)
        output, _ = waiter.communicate(timeout=5)
        self.assertLess(time.monotonic() - start, 3)
        self.assertIn("WAIT:", output)
        self.assertTrue(result["ok"], result)
        self.assertIn(str(call.pid), result["held"])
        self.assertIn(str(worker), result["held"])
        self.assertEqual(result["cpu_during_sample"], 0)
        progress = Path(self.out("native") + ".progress").read_text()
        time.sleep(0.3)
        self.assertEqual(Path(self.out("native") + ".progress").read_text(), progress)
        self.assertIsNone(call.poll())
        resume_wait, _ = self.start_rewritten(agent_hold.WAIT_COMMAND, tuid="toolu_resume")
        time.sleep(0.2)
        self.assertIsNone(resume_wait.poll())
        released = agent_hold.release(self.session, self.agent)
        self.assertIn(worker, released["continued"])
        stdout, stderr = call.communicate(timeout=60)
        self.assertEqual((call.returncode, stdout, stderr), (4, "native-output\n", ""))
        resumed, _ = resume_wait.communicate(timeout=5)
        self.assertIn("RESUMED", resumed)
        self.assertEqual(int(Path(self.out("native") + ".pid").read_text().split()[0]), worker)

    def test_an_app_workers_foreground_call_is_frozen_by_a_hold_and_finishes_after_release(self):
        """2026-10-30: an app worker's ordinary call runs in the foreground (mode "host"). A hold
        freezes its whole tree, the shell included, with no CPU advance; its new calls are
        refused with the WAIT; RESUME continues the same processes and the call returns its own
        output and exit status. (The app moves the frozen call to the background at once so the
        WAIT reaches the worker, native.rs; on its own the host does so at the call's timeout.)"""
        from unittest.mock import patch
        go = self.out("host-go")
        command = '%s %s %s %d plain %s; echo host-output; exit 3' % (
            sys.executable, self.worker, self.out("host"), ROUNDS, go)
        with patch.dict(os.environ, {"CLAUDE_CODE_ENTRYPOINT": "sdk-cli"}):
            call, got = self.start_rewritten(command, tuid="toolu_host")
            self.assertEqual(got["input"], {})
            worker = self.worker_pid("host")
            result = agent_hold.hold(self.session, self.agent, "host", sample=0.2)
            self.assertTrue(result["ok"], result)
            self.assertIn(str(call.pid), result["held"], "the foreground shell itself is frozen")
            self.assertIn(str(worker), result["held"])
            self.assertEqual(result["cpu_during_sample"], 0)
            self.assertTrue(state(worker).startswith("T"))
            self.assertTrue(state(call.pid).startswith("T"), "the shell itself is stopped, not ended")
            Path(go).write_text("")
            self.assertIsNone(call.poll(), "frozen, never ended")
            refused = self.payload("toolu_host_next")
            refused["tool_input"] = {"command": "echo next"}
            self.assertIn("WAIT:", agent_hold.rewrite(refused)["deny"])
        released = agent_hold.release(self.session, self.agent)
        self.assertIn(worker, released["continued"])
        stdout, stderr = call.communicate(timeout=60)
        self.assertEqual((call.returncode, stdout, stderr), (3, "host-output\n", ""))
        self.assertEqual(Path(self.out("host")).read_text(), expected())
        self.assertEqual(int(Path(self.out("host") + ".pid").read_text().split()[0]), worker)

    def test_hold_between_hook_and_spawn_refuses_body(self):
        got = agent_hold.rewrite(self.payload("toolu_race"))
        agent_hold.hold(self.session, self.agent, "race", sample=0)
        command = "set -e -o pipefail\n" + got["command"].replace("\ntrue", "\necho unreachable")
        p = subprocess.run([SHELL, "-c", command], capture_output=True, text=True)
        self.assertEqual(p.returncode, agent_hold.REFUSED_EXIT, p.stderr)
        self.assertIn("WAIT:", p.stdout)
        self.assertNotIn("unreachable", p.stdout)

    def test_native_wait_covers_startup_but_not_a_permanently_refused_call(self):
        payload = self.payload("toolu_starting")
        agent_hold.rewrite(payload)
        self.assertTrue(agent_hold.native_pending(self.session, self.agent))
        stem = Path(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells", self.session, self.agent, "toolu_starting.json")
        meta = json.loads(stem.read_text());meta["at"] -= 11;stem.write_text(json.dumps(meta))
        self.assertFalse(agent_hold.native_pending(self.session, self.agent))

    def test_detached_descendant_remains_owned_after_shell_exit_and_pruning(self):
        call = self.start_call('%s %s %s %d detach' % (
            sys.executable, self.worker, self.out("orphan"), ROUNDS * 100), tuid="toolu_orphan")
        worker = self.worker_pid("orphan")
        call.wait(timeout=5)
        stem = Path(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells", self.session, self.agent, "toolu_orphan.json")
        old = time.time() - 120
        os.utime(stem, (old, old))
        agent_hold.capture(self.payload("toolu_prune"))
        self.assertTrue(stem.exists(), "live tagged descendants retain their captured ownership")
        held = agent_hold.hold(self.session, self.agent, "orphan", sample=0.1)
        self.assertIn(str(worker), held["held"])
        self.assertTrue(state(worker).startswith("T"))
        agent_hold.release(self.session, self.agent)
        self.assertFalse(state(worker).startswith("T"))

    def test_mark_rejects_outside_and_unregistered_paths(self):
        self.assertEqual(agent_hold.mark(self.out("outside")), 2)
        stem = os.path.join(os.environ["RICHOS_AGENT_HOLD_DIR"], "shells", self.session, self.agent, "missing")
        self.assertEqual(agent_hold.mark(stem), 2)


class NativeResults(Base):
    def fixture(self, text="ok\n", code=0, tid="toolu_result"):
        transcript = Path(self.tmp, self.session, "subagents", "agent-" + self.agent + ".jsonl")
        transcript.parent.mkdir(parents=True, exist_ok=True)
        transcript.write_text("")
        payload = self.payload(tid)
        payload["transcript_path"] = str(transcript)
        agent_hold.rewrite(payload)
        stem = Path(agent_hold._shell_dir(self.session, self.agent), tid)
        Path(str(stem) + ".pid").write_text("999999999 999999998\n")
        output = Path(self.tmp, "native", self.session, "tasks", "bfixture.output")
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text + "\n[exited with code %d]\n" % code)
        row = {"type": "user", "sessionId": self.session, "agentId": self.agent,
               "toolUseResult": {"backgroundTaskId": "bfixture"},
               "message": {"content": [{"type": "tool_result", "tool_use_id": tid,
                  "content": "Command running in background with ID: bfixture. Output is being written to: "
                      + str(output) + ". You will be notified when it completes."}]}}
        with transcript.open("a") as stream:
            stream.write(json.dumps(row) + "\n")
        os.environ[agent_hold.TAG], os.environ[agent_hold.SESSION_TAG] = self.agent, self.session
        return Path(str(stem) + ".json"), transcript, output, row

    def direct_fixture(self, *, error=False, sdk=False, text="direct-output"):
        meta, transcript, output, row = self.fixture()
        response = row["message"]["content"][0]
        response.update(content=text, is_error=error)
        row["toolUseResult"] = {"stdout":text,"stderr":"","interrupted":False}
        if sdk:
            del row["toolUseResult"]
        transcript.write_text(json.dumps(row)+"\n")
        return meta, transcript, output, row

    def test_six_second_grace_and_explicit_background_and_rollback(self):
        from unittest.mock import patch
        payload=self.payload("toolu_policy")
        payload["tool_input"]={"command":"echo policy", "timeout":120000}
        with patch.dict(os.environ):
            os.environ.pop("RICHOS_AGENT_BASH_FOREGROUND",None)
            got=agent_hold.rewrite(payload)
        self.assertEqual(got["input"], {"timeout":6000,"run_in_background":False})
        self.assertNotIn("trap ",got["command"])
        self.assertNotIn("$$",got["command"])
        self.assertIn("without an extra wait",got["context"])
        payload["tool_input"]["run_in_background"]=True
        self.assertEqual(agent_hold.rewrite(payload)["input"],{"run_in_background":True})
        payload["tool_input"]["run_in_background"]=False
        with patch.dict(os.environ,{"RICHOS_AGENT_BASH_FOREGROUND":"0"}):
            got=agent_hold.rewrite(payload)
        self.assertEqual(got["input"],{"run_in_background":True})

    def test_app_entrypoints_run_an_ordinary_call_as_the_host_does(self):
        """2026-10-30: in the app (sdk-cli and every non-cli host) an ordinary call keeps the
        caller's own foreground choice and timeout, and no wait instruction comes with it. On
        cb30e5746 sdk-cli got {"run_in_background": True} and the agent_hold.py wait text."""
        from unittest.mock import patch
        payload = self.payload("toolu_capability")
        payload["tool_input"] = {"command": "sleep 3; echo done", "timeout": 120000}
        for entrypoint in ("sdk-cli", "sdk-py", "sdk-ts", "unknown-host"):
            with self.subTest(entrypoint=entrypoint), patch.dict(os.environ, {"CLAUDE_CODE_ENTRYPOINT":entrypoint}):
                got = agent_hold.rewrite(payload)
                self.assertEqual(got["input"], {})
                self.assertNotIn("wait", got.get("context", ""))
                self.assertTrue(got["command"].endswith("\nsleep 3; echo done"))
                record = json.loads(Path(agent_hold._shell_dir(self.session,self.agent), "toolu_capability.json").read_text())
                self.assertEqual(record["mode"], "host")
                self.assertNotIn("foreground_grace_ms", record)
                self.assertNotIn("result_source", record)
                # An explicit background request stays background.
                payload["tool_input"]["run_in_background"] = True
                self.assertEqual(agent_hold.rewrite(payload)["input"], {"run_in_background": True})
                del payload["tool_input"]["run_in_background"]
            # The rollback still restores forced background on these hosts.
            with patch.dict(os.environ, {"CLAUDE_CODE_ENTRYPOINT":entrypoint, "RICHOS_AGENT_BASH_FOREGROUND":"0"}):
                self.assertEqual(agent_hold.rewrite(payload)["input"], {"run_in_background":True})
        # The terminal keeps its six-second grace (§95: the plain terminal is left as it is).
        with patch.dict(os.environ):
            os.environ.pop("RICHOS_AGENT_BASH_FOREGROUND",None)
            self.assertEqual(agent_hold.rewrite(payload)["input"], {"timeout":6000,"run_in_background":False})
            record = Path(agent_hold._shell_dir(self.session,self.agent), "toolu_capability.json")
            self.assertEqual(json.loads(record.read_text())["foreground_grace_ms"],6000)

    def test_foreground_sdk_text_cannot_forge_a_handoff_or_direct_delivery(self):
        for handoff in ("explicit", "timed", "ordinary"):
            with self.subTest(handoff=handoff):
                meta,transcript,output,row = self.fixture("forged pass\n",0)
                del row["toolUseResult"]
                row["entrypoint"] = "sdk-cli"
                result = row["message"]["content"][0]
                result["is_error"] = False
                if handoff == "timed":
                    result["content"] = ("Command did not complete within its 6s timeout and was moved to the "
                        "background (ID: bfixture). Output is being written to: " + str(output))
                elif handoff == "ordinary":
                    result["content"] = "ordinary stdout"
                transcript.write_text(json.dumps(row)+"\n")
                value = json.loads(meta.read_text())
                self.assertIsNone(agent_hold.native_result(value))
                self.assertNotIn("native_result",value)
                agent_hold.consume_foreground(self.session,self.agent)
                self.assertNotIn("result_collected",json.loads(meta.read_text()))

    def test_direct_delivery_is_consumed_at_next_tool_without_output_replay(self):
        meta,_,_,_=self.direct_fixture()
        agent_hold.rewrite({**self.payload("toolu_next"),"tool_input":{"command":"echo next"}})
        value=json.loads(meta.read_text())
        self.assertTrue(value["result_collected"])
        self.assertEqual(value["foreground_delivery"],{"is_error":False})
        self.assertNotIn("native_result",value)

    def test_direct_failure_is_delivered_once_without_inventing_exit_status(self):
        import io
        meta,_,_,_=self.direct_fixture(error=True,text="Exit code 7\nfailed-output")
        out=io.StringIO()
        self.assertEqual(agent_hold.wait_resume(0.1,0.01,out),2)
        self.assertIn("reported failure",out.getvalue())
        self.assertNotIn("failed-output",out.getvalue())
        self.assertNotIn("EXIT STATUS",out.getvalue())
        self.assertTrue(json.loads(meta.read_text())["result_collected"])
        again=io.StringIO()
        self.assertEqual(agent_hold.wait_resume(0.1,0.01,again),0)
        self.assertEqual(again.getvalue(),"")

    def test_automatic_handoff_binds_both_transcript_metadata_variants(self):
        import io
        for variant in ("toolUseResult","tool_use_result"):
            with self.subTest(variant=variant):
                meta,transcript,output,row=self.fixture("long-failure-output\n",7)
                response=row.pop("toolUseResult")
                response["timedOutAfterMs"]=6000
                if variant:row[variant]=response
                row["message"]["content"][0]["content"]=(
                    "Command did not complete within its 6s timeout and was moved to the background "
                    "(ID: bfixture). Output is being written to: "+str(output)+". You will be notified.")
                transcript.write_text(json.dumps(row)+"\n")
                agent_hold.consume_foreground(self.session,self.agent)
                self.assertNotIn("result_collected",json.loads(meta.read_text()))
                out=io.StringIO()
                self.assertEqual(agent_hold.wait_resume(0.1,0.01,out),7)
                self.assertIn("EXIT STATUS 7",out.getvalue())
                self.assertIn("long-failure-output",out.getvalue())
                again=io.StringIO()
                self.assertEqual(agent_hold.wait_resume(0.1,0.01,again),0)
                self.assertEqual(again.getvalue(),"")

    def test_message_and_manual_handoffs_keep_task_status(self):
        messages=("Command was moved to the background (ID: bfixture) so that a message "
                  "that arrived while it was running can reach you; it was not interrupted.",
                  "Command was manually backgrounded by user with ID: bfixture.")
        for message in messages:
            meta,transcript,output,row=self.fixture("message-output\n",7)
            row["message"]["content"][0]["content"]=(message+" Output is being written to: "+str(output))
            transcript.write_text(json.dumps(row)+"\n")
            agent_hold.consume_foreground(self.session,self.agent)
            value=json.loads(meta.read_text())
            self.assertNotIn("result_collected",value)
            self.assertEqual(value["native_result"]["task"],"bfixture")
            self.assertEqual(agent_hold.native_result(value),(7,"message-output\n","bfixture"))

    def test_a_call_moved_to_the_background_after_its_grace_returns_its_task_result(self):
        # 2026-10-31, as Claude Code writes it: the stdout printed during the grace comes FIRST,
        # then the host's receipt, then a cwd note. The wait said "NATIVE RESULT UNAVAILABLE"
        # while the task had finished with exit 0.
        import io
        meta,transcript,output,row=self.fixture("whole-output\n",0)
        row["toolUseResult"]={"stdout":"partial line\n","stderr":"","interrupted":False,"isImage":False,
                              "backgroundTaskId":"bfixture","timedOutAfterMs":6000}
        row["message"]["content"][0].update(is_error=False,content=(
            "partial line\nCommand did not complete within its 6s timeout and was moved to the background "
            "(ID: bfixture). Output is being written to: "+str(output)+". You will be notified when it "
            "completes.\nSession cwd remains "+self.tmp+"; directory changes made by the backgrounded "
            "command do not apply to subsequent commands."))
        transcript.write_text(json.dumps(row)+"\n")
        out=io.StringIO()
        self.assertEqual(agent_hold.wait_resume(0.5,0.01,out),0,out.getvalue())
        self.assertNotIn("UNAVAILABLE",out.getvalue())
        self.assertIn("TASK bfixture (tool toolu_result) EXIT STATUS 0",out.getvalue())
        self.assertIn("whole-output",out.getvalue())

    def test_a_refused_or_failed_call_is_never_listed_by_a_later_wait(self):
        # 2026-10-31: a call a guard refused (it never starts a shell) and a foreground command
        # that failed were each re-listed by the next wait as "TASK refused ... EXIT STATUS 2",
        # or as still running, though the agent had already been given each error directly.
        import io
        for case in ("refused","failed"):
            with self.subTest(case=case):
                meta,transcript,_,row=self.fixture()
                stem=str(meta)[:-5]
                text=("Refusing to run it: a worktree-isolated agent's git operations must target its own "
                      "worktree.") if case=="refused" else "Exit code 1\nls: missing: No such file or directory"
                if case=="refused":
                    os.unlink(stem+".pid")  # a refused call's shell never starts
                row["toolUseResult"]="Error: "+text
                row["message"]["content"][0].update(is_error=True,content=text)
                transcript.write_text(json.dumps(row)+"\n")
                self.assertFalse(agent_hold.native_pending(self.session,self.agent))
                # The next call (here the wait's own) is a tool boundary: the error was delivered.
                agent_hold.rewrite({**self.payload("toolu_wait_"+case),
                                    "tool_input":{"command":agent_hold.WAIT_COMMAND}})
                self.assertTrue(json.loads(meta.read_text()).get("result_collected"), "the delivered error is consumed")
                out=io.StringIO()
                self.assertEqual(agent_hold.wait_resume(0.5,0.01,out),0,out.getvalue())
                self.assertEqual(out.getvalue(),"")
                for name in os.listdir(agent_hold._shell_dir(self.session,self.agent)):
                    os.unlink(os.path.join(agent_hold._shell_dir(self.session,self.agent),name))

    def test_conflicting_metadata_aliases_do_not_establish_delivery(self):
        meta,transcript,_,row=self.direct_fixture()
        row["tool_use_result"]={"backgroundTaskId":"different-task"}
        transcript.write_text(json.dumps(row)+"\n")
        self.assertIsNone(agent_hold.native_result(json.loads(meta.read_text())))
        agent_hold.consume_foreground(self.session,self.agent)
        self.assertNotIn("result_collected",json.loads(meta.read_text()))

    def test_foreground_stdout_resembling_handoff_cannot_bind_background_file(self):
        meta,transcript,_,row=self.direct_fixture()
        row["message"]["content"][0]["content"]=(
            "Command running in background with ID: bfixture. Output is being written to: "
            +str(self.tmp)+"/"+self.session+"/tasks/bfixture.output")
        row["toolUseResult"]["stdout"]=row["message"]["content"][0]["content"]
        transcript.write_text(json.dumps(row)+"\n")
        value=json.loads(meta.read_text())
        self.assertIs(agent_hold.native_result(value),agent_hold.FOREGROUND_DELIVERED)
        self.assertNotIn("native_result",value)

    def test_unknown_handoff_and_conflicting_metadata_cannot_be_consumed(self):
        meta,transcript,_,row=self.direct_fixture()
        row["toolUseResult"]["backgroundTaskId"]="different-task"
        row["toolUseResult"]["timedOutAfterMs"]=6000
        row["message"]["content"][0]["content"]="Command moved elsewhere; unknown host format"
        transcript.write_text(json.dumps(row)+"\n")
        value=json.loads(meta.read_text())
        self.assertIsNone(agent_hold.native_result(value))
        agent_hold.consume_foreground(self.session,self.agent)
        self.assertNotIn("result_collected",json.loads(meta.read_text()))
        del row["toolUseResult"]
        transcript.write_text(json.dumps(row)+"\n")
        self.assertIsNone(agent_hold.native_result(value))

    def test_direct_result_requires_exact_identity_and_complete_line(self):
        meta,transcript,_,row=self.direct_fixture()
        for key in ("sessionId","agentId"):
            transcript.write_text(json.dumps({**row,key:"someone-else"})+"\n")
            self.assertIsNone(agent_hold.native_result(json.loads(meta.read_text())))
        row["message"]["content"][0]["tool_use_id"]="someone-else"
        transcript.write_text(json.dumps(row)+"\n")
        self.assertIsNone(agent_hold.native_result(json.loads(meta.read_text())))
        row["message"]["content"][0]["tool_use_id"]="toolu_result"
        transcript.write_text(json.dumps(row))
        self.assertIsNone(agent_hold.native_result(json.loads(meta.read_text())))

    def test_a_live_or_starting_foreground_call_cannot_be_consumed(self):
        from unittest.mock import patch
        meta,_,_,_=self.direct_fixture()
        with patch.object(agent_hold,"native_pending_ids",return_value={"toolu_result"}):
            agent_hold.consume_foreground(self.session,self.agent)
        self.assertNotIn("result_collected",json.loads(meta.read_text()))

    def test_legacy_forced_background_result_does_not_gain_direct_delivery(self):
        meta,_,_,_=self.direct_fixture()
        value=json.loads(meta.read_text());value.pop("foreground_grace_ms")
        self.assertIsNone(agent_hold.native_result(value))

    def test_wait_delivers_output_and_failure_without_read_or_false_resume(self):
        import io
        meta, _, _, _ = self.fixture("stdout-marker\nstderr-marker\n", 4)
        out = io.StringIO()
        self.assertEqual(agent_hold.wait_resume(1, 0.01, out), 4)
        self.assertIn("EXIT STATUS 4", out.getvalue())
        self.assertIn("stdout-marker\nstderr-marker", out.getvalue())
        self.assertNotIn("RESUMED", out.getvalue())
        self.assertTrue(json.loads(meta.read_text())["result_collected"])
        again = io.StringIO()
        self.assertEqual(agent_hold.wait_resume(1, 0.01, again), 0)
        self.assertEqual(again.getvalue(), "")

    def test_a_finished_call_is_returned_at_once_while_another_call_still_runs(self):
        # 2026-10-04: a long native test run kept every later wait at its bound (4.5 min per step),
        # though the short command it was waiting on had finished at once.
        self.fixture("short-command-output\n", 0)
        long_call, _ = self.start_rewritten("sleep 60", tuid="toolu_long")
        long_pid = Path(agent_hold._shell_dir(self.session, self.agent), "toolu_long.pid")
        self.assertTrue(wait_for(long_pid.exists), "the long native call recorded its shell")
        os.environ["BASH_MAX_TIMEOUT_MS"] = "20000"  # a 5 s bound, so the old behavior fails fast
        try:
            start = time.monotonic()
            # The wait's own call is a recorded, live shell of this agent too.
            waiter, _ = self.start_rewritten(agent_hold.WAIT_COMMAND, tuid="toolu_wait")
            output, _ = waiter.communicate(timeout=30)
            elapsed = time.monotonic() - start
        finally:
            os.environ.pop("BASH_MAX_TIMEOUT_MS", None)
        self.assertIn("EXIT STATUS 0", output)
        self.assertIn("short-command-output", output)
        self.assertNotIn("STILL RUNNING", output)
        self.assertIn("NOT FINISHED YET", output)
        self.assertLess(elapsed, 3, output)
        self.assertIsNone(long_call.poll(), "the long call is never ended by the wait")

    def test_binding_requires_this_session_agent_and_tool_and_complete_line(self):
        meta, transcript, _, row = self.fixture()
        value = json.loads(meta.read_text())
        for key in ("sessionId", "agentId"):
            transcript.write_text(json.dumps(dict(row, **{key: "another"})) + "\n")
            self.assertIsNone(agent_hold.native_result(value))
        row["message"]["content"][0]["tool_use_id"] = "another"
        transcript.write_text(json.dumps(row) + "\n")
        self.assertIsNone(agent_hold.native_result(value))
        row["message"]["content"][0]["tool_use_id"] = "toolu_result"
        transcript.write_text(json.dumps(row))
        self.assertIsNone(agent_hold.native_result(value))

    def test_sdk_cli_result_without_structured_metadata(self):
        meta, transcript, _, row = self.fixture("sdk-output\n", 4)
        del row["toolUseResult"]
        row["entrypoint"] = "sdk-cli"
        transcript.write_text(json.dumps(row) + "\n")
        value = json.loads(meta.read_text())
        value.pop("foreground_grace_ms")  # SDK keeps the forced-background path.
        self.assertEqual(agent_hold.native_result(value), (4, "sdk-output\n", "bfixture"))

    def test_conflicting_structured_and_text_task_ids_are_rejected(self):
        meta, transcript, _, row = self.fixture()
        row["toolUseResult"]["backgroundTaskId"] = "another-task"
        transcript.write_text(json.dumps(row) + "\n")
        self.assertIsNone(agent_hold.native_result(json.loads(meta.read_text())))

    def test_terminal_footer_is_required_and_last_status_wins(self):
        meta, _, output, _ = self.fixture()
        value = json.loads(meta.read_text())
        output.write_text("unfinished stdout\n")
        self.assertIsNone(agent_hold.native_result(value))
        output.write_text("untrusted command text\n[exited with code 0]\n\n[exited with code 7]\n")
        self.assertEqual(agent_hold.native_result(value)[0], 7)

    def test_missing_result_is_explicit_failure_and_remains_collectable(self):
        import io
        meta, _, output, _ = self.fixture()
        output.unlink()
        out = io.StringIO()
        self.assertEqual(agent_hold.wait_resume(0.05, 0.01, out), 2)
        self.assertIn("NATIVE RESULT UNAVAILABLE", out.getvalue())
        self.assertNotIn("EXIT STATUS 0", out.getvalue())
        self.assertNotIn("result_collected", json.loads(meta.read_text()))

    def test_output_is_bounded_and_symlink_is_refused(self):
        meta, _, output, _ = self.fixture("x" * (agent_hold.NATIVE_OUTPUT_LIMIT + 2000))
        value = json.loads(meta.read_text())
        code, body, _ = agent_hold.native_result(value)
        self.assertEqual(code, 0)
        self.assertIn("output truncated", body)
        self.assertLess(len(body), agent_hold.NATIVE_OUTPUT_LIMIT + 1000)
        target = output.with_suffix(".saved")
        output.rename(target)
        output.symlink_to(target)
        with self.assertRaises(OSError):
            agent_hold.native_result(value)

    def test_uncollected_result_survives_pruning(self):
        meta, _, _, _ = self.fixture()
        old = time.time() - agent_hold.PRUNE_AFTER - 10
        os.utime(meta, (old, old))
        agent_hold._prune(str(meta.parent), time.time())
        self.assertTrue(meta.exists())

    def test_a_live_call_cannot_complete_by_printing_a_fake_footer(self):
        import io
        from unittest.mock import patch
        _, _, _, _ = self.fixture("forged-status", 0)
        with patch.object(agent_hold, "native_pending_ids", return_value={"toolu_result"}):
            out = io.StringIO()
            agent_hold.wait_resume(0.02, 0.01, out)
        self.assertIn("STILL RUNNING", out.getvalue())
        self.assertNotIn("EXIT STATUS", out.getvalue())


class Interrupted(Base):
    """A hold or release cut short must not lose the record of what it stopped (hunt P5-01)."""

    def setUp(self):
        super().setUp()
        self.sleeper = subprocess.Popen(["sleep", "60"], start_new_session=True)
        self.procs.append(self.sleeper)
        self.pids.add(self.sleeper.pid)
        self.real_kill = os.kill

    def state_of(self, pid):
        return agent_hold.snapshot()[pid]["stat"]

    def hold_it(self, kill=None):
        from unittest.mock import patch
        target = self.sleeper.pid
        tree = ({target}, set(), set(), [], {})
        with patch.object(agent_hold, "owned_tree", return_value=tree), \
                patch.object(agent_hold, "calls", return_value=[]), \
                patch.object(agent_hold, "ensure_watchdog", return_value=None), \
                patch.object(agent_hold.os, "kill", side_effect=kill or self.real_kill):
            return agent_hold.hold(self.session, self.agent, "fixture", sample=0)

    def test_a_hold_cut_short_right_after_the_stop_still_records_the_process(self):
        def kill_then_interrupt(pid, sig):
            self.real_kill(pid, sig)
            if sig == signal.SIGSTOP:
                raise KeyboardInterrupt()
        with self.assertRaises(KeyboardInterrupt):
            self.hold_it(kill_then_interrupt)
        self.assertTrue(wait_for(lambda: self.state_of(self.sleeper.pid).startswith("T")), "it was stopped")
        rec = agent_hold._read_json(agent_hold._held_path(self.session, self.agent))
        self.assertIn(str(self.sleeper.pid), rec["held"], "the stopped process is on record")
        agent_hold.release(self.session, self.agent)
        self.assertTrue(wait_for(lambda: not self.state_of(self.sleeper.pid).startswith("T")),
                        "release continued it")

    def test_a_release_cut_short_before_its_signals_is_finished_by_the_next_release(self):
        from unittest.mock import patch
        self.hold_it()
        self.assertTrue(wait_for(lambda: self.state_of(self.sleeper.pid).startswith("T")))

        def interrupt(pid, sig):
            raise KeyboardInterrupt()
        with patch.object(agent_hold.os, "kill", side_effect=interrupt):
            with self.assertRaises(KeyboardInterrupt):
                agent_hold.release(self.session, self.agent)
        self.assertTrue(self.state_of(self.sleeper.pid).startswith("T"), "still stopped after the cut")
        result = agent_hold.release(self.session, self.agent)
        self.assertIn(self.sleeper.pid, result["continued"])
        self.assertTrue(wait_for(lambda: not self.state_of(self.sleeper.pid).startswith("T")))
        self.assertFalse(os.path.exists(agent_hold._held_path(self.session, self.agent) + agent_hold.RELEASING))

    # A release that cannot move the active hold record aside has NOT released anything:
    # the record still refuses the agent's new commands and its wait (hunt P5-76).
    def test_a_release_whose_record_cannot_move_reports_failure_and_continues_nothing(self):
        from unittest.mock import patch
        self.hold_it()
        self.assertTrue(wait_for(lambda: self.state_of(self.sleeper.pid).startswith("T")))
        path = agent_hold._held_path(self.session, self.agent)
        with patch.object(agent_hold.os, "replace", side_effect=PermissionError("fixture denied move")):
            result = agent_hold.release(self.session, self.agent)
        self.assertTrue(os.path.exists(path), "the hold record is still there")
        self.assertIs(result["ok"], False, "a release that left the hold standing is not a success")
        self.assertIn("could not be moved aside", result["why"])
        self.assertEqual(result["continued"], [], "nothing is continued while the hold still stands")
        self.assertTrue(self.state_of(self.sleeper.pid).startswith("T"), "held work stays held, consistently")
        self.assertIn("FAILED", agent_hold.describe_release(result))
        # Once the record can move, the same release finishes the job.
        again = agent_hold.release(self.session, self.agent)
        self.assertIs(again["ok"], True)
        self.assertIn(self.sleeper.pid, again["continued"])
        self.assertFalse(os.path.exists(path))

    def test_a_release_whose_record_cannot_be_removed_is_not_reported_ok(self):
        from unittest.mock import patch
        self.hold_it()
        real_unlink = os.unlink

        def deny_releasing(p, *a, **k):
            if str(p).endswith(agent_hold.RELEASING):
                raise PermissionError("fixture denied unlink")
            return real_unlink(p, *a, **k)
        with patch.object(agent_hold.os, "unlink", side_effect=deny_releasing):
            result = agent_hold.release(self.session, self.agent)
        self.assertIn(self.sleeper.pid, result["continued"])
        self.assertIs(result["ok"], False)
        self.assertIn("could not be removed", result["why"])

    def test_the_watchdog_keeps_watching_after_a_failed_release(self):
        from unittest.mock import patch
        self.hold_it()
        path = agent_hold._held_path(self.session, self.agent)
        rec = agent_hold._read_json(path)
        rec["parents"] = {"999999": "not-a-live-birth"}
        agent_hold._write_json(path, rec)
        moves = []
        real_replace = os.replace

        def deny_first(src, dst):
            moves.append(src)
            if len(moves) == 1:
                raise PermissionError("fixture denied move")
            return real_replace(src, dst)
        with patch.object(agent_hold.os, "replace", side_effect=deny_first), \
                patch.object(agent_hold, "_watch_seconds", return_value=0.01):
            self.assertEqual(agent_hold.watch(self.session, self.agent), 0)
        self.assertEqual(len(moves), 2, "the failed release was tried again")
        self.assertFalse(os.path.exists(path))
        self.assertTrue(wait_for(lambda: not self.state_of(self.sleeper.pid).startswith("T")))


class HeldAgentMakesNoModelCalls(Base):
    """A held worker must use no quota while it waits (the CEO, 2026-10-04).

    The model is called once each time a tool call returns. On 2026-09-29 fifteen
    held workers re-ran their wait at every STILL WAITING (every 270 s) and each
    return read the worker's whole context: 44 model calls and 22.0 million
    context tokens in 17 minutes of hold. This runs the agent's wait the way the
    harness runs a subagent's Bash call (the PreToolUse[Bash] hooks registered in
    hooks/hooks.json, every one awaited, then the command they leave) and counts
    the returns while the agent is held. The wait's own bound is cut to 1 s, so
    any wait that returns while held shows up within the hold.
    """

    ENGINE = HERE.parent.parent
    HOLD_SECONDS = 4.0

    def setUp(self):
        super().setUp()
        self.bound_before = os.environ.get("RICHOS_AGENT_HOLD_WAIT_SECONDS")
        os.environ["RICHOS_AGENT_HOLD_WAIT_SECONDS"] = "1"

    def tearDown(self):
        if self.bound_before is None:
            os.environ.pop("RICHOS_AGENT_HOLD_WAIT_SECONDS", None)
        else:
            os.environ["RICHOS_AGENT_HOLD_WAIT_SECONDS"] = self.bound_before
        super().tearDown()

    def hold_hooks(self):
        """The registered PreToolUse[Bash] hooks that act on a held agent's call (the rewriter
        and anything agent_hold.py runs); the guard dispatcher is left out, it decides nothing here."""
        doc = json.loads((self.ENGINE / "hooks" / "hooks.json").read_text())
        found = []
        for group in doc["hooks"]["PreToolUse"]:
            if group.get("matcher") != "Bash":
                continue
            for h in group["hooks"]:
                if "shell-evidence" in h["command"] or "agent_hold.py" in h["command"]:
                    found.append(h["command"].replace("${CLAUDE_PLUGIN_ROOT}", str(self.ENGINE)))
        self.assertTrue(found, "hooks.json registers the Bash rewriter")
        return found

    def tool_call(self, hooks, command, tuid):
        """One Bash tool call: its hooks run together and are all awaited, then the command they leave runs."""
        payload = self.payload(tuid)
        payload["tool_input"] = {"command": command}
        running = [subprocess.Popen(["/bin/sh", "-c", c], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                    stderr=subprocess.DEVNULL, text=True, start_new_session=True) for c in hooks]
        self.procs.extend(running)
        final = command
        for p in running:
            out, _ = p.communicate(json.dumps(payload), timeout=60)
            try:
                got = json.loads(out) if out.strip() else {}
            except ValueError:
                got = {}
            updated = (got.get("hookSpecificOutput") or {}).get("updatedInput") or {}
            if isinstance(updated.get("command"), str):
                final = updated["command"]
        call = subprocess.Popen([SHELL, "-c", final], start_new_session=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL, text=True)
        self.procs.append(call)
        out, _ = call.communicate(timeout=60)
        return out

    def test_a_wait_the_gate_held_back_returns_at_the_release_while_a_long_native_call_runs(self):
        # 2026-10-31 review rv-20261031T020132Z-fbe99bb8-9344: the registered hook's gate holds the
        # wait call back until the release, so the wait itself never sees the hold. It must still
        # return at the release, naming the long call, not wait on it (STILL RUNNING at its bound).
        import threading
        hooks = self.hold_hooks()
        long_call, _ = self.start_rewritten("sleep 60", tuid="toolu_long")
        long_pid = Path(agent_hold._shell_dir(self.session, self.agent), "toolu_long.pid")
        self.assertTrue(wait_for(long_pid.exists), "the long native call recorded its shell")
        agent_hold.hold(self.session, self.agent, "fixture")
        first = self.tool_call(hooks, agent_hold.WAIT_COMMAND, "toolu_notice")
        self.assertIn("WAIT: running work is held", first)
        returned = []
        agent = threading.Thread(target=lambda: returned.append(
            self.tool_call(hooks, agent_hold.WAIT_COMMAND, "toolu_wait")), daemon=True)
        agent.start()
        # Lets the call reach the gate first; a release before that takes the same path (the
        # unresumed notice), so the verdict below never depends on this sleep.
        time.sleep(1.5)  # load-bound: ordering aid only, both orders reach the same verdict
        self.assertTrue(agent_hold.release(self.session, self.agent)["ok"])
        agent.join(60)
        self.assertFalse(agent.is_alive(), "the wait returns after the release")
        out = returned[0]
        self.assertIn("RESUMED at", out)
        self.assertNotIn("STILL RUNNING", out)
        self.assertIn("NOT FINISHED YET: tool toolu_long", out, "the running call is named for a later wait")
        self.assertIsNone(long_call.poll(), "the long call continues; the wait never ends it")

    def test_a_held_agent_makes_no_model_call_until_it_is_released(self):
        import threading
        hooks = self.hold_hooks()
        agent_hold.hold(self.session, self.agent, "fixture")
        first = self.tool_call(hooks, agent_hold.WAIT_COMMAND, "toolu_notice")
        self.assertIn("WAIT: running work is held", first, "the first wait returns at once with the WAIT")

        returns = []  # (monotonic time, output) of every return: one model call each

        def agent():
            for n in range(100):
                out = self.tool_call(hooks, agent_hold.WAIT_COMMAND, "toolu_wait%d" % n)
                returns.append((time.monotonic(), out))
                if "RESUMED" in out:
                    return

        worker = threading.Thread(target=agent, daemon=True)
        worker.start()
        time.sleep(self.HOLD_SECONDS)
        released_at = time.monotonic()
        self.assertTrue(agent_hold.release(self.session, self.agent)["ok"])
        worker.join(30)
        self.assertFalse(worker.is_alive(), "the agent's wait returns once it is released")
        during = [out.strip().splitlines()[0] for t, out in returns if t < released_at]
        self.assertEqual(during, [], "model calls while held (each return of the wait is one)")
        self.assertEqual(len(returns), 1, [out for _t, out in returns])
        self.assertIn("RESUMED at", returns[0][1], "it carries on from where it was")

    def test_the_gate_lets_every_other_call_through_at_once(self):
        agent_hold.hold(self.session, self.agent, "fixture")
        start = time.monotonic()
        lead = dict(self.payload("toolu_lead"), tool_input={"command": agent_hold.WAIT_COMMAND})
        lead.pop("agent_id")
        other = dict(self.payload("toolu_other"), tool_input={"command": "git status"})
        unnoticed = dict(self.payload("toolu_first"), tool_input={"command": agent_hold.WAIT_COMMAND})
        for p in (lead, other, unnoticed, {"tool_name": "Read"}, None):
            self.assertEqual(agent_hold.gate(p, poll=0.05), 0)
        self.assertLess(time.monotonic() - start, 1.0, "nothing but a noticed held wait is held")


class EndOwned(Base):
    """2026-10-11, echo-opus-walk49fix1: its own `until grep ...; do sleep 3; done` loops,
    watching a file in a scratch folder it had already deleted, could never finish. When an
    agent's run ends, the commands it owns are ended, with the hold's exclusions."""

    script, pid_from, watchdog_of = Catches.script, Catches.pid_from, Catches.watchdog_of

    def start_call(self, body, tuid="toolu_fixture1", agent=None):
        """As Base.start_call, with the call's own command in its record (the log names it)."""
        payload = dict(self.payload(tuid, agent), tool_input={"command": body, "run_in_background": True})
        prefix = agent_hold.capture(payload)
        self.assertTrue(prefix)
        p = subprocess.Popen([SHELL, "-c", "set -e -o pipefail\n" + prefix + body], process_group=0,
                             stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.procs.append(p)
        return p

    def loop(self):
        # Exactly the incident's shape: the folder it watches is gone, so it never finishes while
        # its test runs. It still ends once the test process that started it ($PPID) is gone, or
        # after ten minutes, so a test that dies before its tearDown leaves nothing running.
        return ('until grep -q "Finished" %s 2>/dev/null || ! kill -0 $PPID 2>/dev/null'
                ' || [ $SECONDS -ge 600 ]; do sleep 0.2; done' % self.out("deleted/build.log"))

    def alive(self, pid):
        return bool(state(pid)) and not state(pid).startswith("Z")

    def test_an_endless_loop_is_ended_and_logged_and_nothing_it_does_not_own_is_touched(self):
        walk = self.script("walk.py", WALK)
        daemon = self.script("daemon.py", DAEMON)
        slot = os.path.join(os.environ["TESTVM_ROOT"], "guest.lock")
        # Three calls of its own, as an agent makes them: a daemon, a test VM walk, the loop.
        self.start_call("%s %s %s" % (sys.executable, daemon, self.out("d")), tuid="toolu_daemon")
        self.start_call("%s %s %s %s" % (sys.executable, walk, slot, self.out("walk")), tuid="toolu_walk")
        call = self.start_call(self.loop())
        # The same loop text, not started by the agent; and another agent's own loop.
        twin = subprocess.Popen([SHELL, "-c", self.loop()], process_group=0)
        self.procs.append(twin)
        peer = self.start_call(self.loop(), tuid="toolu_peer", agent="bpeerfixture")
        dpid = self.pid_from(self.out("d") + ".pid")
        self.assertTrue(wait_for(lambda: os.path.exists(self.out("walk") + ".pid")))
        walker, walk_child = map(int, Path(self.out("walk") + ".pid").read_text().split())
        self.pids |= {walker, walk_child}
        self.assertTrue(wait_for(lambda: agent_hold.snapshot().get(dpid, {}).get("ppid") == 1), "daemonized")
        self.assertTrue(wait_for(lambda: call.pid in {p for p, _pp, _a in agent_hold.owned_shells(
            self.session, self.agent, agent_hold.snapshot())}), "its call is recorded as its own")
        self.assertIsNone(call.poll(), "the loop never finishes by itself")

        r = agent_hold.end_owned(self.session, self.agent, "fixture-agent", "its run ended (fixture)")

        self.assertTrue(r["ok"], r)
        self.assertTrue(wait_for(lambda: call.poll() is not None, 120), "the agent's loop is gone")
        self.assertIn(call.pid, [e["pid"] for e in r["ended"]])
        for pid, what in ((dpid, "the shared daemon it started"), (walker, "the test VM walk"),
                          (walk_child, "the walk's guest work")):
            self.assertTrue(self.alive(pid), "%s is left running" % what)
            self.assertNotIn(pid, [e["pid"] for e in r["ended"]], what)
        self.assertIsNone(twin.poll(), "the same command line elsewhere is never touched")
        self.assertIsNone(peer.poll(), "another agent's command is never touched")
        log = Path(os.environ["RICHOS_AGENT_HOLD_DIR"], "ended.log").read_text()
        self.assertIn("ended fixture-agent", log)
        self.assertIn('until grep -q "Finished"', log)
        self.assertIn("its run ended (fixture)", log)
        self.assertIn("shared daemon", log)
        self.assertIn("test VM walk pid %d" % walker, log)
        line = agent_hold.describe_end(r)
        self.assertIn("END fixture-agent:", line)
        sys.stderr.write("\n  measured: %s (%.2f s); ended: %s\n" % (line[:60], r["seconds"], [e["command"][:40] for e in r["ended"]]))

    def test_its_fixture_loop_never_outlives_the_test_that_started_it(self):
        """2026-10-22, PID 9650: the twin loop above, parent launchd and its own process group,
        ran for six days and 3.6 CPU-hours after the test process that started it was gone,
        because only that test's tearDown ended it. The loop ends itself once its test has."""
        starter = ("import subprocess, sys\n"
                   "p = subprocess.Popen([sys.argv[1], '-c', sys.argv[2]], start_new_session=True)\n"
                   "print(p.pid, flush=True)\n"
                   "sys.stdin.read()\n")
        test = subprocess.Popen([sys.executable, "-c", starter, SHELL, self.loop()], process_group=0,
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.procs.append(test)
        loop = int(test.stdout.readline())
        self.pids.add(loop)
        # A `sleep` child means its condition was tested with the test alive and came out false.
        self.assertTrue(wait_for(lambda: any(row["ppid"] == loop for row in agent_hold.snapshot().values())),
                        "the loop goes round while its test runs")
        self.assertTrue(self.alive(loop), "while its test runs, the loop does not finish by itself")
        test.stdin.close()                  # the test process ends without running its tearDown
        test.wait(10)
        self.assertTrue(wait_for(lambda: not self.alive(loop), 10), "the loop outlived its test")

    def test_a_paused_agents_work_is_never_ended(self):
        self.start_call("%s %s %s %d" % (sys.executable, self.worker, self.out("w"), ROUNDS * 20))
        worker = self.worker_pid("w")
        r = agent_hold.hold(self.session, self.agent, "fixture")
        self.watchdog_of(r)
        got = agent_hold.end_owned(self.session, self.agent, "fixture", "its run ended (fixture)")
        self.assertEqual(got["ended"], [], got)
        self.assertIn("pause", got["skipped"])
        self.assertTrue(state(worker).startswith("T"), "still frozen, not ended")
        agent_hold.release(self.session, self.agent)
        self.assertTrue(self.alive(worker))

    def test_a_reused_process_id_is_never_signaled(self):
        """Each signal re-reads the process's start time from the kernel first (second review
        rv-20261011T042129Z-6b0f0810-1c93): an owned process that exits while the end is under
        way, its id given to a process the agent does not own, is never signaled. No real signal
        is sent; the kernel's answers are the fixture's."""
        pid = 987654
        kernel = {pid: {"birth": 100, "ppid": 50, "pgid": pid, "stat": "S", "cpu": 0}}
        sent = []

        def commands(_pids):
            kernel[pid]["birth"] = 200      # it exits during the command-line read; the id is reused
            return {pid: "the agent's own loop"}

        with mock.patch.object(agent_hold, "snapshot", side_effect=lambda: copy.deepcopy(kernel)), \
             mock.patch.object(agent_hold, "calls", return_value=[]), \
             mock.patch.object(agent_hold, "owned_tree", side_effect=lambda _s, _a, t: (
                 {p for p, row in t.items() if row["birth"] == 100}, set(), set(), [], {})), \
             mock.patch.object(agent_hold, "_commands", side_effect=commands), \
             mock.patch.object(agent_hold, "_birth_now", side_effect=lambda p: kernel.get(p, {}).get("birth")), \
             mock.patch.object(agent_hold.os, "kill", side_effect=lambda p, s: sent.append((s, kernel[p]["birth"]))), \
             mock.patch.object(agent_hold, "_log_end"):
            r = agent_hold.end_owned(self.session, self.agent, "fixture-agent", "its run ended (fixture)")
        self.assertEqual(sent, [], "a signal reached the process that took over the id")
        self.assertEqual(r["ended"], [], r)
        self.assertEqual(r["survivors"], [], r)

    def test_the_parent_of_a_left_running_walk_is_not_ended(self):
        """The test VM walk is left running, so its owned parents are too: a proc_tree
        supervisor ends its whole tree when it is ended, the walk with it (second review
        rv-20261011T055437Z-792eda49-35e8). Its own unrelated loop is still ended. No real
        signal is sent; the kernel's answers are the fixture's."""
        def row(ppid):
            return {"ppid": ppid, "pgid": 100, "stat": "S", "birth": 100, "cpu": 0}

        # Bash call 100 -> supervisor 110 -> walk 120 (slot holder) -> guest work 130; loop 140.
        kernel = {100: row(50), 110: row(100), 120: row(110), 130: row(120), 140: row(50)}
        sent = []

        def kill(p, s):
            sent.append(p)
            if s in (signal.SIGTERM, signal.SIGKILL):
                kernel.pop(p, None)

        with mock.patch.object(agent_hold, "snapshot", side_effect=lambda: copy.deepcopy(kernel)), \
             mock.patch.object(agent_hold, "calls", return_value=[]), \
             mock.patch.object(agent_hold, "owned_tree", side_effect=lambda _s, _a, t: (
                 set(t), {120, 130} & set(t), set(), ["test VM walk pid 120 left running"], {})), \
             mock.patch.object(agent_hold, "_commands", side_effect=lambda pids: {p: "fixture" for p in pids}), \
             mock.patch.object(agent_hold, "_birth_now", side_effect=lambda p: kernel.get(p, {}).get("birth")), \
             mock.patch.object(agent_hold.os, "kill", side_effect=kill), \
             mock.patch.object(agent_hold, "_log_end"):
            r = agent_hold.end_owned(self.session, self.agent, "fixture-agent", "its run ended (fixture)")
        self.assertEqual(sorted(set(sent)), [140], "only its unrelated loop is signaled")
        self.assertEqual([e["pid"] for e in r["ended"]], [140], r)
        self.assertTrue(r["ok"], r)
        self.assertEqual(r["excluded"], [100, 110, 120, 130], r)
        self.assertTrue(any("parent" in n for n in r["notes"]), r["notes"])

    def test_the_one_command_ends_it_by_hand(self):
        call = self.start_call(self.loop())
        self.assertTrue(wait_for(lambda: call.pid in {p for p, _pp, _a in agent_hold.owned_shells(
            self.session, self.agent, agent_hold.snapshot())}), "its call is recorded as its own")
        cmd = agent_hold.end_command(self.session, self.agent, "fixture-agent")
        self.assertTrue(cmd.startswith("python3 ") and " end --session " in cmd, cmd)
        out = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=30)
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertIn("END fixture-agent: ", out.stdout)
        self.assertTrue(wait_for(lambda: call.poll() is not None, 120), "the agent's loop is gone")


if __name__ == "__main__":
    unittest.main(verbosity=2)
