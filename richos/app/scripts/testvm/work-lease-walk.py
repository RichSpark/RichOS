#!/usr/bin/env python3
"""work-lease-walk.py — in the regular app: a job that needs a teammate Stu must first save goes on
after the team reload, and a Work lease's monitor that dies while the lease is open is told to Stu.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      work-lease-walk.py --out DIR --expect-sha SHA

THE QUESTIONS. (1) Walk 2 of 2026-10-24 (richos-hq docs/verification/2026-10-24-orchestration-
reliability-impl/a6-app-work-lease-eof/walk-2): Stu saved Norm from the shelf to do a commit-and-land
job, ended his turn to use him on the next one, and no next turn ever came; the job sat `running`
for 18 minutes. d7d06ffc7 sends him one `team_reloaded` message after a turn that changed the team.
(2) PRD A6: the Work lease's stall-watch monitor failing while the lease is open reaches Stu.
This is that walk's a6-app-walk.py, committed here so no nightly walk writes it again.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65). stu-reminder-walk.py's steps,
unchanged: identity, first-run, job (stu-reminder-walk.py's JOB, which needs an engineer, so Stu has to
save one; it must be SETTLED with no human message after the request), idle. Then:
  monitor   the Work lease's monitor (`stall_watch.py --monitor --jsonl`, native.rs StallWatch) is
            running for the Stu session that ran the job: pid, parent, start time, RICHOS_SESSION_ID;
            the probe leaves out its own process and its shell, whose command lines name those words
  fail      that pid, verified to be the same process, is killed (SIGKILL, in the disposable guest).
            PASS needs a NEW stall-watch row "the monitor stopped unexpectedly" in THIS conversation's
            inbox for Stu and its words in THIS Stu session's transcript
  reload    the rows that show the job moving after the reload: the `team-reload` rows in Stu's inbox
            and the transcript lines carrying `team_reloaded`; FAIL when there is none (the job did
            not go through a team change, so the question was not asked)
  after     after `fail`: one ordinary request (--after-task) is typed; it must be SETTLED, the lead
            file of the session that ran it must say its turn ended after the send, and no NEW
            "monitor stopped" row may reach Stu's inbox up to --after-quiet seconds after that turn
            end. A turn end is idle between turns (native.rs LeadSignal), not the lease's disposal, so
            this step reports only the turn end; a normal lease close is the a6-app-work-lease-eof
            walk's (richos-hq docs/verification/2026-10-24-orchestration-reliability-impl)
  keep      ALWAYS, pass or fail, before run-walk.py deletes the guest: every session transcript
            (~/.claude/projects), the app's engine-state, the conversation ledger and the bundle's
            Info.plist, as kept/kept.tgz on the host. The first such walk lost the rows.

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
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
_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
srw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(srw)
from relaunch import guest  # noqa: E402

StepFailed = srw.StepFailed
FAIL_MARK = 'the monitor stopped unexpectedly'
RELOAD_MARK = 'team_reloaded'
STEPS = ['identity', 'first-run', 'job', 'idle', 'reload', 'monitor', 'fail', 'after']
# An ordinary request after the monitor was killed, worded as srw.JOB is.
AFTER_JOB = ('Please add a file named more.md whose whole content is the line "More notes for the walk test." '
             'to the Acme repository and commit it, and land it.')

# The Work-lease monitors running in the guest. This source names the three words it looks for, so it
# leaves out its own process and the shell that ran it (nightly 74 walk: pid 24452 was this probe).
PROCS = ('import json,os,subprocess,sys\n'
         'rows=[]\n'
         'me={os.getpid(),os.getppid()}\n'
         'out=subprocess.run(["ps","-axo","pid=,ppid=,lstart=,command="],capture_output=True,text=True).stdout\n'
         'for l in out.splitlines():\n'
         '    p=l.split(None,7)\n'
         '    if len(p)<8 or int(p[0]) in me or "stall_watch.py" not in p[7] or "--monitor" not in p[7] or "--jsonl" not in p[7]: continue\n'
         '    env=subprocess.run(["ps","-E","-o","command=","-p",p[0]],capture_output=True,text=True).stdout\n'
         '    sid=[w.split("=",1)[1] for w in env.split() if w.startswith("RICHOS_SESSION_ID=")]\n'
         '    par=subprocess.run(["ps","-o","command=","-p",p[1]],capture_output=True,text=True).stdout.strip()\n'
         '    rows.append({"pid":int(p[0]),"ppid":int(p[1]),"started":" ".join(p[2:7]),"command":p[7][:300],'
         '"session":sid[0] if sid else "","parent":par[:200]})\n'
         'print(json.dumps(rows))\n')

# Stu's inbox for THIS conversation, the rows from one sender.
ROWS = ('import glob,json,sys\n'
        'out=[]\n'
        'for p in glob.glob(sys.argv[1]+"/engine-state/messages/*/"+sys.argv[2]+"/stu.jsonl"):\n'
        '    for l in open(p):\n'
        '        try: r=json.loads(l)\n'
        '        except Exception: continue\n'
        '        if r.get("from")==sys.argv[3]: out.append(r)\n'
        'print(json.dumps(out))\n')


class WorkLeaseWalk(srw.StuReminderWalk):
    def inbox(self, sender):
        return json.loads(guest(self.vm, 'python3 -c %s %s %s %s' % (
            shlex.quote(ROWS), shlex.quote(self.data), shlex.quote(self.facts['thread']), shlex.quote(sender)), 120) or '[]')

    def monitors(self):
        return json.loads(guest(self.vm, 'python3 -c %s' % shlex.quote(PROCS), 60) or '[]')

    def said_in(self, session, text):
        n = guest(self.vm, 'cat %s/.claude/projects/*/%s.jsonl 2>/dev/null | grep -c %s || true' % (
            shlex.quote(self.home), shlex.quote(session), shlex.quote(text)), 60).strip()
        return int(n) if n.isdigit() else 0

    def monitor(self):
        session = self.facts.get('stu_session')
        end = time.monotonic() + 120
        seen = []
        while time.monotonic() < end:
            seen = self.monitors()
            mine = [m for m in seen if m['session'] == session]
            if len(mine) == 1:
                self.facts['monitor'] = mine[0]
                self.save()
                return {'stu_session': session, 'monitor': mine[0], 'all_monitors': seen}
            time.sleep(5)
        raise StepFailed('no single Work-lease monitor of Stu session %s is running (seen: %s)' % (session, json.dumps(seen)))

    def fail(self):
        m = self.facts['monitor']
        session = m['session']
        failed = lambda: [r for r in self.inbox('stall-watch') if FAIL_MARK in str(r.get('text', ''))]
        before_rows, before_said = len(failed()), self.said_in(session, FAIL_MARK)
        # Kill only the verified pid, and only while it is still the same process (same start time).
        if not [x for x in self.monitors() if x['pid'] == m['pid'] and x['started'] == m['started']]:
            raise StepFailed('the verified monitor %s is no longer the same process; nothing killed' % m['pid'])
        killed_at = self.clock() / 1000
        guest(self.vm, 'kill -KILL %d' % m['pid'], 30)
        end = time.monotonic() + 300
        fresh, said = [], 0
        while time.monotonic() < end:
            fresh = failed()[before_rows:]
            said = self.said_in(session, FAIL_MARK) - before_said
            if fresh and said > 0:
                break
            time.sleep(5)
        # Give Stu's turn on it time to end, so `keep` carries his answer too.
        idle_end = time.monotonic() + 300
        while time.monotonic() < idle_end:
            lead = [d for d in self.lead_files() if d.get('session') == session]
            if lead and (lead[-1].get('turn_ended') or 0) > killed_at:  # both epoch seconds
                break
            time.sleep(5)
        ev = {'killed': m, 'killed_at_guest_epoch': killed_at, 'new_failure_rows': fresh,
              'new_transcript_lines_with_it': said, 'monitors_after': self.monitors(),
              'lead_files': self.lead_files()}
        (self.out / 'fail.json').write_text(json.dumps(ev, indent=2) + '\n')
        if not fresh:
            raise StepFailed('no stall-watch row saying the monitor stopped reached Stu\'s inbox within 300 s; see fail.json')
        if said <= 0:
            raise StepFailed('the failure row reached the inbox but not the transcript of Stu session %s; see fail.json' % session)
        return ev

    def reload(self):
        rows = self.inbox('team-reload')
        session = self.facts.get('stu_session') or (self.facts.get('job') or {}).get('work_session') or ''
        ev = {'team_reload_rows': rows, 'stu_session': session,
              'transcript_lines_with_it': self.said_in(session, RELOAD_MARK) if session else 0}
        (self.out / 'reload.json').write_text(json.dumps(ev, indent=2) + '\n')
        if not rows:
            raise StepFailed('no team-reload row in Stu\'s inbox: the job did not go through a team change; see reload.json')
        return ev

    def after(self):
        if not self.facts.get('monitor'):
            raise StepFailed('fail must have run (no killed monitor on record)')
        failed = lambda: [r for r in self.inbox('stall-watch') if FAIL_MARK in str(r.get('text', ''))]
        rows_before = len(failed())
        sent = self.send(self.a.after_task)
        end = time.monotonic() + self.a.job_within
        pressed, record = 0, None
        while time.monotonic() < end:
            record = self.ours(sent)
            if record and record.get('state') not in srw.OPEN:
                break
            if record and pressed < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                pressed += 1
            time.sleep(3)
        ev = {'sent_between_guest_ms': sent, 'job': record, 'approvals_pressed': pressed,
              'failure_rows_before': rows_before}
        if not record or record.get('state') != 'settled':
            (self.out / 'after.json').write_text(json.dumps(ev, indent=2) + '\n')
            raise StepFailed('the request after the kill was not settled: %s; see after.json' % (
                record.get('state') if record else 'no assignment registered'))
        session = record.get('work_session')
        idle_end = time.monotonic() + self.a.idle_within
        while True:
            ended = [d for d in self.lead_files()
                     if d.get('session') == session and (d.get('turn_ended') or 0) > sent[1] / 1000]
            if ended or time.monotonic() >= idle_end:
                break
            time.sleep(5)
        if ended:
            time.sleep(self.a.after_quiet)  # a failure row would come within seconds of the turn end
        ev.update({'work_session': session, 'turn_ended': ended[-1] if ended else None,
                   'new_failure_rows': failed()[rows_before:], 'monitors_after': self.monitors(),
                   'lead_files': self.lead_files()})
        (self.out / 'after.json').write_text(json.dumps(ev, indent=2) + '\n')
        if not ended:
            raise StepFailed('the session %s that ran the request never ended its turn within %g s; see after.json' % (
                session, self.a.idle_within))
        if ev['new_failure_rows']:
            raise StepFailed('a "monitor stopped" row reached Stu after the request settled and its turn ended; see after.json')
        return {'job': {k: record.get(k) for k in ('id', 'state', 'title', 'work_session')},
                'approvals_pressed': pressed, 'turn_ended': ev['turn_ended'], 'new_failure_rows': []}

    def keep(self):
        dest = self.out / 'kept'
        dest.mkdir(parents=True, exist_ok=True)
        tgz = self.home + '/kept.tgz'
        parts = []
        for root, rel in ((self.home + '/.claude', 'projects'), (self.data, 'engine-state'),
                          (self.data, 'conversation-ledger.jsonl')):
            parts.append('if [ -e %s ]; then echo %s; fi' % (shlex.quote(root + '/' + rel), shlex.quote(root + '\t' + rel)))
        listing = guest(self.vm, '; '.join(parts), 60)
        args = []
        for line in listing.splitlines():
            root, rel = line.split('\t', 1)
            args += ['-C', root, rel]
        if not args:
            raise StepFailed('nothing to keep: no transcripts, engine-state or ledger in the guest')
        plist = guest(self.vm, 'ls -d /Users/admin/testvm/%s/RichOS.app/Contents/Info.plist 2>/dev/null || true' % self.vm, 30).strip()
        guest(self.vm, 'tar -czf %s %s' % (shlex.quote(tgz), ' '.join(shlex.quote(a) for a in args)), 300)
        srw.command_walk.command([HERE / 'guest.sh', self.vm, '--pull', tgz, str(dest / 'kept.tgz')], 600)
        if plist:
            srw.command_walk.command([HERE / 'guest.sh', self.vm, '--pull', plist, str(dest / 'Info.plist')], 120)
        return {'kept': str(dest / 'kept.tgz'), 'members_from': listing.splitlines(), 'info_plist': plist,
                'bytes': (dest / 'kept.tgz').stat().st_size}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--task', default=srw.JOB)
    p.add_argument('--within', type=float, default=300)
    p.add_argument('--job-within', type=float, default=1200)
    p.add_argument('--idle-within', type=float, default=900)
    p.add_argument('--approvals', type=int, default=3)
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--reminder-seconds', type=float, default=120)
    p.add_argument('--after-task', default=AFTER_JOB, help="`after`'s ordinary request")
    p.add_argument('--after-quiet', type=float, default=60,
                   help="seconds after the request's turn ended that a failure row is waited for")
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    walk = WorkLeaseWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in a.steps.split(','):
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
            # `reload` only reads what is there, so it still runs after a failed step.
            if 'reload' in a.steps.split(',') and 'reload' not in [r['step'] for r in report['steps']]:
                try:
                    report['steps'].append({'step': 'reload', 'outcome': 'PASS', 'evidence': walk.reload()})
                except (StepFailed, RuntimeError, ValueError, KeyError, subprocess.TimeoutExpired) as exc:
                    report['steps'].append({'step': 'reload', 'outcome': 'FAIL', 'detail': str(exc)})
            break
    row = {'step': 'keep'}
    try:
        row['evidence'] = walk.keep()
        row['outcome'] = 'PASS'
    except (StepFailed, RuntimeError, ValueError, KeyError, OSError, subprocess.TimeoutExpired) as exc:
        row['outcome'] = 'FAIL'
        row['detail'] = str(exc)
        ok = False  # rows not carried off the guest before it is deleted: the walk failed
    report['steps'].append(row)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f"keep: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
