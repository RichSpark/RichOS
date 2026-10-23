#!/usr/bin/env python3
"""folder-instructions-walk.py — a connected folder's own CLAUDE.md or AGENTS.md reaches the team working in it, and survives.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  command-walk.py --seed-team EMPTY_DIR ENGINE/team/shelf/norm.md ENGINE/team/shelf/tom.md
  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      folder-instructions-walk.py --out DIR --expect-sha SHA [--within SECONDS]

Seed the engineer and the reviewer first. In a fresh guest no teammate who writes code is active,
so Stu has Dean fit two from the shelf, and they register only at the next provider connection:
the first runs spent 25 and 45 minutes of the first job there and never started a worker.

THE QUESTION (richos-hq docs/plans/2026-10-21-folder-instruction-files.md §5.5, ruling §138
addendum 4). A user who already keeps a CLAUDE.md or AGENTS.md in a folder he connects must find
it followed by the workers and reviewers working in that folder, as in the terminal, and must find
every instruction file exactly as it was after the app has worked there. Before the change the app
ignored the file, so the run on main is the red: the rule is not followed.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity     the running app says it was built from --expect-sha
  seed         two folders with Git: Acme, whose committed CLAUDE.md asks that every commit message
               end with [house-rule] (plus a gitignored CLAUDE.local.md and an AGENTS.md that
               Claude Code's rule leaves out beside a CLAUDE.md), and Tools, whose only instruction
               file is an AGENTS.md asking for [agents-rule]; the sha256 of every instruction file
  first-run    adopt-walk.py's: company "Acme" with the Acme folder
  connect      command-walk.py's: Acme connected as a repository
  connect-tools  Tools connected to the same company
  house-rule   typed: add notes.md to the Acme repository, commit it and land it. PASS needs the
               job settled, notes.md landed in Acme, the commit that added it ending with
               [house-rule], a worker and a reviewer each having received the CLAUDE.md (the
               first message of its own transcript), and no payload Stu read holding it
  agents-rule  the same for tools.md in Tools, its AGENTS.md and [agents-rule]
  preserved    every instruction file in both folders has the sha256 it had at seed, and none
               was added or removed

Exit 0 when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
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

STEPS = ['identity', 'seed', 'first-run', 'connect', 'connect-tools', 'house-rule', 'agents-rule', 'preserved']
HOUSE_RULE = 'Every commit message in this repository ends with the tag [house-rule].'
AGENTS_RULE = 'Every commit message in this repository ends with the tag [agents-rule].'
FILES = {
    'Acme': {'README.md': 'Acme notes\n',
             'CLAUDE.md': '# Acme house rules\n\n' + HOUSE_RULE + '\n',
             'AGENTS.md': 'AGENTS-BESIDE-CLAUDE: Claude Code leaves this file out beside a CLAUDE.md.\n',
             '.gitignore': 'CLAUDE.local.md\n'},
    'Tools': {'README.md': 'Tools notes\n',
              'AGENTS.md': '# Tools rules\n\n' + AGENTS_RULE + '\n'},
}
LOCAL = ('Acme', 'CLAUDE.local.md', 'My own notes for this folder, never committed.\n')
NAMES = ('CLAUDE.md', 'CLAUDE.local.md', 'AGENTS.md', 'AGENTS.override.md')
# output-walk.py's worker task shape, the wording proven to reach a back-end WORKER and its land.
TASK = ('Please add a file named {file} whose whole content is the line "{line}" to the {repo} repository, '
        'commit it and land it.')
JOBS = {'house-rule': ('Acme', 'notes.md', 'Notes for the walk test.', '[house-rule]', HOUSE_RULE),
        'agents-rule': ('Tools', 'tools.md', 'Tools for the walk test.', '[agents-rule]', AGENTS_RULE)}


class FolderInstructionsWalk(command_walk.CommandWalk):
    def folder(self, name):
        return self.home + '/' + name

    def instruction_files(self):
        """{path: sha256} of every instruction file in both folders, outside .git."""
        script = ('import hashlib,json,os,sys\n'
                  'out={}\n'
                  'for root in sys.argv[2:]:\n'
                  '    for d,dirs,files in os.walk(root):\n'
                  '        dirs[:]=[x for x in dirs if x!=".git"]\n'
                  '        for f in files:\n'
                  '            if f in sys.argv[1].split(","):\n'
                  '                p=os.path.join(d,f); out[p]=hashlib.sha256(open(p,"rb").read()).hexdigest()\n'
                  'print(json.dumps(out,sort_keys=True))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + ','.join(NAMES) + ' '
                                + ' '.join(shlex.quote(self.folder(n)) for n in FILES), 60))

    def seed(self):
        for name, files in FILES.items():
            root = self.folder(name)
            writes = ' && '.join('printf %s {v} > {p}'.format(v=shlex.quote(text), p=shlex.quote(root + '/' + rel))
                                 for rel, text in files.items())
            guest(self.vm, 'mkdir -p {r} && cd {r} && if [ ! -d .git ]; then {w} && git init -q -b main && git add -A '
                           '&& git -c user.name=QA -c user.email=qa@example.invalid commit -q -m init; fi'
                  .format(r=shlex.quote(root), w=writes), 60)
        name, rel, text = LOCAL
        guest(self.vm, 'printf %s {v} > {p}'.format(v=shlex.quote(text), p=shlex.quote(self.folder(name) + '/' + rel)))
        self.facts['instruction_files'] = self.instruction_files()
        self.save()
        return self.facts['instruction_files']

    def connect_tools(self):
        connected = self.connect_folder(self.folder('Tools'))
        self.facts['connected'] = connected
        self.save()
        return {'connected': connected}

    def receipts(self, repo, rule):
        """Every work receipt for `repo`: its role, whether the prompt its agent RECEIVED (the
        first message of the agent's own transcript) holds the folder's file, and whether the
        payload Stu read (the receipt's) does."""
        script = ('import glob,json,os,sys\n'
                  'data,repo,rule,home=sys.argv[1],os.path.realpath(sys.argv[2]),sys.argv[3],sys.argv[4]\n'
                  'def first_message(aid):\n'
                  '    for root in (home+"/.claude",data):\n'
                  '        for p in glob.glob(root+"/**/agent-"+aid+".jsonl",recursive=True):\n'
                  '            for line in open(p,errors="replace"):\n'
                  '                try: row=json.loads(line)\n'
                  '                except Exception: continue\n'
                  '                if row.get("type")=="user":\n'
                  '                    c=(row.get("message") or {}).get("content")\n'
                  '                    return c if isinstance(c,str) else json.dumps(c)\n'
                  '    return None\n'
                  'out=[]\n'
                  'for p in glob.glob(data+"/engine-state/work-receipts/*/*.json"):\n'
                  '    try: r=json.load(open(p))\n'
                  '    except Exception: continue\n'
                  '    q=r.get("request") or {}\n'
                  '    if q.get("repo")!=repo: continue\n'
                  '    aid=r.get("agent_id")\n'
                  '    got=first_message(aid) if isinstance(aid,str) else None\n'
                  '    stu=json.dumps(r.get("payload") or {})\n'
                  '    out.append({"id":r.get("id"),"role":q.get("role"),"teammate":q.get("teammate"),"status":r.get("status"),\n'
                  '                "agent_id":aid,"transcript_found":got is not None,\n'
                  '                "heading":got is not None and "## The repository\'s own instructions" in got,\n'
                  '                "rule":got is not None and ("> "+rule) in got,\n'
                  '                "other_file":got is not None and "AGENTS-BESIDE-CLAUDE" in got,\n'
                  # The app's own section, never the sentence: Stu may read a connected folder's
                  # file itself and write its rule into the brief in its own words (the first run
                  # on main did), and that is Stu's reading, not the app handing it the file.
                  '                "stu_payload_holds_rule":"## The repository\'s own instructions" in stu or json.dumps("> "+rule)[1:-1] in stu})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data) + ' '
                                + shlex.quote(repo) + ' ' + shlex.quote(rule) + ' ' + shlex.quote(self.home), 120))

    def job(self, step):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        name, file, line, tag, rule = JOBS[step]
        repo = self.folder(name)
        sent = self.send(TASK.format(file=file, line=line, repo=name))
        end = time.monotonic() + self.a.within
        presses, record = 0, None
        while time.monotonic() < end:
            record = self.ours(sent)
            if record and record.get('state') not in OPEN:
                break
            if record and presses < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                presses += 1
            time.sleep(5)
        record = self.settled_read(record) if record else None
        message = guest(self.vm, 'git -C {r} log -1 --format=%B -- {f} 2>/dev/null || true'
                        .format(r=shlex.quote(repo), f=shlex.quote(file)), 60).strip()
        landed = guest(self.vm, 'git -C {r} cat-file -e HEAD:{f} 2>/dev/null && echo yes || echo no'
                       .format(r=shlex.quote(repo), f=shlex.quote(file)), 60) == 'yes'
        receipts = self.receipts(repo, rule)
        notices = (record or {}).get('notices') or []
        evidence = {'sent_between_guest_ms': [round(v) for v in sent], 'approvals_pressed': presses,
                    'assignment': {k: (record or {}).get(k) for k in ('id', 'kind', 'state', 'title', 'detail')},
                    'last_notices': [n.get('text') if isinstance(n, dict) else str(n) for n in notices[-4:]],
                    'landed': landed, 'commit_message': message, 'rule_followed': message.endswith(tag),
                    'receipts': receipts}
        (self.out / (step + '-observed.json')).write_text(json.dumps(evidence, indent=2) + '\n')
        self.shot(step + '-settled.png')
        failures = []
        if not record:
            failures.append('no assignment was registered after the task was sent')
        elif record.get('state') != 'settled':
            failures.append('the job ended %s, not settled' % record.get('state'))
        if not landed:
            failures.append('%s was not landed in %s' % (file, name))
        if not message.endswith(tag):
            failures.append('the commit that added %s does not end with %s: %r' % (file, tag, message[-200:]))
        for role in ('worker', 'reviewer'):
            if not [r for r in receipts if r['role'] == role and r['heading'] and r['rule']]:
                failures.append("no %s received the folder's instruction file" % role)
        if [r for r in receipts if r['other_file']]:
            failures.append("an agent received Acme's AGENTS.md beside its CLAUDE.md")
        if [r for r in receipts if r['stu_payload_holds_rule']]:
            failures.append("the payload Stu read holds the folder's instruction file")
        if failures:
            raise StepFailed('; '.join(failures))
        return {'commit_message': message, 'briefs': [(r['role'], r['teammate']) for r in receipts],
                'approvals_pressed': presses}

    def house_rule(self):
        return self.job('house-rule')

    def agents_rule(self):
        return self.job('agents-rule')

    def preserved(self):
        before = self.facts.get('instruction_files')
        if not before:
            raise StepFailed('seed must have run (no instruction files on record)')
        after = self.instruction_files()
        (self.out / 'preserved.json').write_text(json.dumps({'before': before, 'after': after}, indent=2) + '\n')
        if after != before:
            changed = sorted(set(before) ^ set(after) | {p for p in before if after.get(p) != before[p]})
            raise StepFailed('instruction files changed, added or removed: ' + ', '.join(changed))
        return {'unchanged': len(after)}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    # 45 minutes: in a fresh guest no code teammate is active, so Stu hires a worker and a
    # reviewer before the first job starts (the first run on main spent all of 25 minutes so).
    p.add_argument('--within', type=float, default=2700, help='seconds for each job to settle')
    p.add_argument('--approvals', type=int, default=6, help='most Approve presses per job')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = FolderInstructionsWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError, KeyError) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        # A job that fails its rule still leaves the next job and the preservation check worth
        # running: the red run on main fails both jobs, and its preserved step is still a measure.
        if not ok and step in ('identity', 'seed', 'first-run', 'connect', 'connect-tools'):
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
