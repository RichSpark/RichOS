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
  connect     command-walk.py's: that folder connected as a repository
  relaunch    the app quit and opened again with STALL_WATCH_PENDING_WORK_SECONDS=--reminder-seconds
              in its environment (this guest only; 12 hours otherwise), so Stu's monitor has it
  job         typed: a harmless command for the back end (Stu) to run; followed until the job closes
  idle        Stu's lead file says his turn ended (`turn_ended`): the signal this walk is about
  reminder    a finished teammate of Stu's session, waiting for its review, is registered; within
              the interval plus a stall-watch poll, the UNFINISHED-WORK reminder naming that wait
              is in Stu's own messages (the conversation's inbox for Stu) or his transcript.

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

STEPS = ['identity', 'first-run', 'connect', 'relaunch', 'job', 'idle', 'reminder']
REMINDER_MARK = 'UNFINISHED-WORK'
WAIT_MARK = 'waiting for its review'


class StuReminderWalk(command_walk.CommandWalk):
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
        pressed, record = 0, None
        while time.monotonic() < end:
            record = self.ours(sent)
            if record and record.get('state') not in OPEN:
                break
            if record and pressed < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                pressed += 1
            time.sleep(3)
        if not record:
            raise StepFailed('no assignment was registered after the task was sent')
        if record.get('state') in OPEN:
            raise StepFailed('the job was still %s after %g s' % (record.get('state'), self.a.within))
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
        end = time.monotonic() + self.a.within
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
        raise StepFailed("no lead file of Stu's carried turn_ended within %g s (seen: %s)" % (self.a.within, json.dumps(seen)))

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
        """Where the reminder is: the app's data folder (Stu's inbox), and Stu's transcripts."""
        hits = guest(self.vm, 'grep -rIl %s %s %s/.claude/projects 2>/dev/null || true' % (
            shlex.quote(REMINDER_MARK), shlex.quote(self.data + '/engine-state'), shlex.quote(self.home)), 120).split()
        named = [h for h in hits if guest(self.vm, 'grep -c %s %s' % (shlex.quote(WAIT_MARK), shlex.quote(h)) + ' || true', 60).strip() not in ('', '0')]
        return hits, named

    def reminder(self):
        session = self.facts.get('stu_session')
        if not session:
            raise StepFailed('idle must have run (no Stu session on record)')
        made = self.payload + '/stu-reminder'
        command_walk.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'stu-reminder-guest.py', made + '/'], 300)
        partition = self.partition(session)
        mate = guest(self.vm, '/usr/bin/python3 %s finished %s %s %s' % (
            shlex.quote(made + '/stu-reminder-guest.py'), shlex.quote(self.payload + '/engine'),
            shlex.quote(partition), shlex.quote(session)), 60).strip()
        waited = self.a.reminder_seconds + 240
        end = time.monotonic() + waited
        hits, named = [], []
        while time.monotonic() < end:
            hits, named = self.found()
            if named:
                break
            time.sleep(10)
        evidence = {'session': session, 'partition': partition, 'teammate': mate,
                    'reminder_seconds': self.a.reminder_seconds, 'waited_up_to_s': waited,
                    'files_with_reminder': hits, 'files_naming_the_wait': named,
                    'lead_files': self.lead_files()}
        (self.out / 'reminder.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not named:
            raise StepFailed('no %s notice naming "%s" reached Stu within %.0f s; see reminder.json'
                             % (REMINDER_MARK, WAIT_MARK, waited))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=command_walk.TASK)
    p.add_argument('--within', type=float, default=300, help='seconds for the job to close, and for the idle signal')
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
