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
  relaunch    the app quit and opened again (command-walk.py's): due work is told at the monitor's
              first look while Stu is idle (orchestration reliability R2), so no interval is set
  job         typed: a job for the back end (Stu); it must be SETTLED (finished successfully), or the
              step fails (Approve is pressed as asked)
  idle        the lead file of the session that ran the job says his turn ended (`turn_ended`):
              the signal this walk is about
  reminder    the settled job left nothing unfinished, so a finished teammate of Stu's session,
              waiting for its review, is registered in the app's own registry
              (stu-reminder-guest.py). PASS needs a NEW row from stall-watch, after that
              registration, naming the wait, in THIS conversation's inbox for Stu
              (<app state>/messages/<company>/<conversation>/stu.jsonl), and its words in the
              transcript of THIS Stu session.
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
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed
OPEN = command_walk.OPEN

STEPS = ['identity', 'first-run', 'relaunch', 'job', 'idle', 'reminder']
# A job, as worker-approve-walk.py words one: it is registered as an assignment and run by Stu.
JOB = ('Please add a file named notes.md whose whole content is the line "Notes for the walk test." '
       'to the Acme repository and commit it, and land it.')
REMINDER_MARK = '[DUE]'
WAIT_MARK = 'waiting for its review'


class StuReminderWalk(command_walk.CommandWalk):
    # What after_relaunch reads on a frame to know the app is back (an extended regular expression).
    HOME_TEXT = 'Talk to Rich'

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
                if not axwait.no_answer(exc):  # a press with no answer: read the screen again
                    raise
                continue
            presses += 1
            time.sleep(3)
        return presses

    def ax(self, mode, *args, app=None, timeout=40):
        """Walk.ax, a read (find, tree) asked again after it had no answer (axwait.ask_again: past
        ax.sh's own 20 s deadline on a busy host, or the deadline helper itself failing to end an
        overdue read). A click is sent once: its effect is unknown."""
        for attempt in range(1, 7):
            try:
                return super().ax(mode, *args, app=app, timeout=timeout)
            except StepFailed as exc:
                if not axwait.ask_again(exc, mode) or attempt == 6:
                    raise

    # present() is adopt-walk's, whose find with no answer is "not seen yet" (axwait.py).

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
            found = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), self.HOME_TEXT,
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

    def job(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        sent = self.send(self.a.task)
        self.facts['sent_ms'] = sent
        self.save()
        end = time.monotonic() + self.a.job_within
        pressed, record = 0, None
        while time.monotonic() < end:
            record = self.ours(sent)
            if record and record.get('state') not in OPEN:
                break
            if record and pressed < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                pressed += 1
            time.sleep(3)
        if not record:
            (self.out / 'lead-files.json').write_text(json.dumps(self.lead_files(), indent=2) + '\n')
            (self.out / 'conversation-ledger.tail').write_text(
                guest(self.vm, 'tail -n 40 %s 2>/dev/null || true' % shlex.quote(self.data + '/conversation-ledger.jsonl'), 60))
            raise StepFailed('no assignment was registered after the task was sent')
        if record.get('state') != 'settled':
            raise StepFailed('the job did not finish successfully: it is %s after %g s (a job still open or '
                             'failed is not Stu finishing one)' % (record.get('state'), self.a.job_within))
        self.facts['job_settled_s'] = self.clock() / 1000  # the guest's clock, when the loop saw it settled
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
            # The session that ran this job, ended after it was settled.
            idle = [d for d in seen if d.get('turn_ended') and d.get('session') == self.facts['job'].get('work_session')]
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
        """The conversation's workspace registry (<app data>/engine-state/workspaces/<partition>), the
        one holding Stu's session record. Lines, not words: the app data folder has a space in it."""
        found = guest(self.vm, 'ls -d %s/engine-state/workspaces/*/sessions/%s.json 2>/dev/null || true' % (
            shlex.quote(self.data), shlex.quote(session)), 60).splitlines()
        if len(found) != 1:
            raise StepFailed("no single workspace partition holds Stu's session %s (found: %s)" % (session, found))
        return str(Path(found[0]).parent.parent)

    def found(self):
        """Stu's own inbox for THIS conversation (<app state>/messages/<company>/<conversation>/stu.jsonl,
        the messages `session_tools::observe_stall` saves for him): the rows from stall-watch that
        carry the reminder's mark."""
        script = ('import glob,json,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/messages/*/"+sys.argv[3]+"/stu.jsonl"):\n'
                  '    for l in open(p):\n'
                  '        try: r=json.loads(l)\n'
                  '        except Exception: continue\n'
                  '        if r.get("from")=="stall-watch" and sys.argv[2] in str(r.get("text","")):\n'
                  '            out.append({"file":p,"text":r["text"]})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s %s %s' % (
            shlex.quote(script), shlex.quote(self.data), shlex.quote(REMINDER_MARK),
            shlex.quote(self.facts['thread'])), 120) or '[]')

    def in_transcript(self, session):
        """How many lines of Stu's own transcript for this session carry the reminder: what reached THIS session."""
        said = guest(self.vm, 'cat %s/.claude/projects/*/%s.jsonl 2>/dev/null | grep -c %s || true' % (
            shlex.quote(self.home), shlex.quote(session), shlex.quote(REMINDER_MARK)), 60).strip()
        return int(said) if said.isdigit() else 0

    def reminder(self):
        """Stu's job is settled and his turn has ended (the steps before this one). After the
        registration below, a NEW row from stall-watch naming that wait must come in this
        conversation's inbox for Stu, and a NEW line carrying it in the transcript of THIS Stu session
        (the positions of both are taken before the registration)."""
        session = self.facts.get('stu_session')
        if not session:
            raise StepFailed('idle must have run (no Stu session on record)')
        # The settled job left nothing unfinished (its work is landed), so there is nothing to remind
        # of until something is: a finished teammate of Stu's session, waiting for its review, is
        # registered in the app's own registry, as spawn.sh registers one.
        made = self.payload + '/stu-reminder'
        guest(self.vm, 'mkdir -p ' + shlex.quote(made), 60)
        command_walk.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'stu-reminder-guest.py', made + '/'], 300)
        partition = self.partition(session)
        # Positions before the registration: only what arrives after them counts.
        rows_before, transcript_before = len(self.found()), self.in_transcript(session)
        registered = self.clock() / 1000
        mate = guest(self.vm, '/usr/bin/python3 %s finished %s %s %s' % (
            shlex.quote(made + '/stu-reminder-guest.py'), shlex.quote(self.payload + '/engine'),
            shlex.quote(partition), shlex.quote(session)), 60).strip()
        waited = self.a.reminder_seconds + 240
        end = time.monotonic() + waited
        fresh, transcript_new = [], 0
        while time.monotonic() < end:
            fresh = [r for r in self.found()[rows_before:] if WAIT_MARK in r['text']]
            transcript_new = self.in_transcript(session) - transcript_before if fresh else 0
            if fresh and transcript_new > 0:
                break
            time.sleep(10)
        evidence = {'session': session, 'partition': partition, 'teammate': mate,
                    'registered_at_utc': time.strftime('%H:%M:%SZ', time.gmtime(registered)),
                    'rows_before_registration': rows_before, 'transcript_lines_before_registration': transcript_before,
                    'reminder_seconds': self.a.reminder_seconds, 'waited_up_to_s': waited,
                    'new_rows_naming_the_wait': fresh, 'new_transcript_lines': transcript_new,
                    'all_rows': self.found(), 'lead_files': self.lead_files()}
        (self.out / 'reminder.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not fresh:
            raise StepFailed('no new %s row naming "%s" reached Stu\'s inbox in this conversation after the '
                             'registration within %.0f s; see reminder.json' % (REMINDER_MARK, WAIT_MARK, waited))
        if transcript_new <= 0:
            raise StepFailed("the reminder reached Stu's inbox but no new line of the transcript of his session %s "
                             "carries it within %.0f s; see reminder.json" % (session, waited))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=JOB)
    p.add_argument('--within', type=float, default=300, help='seconds for the walk\'s other waits')
    p.add_argument('--job-within', type=float, default=1200, help='seconds for the job to be settled (finished successfully)')
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
