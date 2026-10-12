#!/usr/bin/env python3
"""nightly-engine.test.py — the nightly engine run runs every engine unit with every mutation pass,
as its own job: never beside an app nightly, from a checkout of its own that it removes, and a
failure reaches the lead through escalate.sh.

Every case is a throwaway repository holding the real nightly-engine.py and stand-ins for the
engine's unit inventory, proof-run.py and escalate.sh. N4 hands the fields a failing run writes
to the real escalate.sh, with a scratch ledger. Nothing here runs an engine unit.
"""
import fcntl
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent
SCRIPT = Path(os.environ.get("NIGHTLY_ENGINE_UNDER_TEST", HERE / "nightly-engine.py"))
ENGINE = HERE.parents[1] / "engine"

UNITS = """#!/usr/bin/env bash
# Stand-in inventory: three units, one a scoped section.
printf 'scripts/a.test.sh\\tsuite\\t0\\t12\\n'
printf 'scripts/b.test.sh\\tsuite\\t0\\t30\\n'
printf 'scripts/hooks/c.test.sh:SEC\\tsection\\t3\\t5\\n'
"""
# Stand-in runner: records its argv, the switches it was given and the checkout it ran in, then
# writes summary.json; a unit named in FIXTURE_FAIL fails.
PROOF_RUN = """import json, os, sys
from pathlib import Path
log = Path(os.environ["FIXTURE_LOG"])
with log.open("a") as out:
    out.write(json.dumps({"argv": sys.argv[1:], "cwd": os.getcwd(),
                          "passes": os.environ.get("RICHOS_MUTATION_PASSES"),
                          "fourteen": os.environ.get("RICHOS_FOURTEEN_MUTANTS")}) + "\\n")
if os.environ.get("FIXTURE_HOLD"):  # a long run that ignores SIGTERM: only the SIGKILL ends it
    import signal, time
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    Path(os.environ["FIXTURE_PIDFILE"]).write_text(str(os.getpid()))
    time.sleep(float(os.environ["FIXTURE_HOLD"]))
lines = Path(sys.argv[sys.argv.index("--commands") + 1]).read_text().splitlines()
bad = os.environ.get("FIXTURE_FAIL", "")
rows = [{"check": "engine " + l.split()[-1], "result": "failed" if bad and l.endswith(bad) else "passed"}
        for l in lines]
with open(sys.argv[sys.argv.index("--summary-out") + 1], "w") as out:
    json.dump({"checks": rows}, out)
sys.exit(1 if any(r["result"] != "passed" for r in rows) else 0)
"""
ESCALATE = """#!/usr/bin/env bash
# Stand-in escalate.sh: records its arguments and the fields file it was given.
fields=""
args=("$@")
for ((i = 0; i < ${#args[@]}; i++)); do
    [ "${args[$i]}" = --fields ] && fields="${args[$((i + 1))]}"
done
{ printf 'ARGS %s\\n' "$*"; cat "$fields"; printf '\\n'; } >> "$FIXTURE_ESCALATIONS"
"""


class NightlyEngine(unittest.TestCase):
    def setUp(self):
        ssd = Path("/Volumes/E1TB/tmp") if os.path.ismount("/Volumes/E1TB") else None
        self.tmp = tempfile.TemporaryDirectory(prefix="nightly-engine-test-", dir=ssd)
        self.base = Path(self.tmp.name)
        self.repo = self.base / "repo"
        self.state = self.base / "state"
        self.app_state = self.base / "app-state"
        self.app_state.mkdir()
        self.log = self.base / "proof-run.jsonl"
        self.escalations = self.base / "escalations.txt"
        (self.base / "gitconfig").write_text("[user]\n\tname = Fixture\n\temail = fixture@example.invalid\n"
                                              "[init]\n\tdefaultBranch = main\n")
        self.env = {k: v for k, v in os.environ.items()
                    if not k.startswith(("GIT_", "RICHOS_MUTATION", "RICHOS_FOURTEEN"))}
        self.env.update(GIT_CONFIG_GLOBAL=str(self.base / "gitconfig"), GIT_CONFIG_NOSYSTEM="1",
                        FIXTURE_LOG=str(self.log), FIXTURE_ESCALATIONS=str(self.escalations))
        for rel, text in (("richos/app/scripts/nightly-engine.py", SCRIPT.read_text()),
                          ("richos/app/scripts/lib/machine_storage.py", (HERE / "lib/machine_storage.py").read_text()),
                          ("richos/engine/scripts/lib/machine_settings.py", (HERE.parents[1] / "engine/scripts/lib/machine_settings.py").read_text()),
                          ("richos/app/scripts/proof-run.py", PROOF_RUN),
                          ("richos/engine/scripts/ci-units.sh", UNITS),
                          ("richos/engine/scripts/escalate.sh", ESCALATE)):
            path = self.repo / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        self.git("init", "-q")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "base")

    def tearDown(self):
        self.tmp.cleanup()

    def git(self, *args):
        done = subprocess.run(["git", *args], cwd=self.repo, env=self.env, capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        return done.stdout

    def run_job(self, *extra, env=None):
        return subprocess.run([sys.executable, str(self.repo / "richos/app/scripts/nightly-engine.py"),
                               "--state-dir", str(self.state), "--app-state-dir", str(self.app_state), *extra],
                              cwd=self.repo, env={**self.env, **(env or {})}, capture_output=True, text=True,
                              timeout=300)

    def calls(self):
        return [json.loads(l) for l in self.log.read_text().splitlines()] if self.log.exists() else []

    def worktrees(self):
        return [l for l in self.git("worktree", "list", "--porcelain").splitlines() if l.startswith("worktree ")]

    def test_every_unit_runs_with_every_mutation_pass_and_no_cap_in_a_checkout_it_removes(self):
        out = self.run_job()
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        [call] = self.calls()
        commands = Path(call["argv"][call["argv"].index("--commands") + 1]).read_text().splitlines()
        self.assertEqual([c.split()[-1] for c in commands],
                         ["scripts/a.test.sh", "scripts/b.test.sh", "scripts/hooks/c.test.sh:SEC"])
        self.assertTrue(all(c.startswith("cd richos/engine && bash scripts/") for c in commands), commands)
        self.assertEqual((call["passes"], call["fourteen"]), ("1", "1"))
        self.assertNotIn("--cap", call["argv"])
        self.assertNotIn("--run-cap", call["argv"])
        # It ran in a checkout of its own at main's commit, which is gone afterwards.
        self.assertNotEqual(Path(call["cwd"]).resolve(), self.repo.resolve())
        self.assertEqual(len(self.worktrees()), 1, self.worktrees())
        self.assertFalse(Path(call["cwd"]).exists())
        self.assertFalse(self.escalations.exists())
        self.assertIn("PASSED: 3 unit(s)", out.stdout)

    def test_a_failure_is_raised_for_the_lead_naming_the_unit_and_the_checkout_is_removed(self):
        out = self.run_job(env={"FIXTURE_FAIL": "scripts/b.test.sh"})
        self.assertEqual(out.returncode, 1, out.stdout + out.stderr)
        text = self.escalations.read_text()
        self.assertIn("--teammate nightly-engine", text)
        fields = json.loads(text.splitlines()[1])
        self.assertEqual(fields["state"], "work-complete")
        self.assertEqual(fields["for"], "lead")
        self.assertIn("1 check(s) did not pass", fields["title"])
        self.assertIn("engine scripts/b.test.sh (failed)", fields["question"])
        self.assertEqual(len(self.worktrees()), 1, self.worktrees())

    def test_it_never_starts_beside_an_app_nightly(self):
        lock = open(self.app_state / "release.lock", "wb")
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            out = self.run_job()
        finally:
            lock.close()
        self.assertEqual(out.returncode, 75, out.stdout + out.stderr)
        self.assertIn("NOT STARTED", out.stdout)
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.state.exists())
        # Released, the same lock file lets it run.
        self.assertEqual(self.run_job().returncode, 0)

    def test_an_app_nightly_that_starts_after_it_does_stops_the_run(self):
        # V01: the exclusion held in one launch order only. The app nightly never looks for this job,
        # so this job must notice the app nightly's lock appearing mid-run and yield.
        pidfile = self.base / "runner.pid"
        job = subprocess.Popen([sys.executable, str(self.repo / "richos/app/scripts/nightly-engine.py"),
                                "--state-dir", str(self.state), "--app-state-dir", str(self.app_state),
                                "--yield-poll", "0.2", "--stop-grace", "1"],
                               cwd=self.repo, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                               env={**self.env, "FIXTURE_HOLD": "120", "FIXTURE_PIDFILE": str(pidfile)})
        lock = None
        try:
            for _ in range(200):
                if pidfile.exists() and pidfile.read_text().strip():
                    break
                time.sleep(0.1)
            self.assertTrue(pidfile.exists(), "the stand-in runner never started")
            runner = int(pidfile.read_text())
            lock = open(self.app_state / "release.lock", "wb")  # the app nightly starts now
            fcntl.flock(lock, fcntl.LOCK_EX)
            out, _ = job.communicate(timeout=60)
        finally:
            if lock:
                lock.close()
            if job.poll() is None:
                job.kill()
                job.wait()
        self.assertEqual(job.returncode, 75, out)
        self.assertIn("YIELDED", out)
        with self.assertRaises(ProcessLookupError):  # the runner (which ignored SIGTERM) is gone
            os.kill(runner, 0)
        self.assertFalse(self.escalations.exists())
        self.assertEqual(len(self.worktrees()), 1, self.worktrees())

    def test_the_fields_a_failing_run_writes_are_accepted_by_the_real_escalate(self):
        # The stand-in above proves what is sent; this proves the real channel takes it.
        real = ENGINE / "scripts/escalate.sh"
        (self.repo / "richos/engine/scripts/escalate.sh").write_text(
            "#!/usr/bin/env bash\nexec bash %s \"$@\"\n" % shlex.quote(str(real)))
        self.git("add", "-A")
        self.git("commit", "-q", "-m", "the real escalation channel")
        ledger = self.base / "ledger.jsonl"
        out = self.run_job(env={"FIXTURE_FAIL": "scripts/a.test.sh", "RICHOS_ESCALATION_LEDGER": str(ledger)})
        self.assertEqual(out.returncode, 1, out.stdout + out.stderr)
        self.assertNotIn("ESCALATION NOT DELIVERED", out.stdout)
        rows = [json.loads(l) for l in ledger.read_text().splitlines() if l.strip()]
        self.assertEqual(len(rows), 1, rows)
        self.assertIn("1 check(s) did not pass", json.dumps(rows[0]))

    def test_run_folders_are_bounded(self):
        for _ in range(5):
            self.assertEqual(self.run_job().returncode, 0)
        runs = [p for p in self.state.iterdir() if p.is_dir()]
        self.assertEqual(len(runs), 3, runs)


if __name__ == "__main__":
    result = unittest.main(exit=False, verbosity=2).result
    sys.exit(0 if result.wasSuccessful() else 1)
