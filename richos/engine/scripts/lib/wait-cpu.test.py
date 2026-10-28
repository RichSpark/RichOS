#!/usr/bin/env python3
"""A wait costs about 0% CPU: each waiter here waits 60 s on nothing, and its CPU is bounded.

2026-10-28, 17:33Z: ten `agent_hold.py wait` processes were at 76.8, 74.7, 74.0, 72.3,
42.6, 29.2, 28.4, 25.0, 24.6 and 8.3% CPU, about 4.6 of the Mac's 10 cores spent only
waiting, while every proof run was refused CPU admission. The cause: the wait looked
again every 0.5 s (and every 0.05 s for up to 2 s per call record once nothing ran),
and every look re-read up to 4 MB of the agent's transcript for every uncollected call
record. In the app's SDK sessions no record ever binds to its output, so one agent's 222
records were re-parsed, about 0.7 GB of JSON, at every look.

Every case below starts one real waiter process in the state it is in while an agent
waits, lets it wait CASE_SECONDS (60 s) with nothing happening, and then measures it.
Its CPU is the user plus system time of the waiter AND of every process it started and
reaped (a `ps` it runs counts), read as RUSAGE_CHILDREN by a small parent process whose
only child is the waiter: POSIX counts there every descendant that ended and was waited
for. The bound is CPU_BOUND (0.6 s per 60 s wait: 1% of one core, about 0.1% of this
Mac), which includes the interpreter's own start (about 0.05 s). Each case also checks
that the waiter really waited the whole time on the path it is meant to (its wall time
and its own output), so a waiter that crashed or returned early cannot pass as quiet.

All cases run at once, each in its own temporary state directory, so the suite takes
about CASE_SECONDS plus setup. Run with the app's variables unset; setUpModule refuses
to run with them set, because they would point waiters at live state.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest

HERE = Path(__file__).resolve().parent
AGENT_HOLD = HERE / "agent_hold.py"
CASE_SECONDS = 60.0
CPU_BOUND = 0.6

# The only child of this parent is the waiter; RUSAGE_CHILDREN then holds the waiter's CPU and
# that of every descendant it reaped. argv[1] is the waiter's argv as JSON, argv[2] its stdin.
MEASURE = r'''
import json, resource, subprocess, sys, time
t0 = time.monotonic()
p = subprocess.run(json.loads(sys.argv[1]), input=sys.argv[2].encode(), stdout=subprocess.PIPE,
                   stderr=subprocess.STDOUT, start_new_session=sys.argv[3] == "session")
ru = resource.getrusage(resource.RUSAGE_CHILDREN)
print(json.dumps({"cpu": ru.ru_utime + ru.ru_stime, "wall": time.monotonic() - t0, "code": p.returncode,
                  "out": p.stdout.decode("utf-8", "replace")[-3000:]}))
'''

# The registry the watchdog asks: never finished.
REGISTRY = 'print("active\\t")\n'

LIVE_VARIABLES = ("RICHOS_WORKSPACES_DIR", "RICHOS_APP_STATE", "RICHOS_ENGINE_ROOT", "RICHOS_AGENT_HOLD_DIR",
                  "STALL_WATCH_STATE_DIR", "CLAUDE_PROJECT_DIR", "CLAUDE_CONFIG_DIR")


def setUpModule():
    live = [k for k in os.environ if k in LIVE_VARIABLES or k.startswith("RICHOS_APP_")]
    if live:
        raise unittest.SkipTest("refused: the app's variables are set (%s); they would point the waiters "
                                "at live state. Unset them and run again." % ", ".join(sorted(live)))


def _birth(pid):
    out = subprocess.run(["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True, text=True,
                         env={**os.environ, "LC_ALL": "C", "TZ": "UTC0"}).stdout.strip()
    import calendar
    return calendar.timegm(time.strptime(out, "%a %b %d %H:%M:%S %Y"))


class Case:
    """One waiter, measured over its whole life by its own parent process."""

    def __init__(self, name, argv, env, stdin="", session=False):
        self.name, self.result, self.error = name, None, None
        self.thread = threading.Thread(target=self._run, args=(argv, env, stdin, session), daemon=True)

    def _run(self, argv, env, stdin, session):
        try:
            # load-bound: the waiter's own CASE_SECONDS bound ends it; this only catches a hung waiter.
            p = subprocess.run([sys.executable, "-c", MEASURE, json.dumps(argv), stdin,
                                "session" if session else "-"], env=env,
                               capture_output=True, text=True, timeout=CASE_SECONDS * 3)
            self.result = json.loads(p.stdout.strip().splitlines()[-1])
        except Exception as error:   # reported by the test, never swallowed
            self.error = "%s: %s" % (type(error).__name__, error)


class WaitCpu(unittest.TestCase):
    """Every waiter, 60 s of waiting on nothing, at most CPU_BOUND seconds of CPU."""

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp(prefix="wait-cpu-test.")
        cls.own = []        # processes this test started, stopped in tearDownClass
        cls.after = []      # (seconds from start, action) for waiters that end on an outside event
        cls.cases = {}
        for name in sorted(n for n in dir(cls) if n.startswith("case_")):
            got = getattr(cls, name)()
            cls.cases[name[5:]] = Case(name[5:], **got) if isinstance(got, dict) else Case(name[5:], *got)
        start = time.monotonic()
        for c in cls.cases.values():
            c.thread.start()
        for at, action in sorted(cls.after, key=lambda a: a[0]):
            time.sleep(max(0.0, start + at - time.monotonic()))
            action()
        for c in cls.cases.values():
            c.thread.join(CASE_SECONDS * 3)
        for name, c in sorted(cls.cases.items()):
            r = c.result or {}
            sys.stderr.write("  measured: %-28s cpu %.3f s over %.1f s wall (exit %s)\n"
                             % (name, r.get("cpu", -1), r.get("wall", -1), r.get("code")))

    @classmethod
    def tearDownClass(cls):
        for p in cls.own:
            try:
                p.kill()
            except OSError:
                pass
            p.wait()
        for f in getattr(cls, "own_files", []):
            f.close()
        shutil.rmtree(cls.tmp, ignore_errors=True)

    # ------------------------------------------------------------------ fixtures

    @classmethod
    def spawn_sleeper(cls):
        """A process this test owns that runs past every case: what a waiter waits on."""
        p = subprocess.Popen(["sleep", str(int(CASE_SECONDS * 3))])
        cls.own.append(p)
        return p

    @classmethod
    def hold_env(cls, name):
        state = os.path.join(cls.tmp, name)
        os.makedirs(state)
        env = {k: v for k, v in os.environ.items() if k not in ("BASH_MAX_TIMEOUT_MS", "RICHOS_AGENT_HOLD_WAIT_SECONDS")}
        env.update({"RICHOS_AGENT_HOLD_DIR": state, "RICHOS_AGENT_OWNER": "a%sagent" % name.replace("-", ""),
                    "RICHOS_AGENT_SESSION": "wait-cpu-session", "TESTVM_ROOT": os.path.join(state, "testvm"),
                    "PYTHONDONTWRITEBYTECODE": "1"})
        return state, env

    @staticmethod
    def shells(state, env):
        d = os.path.join(state, "shells", env["RICHOS_AGENT_SESSION"], env["RICHOS_AGENT_OWNER"])
        os.makedirs(d, exist_ok=True)
        return d

    @classmethod
    def transcript(cls, state, env, megabytes=4):
        """A host transcript shaped like a real one (8.5 MB measured, mostly api-request rows)
        in which no call record's tool result is ever found: the app's SDK sessions today."""
        path = Path(state, env["RICHOS_AGENT_SESSION"], "subagents", "agent-%s.jsonl" % env["RICHOS_AGENT_OWNER"])
        path.parent.mkdir(parents=True)
        blob = "x" * 6000
        with path.open("w") as out:
            i = 0
            while out.tell() < megabytes * 1024 * 1024:
                out.write(json.dumps({"type": "api-request-blob", "data": {"messages": [{"role": "user",
                          "content": [{"type": "text", "text": blob}]}]}}) + "\n")
                out.write(json.dumps({"type": "user", "sessionId": env["RICHOS_AGENT_SESSION"],
                          "agentId": env["RICHOS_AGENT_OWNER"], "message": {"content": [
                              {"type": "tool_result", "tool_use_id": "toolu_other%d" % i,
                               "content": "Command running in background with ID: b%d. Output is being written to: "
                                          "/elsewhere/tasks/b%d.output" % (i, i)}]}}) + "\n")
                i += 1
        return str(path)

    @classmethod
    def unbindable_records(cls, state, env, count=40):
        """Finished native calls whose result never binds, uncollected (they stay 24 h)."""
        d = cls.shells(state, env)
        transcript = cls.transcript(state, env)
        at = time.time() - 3600
        for n in range(count):
            Path(d, "toolu_old%03d.json" % n).write_text(json.dumps({
                "at": at + n, "tool_use_id": "toolu_old%03d" % n, "mode": "native", "command": "echo %d" % n,
                "cwd": state, "result_source": {"transcript": transcript, "offset": 0,
                                                "session": env["RICHOS_AGENT_SESSION"],
                                                "agent": env["RICHOS_AGENT_OWNER"]}}))

    @classmethod
    def running_native_call(cls, state, env):
        """A native call whose shell is alive (this test's own sleeper, recorded as calls() checks)."""
        d = cls.shells(state, env)
        at = time.time()
        sleeper = cls.spawn_sleeper()
        Path(d, "toolu_running.json").write_text(json.dumps({"at": at, "tool_use_id": "toolu_running",
                                                             "mode": "native", "command": "sleep", "cwd": state}))
        Path(d, "toolu_running.pid").write_text("%d %d\n" % (sleeper.pid, os.getpid()))
        return sleeper

    @classmethod
    def held(cls, state, env, notice=True, parents=None):
        """This agent is held, and (notice) its WAIT was already handed over."""
        session, agent = env["RICHOS_AGENT_SESSION"], env["RICHOS_AGENT_OWNER"]
        at = time.time()
        path = os.path.join(state, "held", "%s__%s.json" % (session, agent))
        os.makedirs(os.path.dirname(path))
        Path(path).write_text(json.dumps({"session_id": session, "agent_id": agent, "name": "fixture",
                                          "at": at, "held": {}, "parents": parents or {}, "detached": {}}))
        if notice:
            os.makedirs(os.path.join(state, "wait-notices"))
            Path(state, "wait-notices", "%s__%s.json" % (session, agent)).write_text(json.dumps({"at": at}))
        return path

    @staticmethod
    def wait_argv():
        return [sys.executable, "-B", str(AGENT_HOLD), "wait", "--max-seconds", "%g" % CASE_SECONDS]

    # ------------------------------------------------------------------ the waiters

    @classmethod
    def case_agent_hold_wait_held(cls):
        """A held agent's wait (the CLI path; gate() normally holds it before it starts)."""
        state, env = cls.hold_env("held")
        cls.held(state, env)
        return cls.wait_argv(), env

    @classmethod
    def case_agent_hold_wait_native_running(cls):
        """An agent waits for its own native task; 40 old records never bind (17:33Z's shape)."""
        state, env = cls.hold_env("native-running")
        cls.unbindable_records(state, env)
        cls.running_native_call(state, env)
        return cls.wait_argv(), env

    @classmethod
    def case_agent_hold_wait_nothing_running(cls):
        """Nothing runs; each never-binding record gets its 2 s look (collect_native)."""
        state, env = cls.hold_env("nothing-running")
        cls.unbindable_records(state, env)
        return cls.wait_argv(), env

    @classmethod
    def case_agent_hold_wait_frozen_foreground(cls):
        """A legacy frozen foreground command, running again, collected when it ends (_collect)."""
        state, env = cls.hold_env("frozen-foreground")
        d = cls.shells(state, env)
        job = cls.spawn_sleeper()
        Path(d, "toolu_frozen.json").write_text(json.dumps({"at": time.time(), "tool_use_id": "toolu_frozen",
                                                            "mode": "fg", "command": "make", "cwd": state}))
        Path(d, "toolu_frozen.detached").write_text(json.dumps({"at": time.time(), "job": job.pid,
                                                                "job_birth": _birth(job.pid), "command": "make",
                                                                "tool_use_id": "toolu_frozen"}))
        return cls.wait_argv(), env

    @classmethod
    def case_agent_hold_gate(cls):
        """gate(): the hook holds a held agent's wait call before it starts, until the release."""
        state, env = cls.hold_env("gate")
        path = cls.held(state, env)
        payload = {"tool_name": "Bash", "session_id": env["RICHOS_AGENT_SESSION"],
                   "agent_id": env["RICHOS_AGENT_OWNER"], "tool_use_id": "toolu_gate",
                   "tool_input": {"command": "python3 %s wait" % AGENT_HOLD}}
        code = ("import json, sys; sys.path.insert(0, %r); import agent_hold; "
                "sys.exit(agent_hold.gate(json.load(sys.stdin)))" % str(HERE))
        cls.after.append((CASE_SECONDS, lambda: os.unlink(path)))   # the release
        return [sys.executable, "-B", "-c", code], env, json.dumps(payload)

    @classmethod
    def case_agent_hold_watchdog(cls):
        """The hold's watchdog, its lead alive and the registry saying active, until the release."""
        state, env = cls.hold_env("watchdog")
        registry = os.path.join(state, "registry.py")
        Path(registry).write_text(REGISTRY)
        env["RICHOS_AGENT_HOLD_REGISTRY"] = registry
        env.pop("RICHOS_AGENT_HOLD_WATCH_SECONDS", None)
        lead = cls.spawn_sleeper()
        path = cls.held(state, env, parents={str(lead.pid): _birth(lead.pid)})
        cls.after.append((CASE_SECONDS, lambda: os.unlink(path)))   # released by someone else
        return [sys.executable, "-B", str(AGENT_HOLD), "watch", "--session", env["RICHOS_AGENT_SESSION"],
                "--agent", env["RICHOS_AGENT_OWNER"]], env

    # ------------------------------------------------------------------ the checks

    def measured(self, name, wall_at_least=CASE_SECONDS - 1):
        c = self.cases[name]
        self.assertIsNone(c.error, c.error)
        r = c.result
        self.assertGreaterEqual(r["wall"], wall_at_least, "it waited the whole time: %r" % r)
        self.assertLess(r["cpu"], CPU_BOUND, "%s used %.2f s of CPU over a %.0f s wait (bound %.1f s): %s"
                        % (name, r["cpu"], r["wall"], CPU_BOUND, r["out"][-600:]))
        return r

    def test_agent_hold_wait_held(self):
        r = self.measured("agent_hold_wait_held")
        self.assertIn("STILL WAITING", r["out"])

    def test_agent_hold_wait_native_running(self):
        r = self.measured("agent_hold_wait_native_running")
        self.assertIn("STILL RUNNING", r["out"])

    def test_agent_hold_wait_nothing_running(self):
        r = self.measured("agent_hold_wait_nothing_running")
        self.assertIn("NATIVE RESULT UNAVAILABLE for toolu_old039", r["out"], "every record got its look")
        self.assertEqual(r["code"], 2, "a missing result is never a pass")

    def test_agent_hold_wait_frozen_foreground(self):
        r = self.measured("agent_hold_wait_frozen_foreground")
        self.assertIn("STILL RUNNING", r["out"])
        self.assertIn("frozen (make)", r["out"])

    def test_agent_hold_gate(self):
        r = self.measured("agent_hold_gate")
        self.assertEqual(r["code"], 0)
        self.assertLess(r["wall"], CASE_SECONDS + 10, "it let the call run soon after the release")

    def test_agent_hold_watchdog(self):
        r = self.measured("agent_hold_watchdog")
        self.assertEqual(r["code"], 0)
        self.assertLess(r["wall"], CASE_SECONDS + 10, "it ended soon after the release")


if __name__ == "__main__":
    unittest.main()
