#!/usr/bin/env python3
"""reap-walk.py's grade, offline. The live walk is its own proof; these hold the grade to its rules.

The rows are the shape reap-guest.py's watcher writes: {"kind": "seen", name, pid, pgid, start,
t_ms} when a heartbeat has recorded itself, {"kind": "gone", name, pid, pgid, t_ms} when it ends.
"""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest

HERE = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('reap_walk', HERE / 'reap-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

T = 1_790_501_425_212


def seen(name, pid, pgid, at):
    return {'kind': 'seen', 'name': name, 'pid': pid, 'pgid': pgid, 'start': 'Sun', 't_ms': at}


def gone(name, pid, at):
    return {'kind': 'gone', 'name': name, 'pid': pid, 'pgid': pid, 't_ms': at}


BOTH = [seen('stop-bg', 700, 690, T - 9000), seen('stop-fg', 710, 705, T - 8000)]


class Grade(unittest.TestCase):
    def test_both_ended_inside_the_bound_passes_with_the_latency(self):
        evidence, failure = walk.grade_ended(BOTH + [gone('stop-bg', 700, T + 80), gone('stop-fg', 710, T + 95)],
                                             ['stop-fg', 'stop-bg'], T, 2000, {'stop-fg': [], 'stop-bg': []})
        self.assertIsNone(failure)
        self.assertEqual(evidence['commands']['stop-bg']['ended_after_ms'], 80)

    def test_a_case_whose_pids_were_not_recorded_is_refused_never_passed(self):
        evidence, failure = walk.grade_ended(BOTH[:1] + [gone('stop-bg', 700, T + 80)], ['stop-fg', 'stop-bg'], T, 2000)
        self.assertTrue(failure.startswith('REFUSED'), failure)
        self.assertIn('stop-fg', failure)

    def test_a_command_that_ended_before_the_trigger_proves_nothing_and_is_refused(self):
        _, failure = walk.grade_ended(BOTH + [gone('stop-bg', 700, T - 5), gone('stop-fg', 710, T + 50)],
                                      ['stop-fg', 'stop-bg'], T, 2000)
        self.assertTrue(failure.startswith('REFUSED'), failure)

    def test_late_still_running_or_leaving_its_group_behind_fails(self):
        _, late = walk.grade_ended(BOTH + [gone('stop-bg', 700, T + 2500), gone('stop-fg', 710, T + 90)],
                                   ['stop-fg', 'stop-bg'], T, 2000)
        self.assertIn('past the 2000 ms bound', late)
        _, running = walk.grade_ended(BOTH + [gone('stop-fg', 710, T + 90)], ['stop-fg', 'stop-bg'], T, 2000)
        self.assertIn('still running', running)
        _, left = walk.grade_ended(BOTH + [gone('stop-bg', 700, T + 80), gone('stop-fg', 710, T + 95)],
                                   ['stop-fg', 'stop-bg'], T, 2000, {'stop-fg': ['711'], 'stop-bg': []})
        self.assertIn('survived', left)

    def test_a_recycled_pid_is_not_the_recorded_command(self):
        # A `gone` row for the same name but another pid is a different process.
        _, failure = walk.grade_ended(BOTH + [gone('stop-bg', 999, T + 80), gone('stop-fg', 710, T + 95)],
                                      ['stop-fg', 'stop-bg'], T, 2000)
        self.assertIn('still running', failure)

    def test_a_command_that_finishes_by_itself_is_never_cut_short(self):
        rows = [seen('normal-fg', 800, 800, T)]
        self.assertIsNone(walk.grade_normal(rows + [gone('normal-fg', 800, T + 8050)], 'normal-fg', T + 8000)[1])
        self.assertIn('never finished', walk.grade_normal(rows + [gone('normal-fg', 800, T + 3000)], 'normal-fg', None)[1])
        self.assertTrue(walk.grade_normal([], 'normal-fg', T)[1].startswith('REFUSED'))

    def test_the_ask_is_worded_as_a_job_and_never_asks_rich_to_do_it_himself(self):
        # "tell me exactly what it prints" alone registered a `check` (the 2026-09-27 command
        # diagnosis, proof-run-1). "for me yourself" makes it Rich's own (rich.md, a6ddf48f4):
        # walk-2bfb274d19d0 had it handed to Stu and run by Rich's session as well.
        both, one = walk.ask_text('stop'), walk.ask_text('normal', background=False, seconds=8)
        for text in (both, one):
            self.assertIn('harmless test command', text)
            self.assertIn('for me with a shell tool', text)
            self.assertIn('in my Acme folder', text)
            self.assertNotIn('yourself', text)
            self.assertNotIn('New task', text)
        self.assertIn('run_in_background: true): python3 heartbeat.py beat stop-bg /tmp/reap-walk ', both)
        self.assertIn('python3 heartbeat.py beat stop-fg /tmp/reap-walk --seconds %d ' % walk.FOREGROUND_SECONDS, both)
        self.assertIn('tell me when they have finished and what they printed', both)
        self.assertIn('python3 heartbeat.py beat normal-fg /tmp/reap-walk --seconds 8 ', one)
        self.assertNotIn('normal-bg', one)
        self.assertIn('tell me when it has finished and what it printed', one)

    def test_a_job_handed_to_stu_whose_command_rich_ran_fails(self):
        # walk-2bfb274d19d0 (bundle 8cdad8794): the assignment registered at 1792664832309, and
        # both heartbeats' Bash calls came from session 45e8751c, bound as "rich", agent null.
        job = [{'id': '786d646c-45ab-498c-8520-ac105ba41722', 'kind': 'task', 'state': 'running'}]
        rich = {'role': 'rich', 'session': '45e8751c-61e7-4d3f-a48b-9ea5c07e226f', 'agent': None,
                'command': 'cd Acme && python3 heartbeat.py beat stop-bg /tmp/reap-walk'}
        stu = dict(rich, role='stu', session='stu-session')
        helper = dict(stu, agent='a1b2')
        why = walk.rich_ran_handed([stu, rich], job)
        self.assertIn("Rich's own session ran its commands", why)
        self.assertIn('786d646c', why)
        self.assertIn('45e8751c', why)
        self.assertIsNone(walk.rich_ran_handed([stu, helper], job))
        # Nothing handed to Stu: Rich running it himself is no double run (and stoppable() refuses
        # the Stop case on its own).
        self.assertIsNone(walk.rich_ran_handed([rich], []))

    def test_a_heartbeat_command_is_read_as_the_shell_splits_it(self):
        # Review rv-20261022T114810Z-cdd66038-220d: `python3 "heartbeat.py" beat` was missed.
        for ran in ('cd /h/Acme && python3 heartbeat.py beat stop-fg /tmp/reap-walk --seconds 300',
                    'python3 "heartbeat.py" beat stop-fg /tmp/reap-walk',
                    "python3  /h/Acme/heartbeat.py   'beat'  stop-bg /tmp/reap-walk",
                    'cd /h/Acme&&python3 ./heartbeat.py beat stop-bg /tmp/reap-walk',
                    'cd /h/Acme; PYTHONUNBUFFERED=1 /usr/bin/python3 heartbeat.py beat stop-bg /tmp/reap-walk &',
                    './heartbeat.py beat stop-fg /tmp/reap-walk | tee /tmp/out'):
            self.assertTrue(walk.runs_heartbeat(ran, 'stop'), ran)
        for other in ('cat heartbeat.py', 'ls /h/Acme/heartbeat.py',
                      'python3 heartbeat.py beat quit-fg /tmp/reap-walk',  # another step's
                      'echo "python3 heartbeat.py beat stop-fg is my test"', 'python3 heartbeat.py',
                      # Review rv-20261022T124912Z-7ceba023-ce38: printed, never run.
                      "printf '%s ' python3 heartbeat.py beat stop-fg /tmp/reap-walk",
                      "printf 'python3 \"heartbeat.py\" beat stop-fg'"):
            self.assertFalse(walk.runs_heartbeat(other, 'stop'), other)

    def test_the_foreground_heartbeat_fits_one_foreground_tool_call(self):
        # Claude Code's Bash tool takes at most 600000 ms per foreground call. Asked for a 900-s
        # "foreground" heartbeat, the back end ran it with run_in_background: true (guest
        # walk-1a978b4e081b, 2026-09-27), its turn ended and the assignment settled with both
        # commands still running, so there was nothing to Stop.
        self.assertLess(walk.FOREGROUND_SECONDS, 600)
        self.assertIn('--seconds %d' % walk.FOREGROUND_SECONDS, walk.ask_text('stop'))

    def test_stop_needs_the_assignment_still_open_and_refuses_otherwise(self):
        running = {'id': 'a1', 'kind': 'task', 'state': 'running', 'title': 'Run heartbeat.py'}
        settled = dict(running, state='settled')
        self.assertEqual(walk.stoppable([running]), (running, None))
        row, why = walk.stoppable([settled])
        self.assertIsNone(row)
        self.assertTrue(why.startswith('REFUSED'), why)
        self.assertIn('settled', why)
        self.assertTrue(walk.stoppable([])[1].startswith('REFUSED'))
        # The newest open task is the one pressed, never an older one.
        older = dict(running, id='a0', registered_at_ms=1)
        newer = dict(running, id='a2', registered_at_ms=2)
        self.assertEqual(walk.stoppable([older, newer])[0]['id'], 'a2')

    def test_only_heartbeats_recorded_after_the_ask_count_as_started(self):
        old = BOTH  # recorded 8-9 s before T by an earlier ask with the same tag
        self.assertFalse(walk.recorded(old, ['stop-fg', 'stop-bg'], T))
        fresh = old + [seen('stop-bg', 900, 890, T + 10), seen('stop-fg', 910, 905, T + 20)]
        self.assertTrue(walk.recorded(fresh, ['stop-fg', 'stop-bg'], T))
        self.assertFalse(walk.recorded(old + [seen('stop-bg', 900, 890, T + 10)], ['stop-fg', 'stop-bg'], T))

    def test_the_script_in_his_repository_is_the_heartbeat_alone(self):
        # Guest walk-1a978b4e081b, 2026-09-27: the back end read heartbeat.py before running it,
        # found the whole guest helper (a process-table recorder, a death watcher, a docstring
        # about the harness grading it), which is not what it was told the script does, and
        # declined. What goes into his repository is a separate file that does only `beat`.
        self.assertEqual(walk.HEARTBEAT.name, 'reap-heartbeat.py')
        source = walk.HEARTBEAT.read_text()
        for other in ('def watch', 'def record', 'def group', 'reap-walk'):
            self.assertNotIn(other, source)
        refused = subprocess.run([sys.executable, str(walk.HEARTBEAT), 'watch', '/tmp/x', '/tmp/y'],
                                 capture_output=True, text=True, timeout=10)
        self.assertEqual(refused.returncode, 2)

    def test_the_watcher_records_the_heartbeat_starting_and_ending(self):
        with tempfile.TemporaryDirectory() as d:
            beats, out = Path(d) / 'beats', Path(d) / 'watch.jsonl'
            watcher = subprocess.Popen([sys.executable, str(HERE / 'reap-guest.py'), 'watch', str(beats), str(out),
                                        '--seconds', '6'])
            try:
                ran = subprocess.run([sys.executable, str(walk.HEARTBEAT), 'beat', 'unit-fg', str(beats), '--seconds', '1'],
                                     capture_output=True, text=True, timeout=20)
                self.assertEqual(ran.returncode, 0, ran.stderr)
                self.assertIn('unit-fg is running as pid', ran.stdout)
                self.assertTrue((beats / 'unit-fg.done').exists())
                end = time.monotonic() + 5
                rows = []
                while time.monotonic() < end:
                    rows = [json.loads(r) for r in out.read_text().splitlines()] if out.exists() else []
                    if any(r['kind'] == 'gone' for r in rows):
                        break
                    time.sleep(0.1)
            finally:
                watcher.terminate()
                watcher.wait(10)
            me = json.loads((beats / 'unit-fg.id').read_text())
            self.assertEqual(sorted(me), ['name', 'pgid', 'pid', 'sid', 'start'])
            kinds = [(r['kind'], r.get('pid')) for r in rows if r['kind'] != 'watch-start']
            self.assertEqual(kinds, [('seen', me['pid']), ('gone', me['pid'])])

    def test_the_quit_question_names_one_command_or_several(self):
        # lifecycle.rs quit_question, the three forms: one command per process group outside
        # the provider's (lease_commands.rs), so the quit step's two heartbeats read "2 commands".
        self.assertTrue(walk.names_commands('You have a command Rich started still running. Quitting stops the work.'))
        self.assertTrue(walk.names_commands('You have 2 commands Rich started still running. Quitting stops the work.'))
        self.assertTrue(walk.names_commands('You have a command Rich started that may still be running.'))
        self.assertFalse(walk.names_commands('You have 1 assignment still running in the background.'))

    def test_the_claude_crash_names_the_one_live_lease_whose_last_snapshot_has_every_group(self):
        # provider-supervisor.py snapshots once a second, and a provider crash is reaped from the
        # last snapshot (G9): a group not yet in the state file is not the case being measured.
        live = {'provider': 2364, 'ended': False, 'outside_provider_groups': [3843, 4803]}
        partial = dict(live, outside_provider_groups=[3843])
        ended = dict(live, provider=1500, ended=True)
        self.assertEqual(walk.owner_of([live, {'provider': 1823, 'outside_provider_groups': []}], [3843, 4803]), live)
        self.assertIsNone(walk.owner_of([partial], [3843, 4803]))
        self.assertIsNone(walk.owner_of([ended], [3843, 4803]))
        self.assertIsNone(walk.owner_of([live, dict(live, provider=9)], [3843, 4803]))

    def test_the_trigger_is_the_press_itself_never_a_clock_read_before_ax_sh(self):
        # Guest walk-0ddfdbf8ff00: one ax.sh call took 2.63-5.62 s, and Stop's first grade read
        # the clock before it, so 3340 ms "after the trigger" was mostly the harness pressing.
        clicked = [{'meta': True}, {'clicked': True, 'pressed_at_ms': T, 'returned_at_ms': T + 40}]
        self.assertEqual(walk.pressed_at(clicked), {'pressed_at_ms': T, 'returned_at_ms': T + 40})
        with self.assertRaises(walk.StepFailed):
            walk.pressed_at([{'meta': True}, {'clicked': True}])

    def test_a_stop_he_made_reads_as_a_stop_never_as_a_failure(self):
        # Guest walk-0ddfdbf8ff00: both Stops and the Quit left "failed ... Claude channel closed
        # (child exited?)." on his timeline (fixed in work_host.rs; the walk now guards it).
        stopped = {'id': 'a1', 'state': 'interrupted', 'user_stop_requested_at_ms': 1, 'notices': [
            {'kind': 'interrupted', 'text': 'Run two heartbeat.py test commands... It was stopped. Everything it had done is kept.'}]}
        self.assertIsNone(walk.told_stopped(stopped))
        # Today's host raises no notice of its own (2002ec31b); his Stop on the record is the fact.
        self.assertIsNone(walk.told_stopped(dict(stopped, notices=[])))
        self.assertIn('not as a stop', walk.told_stopped(dict(stopped, notices=[], user_stop_requested_at_ms=None)))
        failed = {'id': 'a1', 'state': 'failed', 'detail': 'Claude channel closed (child exited?).', 'notices': [
            {'kind': 'failed', 'text': 'Run two... It stopped before it finished. Claude channel closed (child exited?).'}]}
        self.assertIn('failed', walk.told_stopped(failed))
        self.assertIn('still running', walk.told_stopped(dict(stopped, state='running')))

    def test_ps_time_is_read_past_an_hour(self):
        self.assertAlmostEqual(walk.cpu_seconds('0:01.46'), 1.46)
        self.assertAlmostEqual(walk.cpu_seconds(' 1:02:03.50\n'), 3723.5)


class Enter(unittest.TestCase):
    """enter() after a relaunch: the conversation's row is waited for before it is pressed, so a
    read the relaunched app did not answer is asked again (axwait) instead of ending the step.
    esc-20261021T205850Z-48c70d7d: the press of "Onboarding" got 'unresponsive' on AXWindows."""

    def walk_on(self, finds):
        w = walk.Walk.__new__(walk.Walk)
        w.facts, w.calls, w.pressed, w.args = {'thread_title': 'Onboarding'}, [], [], []
        def ax(mode, *a, **k):
            w.calls.append((mode, a[1] if len(a) > 1 else None))
            w.args.append(a)
            if mode == 'click':
                w.pressed.append(a[1])
                return [{'clicked': True}]
            answer = finds.pop(0) if finds else True
            if isinstance(answer, Exception):
                raise answer
            return [{'title': a[1]}] if answer else []
        w.ax = ax
        return w

    def setUp(self):
        self.sleep, self.mono = walk.adopt.time.sleep, walk.adopt.time.monotonic
        self.rsleep = walk.time.sleep
        self.now = 0.0
        def sleep(s):
            self.now += s
        walk.adopt.time.sleep = walk.time.sleep = sleep
        walk.adopt.time.monotonic = lambda: self.now

    def tearDown(self):
        walk.adopt.time.sleep, walk.adopt.time.monotonic = self.sleep, self.mono
        walk.time.sleep = self.rsleep

    def test_stop_opens_the_work_summary_by_the_chip_label_every_state_ends_with(self):
        """walk-21f83423b356: "assignment running" is one state's words; the chip's label
        always ends "Open the work summary." (main.js renderDrillChip)."""
        source = (HERE / 'reap-walk.py').read_text()
        body = source[source.index('    def stop(self'):source.index('    def quit(self')]
        self.assertIn("self.wait_for('Open the work summary')", body)
        self.assertNotIn("'assignment running'", body)
        chip = (HERE.parent.parent / 'ui' / 'main.js').read_text()
        self.assertIn('". Open the work summary."', chip)

    # The finds, in order: the home door, the conversation's composer, the conversation's
    # title in its crumb, and (only when another conversation opened) its row.

    def test_the_door_opens_the_conversation_and_no_row_is_pressed(self):
        """walk-5a4e3ff2c768: the door opened "Acme / Onboarding" with the sidebar closed."""
        w = self.walk_on([True, True, True])
        w.enter()
        self.assertEqual(w.pressed, ['Talk to Rich'])
        self.assertEqual(w.calls[-1], ('find', 'Onboarding'))
        # walk-08aeea74dab4: the crumb's words are its AXValue, so it is matched by value.
        self.assertEqual(w.args[-1][:4], ('--value', 'Onboarding', '--role', 'AXStaticText'))

    def test_the_home_door_is_waited_for_when_home_is_not_drawn_yet(self):
        """walk-45ed47dcaab0: checked once before home was drawn, the door was skipped and
        the walk waited on the home screen for a conversation row."""
        w = self.walk_on([False, False, True, True, True])
        w.enter()
        self.assertEqual(w.pressed, ['Talk to Rich'])
        self.assertEqual(w.calls[:3], [('find', 'Talk to Rich')] * 3)

    def test_an_unanswered_read_of_the_row_is_asked_again_before_the_press(self):
        unanswered = walk.StepFailed('command failed (1): ax.sh find\n{"error": "unresponsive", "detail": '
                                     '"did not answer for AXWindows (kAXErrorCannotComplete)"}')
        w = self.walk_on([True, True, False, unanswered, True])
        w.enter()
        self.assertEqual(w.pressed, ['Talk to Rich', 'Onboarding'])
        self.assertEqual(w.calls[-2:], [('find', 'Onboarding'), ('click', 'Onboarding')])


if __name__ == '__main__':
    unittest.main()
