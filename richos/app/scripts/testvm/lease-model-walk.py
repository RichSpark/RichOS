#!/usr/bin/env python3
"""lease-model-walk.py — in the regular app, from a profile that never chose a model: which model and
effort Rich's and Stu's Claude Code processes were started with, and which model answered.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      lease-model-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION (CEO, 2026-11-17: "Fable at low effort might actually be a good new default for Rich in
the RichOS app. And for Stu, Fable at medium effort might be the best new default."). With no model
ever chosen, was the conversation's Claude Code process started with `--model fable --effort low` and
the work's with `--model fable --effort medium`, and did each answer as a Fable model. Read off the
running processes and the sessions' own transcripts in the guest, not off the source. --rich and
--stu name another expectation ("opus:low"), for a walk of a profile that chose.

AND, with --steps naming them (round 86.1, CEO 2026-11-17: "round-86.1 is good to go. Implement
now."): a level chosen in Settings is what that role's Claude Code process is started with, and
choosing Max shows the warning.

  run-walk.py ... -- lease-model-walk.py --out DIR --expect-sha SHA --rich fable:medium --stu fable:high \\
      --steps identity,first-run,max-warning,effort,relaunch,job,settings,leases

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65). stu-reminder-walk.py's steps,
unchanged: identity, first-run, relaunch, job (its JOB, which Rich hands to Stu, so both sessions
start and both answer). Then:
  settings  config.json as the app left it: its `rich_model` and `stu_model` keys, or their absence
  leases    every Claude Code process under the recorded app pid that carries --model: its --model,
            --effort and --session-id as the kernel holds them (`ps`), which is Stu's (the job's
            work_session) and which is Rich's, and the models and efforts the assistant rows of each
            session's transcript record. PASS needs the expected flags on both and every answer of
            each from a model whose name carries the expected alias.

  max-warning  Settings opened by its button; Max pressed in the conversation's effort control. PASS
            needs the warning's two buttons on screen ("Use <level>, the recommended level" and "Use
            Max anyway"), "WARNING!" read off a picture of the guest's screen (max-warning.png), and,
            after the first button is pressed, the warning gone with Settings still open.
  effort    in the same Settings panel the level --rich names is pressed in the conversation's
            control and the level --stu names in the team's, and Settings is closed with Escape,
            which saves. PASS needs config.json holding exactly those two levels as chosen
            (`rich_effort_<model>`, `stu_effort_<model>`). The relaunch, job and leases steps after
            it then read the processes: started with that --effort, answering at it.

Evidence in --out: report.json, facts.json, leases.json. Exit 0 when every step passes.
It quits nothing: run-walk.py quits the app by its pid and deletes the clone (CEO 54).
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
stu_reminder_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(stu_reminder_walk)
StepFailed = stu_reminder_walk.StepFailed

STEPS = ['identity', 'first-run', 'relaunch', 'job', 'settings', 'leases']
# Steps a run names with --steps; not in the default run, which is the never-chosen profile.
OTHER_STEPS = ['max-warning', 'effort']

# The assistant rows of one session's transcript, wherever the lease's account folder put it.
TRANSCRIPT = r"""import glob,json,os,sys
home,data,session=sys.argv[1:4]
seen={}
paths=glob.glob(home+'/.claude/projects/*/'+session+'.jsonl')+glob.glob(data+'/**/projects/*/'+session+'.jsonl',recursive=True)
for path in paths:
    for line in open(path,errors='replace'):
        try: row=json.loads(line)
        except ValueError: continue
        if row.get('type')!='assistant': continue
        model=(row.get('message') or {}).get('model')
        if model=='<synthetic>': continue
        key=str(model)+' effort='+str(row.get('effort'))
        seen[key]=seen.get(key,0)+1
print(json.dumps({'files':len(paths),'answers':seen}))
"""


def flag(words, name):
    return words[words.index(name) + 1] if name in words and words.index(name) + 1 < len(words) else None


class LeaseModelWalk(stu_reminder_walk.StuReminderWalk):
    def present_id(self, dom_id):
        try:
            return bool(self.ax('find', '--id', dom_id, '--first'))
        except StepFailed:
            return False

    def until(self, what, test, seconds, every=2):
        end = time.monotonic() + seconds
        while True:
            value = test()
            if value:
                return value
            if time.monotonic() >= end:
                raise StepFailed('%s did not happen within %g s' % (what, seconds))
            time.sleep(every)

    def open_settings(self):
        if not self.present_id('ef-rich-low'):
            self.ax('click', '--id', 'set-btn', '--first')
            self.until("the effort control in Settings", lambda: self.present_id('ef-rich-low'), 60)

    def max_warning(self):
        self.open_settings()
        recommended = self.a.max_recommended
        keep = 'Use %s, the recommended level' % recommended
        self.ax('click', '--id', 'ef-rich-max', '--first')
        self.until('the Max warning', lambda: self.present(keep) and self.present('Use Max anyway'), 60)
        self.shot('max-warning.png')
        read = subprocess.run([str(HERE.parent / 'qa' / 'ocr-find.sh'), 'WARNING', str(self.out / 'max-warning.png'),
                               '--first', '--quiet'], capture_output=True, text=True, timeout=120)
        if read.returncode != 0:
            raise StepFailed('"WARNING!" is not on the picture of the screen (max-warning.png): %s' % (read.stdout + read.stderr).strip())
        self.press(keep, contains=False)
        self.until('the warning to close', lambda: not self.present('Use Max anyway'), 60)
        if not self.present_id('ef-rich-low'):
            raise StepFailed('Settings closed with the warning')
        self.shot('max-warning-after.png')
        return {'warning_buttons': [keep, 'Use Max anyway'], 'warning_read_on_screen': True,
                'settings_open_after': True, 'pictures': ['max-warning.png', 'max-warning-after.png']}

    def effort(self):
        self.open_settings()
        want = {'rich': self.a.rich.split(':'), 'stu': self.a.stu.split(':')}
        for who, (_model, level) in want.items():
            self.ax('click', '--id', 'ef-%s-%s' % (who, level), '--first')
        self.shot('effort-chosen.png')
        stu_reminder_walk.command_walk.adopt_walk.command([HERE / 'ax.sh', self.vm, '--key', '53'], 60)  # Escape closes Settings, which saves
        keys = {'%s_effort_%s' % (who, model): level for who, (model, level) in want.items()}

        def saved():
            text = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(self.data + '/config.json'), 60)
            held = json.loads(text) if text.strip() else {}
            return held if all(held.get(k) == v for k, v in keys.items()) else None
        held = self.until('config.json to hold the chosen levels %s' % json.dumps(keys), saved, 60)
        return {'chosen': keys, 'effort_keys_in_file': sorted(k for k in held if '_effort_' in k), 'picture': 'effort-chosen.png'}

    def settings(self):
        text = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(self.data + '/config.json'), 60)
        held = json.loads(text) if text.strip() else {}
        return {'file': bool(text.strip()), 'rich_model': held.get('rich_model', 'absent'),
                'stu_model': held.get('stu_model', 'absent'),
                'efforts': {k: v for k, v in held.items() if '_effort_' in k}, 'keys': sorted(held)}

    def descendants(self, pid):
        parent = {}
        for row in guest(self.vm, 'ps -axww -o pid=,ppid=', 30).splitlines():
            p = row.split()
            if len(p) == 2 and all(w.isdigit() for w in p):
                parent[int(p[0])] = int(p[1])
        mine, grew = set(), True
        while grew:
            grew = False
            for child, par in parent.items():
                if child not in mine and (par == pid or par in mine):
                    mine.add(child)
                    grew = True
        return mine

    def leases(self):
        job = self.facts.get('job') or {}
        if not job.get('work_session'):
            raise StepFailed('job must have run (no work session on record)')
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
        pid = (state / 'app.pid').read_text().strip()
        if not pid.isdigit():
            raise StepFailed('recorded app pid is not numeric')
        mine = self.descendants(int(pid))
        found = []
        for line in guest(self.vm, 'ps -axww -o pid=,command=', 30).splitlines():
            head = line.strip().split(None, 1)
            if len(head) != 2 or not head[0].isdigit() or int(head[0]) not in mine:
                continue
            words = head[1].split()
            if '--model' not in words or '--session-id' not in words:
                continue
            session = flag(words, '--session-id')
            answers = json.loads(guest(self.vm, 'python3 -c %s %s %s %s' % (
                shlex.quote(TRANSCRIPT), shlex.quote(self.home), shlex.quote(self.data), shlex.quote(session)), 120))
            found.append({'pid': int(head[0]), 'session': session, 'model': flag(words, '--model'),
                          'effort': flag(words, '--effort'),
                          'role': 'stu' if session == job['work_session'] else 'rich',
                          'executable': words[0], 'transcript': answers})
        want = {'rich': self.a.rich.split(':'), 'stu': self.a.stu.split(':')}
        (self.out / 'leases.json').write_text(json.dumps({'app_pid': int(pid), 'want': want, 'leases': found}, indent=2) + '\n')
        problems = []
        for role, (model, effort) in want.items():
            rows = [r for r in found if r['role'] == role]
            if not rows:
                problems.append('no running Claude Code process for %s' % role)
            for r in rows:
                if (r['model'], r['effort']) != (model, effort):
                    problems.append('%s (pid %d) was started with --model %s --effort %s, not %s %s' % (
                        role, r['pid'], r['model'], r['effort'], model, effort))
                answers = r['transcript']['answers']
                if not answers:
                    problems.append('%s (session %s) has no answer in its transcript' % (role, r['session']))
                # A row that records no effort says nothing either way; one that records another does.
                for key in answers:
                    if model not in key.split(' ')[0] or key.split(' effort=')[1] not in (effort, 'None'):
                        problems.append('%s was answered by %s' % (role, key))
                if answers and not any(key.endswith(' effort=' + effort) for key in answers):
                    problems.append('no answer of %s records effort %s: %s' % (role, effort, sorted(answers)))
        if problems:
            raise StepFailed('; '.join(problems) + ' (see leases.json)')
        return {'leases': found}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--rich', default='fable:low', help="the conversation's expected model:effort")
    p.add_argument('--stu', default='fable:medium', help="the work's expected model:effort")
    p.add_argument('--max-recommended', default='Low',
                   help="max-warning: the level the warning's first button names (the conversation's recommended one)")
    p.add_argument('--task', default=stu_reminder_walk.JOB)
    p.add_argument('--within', type=float, default=300, help="seconds for the walk's other waits")
    p.add_argument('--job-within', type=float, default=1200, help='seconds for the job to be settled')
    p.add_argument('--approvals', type=int, default=3, help='most Approve presses on the job')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS + OTHER_STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = LeaseModelWalk(a)
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
