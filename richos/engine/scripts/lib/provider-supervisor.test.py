#!/usr/bin/env python3
"""Real OS process tests with a fictional provider, never the user's sessions."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest

SUPERVISOR=Path(__file__).resolve().parents[1]/"provider-supervisor.py"
def alive(pid):
    result=subprocess.run(["/bin/ps","-o","stat=","-p",str(pid)],text=True,capture_output=True)
    return result.returncode==0 and bool(result.stdout.strip()) and not result.stdout.strip().startswith("Z")

class Supervisor(unittest.TestCase):
    def test_parent_crash_stops_owned_provider_and_child_but_preserves_unrelated_process(self):
        with tempfile.TemporaryDirectory(prefix="supervisor fixture ") as root:
            root=Path(root); receipt=root/"pids.json"
            fake=root/"provider.py"
            fake.write_text("import subprocess,os,json,time,sys\nchild=subprocess.Popen(['/bin/sleep','120'])\nopen(sys.argv[1],'w').write(json.dumps([os.getpid(),child.pid,os.environ['RICHOS_SESSION_PID']]))\ntime.sleep(120)\n")
            launcher=root/"desktop.py"
            launcher.write_text("import subprocess,sys,time\nsubprocess.Popen(sys.argv[1:],start_new_session=True)\ntime.sleep(120)\n")
            unrelated=subprocess.Popen(["/bin/sleep","120"])
            desktop=subprocess.Popen([sys.executable,str(launcher),sys.executable,str(SUPERVISOR),sys.executable,str(fake),str(receipt)])
            try:
                for _ in range(100):
                    if receipt.exists():break
                    time.sleep(.05)
                provider,child,identity=json.loads(receipt.read_text())
                self.assertEqual(str(provider),identity)
                self.assertTrue(alive(provider));self.assertTrue(alive(child))
                desktop.kill();desktop.wait()
                for _ in range(100):
                    if not alive(provider) and not alive(child):break
                    time.sleep(.05)
                self.assertFalse(alive(provider));self.assertFalse(alive(child))
                self.assertIsNone(unrelated.poll())
            finally:
                if desktop.poll() is None:desktop.kill();desktop.wait()
                unrelated.kill();unrelated.wait()

PROVIDER_WITH_TOOL_SHELL = r'''
import json, os, signal, subprocess, sys, time
receipt, beat = sys.argv[1], sys.argv[2]
if len(sys.argv) > 3 and sys.argv[3] == "slow-to-stop":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)   # a provider that takes its time over SIGTERM
# A tool shell the way Claude Code starts one: its own session and process group,
# no controlling terminal. It starts a background heartbeat and a grandchild.
shell = subprocess.Popen(["/bin/sh", "-c",
    "(while :; do date +%s > '" + beat + "'; sleep 0.2; done) & sleep 300 & wait"], start_new_session=True)
# A same-group child, the way sourcekit-lsp or caffeinate sit in the lead's group.
same = subprocess.Popen(["/bin/sleep", "300"])
time.sleep(0.6)
open(receipt, "w").write(json.dumps({"provider": os.getpid(), "shell": shell.pid, "same": same.pid}))
if len(sys.argv) > 3 and sys.argv[3] == "exit-soon":
    time.sleep(1.5)
    os._exit(0)
time.sleep(300)
'''


# Tom's review of P0, finding 1 (2026-10-29): the lead starts a command with `nohup … &` in a
# tool shell that exits at once, so the command is reparented to launchd before any snapshot
# sees the shell or its group. The command writes its own pid and keeps a heartbeat.
PROVIDER_WITH_NOHUP = r'''
import json, os, subprocess, sys, time
receipt, beat, out = sys.argv[1], sys.argv[2], sys.argv[3]
loop = "import os,sys,time\nopen(sys.argv[1],'w').write(str(os.getpid()))\nwhile True:\n open(sys.argv[2],'w').write(str(time.time())); time.sleep(0.2)\n"
shell = subprocess.Popen(["/bin/sh", "-c", 'nohup "$0" -c "$1" "$2" "$3" >/dev/null 2>&1 &',
                          sys.executable, loop, out, beat], start_new_session=True)
shell.wait()                                  # the tool call returns at once, as `nohup … &` does
open(receipt, "w").write(json.dumps({"provider": os.getpid(), "shell": shell.pid}))
time.sleep(300)
'''


def without_operator_names():
    """This process's environment minus the operator's three supervisor names, so a product
    case proves its parameters came from the command line."""
    return {k: v for k, v in os.environ.items()
            if k not in ("OPERATOR_REAP_GRACE", "RICHOS_OPERATOR_REAP_STATE", "RICHOS_OPERATOR_REAP_LOG")}


def beating(path, window=0.7):
    try:
        return time.time() - os.path.getmtime(path) < window
    except OSError:
        return False


def group_members(pgid):
    r = subprocess.run(["/bin/ps", "-A", "-o", "pid=,pgid=,stat="], text=True, capture_output=True)
    return [int(p) for p, g, s in (l.split()[:3] for l in r.stdout.splitlines() if len(l.split()) >= 3)
            if int(g) == pgid and not s.startswith("Z")]


class ReapDescendants(unittest.TestCase):
    """Spec r3 (q) item 2 (F5), Frank G8 and G9; the P15 shape, in-process."""

    def run_case(self, trigger, reap=True, provider_arg=None, product=False):
        with tempfile.TemporaryDirectory(prefix="supervisor reap ") as root:
            root = Path(root)
            receipt, beat, state = root/"pids.json", root/"beat", root/"state.json"
            fake = root/"provider.py"; fake.write_text(PROVIDER_WITH_TOOL_SHELL)
            launcher = root/"desktop.py"
            launcher.write_text("import subprocess,sys,time\np=subprocess.Popen(sys.argv[2:],start_new_session=True)\n"
                                "open(sys.argv[1],'w').write(str(p.pid))\ntime.sleep(120)\n")
            unrelated = subprocess.Popen(["/bin/sleep", "120"])
            if product:
                # THE PRODUCT'S SHAPE (design C3, C7): every parameter on the supervisor's own
                # command line, and none of the operator's environment names set at all.
                options = ["--reap-descendants", "--reap-grace=0", "--reap-state=%s" % state,
                           "--reap-log=%s" % (root/"reap.log")]
                env = without_operator_names()
            else:
                options = ["--reap-descendants"] if reap else []
                env = dict(os.environ, OPERATOR_REAP_GRACE="1", RICHOS_OPERATOR_REAP_STATE=str(state),
                           RICHOS_OPERATOR_REAP_LOG=str(root/"reap.log"))
            args = [sys.executable, str(SUPERVISOR)] + options + \
                   [sys.executable, str(fake), str(receipt), str(beat)] + ([provider_arg] if provider_arg else [])
            desktop = subprocess.Popen([sys.executable, str(launcher), str(root/"sup.pid")] + args, env=env)
            shell_group = None
            try:
                for _ in range(100):
                    if receipt.exists() and (root/"sup.pid").exists():
                        break
                    time.sleep(0.05)
                pids = json.loads(receipt.read_text()); sup = int((root/"sup.pid").read_text())
                pids["desktop"] = desktop.pid
                shell_group = pids["shell"]
                time.sleep(1.4)                      # at least one in-process snapshot
                self.assertTrue(beating(beat), "fixture: the heartbeat never started")
                snapshot = json.loads(state.read_text()) if state.exists() else {}
                if trigger == "owner":
                    desktop.kill(); desktop.wait()
                elif trigger == "sigterm":
                    os.kill(sup, signal.SIGTERM)
                elif trigger == "provider-killed":
                    os.kill(pids["provider"], signal.SIGKILL)     # G9: the provider crashes
                elif trigger == "provider-exits":
                    pass                                          # it exits by itself (exit-soon)
                elif trigger == "group-kill":
                    os.killpg(sup, signal.SIGKILL)                # today's host fence (owned_process.rs)
                t0 = time.time()
                # Measured on the processes themselves, not through the heartbeat's staleness
                # window: the moment the provider, the tool shell and every member of its
                # group are gone.
                dead_after = None
                while time.time() - t0 < 4:
                    if not alive(pids["provider"]) and not group_members(shell_group):
                        dead_after = time.time() - t0
                        break
                    time.sleep(0.02)
                while time.time() - t0 < 4 and beating(beat):
                    time.sleep(0.1)
                stopped_after = time.time() - t0
                final = json.loads(state.read_text()) if state.exists() else {}
                return {"pids": pids, "beating": beating(beat), "stopped_after": stopped_after, "dead_after": dead_after,
                        "final": final,
                        "left": group_members(shell_group), "snapshot": snapshot,
                        "unrelated_alive": unrelated.poll() is None,
                        "log": (root/"reap.log").read_text() if (root/"reap.log").exists() else ""}
            finally:
                if desktop.poll() is None:
                    desktop.kill(); desktop.wait()
                unrelated.kill(); unrelated.wait()
                if shell_group:
                    try:
                        os.killpg(shell_group, signal.SIGKILL)   # the fixture's own group, captured at spawn
                    except (ProcessLookupError, PermissionError):
                        pass

    def assert_reaped(self, result):
        self.assertFalse(result["beating"], "the tool shell's heartbeat outlived the lead")
        self.assertLessEqual(result["stopped_after"], 1 + 1 + 0.5, "not within OPERATOR_REAP_GRACE plus one second")
        self.assertEqual(result["left"], [], "members of the tool shell's group survived")
        self.assertTrue(result["unrelated_alive"], "an unrelated process was touched")
        self.assertIn("killed group %d" % result["pids"]["shell"], result["log"])

    def test_R1_owner_death_reaps_the_tool_shell_group(self):
        self.assert_reaped(self.run_case("owner"))

    def test_R2_sigterm_from_the_host_reaps_the_tool_shell_group(self):
        self.assert_reaped(self.run_case("sigterm"))

    def test_R3_G9_a_crashed_provider_is_reaped_from_the_last_snapshot(self):
        self.assert_reaped(self.run_case("provider-killed"))

    def test_R4_G9_a_provider_that_exits_by_itself_is_reaped_too(self):
        self.assert_reaped(self.run_case("provider-exits", provider_arg="exit-soon"))

    def test_R5_control_without_the_flag_the_tool_shell_survives_its_lead(self):
        result = self.run_case("owner", reap=False)
        self.assertTrue(result["beating"] or result["left"],
                        "control: without --reap-descendants the tool shell should outlive the lead (F5)")

    def test_R6_G8_idle_state_counts_only_descendants_outside_the_lead_group(self):
        result = self.run_case("owner")
        outside = result["snapshot"].get("outside_provider_group") or []
        self.assertIn(result["pids"]["shell"], outside, "the tool shell (own group) must count")
        self.assertNotIn(result["pids"]["same"], outside, "a same-group child (language server) must not count")

    def test_R7_a_recycled_group_id_is_never_adopted_or_signaled(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location("provider_supervisor", str(SUPERVISOR))
        mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
        rec = mod.Record(provider=100, own_group=100)
        rec.procs = {200: (5000, 200), 201: (5001, 200)}; rec.groups = {200: 5000}
        recycled = {100: (1, 100, 4000), 200: (1, 200, 9999), 300: (1, 200, 9999)}   # pid 200 reused, new members
        self.assertEqual(rec.live_groups(recycled), set(), "a recycled group leader with no live recorded member")
        rec.snapshot(recycled)
        self.assertNotIn(300, rec.procs, "a process in a recycled group was adopted")
        # A real member lives on. A fresh record, because a pid absent from one table is dead
        # and the record is right to forget it (design C4): it can never come back with the
        # same start, which is what reusing the first record here used to pretend.
        rec = mod.Record(provider=100, own_group=100)
        rec.procs = {200: (5000, 200), 201: (5001, 200)}; rec.groups = {200: 5000}
        still = {100: (1, 100, 4000), 201: (1, 200, 5001)}
        self.assertEqual(rec.live_groups(still), {200})

    def test_R8_the_flagless_main_is_the_compatibility_path(self):
        # N1, split (design 1.5): the product now runs WITH the reap (R10, R11). The flagless
        # `main()` stays exactly what it was, at the lines other files cite, for any caller
        # that names no option.
        source = SUPERVISOR.read_text().splitlines()
        self.assertTrue(any(l.startswith("def main():") for l in source[:20]))
        main_body = "\n".join(source[source.index("def main():"):source.index("def main():") + 26])
        self.assertNotIn("reap", main_body, "the compatibility main() must not know about reaping")
        self.assertFalse("main() above is the product path" in "\n".join(source),
                         "a comment still says the flagless main() is what the product runs")

    def test_R9_control_the_flag_alone_does_not_survive_the_host_s_group_sigkill(self):
        # Design C1: the host's fence SIGKILLs the supervisor's whole group, which no Python
        # can catch, so the reap never runs and the tool shell outlives its lead. This is why
        # the host sends SIGTERM first (owned_process.rs, `OwnedChild::supervised`).
        result = self.run_case("group-kill")
        self.assertTrue(result["beating"] or result["left"],
                        "control: a group SIGKILL should leave the tool shell running (C1)")

    def test_R10_product_mode_grace_zero_ends_the_tree_within_one_second_of_sigterm(self):
        # The provider ignores SIGTERM, as one winding down would: only a grace of zero, a
        # SIGKILL to the provider first, ends the tree inside the host's bound (design C3).
        result = self.run_case("sigterm", product=True, provider_arg="slow-to-stop")
        self.assertIsNotNone(result["dead_after"], "the provider or the tool shell's group outlived the reap")
        self.assertLessEqual(result["dead_after"], 1.0, "not within 1 s of SIGTERM: %.3f s" % result["dead_after"])
        self.assertFalse(result["beating"], "the tool shell's heartbeat outlived the lead")
        self.assertEqual(result["left"], [], "members of the tool shell's group survived")
        self.assertTrue(result["unrelated_alive"], "an unrelated process was touched")
        self.assertIn("killed group %d" % result["pids"]["shell"], result["log"], "the --reap-log file names no group")
        self.assertIn(result["pids"]["shell"], result["snapshot"].get("outside_provider_group") or [],
                      "the --reap-state file was not written")
        self.assertIn(result["pids"]["shell"], result["snapshot"].get("outside_provider_groups") or [],
                      "the --reap-state file does not name the command's group")
        self.assertEqual(result["snapshot"].get("owner"), result["pids"]["desktop"], "the state file names no owner")
        # After the reap the file says what is left, so a host still holding the lease does
        # not read the last snapshot's commands as running.
        self.assertTrue(result["final"].get("ended"), "no final state was written: %s" % result["final"])
        self.assertEqual(result["final"].get("outside_provider_groups"), [], "the final state still lists commands")

    def test_R14_a_command_started_with_nohup_counts_as_running_and_is_reaped_with_its_lead(self):
        """Tom's P0 finding 1 (richos-hq follow-ups.md:357): a real `nohup … &` in a tool shell
        that exits at once. The lease's state file, which the update gate and the quit question
        read (lease_commands.rs `read`: any group listed is a running command), must list the
        command's group, and the reap must end it with its lead."""
        with tempfile.TemporaryDirectory(prefix="supervisor nohup ") as root:
            root = Path(root)
            receipt, beat, out, state = root/"pids.json", root/"beat", root/"nohup.pid", root/"state.json"
            fake = root/"provider.py"; fake.write_text(PROVIDER_WITH_NOHUP)
            launcher = root/"desktop.py"
            launcher.write_text("import subprocess,sys,time\np=subprocess.Popen(sys.argv[2:],start_new_session=True)\n"
                                "open(sys.argv[1],'w').write(str(p.pid))\ntime.sleep(120)\n")
            args = [sys.executable, str(SUPERVISOR), "--reap-descendants", "--reap-grace=0",
                    "--reap-state=%s" % state, "--reap-log=%s" % (root/"reap.log"),
                    sys.executable, str(fake), str(receipt), str(beat), str(out)]
            desktop = subprocess.Popen([sys.executable, str(launcher), str(root/"sup.pid")] + args,
                                       env=without_operator_names())
            detached = None
            try:
                for _ in range(100):
                    if receipt.exists() and out.exists() and out.read_text() and (root/"sup.pid").exists():
                        break
                    time.sleep(0.05)
                detached = int(out.read_text())
                sup = int((root/"sup.pid").read_text())
                group = int(json.loads(receipt.read_text())["shell"])
                ps = subprocess.run(["/bin/ps", "-o", "ppid=,pgid=", "-p", str(detached)], text=True, capture_output=True)
                self.assertEqual([int(v) for v in ps.stdout.split()], [1, group],
                                 "fixture: the nohup command is reparented to launchd in the exited shell's group")
                reading = {}
                for _ in range(40):                  # the snapshot runs once a second
                    time.sleep(0.1)
                    reading = json.loads(state.read_text()) if state.exists() else {}
                    if group in (reading.get("outside_provider_groups") or []):
                        break
                self.assertIn(group, reading.get("outside_provider_groups") or [],
                              "the update gate would read no running command: %s" % reading)
                self.assertIn(detached, reading.get("outside_provider_group") or [], reading)
                os.kill(sup, signal.SIGTERM)
                for _ in range(40):
                    if not alive(detached):
                        break
                    time.sleep(0.05)
                self.assertFalse(alive(detached), "the nohup command outlived its lead")
                log = root/"reap.log"
                for _ in range(40):                  # the reap writes its line after its last pass
                    if "killed group %d" % group in (log.read_text() if log.exists() else ""):
                        break
                    time.sleep(0.05)
                self.assertIn("killed group %d" % group, log.read_text())
            finally:
                if desktop.poll() is None:
                    desktop.kill(); desktop.wait()
                if detached and alive(detached):
                    os.kill(detached, signal.SIGKILL)   # the fixture's own process, its pid captured from itself

    def test_R11_the_provider_s_argv_and_environment_equal_the_flagless_path_s(self):
        seen = {}
        for mode in ("flagless", "product"):
            with tempfile.TemporaryDirectory(prefix="supervisor argv ") as root:
                root = Path(root)
                out = root/"seen.json"
                fake = root/"provider.py"
                fake.write_text("import json,os,sys\nenv=dict(os.environ)\n"
                                "assert env.pop('RICHOS_SESSION_PID')==str(os.getpid())\n"
                                "open(sys.argv[1],'w').write(json.dumps({'argv':sys.argv[2:],'env':env}))\n")
                launcher = root/"desktop.py"
                launcher.write_text("import subprocess,sys\nsubprocess.Popen(sys.argv[1:],start_new_session=True).wait()\n")
                options = [] if mode == "flagless" else [
                    "--reap-descendants", "--reap-grace=0", "--reap-state=%s" % (root/"state.json"),
                    "--reap-log=%s" % (root/"reap.log")]
                provider = [sys.executable, str(fake), str(out), "--a-flag", "value with spaces"]
                subprocess.run([sys.executable, str(launcher), sys.executable, str(SUPERVISOR)] + options + provider,
                               env=without_operator_names(), timeout=20)
                self.assertTrue(out.exists(), "%s: the provider never started" % mode)
                seen[mode] = json.loads(out.read_text())
        self.assertEqual(seen["flagless"]["argv"], ["--a-flag", "value with spaces"], "fixture")
        self.assertEqual(seen["product"]["argv"], seen["flagless"]["argv"], "the provider's argv changed")
        self.assertEqual(seen["product"]["env"], seen["flagless"]["env"], "the provider's environment changed")

    def test_R12_a_long_lived_lease_s_record_holds_only_live_entries(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location("provider_supervisor", str(SUPERVISOR))
        mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
        rec = mod.Record(provider=100, own_group=100)
        idle = {100: (1, 100, 1000), 101: (100, 100, 1001)}          # the provider and a same-group server
        largest = 0
        for n in range(200):
            shell, child = 10_000 + 2 * n, 10_001 + 2 * n
            running = dict(idle)
            running[shell] = (100, shell, 50_000 + n)                 # a tool shell: its own group
            running[child] = (shell, shell, 60_000 + n)               # and its command
            rec.snapshot(running)
            self.assertIn(shell, rec.groups, "fixture: the running command's group was never recorded")
            rec.snapshot(dict(idle))                                  # the command has ended
            largest = max(largest, len(rec.procs) + len(rec.groups))
        # One snapshot's work is proportional to what the record holds: bounded by what is
        # ALIVE, not by every command the lease has ever run.
        self.assertEqual(sorted(rec.procs), [100, 101], "the record kept ended commands: %d entries" % len(rec.procs))
        self.assertEqual(rec.groups, {}, "the record kept ended groups: %d" % len(rec.groups))
        self.assertLessEqual(largest, 2, "one snapshot's work grew with the number of commands ever run")


    def test_R13_process_sweep_inherits_job_tmpdir_and_is_bounded(self):
        import importlib.util
        from unittest.mock import patch
        spec = importlib.util.spec_from_file_location("provider_supervisor_cleanup", str(SUPERVISOR))
        mod = importlib.util.module_from_spec(spec); spec.loader.exec_module(mod)
        with tempfile.TemporaryDirectory(prefix="nightly process cleanup ") as root:
            job = Path(root) / "home.noindex/tmp"
            job.mkdir(parents=True)
            with patch.dict(os.environ, {"TMPDIR": str(job), "RICHOS_MACHINE_SETTINGS_RESOLVED": "1"}), \
                    patch("subprocess.run", return_value=subprocess.CompletedProcess([], 0, stderr=b"")) as run:
                mod._sweep_process_scratch()
                args, kwargs = run.call_args
                self.assertIn("--process-only", args[0])
                self.assertIn("--apply", args[0])
                self.assertNotIn("env", kwargs)  # inherit the allocating job, never host defaults
                self.assertEqual(os.environ["TMPDIR"], str(job))
                self.assertGreater(kwargs["timeout"], 0)
                self.assertFalse(kwargs.get("start_new_session", False))


class _Named(unittest.TextTestResult):
    """Prints `  PASS  <id>` / `  FAIL  <id>` per test, where <id> is the method's
    second word (R1, R2, ...; S1 for the product case), so a mutation harness can
    tell "red for this reason" from "red somewhere else"."""

    @staticmethod
    def _id(test):
        parts = test._testMethodName.split("_")
        return parts[1] if len(parts) > 1 and parts[1][:1] == "R" else "S1"

    def addSuccess(self, test):
        super().addSuccess(test)
        print("  PASS  %s %s" % (self._id(test), test._testMethodName), flush=True)

    def addFailure(self, test, err):
        super().addFailure(test, err)
        print("  FAIL  %s %s" % (self._id(test), test._testMethodName), flush=True)

    def addError(self, test, err):
        super().addError(test, err)
        print("  FAIL  %s %s (error)" % (self._id(test), test._testMethodName), flush=True)


if __name__=="__main__":
    runner = unittest.TextTestRunner(resultclass=_Named, verbosity=0)
    result = runner.run(unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__]))
    if not result.wasSuccessful():
        sys.exit(1)
    # The mutation harness is run by provider-supervisor.test.sh, after this.
