#!/usr/bin/env python3
"""The app's own sweep (`scratch-reaper.sh --apply --app <data folder>`, scan_app): what the RichOS
app left behind goes, and nothing live, held or not the app's is touched (CEO §54 addendum 2).

Everything runs in a temporary sandbox: HOME, CLAUDE_CONFIG_DIR, TMPDIR and a LITERAL reaper
config naming a sandbox Claude scratch root. Liveness is real, never injected: a "claude" here is
a copy of /bin/sleep under a name no other process on the machine has, declared as the session
process name, with a real session file naming its pid and start time. Only processes this test
started are signaled, by the pids it captured.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import unittest
import uuid

HERE = Path(__file__).resolve().parent
REAPER = HERE.parent / "scratch-reaper.sh"


class AppSweep(unittest.TestCase):
    def setUp(self):
        self.root = Path(os.path.realpath(tempfile.mkdtemp(prefix="reaper-app-test.")))
        self.home = self.root / "home"
        self.config_dir = self.home / ".claude"
        (self.config_dir / "sessions").mkdir(parents=True)
        self.tmp = self.root / "tmp"
        self.tmp.mkdir()
        self.claude_root = self.root / "claude-root"
        self.data = self.home / "Library/Application Support/com.richos.app"
        self.slug = self.claude_root / "-Users-x-Library-Application-Support-com-richos-app-coordination"
        self.slug.mkdir(parents=True)
        # `ps -o comm` reports argv[0], so the real /bin/sleep started under this name is a
        # session process to the reaper (a COPY of /bin/sleep is killed at exec on macOS).
        self.name = "rapp" + uuid.uuid4().hex[:10]
        self.config = self.root / "orchestration.config"
        self.config.write_text("\n".join([
            'SCRATCH_CLAUDE_ROOTS="%s"' % self.claude_root,
            'SCRATCH_TMP_PATTERNS="none-*"',
            'SCRATCH_AGE_FLOOR_MINUTES="60"',
            'SCRATCH_NIGHTLY_DIR="%s"' % (self.root / "nightly"),
            'SCRATCH_NIGHTLY_KEEP="3"',
            'SCRATCH_NOTICE_BYTES="1"',
            'SCRATCH_SESSION_PROCESS_NAMES="%s"' % self.name,
        ]) + "\n")
        self.children = []

    def tearDown(self):
        for child in self.children:
            if child.poll() is None:
                child.kill()
                child.wait()
        shutil.rmtree(self.root, ignore_errors=True)

    # --- fixtures -----------------------------------------------------------------------------
    def hosted(self, session=None):
        """A session the app hosted: its binding record, and Claude Code's scratch for it."""
        session = session or str(uuid.uuid4())
        record = self.data / "engine-state/evidence" / session
        record.mkdir(parents=True)
        (record / "binding.json").write_text(json.dumps({"session_id": session, "role": "stu"}))
        scratch = self.slug / session
        (scratch / "tasks").mkdir(parents=True)
        (scratch / "tasks/b1.output").write_text("background command output\n" * 100)
        (scratch / "scratchpad").mkdir()
        return session, scratch

    def session_process(self):
        child = subprocess.Popen([self.name, "600"], executable="/bin/sleep")
        self.children.append(child)
        out = subprocess.run(["ps", "-o", "comm=", "-p", str(child.pid)], capture_output=True, text=True).stdout
        assert out.strip() == self.name, "the session process is not running as %s: %r" % (self.name, out)
        return child

    def live(self, session):
        """A running session process, with the session file Claude Code writes for it."""
        child = self.session_process()
        start =subprocess.run(["ps", "-o", "lstart=", "-p", str(child.pid)], capture_output=True, text=True,
                               env=dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC0")).stdout.strip()
        (self.config_dir / "sessions" / ("%d.json" % child.pid)).write_text(
            json.dumps({"pid": child.pid, "sessionId": session, "procStart": start}))
        return child

    def sweep(self):
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(self.home),
               "CLAUDE_CONFIG_DIR": str(self.config_dir), "TMPDIR": str(self.tmp),
               "SCRATCH_REAPER_CONFIG": str(self.config), "LANG": "C"}
        r = subprocess.run(["bash", str(REAPER), "--apply", "--verbose", "--app", str(self.data)],
                           capture_output=True, text=True, env=env, timeout=120)
        return r.returncode, r.stdout, r.stderr

    def log(self):
        p = self.config_dir / "state/scratch-reaper.log"
        return p.read_text() if p.exists() else ""

    # --- cases --------------------------------------------------------------------------------
    def test_an_ended_session_of_the_app_is_deleted_and_logged(self):
        _session, scratch = self.hosted()
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(scratch.exists(), out)
        self.assertIn("DELETED %s" % scratch, self.log())
        self.assertIn("class=app-session", self.log())
        self.assertIn("scope=app", out)

    def test_a_live_session_is_kept_and_the_same_one_ended_is_deleted(self):
        session, scratch = self.hosted()
        child = self.live(session)
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertTrue(scratch.exists(), "a live session's scratch was deleted:\n" + out)
        # The positive control: the same directory, its process ended, goes.
        child.kill()
        child.wait()
        rc, out, err = self.sweep()
        self.assertFalse(scratch.exists(), out + err)

    def test_a_file_held_open_inside_an_ended_session_keeps_it(self):
        _session, scratch = self.hosted()
        with open(scratch / "tasks/b1.output") as held:
            # Popen returns once the child has exec'd, so it already holds the file.
            child = subprocess.Popen(["/bin/sleep", "600"], stdin=held)
            self.children.append(child)
            rc, out, err = self.sweep()
        self.assertTrue(scratch.exists(), "a held directory was deleted:\n" + out)
        self.assertIn("holds a file open", out)

    def test_scratch_the_app_did_not_host_is_never_touched(self):
        self.hosted()
        terminal =self.slug / str(uuid.uuid4())
        (terminal / "tasks").mkdir(parents=True)
        (terminal / "tasks/b1.output").write_text("a terminal session's output\n")
        other_slug = self.claude_root / "-Users-x-some-project"
        mine = other_slug / str(uuid.uuid4())
        mine.mkdir(parents=True)
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertTrue(terminal.exists() and mine.exists(), out)
        self.assertRegex(out, r"unattributed=[1-9]")

    def test_a_running_claude_no_session_file_names_makes_newer_scratch_undecidable(self):
        _session, scratch = self.hosted()
        self.session_process()
        # Written after that process started (its start has one-second resolution).
        later = time.time() + 5
        os.utime(scratch / "tasks/b1.output", (later, later))
        rc, out, err = self.sweep()
        self.assertEqual(rc, 3, out + err)
        self.assertTrue(scratch.exists(), out)

    def test_a_checkout_inside_is_refused_by_the_wall(self):
        _session, scratch = self.hosted()
        (scratch / "repo/.git").mkdir(parents=True)
        rc, out, err = self.sweep()
        self.assertEqual(rc, 3, out + err)
        self.assertTrue(scratch.exists(), out)
        self.assertIn("wall 2", out)

    def test_a_missing_data_folder_refuses_and_deletes_nothing(self):
        _session, scratch = self.hosted()
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(self.home),
               "CLAUDE_CONFIG_DIR": str(self.config_dir), "TMPDIR": str(self.tmp),
               "SCRATCH_REAPER_CONFIG": str(self.config)}
        r = subprocess.run(["bash", str(REAPER), "--apply", "--app", str(self.root / "no-such-app")],
                           capture_output=True, text=True, env=env, timeout=120)
        self.assertEqual(r.returncode, 2, r.stdout + r.stderr)
        self.assertTrue(scratch.exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
