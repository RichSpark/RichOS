#!/usr/bin/env python3
"""stu-reminder-walk.py — in the regular app, Stu's unfinished-work reminder reaches Stu.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      stu-reminder-walk.py --out DIR --expect-sha SHA [--reminder-seconds SECONDS]

THE QUESTION (one-app plan 2026-10-21, section 2 step 1; CEO ruling 137). Stu's stall-watch
(`native.rs` StallWatch, `--monitor --jsonl`) reminds an idle Stu of finished work still waiting,
but only once it knows Stu's turn ended. Stu runs `claude --print`, whose transcript never records
a turn's end, so the lease now writes the signal itself (native.rs LeadSignal: the file
`engine-state/stall-watch-lead/<session>.json`, named to the monitor as RICHOS_WATCH_LEAD). This
walk shows the reminder reaching Stu in the real, regular app, on fixture data in a guest.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  identity    the running app says it was built from --expect-sha
  first-run   adopt-walk.py's: company "Acme" with a folder that is a Git repository
  relaunch    the app quit and opened again with STALL_WATCH_PENDING_WORK_SECONDS=--reminder-seconds
              in its environment (this guest only; 12 hours otherwise), so Stu's monitor has it
  job         typed: a job for the back end (Stu); the walk goes on once Stu has begun it (its own
              review and land are not this walk's question)
  idle        Stu's lead file says his turn ended (`turn_ended`): the signal this walk is about
  reminder    the UNFINISHED-WORK reminder is in Stu's own inbox (<app state>/messages/<company>/
              <conversation>/stu.jsonl, from stall-watch). The job Stu was given is waited for first;
              only if its reminder does not come within the interval plus a stall-watch poll is a
              finished teammate, waiting for its review, registered by hand, and the evidence says
              which of the two produced the reminder.
  (`connect`, command-walk.py's, is not in the default run: its Settings menu path is stale on main.)

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed
OPEN = command_walk.OPEN

STEPS = ['identity', 'first-run', 'relaunch', 'job', 'idle', 'reminder']
# A job, as worker-approve-walk.py words one: it is registered as an assignment and run by Stu.
JOB = ('Please add a file named notes.md whose whole content is the line "Notes for the walk test." '
       'to the Acme repository and commit it, and land it.')
REMINDER_MARK = 'UNFINISHED-WORK'
WAIT_MARK = 'waiting for its review'


class StuReminderWalk(command_walk.CommandWalk):
    def decline_first_run_sheets(self, tries=5, seconds=150):
        """adopt-walk.py's, for a development bundle: it carries no engine to install (the guest's
        engine comes in through RICHOS_ENGINE_DIR), so its setup sheet says so and offers only
        `Close`, which is pressed by its exact title (never the window's own close button)."""
        presses = 0
        end = time.monotonic() + seconds
        while presses < tries and time.monotonic() < end:
            if self.present('Add this company'):
                break
            try:
                if self.present('Not now'):
                    self.press('Not now')
                elif self.present('Close'):
                    self.press('Close', contains=False)
                else:
                    time.sleep(1)
                    continue
            except StepFailed as exc:
                if 'command failed (124)' not in str(exc):  # a read past the 20 s deadline: read again
                    raise
                continue
            presses += 1
            time.sleep(3)
        return presses

    def ax(self, mode, *args, app=None, timeout=40):
        """Walk.ax, read again after a read past ax.sh's own 20 s deadline (exit 124: a busy host,
        not a missing grant). The presses this walk makes are each safe to repeat."""
        for attempt in (1, 2, 3):
            try:
                return super().ax(mode, *args, app=app, timeout=timeout)
            except StepFailed as exc:
                if 'command failed (124)' not in str(exc) or attempt == 3:
                    raise

    def present(self, title, role='AXButton', app=None):
        """Walk.present, where a read past ax.sh's 20 s deadline on a busy host (exit 124: the
        read's own deadline, not a missing grant) is "not seen yet", so the caller's loop reads again."""
        try:
            return super().present(title, role, app)
        except StepFailed as exc:
            if 'command failed (124)' in str(exc):
                return False
            raise

    def wait_for(self, title, role='AXButton', app=None, seconds=30):
        """Walk.wait_for, with the room a busy host needs: a read can take its whole 20 s deadline."""
        return super().wait_for(title, role, app, max(seconds, 120))

    def after_relaunch(self):
        """The relaunch opens on the home screen, whose live canvas keeps every accessibility read
        past ax.sh's 20 s deadline (CommandWalk.relaunch's first composer read ended 124 on it,
        2026-10-20 and 2026-10-21). As asked-again-walk.py does: the home screen is recognized by
        OCR on a frame, and Enter, its own key for Talk to Rich, goes in; the composer is read after."""
        began = time.monotonic()
        while time.monotonic() - began < self.a.relaunch_within:
            self.shot('home-after-relaunch.png')
            found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'Talk to Rich',
                                    str(self.out / 'home-after-relaunch.png'), '--first', '--quiet'],
                                   capture_output=True, text=True, timeout=60)
            if found.returncode not in (0, 1):
                raise StepFailed('ocr-find could not read the frame: ' + found.stderr)
            if found.returncode == 0:
                seen = round(time.monotonic() - began, 1)
                command_walk.command([HERE / 'ax.sh', self.vm, '--key', '36'])
                time.sleep(3)
                return {'home_on_screen_after_s': seen, 'entered_with': 'Enter'}
            time.sleep(2)
        return {'home_on_screen_after_s': None}

    def relaunch(self):
        """command-walk.py's relaunch, with the shortened interval in the app's environment."""
        import relaunch as relaunch_module
        original = relaunch_module.relaunch
        environment = {'STALL_WATCH_PENDING_WORK_SECONDS': str(int(self.a.reminder_seconds))}
        relaunch_module.relaunch = lambda vm, app=None, env=None: original(vm, app, dict(environment, **(env or {})))
        try:
            return super().relaunch()
        finally:
            relaunch_module.relaunch = original

    def job(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        sent = self.send(self.a.task)
        self.facts['sent_ms'] = sent
        self.save()
        end = time.monotonic() + self.a.within
        pressed, record = 0, None  # no Approve is pressed: the job is only to reach Stu
        while time.monotonic() < end:
            record = self.ours(sent)
            # Stu has the job once it is registered and he has begun it; the job's own review and
            # land take as long as they take and are not this walk's question.
            if record and record.get('state') != 'registered':
                break
            time.sleep(3)
        if not record:
            (self.out / 'lead-files.json').write_text(json.dumps(self.lead_files(), indent=2) + '\n')
            (self.out / 'conversation-ledger.tail').write_text(
                guest(self.vm, 'tail -n 40 %s 2>/dev/null || true' % shlex.quote(self.data + '/conversation-ledger.jsonl'), 60))
            raise StepFailed('no assignment was registered after the task was sent')
        if record.get('state') == 'registered':
            raise StepFailed('the job was still registered, not begun, after %g s' % self.a.within)
        self.facts['job'] = {k: record.get(k) for k in ('id', 'state', 'title', 'work_session')}
        self.save()
        return {'job': self.facts['job'], 'approvals_pressed': pressed}

    def lead_files(self):
        """Stu's lead files (one per lease session), as the app wrote them."""
        script = ('import glob,json,os,sys\n'
                  'rows=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/stall-watch-lead/*.json"):\n'
                  '    try: d=json.load(open(p))\n'
                  '    except Exception: continue\n'
                  '    d["mtime"]=os.path.getmtime(p)\n'
                  '    rows.append(d)\n'
                  'print(json.dumps(sorted(rows,key=lambda d:d["mtime"])))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s' % (shlex.quote(script), shlex.quote(self.data)), 60) or '[]')

    def idle(self):
        """The signal itself: after the job closed, a lead file carries `turn_ended`."""
        end = time.monotonic() + self.a.idle_within
        seen = []
        while time.monotonic() < end:
            seen = self.lead_files()
            idle = [d for d in seen if d.get('turn_ended')]
            if idle:
                self.facts['stu_session'] = idle[-1]['session']
                self.save()
                (self.out / 'lead-files.json').write_text(json.dumps(seen, indent=2) + '\n')
                return {'session': idle[-1]['session'], 'started': idle[-1].get('started'),
                        'turn_ended': idle[-1]['turn_ended']}
            time.sleep(3)
        (self.out / 'lead-files.json').write_text(json.dumps(seen, indent=2) + '\n')
        raise StepFailed("no lead file of Stu's carried turn_ended within %g s (seen: %s)" % (self.a.idle_within, json.dumps(seen)))

    def partition(self, session):
        found = guest(self.vm, 'ls -d %s/engine-state/workspaces/*/sessions/%s.json 2>/dev/null || true' % (
            shlex.quote(self.data), shlex.quote(session)), 60).split()
        if found:
            return str(Path(found[0]).parent.parent)
        parts = guest(self.vm, 'ls -d %s/engine-state/workspaces/*/ 2>/dev/null || true' % shlex.quote(self.data), 60).split()
        if len(parts) == 1:
            return parts[0].rstrip('/')
        raise StepFailed("no workspace partition holds Stu's session %s (partitions: %s)" % (session, parts))

    def found(self):
        """Stu's own inbox (<app state>/messages/<company>/<conversation>/stu.jsonl, the messages
        `session_tools::observe_stall` saves for him): the rows from stall-watch that carry the
        reminder's mark, and of those the ones naming the registered wait."""
        script = ('import glob,json,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/messages/*/*/stu.jsonl"):\n'
                  '    for l in open(p):\n'
                  '        try: r=json.loads(l)\n'
                  '        except Exception: continue\n'
                  '        if r.get("from")=="stall-watch" and sys.argv[2] in str(r.get("text","")):\n'
                  '            out.append({"file":p,"text":r["text"]})\n'
                  'print(json.dumps(out))\n')
        rows = json.loads(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(script), shlex.quote(self.data), shlex.quote(REMINDER_MARK)), 120) or '[]')
        return rows, [r for r in rows if WAIT_MARK in r['text']]

    def wait_for_reminder(self, seconds, naming_the_wait):
        end = time.monotonic() + seconds
        rows, named = [], []
        while time.monotonic() < end:
            rows, named = self.found()
            if named if naming_the_wait else rows:
                break
            time.sleep(10)
        return rows, named

    def reminder(self):
        """First as the app lives it: the job Stu was given, so the reminder is due once he is idle
        and has something unfinished. Only when it does not come within the interval and a
        stall-watch poll is a finished teammate registered by hand (stu-reminder-guest.py), and
        which of the two produced the reminder is recorded."""
        session = self.facts.get('stu_session')
        if not session:
            raise StepFailed('idle must have run (no Stu session on record)')
        waited = self.a.reminder_seconds + 240
        rows, named = self.wait_for_reminder(waited, False)
        how, partition, mate = 'the job Stu was given', None, None
        if not rows:
            made = self.payload + '/stu-reminder'
            guest(self.vm, 'mkdir -p ' + shlex.quote(made), 60)
            command_walk.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'stu-reminder-guest.py', made + '/'], 300)
            partition = self.partition(session)
            mate = guest(self.vm, '/usr/bin/python3 %s finished %s %s %s' % (
                shlex.quote(made + '/stu-reminder-guest.py'), shlex.quote(self.payload + '/engine'),
                shlex.quote(partition), shlex.quote(session)), 60).strip()
            how = 'a finished teammate registered by hand'
            rows, named = self.wait_for_reminder(waited, True)
        evidence = {'session': session, 'came_from': how, 'partition': partition, 'teammate': mate,
                    'reminder_seconds': self.a.reminder_seconds, 'waited_up_to_s_each': waited,
                    'stall_watch_rows_in_stus_inbox': rows, 'naming_the_wait': named,
                    'lead_files': self.lead_files()}
        (self.out / 'reminder.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not (named if mate else rows):
            raise StepFailed('no %s row from stall-watch reached Stu\'s inbox within %.0f s of either kind of wait; '
                             'see reminder.json' % (REMINDER_MARK, waited))
        return evidence



def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=JOB)
    p.add_argument('--within', type=float, default=300, help='seconds for Stu to begin the job')
    p.add_argument('--idle-within', type=float, default=900,
                   help="seconds for Stu's turn to end once the job is begun (his job's own length: 117 s in one "
                        "walk, more than 300 s in another)")
    p.add_argument('--approvals', type=int, default=3, help='most Approve presses on the job')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--reminder-seconds', type=float, default=120,
                   help="stall-watch's unfinished-work reminder interval in this guest (12 hours otherwise)")
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = StuReminderWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            try:
                walk.shot('failed-%s.png' % step)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                pass
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
