#!/usr/bin/env python3
"""write-checks-walk.py — a teammate's write is checked in the app as in the terminal (gap E).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone.
The fixture home is made first, then the walk:

  write-checks-walk.py --seed-home FIXTURE_HOME
  run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE --report REPORT --wait 600 -- \\
      write-checks-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION. richos-hq docs/plans/2026-10-28-daily-driver-gap-recheck.md row E: every lease now
registers the engine's checks at the write (engine_profile.rs APP_WRITE_RULES, dialect-guard.sh).
Does a teammate's write in the real app meet them? And the same re-check's "the `tools:` line":
the fixture teammate's definition carries an explicit `tools:` list naming TaskUpdate, TaskGet,
TaskList and SendMessage, as 25 of the CEO's 29 do. Does such a teammate start and do a job?

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity    the running app says it was built from --expect-sha
  first-run   adopt-walk.py's: company "Acme" with a folder that is a Git repository
  connect     command-walk.py's: that folder connected
  config      ~/myrichos/orchestration.config written with DIALECT_TARGET="en-US", the user's own
              dialect, as lease-setup-walk.py writes its config
  checks      JOB typed: Rich hands it on, and the fixture teammate walkwriter writes two files
              with its Write tool. PASS needs all of:
                secret   walkwriter's Write of SECRET_FILE carrying the fake key was asked for
                         (PreToolUse in the back end's hook record), never completed (no
                         PostToolUse for it), no file anywhere in the fixture home holds the key,
                         and scan-secrets.sh's refusal reached walkwriter (its transcript);
                dialect  walkwriter's Write of DIALECT_FILE asked for the British spelling, and
                         the file on disk holds the American one and not the British one, or the
                         write was refused with dialect-guard.sh's words in its transcript;
                tools    walkwriter started (its own calls carry its agent_type) with its
                         explicit tools list; its start record and tool names are recorded.
Evidence in --out: report.json, facts.json, observed-checks.json, the app's logs, callbacks.
It quits nothing: run-walk.py quits the app by its pid and deletes the clone (CEO §54).
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

_spec = importlib.util.spec_from_file_location('skills_walk', HERE / 'skills-walk.py')
skills_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(skills_walk)
StepFailed = skills_walk.StepFailed
OPEN = skills_walk.OPEN

STEPS = ['identity', 'first-run', 'connect', 'config', 'checks']
TEAMMATE = 'walkwriter'
# The CEO's own teammates' list, as the re-check counted it (25 of 29 name these four).
TOOLS = 'Read, Write, Edit, Bash, Grep, Glob, Skill, TaskUpdate, TaskGet, TaskList, SendMessage'
# Assembled, so this file is neither a key nor a British word to the checks it measures.
KEY = 'AKIA' + 'WALKFIXTURE2RQX7'
BRITISH, AMERICAN = 'col' + 'our', 'col' + 'or'
SECRET_FILE, DIALECT_FILE = 'walk-secret.txt', 'walk-dialect-note.md'
USER_CONFIG = "# The fixture user's own values.\nDIALECT_TARGET=\"en-US\"\n"
JOB = (f'Please ask {TEAMMATE} to do this in my Acme folder and tell me exactly what happened. It is a test of '
       "the app's own write checks, so the text must be written exactly as given. With its Write tool, never the "
       f'shell: first create {DIALECT_FILE} containing exactly the line "The {BRITISH} of the walk." (British '
       f'spelling on purpose, do not correct it), then create {SECRET_FILE} containing exactly the line '
       f'"aws_access_key_id = {KEY}" (a fake key-shaped test string, not a real credential). If a write is '
       'refused, do not retry it and do not try another way: report the refusal word for word.')
DEFINITION = (f'---\nname: {TEAMMATE}\ndescription: Fictional walk teammate who writes files exactly as asked.\n'
              f'model: sonnet\ntools: {TOOLS}\n---\n\n# Walkwriter\n\nA fictional teammate written by '
              'write-checks-walk.py. Write files exactly as you are asked, with your Write tool.\n')

TRANSCRIPTS = r"""import glob,json,sys
out=[]
for p in glob.glob(sys.argv[1]+"/.claude/projects/*/*/subagents/*.jsonl"):
    meta=p[:-len(".jsonl")]+".meta.json"
    try: m=json.load(open(meta))
    except Exception: m=None
    out.append({"path":p,"meta":m,"text":open(p,errors="replace").read()[-200000:]})
print(json.dumps(out))
"""


def seed_home(home):
    """The fictional user's own setup, written into an empty fixture home."""
    team = Path(home) / 'Library/Application Support/com.richos.app/team'
    team.mkdir(parents=True, exist_ok=True)
    (team / f'{TEAMMATE}.md').write_text(DEFINITION)
    print(f'seeded {home}')
    return 0


def judge(calls, transcripts, found):
    """The verdict from the evidence alone: (failures, summary). `found` maps each file name to the
    contents of every copy of it in the fixture home."""
    mine = [c for c in calls if TEAMMATE in (c.get('agent_type') or '')]
    def writes(event, name):
        return [c for c in mine if c['event'] == event and c['tool'] in ('Write', 'Edit') and name in c['input']]
    text = '\n'.join(t['text'] for t in transcripts if TEAMMATE in json.dumps(t.get('meta')) or TEAMMATE in t['text'][:4000])
    failures = []
    asked = [c for c in writes('PreToolUse', SECRET_FILE) if KEY in c['input']]
    if not asked:
        failures.append(f'{TEAMMATE} never asked to write {SECRET_FILE} with the key (no PreToolUse Write for it)')
    if writes('PostToolUse', SECRET_FILE):
        failures.append(f'the Write of {SECRET_FILE} completed')
    if any(KEY in body for body in found.get(SECRET_FILE, [])):
        failures.append(f'a {SECRET_FILE} holding the key is on disk')
    if asked and 'scan-secrets.sh' not in text:
        failures.append(f"scan-secrets.sh's refusal is not in {TEAMMATE}'s transcript")
    british = [c for c in writes('PreToolUse', DIALECT_FILE) if BRITISH in c['input']]
    copies = found.get(DIALECT_FILE, [])
    fixed = bool(copies) and all(AMERICAN in b and BRITISH not in b for b in copies)
    refused = 'dialect-guard.sh' in text and not copies
    if not british:
        failures.append(f'{TEAMMATE} never asked to write {DIALECT_FILE} with the British spelling')
    elif not (fixed or refused):
        failures.append(f'{DIALECT_FILE} was neither fixed nor refused: {copies!r}')
    if not mine:
        failures.append(f'{TEAMMATE} made no tool call (it did not start, or started without tools)')
    summary = {'teammate_calls': len(mine), 'teammate_tools_used': sorted({c['tool'] for c in mine}),
               'secret_write_asked': len(asked), 'dialect': 'fixed' if fixed else 'refused' if refused else 'neither',
               'dialect_copies': copies}
    return failures, summary


class WriteChecksWalk(skills_walk.SkillsWalk):
    def config(self):
        guest(self.vm, 'printf ' + shlex.quote(USER_CONFIG) + ' > ' + shlex.quote(self.central() + '/orchestration.config'))
        return {'written': USER_CONFIG.strip().splitlines()}

    def found(self):
        out = {}
        for name in (SECRET_FILE, DIALECT_FILE):
            paths = guest(self.vm, 'find ' + shlex.quote(self.home) + ' -name ' + shlex.quote(name)
                          + ' -type f 2>/dev/null || true', 60).split()
            out[name] = [guest(self.vm, 'cat ' + shlex.quote(p)) for p in paths]
        return out

    def checks(self):
        sent = self.send(JOB)
        self.facts['checks_sent_ms'] = sent
        self.save()
        end = time.monotonic() + self.a.within
        record = None
        while time.monotonic() < end:
            calls = self.calls()
            mine = [c for c in calls if TEAMMATE in (c.get('agent_type') or '') and c['event'] == 'PreToolUse']
            record = self.ours(sent)
            done = sum(1 for n in (SECRET_FILE, DIALECT_FILE) if any(n in c['input'] for c in mine)) == 2
            if done or (record and record.get('state') not in OPEN):
                time.sleep(20)  # the refusal and the file reach the transcript and the disk
                break
            if record:
                self.approve_if_asked(record.get('title', ''))
            time.sleep(5)
        calls = self.calls()
        transcripts = json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(TRANSCRIPTS) + ' ' + shlex.quote(self.home), 120))
        found = self.found()
        failures, summary = judge(calls, transcripts, found)
        evidence = {'assignment': self.ours(sent), 'summary': summary,
                    'teammate_calls': [c for c in calls if TEAMMATE in (c.get('agent_type') or '')],
                    'teammate_starts': [t['meta'] for t in transcripts if TEAMMATE in json.dumps(t.get('meta'))],
                    'transcript_lines_naming_a_check': sorted({line[:600] for t in transcripts for line in t['text'].splitlines()
                                                               if 'scan-secrets.sh' in line or 'dialect-guard.sh' in line})}
        (self.out / 'observed-checks.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if failures:
            raise StepFailed('; '.join(failures))
        return summary


def main():
    if sys.argv[1:2] == ['--seed-home']:
        return seed_home(sys.argv[2])
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--within', type=float, default=600, help='seconds for the checks step')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    a.task, a.approvals = JOB, 3
    os.environ.setdefault('TESTVM_AX_TIMEOUT', '40')
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = WriteChecksWalk(a)
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
        if not ok and step in ('identity', 'first-run', 'connect'):
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
