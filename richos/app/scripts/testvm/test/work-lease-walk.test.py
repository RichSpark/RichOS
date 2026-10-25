#!/usr/bin/env python3
"""work-lease-walk.py's verdict, with no guest: its steps are stubbed on the walk class.

(1) Nightly 74 walk: a failed final `keep` (the transcripts and rows NOT carried off the guest before
it is deleted) left the exit code 0. A keep failure is a failed walk.
(2) `after`: once the monitor was killed, one ordinary request to Stu is answered and its work
settles, its turn ends, and no "monitor stopped" row follows. It reports a turn end, never a lease close.
(3) Nightly 74 walk: the monitor probe's own `python -c` matched itself, since its source names
`stall_watch.py --monitor --jsonl`; it leaves out itself and its parent shell."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
sys.dont_write_bytecode = True

spec = importlib.util.spec_from_file_location('work_lease_walk', HERE / 'work-lease-walk.py')
wlw = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wlw)
Walk = wlw.WorkLeaseWalk
FAIL_ROW = {'from': 'stall-watch', 'text': 'STALL-WATCH: ' + wlw.FAIL_MARK + ' (its output ended).'}


def run_main(out, steps, **stubs):
    argv = ['work-lease-walk.py', 'walk-x', '--out', str(out), '--expect-sha', 'abc', '--steps', steps]
    with mock.patch.object(Walk, '__init__', lambda self, a: None), mock.patch.object(sys, 'argv', argv):
        patches = [mock.patch.object(Walk, k, v) for k, v in stubs.items()]
        for p in patches:
            p.start()
        try:
            return wlw.main()
        finally:
            for p in patches:
                p.stop()


class KeepIsPartOfTheVerdict(unittest.TestCase):
    def test_a_failed_keep_fails_the_walk(self):
        with tempfile.TemporaryDirectory() as out:
            def keep(self):
                raise wlw.StepFailed('guest.sh --pull failed')
            self.assertEqual(run_main(out, 'identity', identity=lambda self: {}, keep=keep), 1)
            rows = json.loads((Path(out) / 'report.json').read_text())['steps']
            self.assertEqual([(r['step'], r['outcome']) for r in rows], [('identity', 'PASS'), ('keep', 'FAIL')])

    def test_every_step_and_keep_passing_exits_0(self):
        with tempfile.TemporaryDirectory() as out:
            self.assertEqual(run_main(out, 'identity', identity=lambda self: {}, keep=lambda self: {'kept': 'x'}), 0)


class KeepCarriesTheStatedActionLog(unittest.TestCase):
    """Codex's acceptance case for nightly 75 needs the ARM 2 / ARM 3 verdict rows. ass-kicker writes
    them to RICHOS_SA_ENTITY_ROOT/.claude/state/stated-actions.jsonl, the app's coordination root."""

    def keep(self, present):
        w = object.__new__(Walk)
        w.vm, w.home, w.data = 'walk-x', '/g/home', '/g/home/Library/Application Support/com.richos.app'
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        w.out = Path(tmp.name)
        sent = []

        def guest(vm, command, timeout):
            sent.append(command)
            if command.startswith('if [ -e'):
                return '\n'.join(r + '\t' + p for r, p in wlw.kept_paths(w.home, w.data) if p in present)
            return ''

        def pull(argv, timeout):
            Path(argv[-1]).write_bytes(b'tgz')
        with mock.patch.object(wlw, 'guest', guest), mock.patch.object(wlw.srw.command_walk, 'command', pull):
            return w.keep(), sent

    def test_the_verdict_log_is_asked_for_and_archived(self):
        _ev, sent = self.keep({'projects', 'coordination/.claude/state'})
        self.assertIn('coordination/.claude/state', sent[0])
        tar = [c for c in sent if c.startswith('tar -czf')]
        self.assertEqual(len(tar), 1)
        self.assertIn("'/g/home/Library/Application Support/com.richos.app' coordination/.claude/state", tar[0])

    def test_a_guest_without_it_still_keeps_the_rest(self):
        ev, sent = self.keep({'projects'})
        tar = [c for c in sent if c.startswith('tar -czf')][0]
        self.assertNotIn('coordination', tar)
        self.assertEqual(ev['bytes'], 3)


class AnswersTheGitNameCardOnce(unittest.TestCase):
    """Nightly 75: the team's first save waited for the Git name card and the job never settled."""

    def walk(self, on_screen):
        w = object.__new__(Walk)
        w.facts, w.pressed = {}, []
        w.present = lambda title, role='AXButton', app=None: title in on_screen
        w.press = lambda title, role='AXButton', app=None, contains=True: w.pressed.append(title)
        w.clock = lambda: 1234.4
        w.save = lambda: None
        return w

    def test_the_card_is_answered_once_and_the_approve_still_pressed(self):
        w = self.walk({wlw.GIT_NAME_ANSWER, 'Approve Add notes.md'})
        self.assertTrue(w.approve_if_asked('Add notes.md'))
        self.assertTrue(w.approve_if_asked('Add notes.md'))
        self.assertEqual(w.pressed, [wlw.GIT_NAME_ANSWER, 'Approve Add notes.md', 'Approve Add notes.md'])
        self.assertEqual(w.facts['git_name_answered_ms'], 1234)

    def test_no_card_no_answer(self):
        w = self.walk(set())
        self.assertFalse(w.approve_if_asked('Add notes.md'))
        self.assertNotIn(wlw.GIT_NAME_ANSWER, w.pressed)
        self.assertNotIn('git_name_answered_ms', w.facts)


class After(unittest.TestCase):
    """One settled request after the kill, its session's turn ended, and no new failure row."""

    def walk(self, rows_after):
        w = object.__new__(Walk)
        w.a = mock.Mock(after_task='another job', job_within=5, idle_within=5, approvals=0, after_quiet=0)
        w.facts = {'monitor': {'pid': 1, 'session': 's1'}, 'thread': 't'}
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        w.out = Path(tmp.name)
        inbox = [[FAIL_ROW], [FAIL_ROW] + rows_after]
        w.inbox = lambda sender: inbox.pop(0) if len(inbox) > 1 else inbox[0]
        w.send = lambda text: [1000_000, 1001_000]
        w.ours = lambda sent: {'id': 'j2', 'state': 'settled', 'title': 'Add more', 'work_session': 's2', 'notices': []}
        w.lead_files = lambda: [{'session': 's2', 'turn_ended': 1002}]
        w.monitors = lambda: []
        w.approve_if_asked = lambda title: False
        return w

    def setUp(self):
        sleep = mock.patch('time.sleep', lambda s: None)
        sleep.start()
        self.addCleanup(sleep.stop)

    def test_a_settled_request_and_a_quiet_close_pass(self):
        ev = self.walk([]).after()
        self.assertEqual(ev['job']['state'], 'settled')
        self.assertEqual(ev['new_failure_rows'], [])

    def test_a_failure_row_after_the_normal_close_fails(self):
        with self.assertRaises(wlw.StepFailed):
            self.walk([FAIL_ROW]).after()

    def test_a_request_that_does_not_settle_fails(self):
        w = self.walk([])
        w.ours = lambda sent: {'id': 'j2', 'state': 'failed', 'title': 'x', 'work_session': 's2'}
        with self.assertRaises(wlw.StepFailed):
            w.after()

    def test_the_evidence_says_the_turn_ended_never_that_the_lease_closed(self):
        # Nightly 74 walk: `after` said "the Work lease closed normally" while the kept lease file still
        # said ended=false. The lead file's `turn_ended` is idle between turns (native.rs LeadSignal).
        w = self.walk([])
        ev = w.after()
        self.assertEqual(ev['turn_ended']['turn_ended'], 1002)
        written = (w.out / 'after.json').read_text()
        for said in (json.dumps(ev), written):
            self.assertNotIn('close', said.lower())
        with self.assertRaises(wlw.StepFailed) as failed:
            self.walk([FAIL_ROW]).after()
        self.assertNotIn('close', str(failed.exception).lower())

    def test_a_lease_whose_turn_never_ends_fails(self):
        w = self.walk([])
        w.lead_files = lambda: [{'session': 's2', 'turn_ended': 999}]  # before the request was sent
        w.a.idle_within = 0
        with self.assertRaises(wlw.StepFailed):
            w.after()


class ProbeLeavesOutItself(unittest.TestCase):
    def test_the_probe_and_its_shell_are_not_monitors(self):
        # Run as the guest runs it: a shell whose command line carries the probe's source. `; true`
        # keeps the shell from exec'ing python, so both the shell and python name the three words.
        shell = subprocess.Popen(['/bin/sh', '-c', 'python3 -c %s; true' % wlw.shlex.quote(wlw.PROCS)],
                                 stdout=subprocess.PIPE, text=True)
        out, _ = shell.communicate(timeout=60)
        self.assertEqual(shell.returncode, 0)
        ours = [r for r in json.loads(out) if shell.pid in (r['pid'], r['ppid'])]
        self.assertEqual(ours, [], 'the probe counted itself or its shell as a monitor')


if __name__ == '__main__':
    unittest.main()
