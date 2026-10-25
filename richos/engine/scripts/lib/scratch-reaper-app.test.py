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
import sys
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

    def live(self, session, registry=None):
        """A running session process, with the session file Claude Code writes for it, in the
        default registry or in an added account's (`registry`)."""
        child = self.session_process()
        start = subprocess.run(["ps", "-o", "lstart=", "-p", str(child.pid)], capture_output=True, text=True,
                               env=dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC0")).stdout.strip()
        registry = registry or (self.config_dir / "sessions")
        registry.mkdir(parents=True, exist_ok=True)
        (registry / ("%d.json" % child.pid)).write_text(
            json.dumps({"pid": child.pid, "sessionId": session, "procStart": start}))
        return child

    def resumed_earlier(self, scratch):
        """The scratch was last written before the live process started: a resumed session's."""
        old = time.time() - 3600
        for p in [scratch] + list(scratch.rglob("*")):
            os.utime(p, (old, old))

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

    # Review rv-20261025T000521Z-2fe81615-e0f2 #1, its fixture with real processes: an added
    # Claude account runs in its own configuration folder, so its live, resumed session registers
    # there; its scratch, older than the resumed process, was planned DELETE.
    def test_a_live_session_in_an_added_accounts_registry_is_kept(self):
        session, scratch = self.hosted()
        self.live(session, self.data / "claude-accounts/2/sessions")
        self.resumed_earlier(scratch)
        rc, out, err = self.sweep()
        self.assertTrue(scratch.exists(), "a live resumed session's scratch was deleted:\n" + out)

    def test_a_live_session_in_a_listed_account_folder_is_kept(self):
        session, scratch = self.hosted()
        folder = self.home / "claude-accounts/3"
        (self.data / "claude-accounts.json").write_text(json.dumps(
            {"accounts": [{"id": "1", "label": "Home"}, {"id": "3", "label": "Work", "folder": str(folder)}]}))
        self.live(session, folder / "sessions")
        self.resumed_earlier(scratch)
        rc, out, err = self.sweep()
        self.assertTrue(scratch.exists(), "a live resumed session's scratch was deleted:\n" + out)

    def test_a_session_a_running_command_line_carries_is_kept_with_no_registry(self):
        session, scratch = self.hosted()
        self.resumed_earlier(scratch)
        resumed = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(600)", "--resume", session])
        self.children.append(resumed)
        rc, out, err = self.sweep()
        self.assertTrue(scratch.exists(), "a session on a running command line was deleted:\n" + out)
        self.assertIn("a running process names this session", out)

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

    # --- app-profile: a lease's engine-profiles/<id> and <id>-standing --------------------------
    def profile(self, owner=None):
        """A lease's two folders, with the owner record the app writes (or none)."""
        lease = str(uuid.uuid4())
        base = self.data / "engine-profiles" / lease
        (base / "agents").mkdir(parents=True)
        (base / "agents/rich.md").write_text("an agent\n")
        standing = base.with_name(lease + "-standing")
        (standing / "agents").mkdir(parents=True)
        if owner is not None:
            pid, start = owner
            (base / ".richos-owner.json").write_text(json.dumps({"pid": pid, "procStart": start}))
        return base, standing

    def started(self, pid):
        return subprocess.run(["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True, text=True,
                              env=dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC0")).stdout.strip()

    def sweep_with(self, *extra):
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": str(self.home),
               "CLAUDE_CONFIG_DIR": str(self.config_dir), "TMPDIR": str(self.tmp),
               "SCRATCH_REAPER_CONFIG": str(self.config), "LANG": "C"}
        r = subprocess.run(["bash", str(REAPER), "--apply", "--verbose", "--app", str(self.data), *extra],
                           capture_output=True, text=True, env=env, timeout=120)
        return r.returncode, r.stdout, r.stderr

    def test_a_profile_whose_app_has_ended_goes_with_its_standing_copy(self):
        app = subprocess.Popen(["/bin/sleep", "600"])
        self.children.append(app)
        start = self.started(app.pid)
        app.kill()
        app.wait()
        base, standing = self.profile((app.pid, start))
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(base.exists() or standing.exists(), out)
        self.assertIn("class=app-profile", self.log())

    def test_a_profile_whose_app_runs_is_kept_unless_that_app_is_quitting(self):
        app = subprocess.Popen(["/bin/sleep", "600"])
        self.children.append(app)
        base, standing = self.profile((app.pid, self.started(app.pid)))
        rc, out, err = self.sweep()
        self.assertTrue(base.exists() and standing.exists(), out)
        self.assertIn("is running and retires it itself", out)
        rc, out, err = self.sweep_with("--app-exiting", str(app.pid))
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(base.exists() or standing.exists(), out)

    def test_a_profile_a_running_process_names_is_kept_whatever_its_owner(self):
        base, standing = self.profile((999999, "Thu Jan  1 00:00:00 1970"))
        user = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(600)", "--plugin-dir", str(base)])
        self.children.append(user)
        rc, out, err = self.sweep()
        self.assertTrue(base.exists() and standing.exists(), out)
        self.assertIn("a running process starts from it", out)

    def test_a_profile_without_an_owner_record_waits_for_the_floor(self):
        base, standing = self.profile(None)
        rc, out, err = self.sweep()
        self.assertTrue(base.exists() and standing.exists(), out)
        old = time.time() - 2 * 3600
        for folder in (base, standing):
            for p in [folder] + list(folder.rglob("*")):
                os.utime(p, (old, old))
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(base.exists() or standing.exists(), out)

    def ended_app(self):
        """An app process that has ended: its pid and start, as the owner record names them."""
        app = subprocess.Popen(["/bin/sleep", "600"])
        self.children.append(app)
        start = self.started(app.pid)
        app.kill()
        app.wait()
        return app.pid, start

    # Review rv-20261025T021219Z-91f589b0-af71 #1: a sweep out of budget took a lease's base
    # profile and left its -standing copy for the next one. The owner record was only in the
    # base, so the copy then read as "no owner record" and waited for the 60-minute floor. The
    # app now writes the record into both folders (engine_profile.rs), and each is decided on
    # its own.
    def test_a_standing_copy_whose_base_is_gone_goes_on_its_own_owner_record(self):
        pid, start = self.ended_app()
        base, standing = self.profile((pid, start))
        (standing / ".richos-owner.json").write_text(json.dumps({"pid": pid, "procStart": start}))
        shutil.rmtree(base)     # what the earlier, bounded sweep took
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(standing.exists(), out)
        self.assertIn("DELETED %s" % standing, self.log())

    def test_a_backlog_that_splits_a_profile_pair_clears_it_across_sweeps(self):
        """The review's fixture, its cost model unchanged: seven ended sessions, then one pair,
        0.51 s per open-file query on a clock only those queries move."""
        import importlib.util
        from unittest import mock
        sys.path.insert(0, str(HERE))
        spec = importlib.util.spec_from_file_location("reaper_split_pair", HERE / "scratch-reaper.py")
        reaper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reaper)

        class Ended:
            def process_start(self, pid):
                return "gone", ""

            def session_state(self, session):
                return "ended", "no process"

        for _ in range(7):
            self.hosted()
        lease = "11111111-1111-1111-1111-111111111111"
        base = self.data / "engine-profiles" / lease
        standing = base.with_name(lease + "-standing")
        for folder in (base, standing):
            (folder / "agents").mkdir(parents=True)
            (folder / "agents/rich.md").write_text("agent\n")
            (folder / reaper.OWNER_FILE).write_text(json.dumps({"pid": 999999, "procStart": "ended"}))
        seen = []
        env = {"HOME": str(self.home), "CLAUDE_CONFIG_DIR": str(self.config_dir), "TMPDIR": str(self.tmp)}
        with mock.patch.dict(os.environ, env), \
                mock.patch.object(reaper, "claude_processes", return_value=[]):
            for _sweep in range(3):
                clock = [1000.0]

                def holder(path):
                    clock[0] += 0.51
                    return ""
                r = reaper.Reaper({"age_floor_minutes": 60, "claude_roots": [str(self.claude_root)]}, Ended())
                r.command_lines = lambda: []
                r.holder = holder
                r.collect_test_instances = lambda paths, stamp: []
                r._record_failures = lambda failures, stamp: None
                r.app_until = 1000.0 + reaper.APP_BUDGET_SECONDS / 2
                r.app_apply_until = 1000.0 + reaper.APP_BUDGET_SECONDS
                with mock.patch.object(reaper.time, "time", lambda: clock[0]):
                    r.scan_app({"data": str(self.data)})
                    r.apply(str(self.root / "deletions.log"))
                seen.append((base.exists(), standing.exists(),
                             [e.why for e in r.entries if e.path == str(standing)]))
                if not base.exists() and not standing.exists():
                    break
        self.assertFalse(base.exists() or standing.exists(), "base, standing, why after each sweep: %s" % seen)

    # --- app-voice: a spoken turn's recording a killed app left -----------------------------------
    def recording(self, pid, folder=None):
        """utt-<pid>.wav, one second of 16 kHz mono silence (32044 bytes, the size the review
        measured), as stt.rs transcribe_inner writes it."""
        import wave
        folder = folder or (self.data / "voice-scratch")
        folder.mkdir(parents=True, exist_ok=True)
        path = folder / ("utt-%d.wav" % pid)
        with wave.open(str(path), "wb") as wav:
            wav.setnchannels(1)
            wav.setsampwidth(2)
            wav.setframerate(16000)
            wav.writeframes(b"\0\0" * 16000)
        return path

    # Review rv-20261025T020709Z-8339d95d-d408 #1: a force-quit during a spoken turn leaves
    # voice-scratch/utt-<pid>.wav, and no sweep visited voice-scratch.
    def test_a_recording_whose_app_has_ended_goes(self):
        pid, _start = self.ended_app()
        wav = self.recording(pid)
        self.assertEqual(wav.stat().st_size, 32044)
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(wav.exists(), out)
        self.assertIn("DELETED %s" % wav, self.log())
        self.assertIn("class=app-voice", self.log())

    def test_a_recording_whose_app_runs_is_kept_unless_that_app_is_quitting(self):
        app = subprocess.Popen(["/bin/sleep", "600"])
        self.children.append(app)
        wav = self.recording(app.pid)
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertTrue(wav.exists(), out)
        self.assertIn("is running: it may be recording or decoding it now", out)
        rc, out, err = self.sweep_with("--app-exiting", str(app.pid))
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(wav.exists(), out)

    def test_a_recording_older_than_the_process_now_on_its_pid_goes(self):
        app = subprocess.Popen(["/bin/sleep", "600"])
        self.children.append(app)
        wav = self.recording(app.pid)
        old = time.time() - 3600
        os.utime(wav, (old, old))
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(wav.exists(), out)
        self.assertIn("started after it was written", self.log())

    def test_a_recording_a_process_holds_open_is_kept(self):
        pid, _start = self.ended_app()
        wav = self.recording(pid)
        with open(wav, "rb") as held:
            reader = subprocess.Popen(["/bin/sleep", "600"], stdin=held)
        self.children.append(reader)
        rc, out, err = self.sweep()
        self.assertTrue(wav.exists(), out)
        self.assertIn("holds a file open inside it", out)

    def test_other_files_in_voice_scratch_are_not_the_sweeps(self):
        folder = self.data / "voice-scratch"
        folder.mkdir(parents=True)
        other = folder / "notes.wav"
        other.write_bytes(b"RIFF")
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertTrue(other.exists(), out)

    def test_a_speech_warmup_folder_whose_app_has_ended_goes_and_a_live_ones_stays(self):
        """stt.rs warm(): $TMPDIR/richos-speech-warm-<pid>-<nanos>/, holding utt-<pid>.wav and
        the decoder's output, removed when the warm-up returns. A force-quit during it, at an
        app start, leaves the folder."""
        (self.data / "engine-state").mkdir(parents=True)
        pid, _start = self.ended_app()
        dead = self.tmp / ("richos-speech-warm-%d-1792908145000000000" % pid)
        self.recording(pid, dead)
        (dead / "decoder.stdout").write_text("")
        app = subprocess.Popen(["/bin/sleep", "600"])
        self.children.append(app)
        live = self.tmp / ("richos-speech-warm-%d-1792908145000000001" % app.pid)
        self.recording(app.pid, live)
        theirs = self.tmp / "richos-speech-warm-notes"
        theirs.mkdir()
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(dead.exists(), out)
        self.assertTrue(live.exists() and theirs.exists(), out)

    def test_an_unreadable_voice_scratch_is_undecidable_never_clean(self):
        pid, _start = self.ended_app()
        wav = self.recording(pid)
        folder = wav.parent
        folder.chmod(0)
        try:
            rc, out, err = self.sweep()
        finally:
            folder.chmod(0o755)
        self.assertEqual(rc, 3, out + err)
        self.assertTrue(wav.exists(), out)
        self.assertIn("could not be read", out)

    def test_an_unreadable_engine_profiles_folder_is_undecidable_never_clean(self):
        pid, start = self.ended_app()
        base, _standing = self.profile((pid, start))
        root = base.parent
        root.chmod(0)
        try:
            rc, out, err = self.sweep()
        finally:
            root.chmod(0o755)
        self.assertEqual(rc, 3, out + err)
        self.assertTrue(base.exists(), out)
        self.assertIn("could not be read", out)

    # --- app-dictation: one dictation's scratch folder a killed dictation tool left ---------------
    def dictation_scratch(self, owner):
        """<data>/dictation-scratch/<24 hex>/, as the tool's scratch.rs makes it: its owner record
        (the tool's pid and start), the recording and the decoder's two output files."""
        folder = self.data / "dictation-scratch" / uuid.uuid4().hex[:24]
        folder.mkdir(parents=True)
        pid, start = owner
        (folder / ".richos-owner.json").write_text(json.dumps({"pid": pid, "procStart": start}))
        self.recording(pid, folder)
        (folder / "decoder.stdout").write_text(" Hello there.\n")
        (folder / "decoder.stderr").write_text("")
        return folder

    # The handover of cc/echo-opus-appclean1: a force-quit of the dictation tool while it decodes
    # leaves dictation-scratch/<random>, which carried no owner record and no sweep visited.
    def test_dictation_scratch_goes_when_its_tool_has_ended_and_stays_while_it_runs_or_is_held(self):
        abandoned = self.dictation_scratch(self.ended_app())
        tool = subprocess.Popen(["/bin/sleep", "600"])
        self.children.append(tool)
        live = self.dictation_scratch((tool.pid, self.started(tool.pid)))
        held_folder = self.dictation_scratch(self.ended_app())
        with open(held_folder / "decoder.stdout", "ab") as out_file:
            decoder = subprocess.Popen(["/bin/sleep", "600"], stdout=out_file)
        self.children.append(decoder)
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(abandoned.exists(), out)
        self.assertIn("DELETED %s" % abandoned, self.log())
        self.assertIn("class=app-dictation", self.log())
        self.assertTrue(live.exists(), out)
        self.assertIn("the dictation tool that made it (pid %d) is running" % tool.pid, out)
        self.assertTrue(held_folder.exists(), out)
        self.assertIn("holds a file open inside it", out)

    # --- app-mcp-log: Claude Code's MCP logs for the app's coordination folder ---------------------
    def mcp_log(self, folder, name="2026-10-25T00-00-00-000Z.txt"):
        slug = "".join(c if c.isalnum() else "-" for c in str(folder))
        log = self.home / "Library/Caches/claude-cli-nodejs" / slug / "mcp-logs-richos-status" / name
        log.parent.mkdir(parents=True, exist_ok=True)
        log.write_text("[]\n")
        return log

    def test_an_mcp_log_no_running_claude_can_write_goes_and_another_projects_stays(self):
        self.hosted()
        mine = self.mcp_log(self.data / "coordination")
        theirs = self.mcp_log(self.root / "some-project")
        rc, out, err = self.sweep()
        self.assertEqual(rc, 0, out + err)
        self.assertFalse(mine.exists(), out)
        self.assertTrue(theirs.exists(), out)
        self.assertIn("class=app-mcp-log", self.log())

    def test_an_mcp_log_written_after_a_running_claude_started_stays(self):
        self.hosted()
        self.session_process()
        log = self.mcp_log(self.data / "coordination")
        later = time.time() + 5
        os.utime(log, (later, later))
        rc, out, err = self.sweep()
        self.assertTrue(log.exists(), out)
        self.assertIn("a running session may still write it", out)

    # Review rv-20261025T010610Z-2cc154e7-1d03 #2, its fixture: the app stops waiting for a
    # sweep at 10 s (app_cleanup.rs BOUND), and a backlog whose scan alone took longer
    # survived every sweep, each one starting the same scan again. Its cost model, not a
    # timing: 0.51 s per open-file query (the lsof time measured in the reaper's source), on
    # a clock that only those queries move, so the verdict cannot depend on this Mac's load.
    def test_a_backlog_longer_than_the_apps_bound_clears_across_sweeps(self):
        import importlib.util
        from unittest import mock
        sys.path.insert(0, str(HERE))
        spec = importlib.util.spec_from_file_location("reaper_under_test", HERE / "scratch-reaper.py")
        reaper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(reaper)

        class Ended:
            def process_start(self, pid):
                return "gone", ""

            def session_state(self, session):
                return "ended", "no process"

        paths = [self.hosted()[1] for _ in range(24)]
        remaining = []
        env = {"HOME": str(self.home), "CLAUDE_CONFIG_DIR": str(self.config_dir), "TMPDIR": str(self.tmp)}
        with mock.patch.dict(os.environ, env), \
                mock.patch.object(reaper, "claude_processes", return_value=[]):
            for _sweep in range(6):
                clock = [1000.0]

                def holder(path):
                    clock[0] += 0.51
                    self.assertLess(clock[0] - 1000.0, 10.0, "the sweep ran past the app's bound")
                    return ""
                r = reaper.Reaper({"age_floor_minutes": 60, "claude_roots": [str(self.claude_root)]}, Ended())
                r.command_lines = lambda: []
                r.holder = holder
                r.collect_test_instances = lambda paths, stamp: []
                r._record_failures = lambda failures, stamp: None
                # What main() sets for --app, from the reaper's own default budget.
                budget = getattr(reaper, "APP_BUDGET_SECONDS", 8.0)
                r.app_until = 1000.0 + budget / 2
                r.app_apply_until = 1000.0 + budget
                with mock.patch.object(reaper.time, "time", lambda: clock[0]):
                    r.scan_app({"data": str(self.data)})
                    r.apply(str(self.root / "deletions.log"))
                remaining.append(sum(p.exists() for p in paths))
                if not remaining[-1]:
                    break
        self.assertEqual(remaining[-1], 0, "remaining after each sweep: %s" % remaining)
        self.assertTrue(all(a > b for a, b in zip([24] + remaining, remaining)),
                        "a sweep deleted nothing: remaining after each sweep %s" % remaining)

    def test_a_sweep_out_of_time_defers_and_says_so(self):
        _session, scratch = self.hosted()
        rc, out, err = self.sweep_with("--app-budget", "0")
        self.assertEqual(rc, 0, out + err)
        self.assertTrue(scratch.exists(), out)
        self.assertIn("deferred=1", out)
        rc, out, err = self.sweep_with()
        self.assertFalse(scratch.exists(), out + err)

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
