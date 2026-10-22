#!/usr/bin/env python3
"""multico-walk.py — one repository connected to two companies, and a job worked in each.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      multico-walk.py --out DIR --expect-sha SHA [--within SECONDS]

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

HIS WORDS (2026-10-21, ruling §138): "the regular app needs to have EVERYTHING I USE AND HAVE."
His terminal session reaches the repositories of several companies at once; plan item 11
(richos-hq docs/plans/2026-10-21-everything-for-every-user.md) is that a repository may be connected
for work in more than one company, while `roots` stays exclusive for deciding which company a path
belongs to (`entity.rs`, `connect_repository`).

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65). It is command-walk.py's
walk with a second company, so every driving method is that walk's:
  identity        the running app says it was built from --expect-sha
  first-run       company "Acme" with a folder that is a Git repository (adopt-walk.py's)
  connect         that folder connected to Acme through Settings > Connected folders
  (A job passes either as a settled assignment on the company's thread, or, when the conversation
   answers a one-line command itself, as the back end's Bash hook record made after the send.)
  job-acme        `git log --oneline` run in the folder from Acme's conversation: the assignment is
                  on Acme's thread and settled, and its last notice carries the repository's HEAD
  second-company  "+ New chat or company" > New company: "Beta", with a folder of its own; Beta
                  gets its conversation
  connect-shared  the SAME folder connected to Beta through Settings > Connected folders. PASS needs
                  the registry to list it under both companies' connected repositories, under
                  Acme's roots only, and not under Beta's roots
  job-beta        the same command in that folder from Beta's conversation: the assignment is on
                  Beta's thread (not Acme's) and settled, its last notice carries the HEAD

Evidence is written to --out. Exit 0 when every step passes. Every app instance is quit by
run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
import shlex
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
cw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cw)
StepFailed = cw.StepFailed
OPEN = cw.OPEN

STEPS = ['identity', 'first-run', 'connect', 'job-acme', 'second-company', 'connect-shared', 'job-beta']
# Worded as command-walk.py's TASK is (a job in "my Acme folder"): without the folder named, the
# front desk answered the request itself and no assignment was registered (run of 2026-10-21).
JOB = ('Please run this harmless test command for me yourself with your shell tool, in my Acme '
       'folder, and tell me when it has finished and what it printed: git -C {folder} log --oneline')


class Walk(cw.CommandWalk):
    def __init__(self, a):
        super().__init__(a)
        self.beta = self.home + '/Beta'

    def ax(self, mode, *args, app=None, timeout=150):
        # A read (find, tree) with no answer on a loaded host is asked again, at most five times in
        # all; a click is sent once (axwait.ask_again); any other failure is raised as before.
        for attempt in range(5):
            try:
                return super().ax(mode, *args, app=app, timeout=timeout)
            except StepFailed as exc:
                if not axwait.ask_again(exc, mode) or attempt == 4:
                    raise
                time.sleep(5)

    def on_screen(self, words):
        """Whether a static text, title or description on the guest's screen carries `words`."""
        for flags in (('--value',), ('--title',)):
            try:
                if self.ax('find', *flags, words, '--contains'):
                    return True
            except StepFailed as exc:
                if not axwait.absent(exc):
                    raise
        return False

    def real(self):
        """The folder as the registry stores it: canonical, so a symlinked home does not hide a match."""
        return guest(self.vm, 'python3 -c ' + shlex.quote('import os,sys;print(os.path.realpath(sys.argv[1]))')
                     + ' ' + shlex.quote(self.company)).strip()

    def registry(self):
        return json.loads(guest(self.vm, 'cat ' + shlex.quote(self.data + '/entities.json')))

    def threads(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true')
        return [json.loads(r) for r in rows.splitlines() if '"ThreadCreated"' in r]

    def job_commands(self, marker):
        """The back end's Bash hook records made in the conversation turn that began with `marker`:
        callbacks are read in order per session and only those after a prompt carrying the marker
        (and before the next prompt without it) count, so another company's job cannot pass this one."""
        script = ('import json,glob,sys\n'
                  'out=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl"):\n'
                  '    on=False\n'
                  '    for l in open(p):\n'
                  '        try: c=json.loads(l).get("callback",{})\n'
                  '        except Exception: continue\n'
                  '        e=c.get("hook_event_name")\n'
                  '        if e=="UserPromptSubmit": on=sys.argv[2] in c.get("prompt","")\n'
                  '        elif on and e=="PostToolUse" and c.get("tool_name")=="Bash" and not c.get("agent_id"):\n'
                  '            r=c.get("tool_response") or {}\n'
                  '            out.append({"command":c.get("tool_input",{}).get("command",""),\n'
                  '                        "background":bool(c.get("tool_input",{}).get("run_in_background")),\n'
                  '                        "stdout":(r.get("stdout","") if isinstance(r,dict) else str(r))[:2000]})\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data)
                                + ' ' + shlex.quote(marker), 60))

    def job(self, thread, label):
        """Send the command from the open conversation and wait for the assignment on `thread`."""
        short, subject = self.head()

        marker = f'[{label} job]'

        def ran(commands):
            return [c for c in commands if 'git' in c.get('command', '') and short in c.get('stdout', '')]

        sent = self.send(f'{marker} ' + JOB.format(folder=shlex.quote(self.company)))
        pressed, record = [0], None

        def mine():
            # `thread` is known for Acme; Beta's is minted by the send, so it is the new Beta one.
            if thread:
                return thread
            fresh = [t for t in self.threads() if t.get('entity_id') == self.facts.get('beta_entity')
                     and t['thread_id'] not in self.facts.get('threads_before_beta_job', [])]
            return fresh[0]['thread_id'] if fresh else None

        def pick():
            rows = [r for r in self.records() if r.get('thread_id') == mine() and r.get('registered_at_ms', 0) >= sent[0]]
            return max(rows, key=lambda r: r.get('registered_at_ms', 0)) if rows else None

        # A one-line command may be answered by the conversation itself, with no assignment
        # (seen 2026-10-21 on this build, twice). Then the proof is the back end's own Bash hook
        # record made after this send, with the repository's head in its output.
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            record = pick()
            if record or ran(self.job_commands(marker)):
                break
            time.sleep(3)
        if record:
            record = self.until_closed(pick, max(end - time.monotonic(), 1), pressed)
        record = self.settled_read(record) if record and record.get('state') not in OPEN else record
        commands = self.job_commands(marker)
        if not record and ran(commands) and mine():
            evidence = {'thread': mine(), 'repository_head': {'short': short, 'subject': subject},
                        'assignment': None, 'answered_in_conversation': True, 'back_end_bash': commands}
            (self.out / f'{label}-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
            return {'thread': mine(), 'state': 'answered in the conversation', 'head': f'{short} {subject}',
                    'approvals_pressed': 0}
        evidence = {'thread': mine(), 'repository_head': {'short': short, 'subject': subject},
                    'approvals_pressed': pressed[0], 'assignment': record, 'back_end_bash': commands}
        (self.out / f'{label}-observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not record:
            raise StepFailed(f'no assignment was registered on the {label} conversation')
        if record.get('state') in OPEN:
            raise StepFailed(f"the assignment was still {record.get('state')} after {self.a.within:g} s")
        failures = cw.verdict(record, short, subject, commands)
        if failures:
            raise StepFailed('; '.join(failures))
        return {'thread': mine(), 'state': record['state'], 'notice': record['notices'][-1]['text'],
                'head': f'{short} {subject}', 'approvals_pressed': pressed[0]}

    def job_acme(self):
        if not self.facts.get('thread'):
            raise StepFailed('first-run must have run (no thread on record)')
        return self.job(self.facts['thread'], 'acme')

    def second_company(self):
        guest(self.vm, 'mkdir -p {c} && cd {c} && if [ ! -d .git ]; then printf "Beta notes\\n" > README.md '
                       '&& git init -q && git add README.md && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m init; fi'.format(c=shlex.quote(self.beta)))
        before = {t['thread_id'] for t in self.threads()}
        # The rail's "+ New chat or company" button, by its DOM id. With the sidebar hidden it is not
        # in the tree (run of 11:01Z), so the sidebar toggle is pressed first and the button asked for again.
        for attempt in range(2):
            try:
                self.ax('click', '--id', 'rail-new-thread')
                break
            except StepFailed as exc:
                self.shot(f'second-company-no-rail-{attempt}.png')
                if attempt or not axwait.absent(exc):
                    raise
                self.ax('click', '--id', 'rail-toggle')
                time.sleep(2)
        time.sleep(1)
        opened = False
        for role in ('AXMenuItem', 'AXButton', 'AXStaticText'):
            if self.present('New company', role):
                self.press('New company', role)
                opened = True
                break
        if not opened:
            raise StepFailed('the "+ New chat or company" menu offers no "New company"')
        self.wait_for('Add this company', seconds=20)
        self.type_into('Beta', '--role', 'AXTextField', '--title', "What's the company called?")
        self.type_into(self.beta, '--role', 'AXTextField', '--title', 'Its folder on this Mac')
        self.press('Add this company')
        # A company's first conversation is minted by its first send (`create_thread_in`, main.js), so
        # the proof that Beta exists is the registry row; its thread is read at the job (run of 11:20Z).
        end = time.monotonic() + 60
        while time.monotonic() < end:
            rows = {e['id']: e for e in self.registry().get('entities', [])}
            if 'beta' in rows:
                self.facts['beta_entity'] = 'beta'
                self.facts['threads_before_beta_job'] = sorted(before)
                self.save()
                time.sleep(3)
                return {'entity': 'beta', 'roots': rows['beta'].get('roots')}
            time.sleep(1)
        raise StepFailed('Beta was not registered within 60 s: ' + json.dumps(self.registry())[:600])

    def open_sheet(self, company):
        """Settings > Connected folders (the shared adopt-walk.py step: the row is a button), with
        `company` chosen in the sheet's Company menu."""
        self.press('Settings', role='AXPopUpButton')
        time.sleep(2)
        self.open_connected_folders(settings_open=True)
        self.ax('click', '--title', 'Company', '--role', 'AXPopUpButton', '--first')
        self.ax('click', '--title', company, '--role', 'AXMenuItem', '--first')

    def close_sheet(self):
        """Close the Connected folders sheet and see it gone: one press was not enough on a loaded
        host (run of 2026-10-21: the sheet still stood and blocked the next menu click)."""
        for _ in range(4):
            try:
                self.press('Close')
            except StepFailed:
                pass
            time.sleep(2)
            if not self.present('Project folder location', 'AXTextField'):
                return
        raise StepFailed('the Connected folders sheet would not close')

    def connect(self):
        self.open_sheet('Acme')
        self.type_into(self.company, '--role', 'AXTextField', '--title', 'Project folder location')
        self.press('Connect folder')
        end = time.monotonic() + 45
        while time.monotonic() < end:
            rows = {e['id']: e for e in self.registry().get('entities', [])}
            if self.real() in rows.get('acme', {}).get('connected_repositories', []):
                self.close_sheet()
                self.facts['connected'] = rows['acme']['connected_repositories']
                self.save()
                return {'connected': self.facts['connected']}
            time.sleep(1)
        raise StepFailed('the folder was not connected to Acme within 45 s: ' + json.dumps(rows)[:600])

    def connect_shared(self):
        self.open_sheet('Beta')
        self.type_into(self.company, '--role', 'AXTextField', '--title', 'Project folder location')
        # Round 36 screen 2: before anything is connected the sheet says the folder is already
        # another company's, and the button says what it will do.
        time.sleep(2)
        shared = {'notice': self.on_screen('Already connected to Acme'),
                  'button': self.on_screen('Connect to Beta too')}
        self.shot('connect-shared-notice.png')
        if not all(shared.values()):
            raise StepFailed('the sheet does not show the shared-folder notice and button: ' + json.dumps(shared))
        self.press('Connect to Beta too')
        end = time.monotonic() + 45
        folder, rows = self.real(), {}
        while time.monotonic() < end:
            rows = {e['id']: e for e in self.registry().get('entities', [])}
            if folder in rows.get('beta', {}).get('connected_repositories', []):
                break
            time.sleep(1)
        time.sleep(2)
        shared['also_in_on_beta_row'] = self.on_screen('Also in Acme')
        self.shot('connect-shared-after.png')
        self.close_sheet()
        acme, beta = rows.get('acme', {}), rows.get('beta', {})
        failures = []
        if folder not in beta.get('connected_repositories', []):
            failures.append('the registry does not list the folder under Beta')
        if folder not in acme.get('connected_repositories', []):
            failures.append('Acme lost its connection to the folder')
        if folder not in acme.get('roots', []):
            failures.append('the folder is no longer one of Acme roots')
        if folder in beta.get('roots', []):
            failures.append('the folder was added to Beta roots, so a path would resolve to two companies')
        if failures:
            raise StepFailed('; '.join(failures) + ' | ' + json.dumps(rows)[:800])
        if not shared['also_in_on_beta_row']:
            failures.append('Beta\'s folder row does not say "Also in Acme"')
            raise StepFailed('; '.join(failures))
        return {'acme': acme, 'beta': beta, 'screen': shared}

    def job_beta(self):
        if not self.facts.get('beta_entity'):
            raise StepFailed('second-company must have run (no Beta on record)')
        return self.job(None, 'beta')


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=240, help='seconds for each assignment to close')
    p.add_argument('--approvals', type=int, default=3, help='most Approve presses on one assignment')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = Walk(a)
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
        print(f"{step}: {row['outcome']}" + (f" - {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
