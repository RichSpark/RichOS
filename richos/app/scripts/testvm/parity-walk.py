#!/usr/bin/env python3
"""parity-walk.py — in the regular app, a team commit carries no co-author line when the user switched
it off, and the team's `python3` is the user's own (Hank's parity audit 2026-10-30, items 2 and 3).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  parity-walk.py --seed-home FIXTURE_HOME
  run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE --report REPORT -- \\
      parity-walk.py --out DIR --expect-sha SHA

--seed-home writes the fixture home a user would have: `.claude/settings.json` with
`"attribution": {"commit": "", "pr": ""}` (the terminal's own settings file, which the app reads it
from when its own `settings.json` sets none), a `.zprofile` that puts the user's Python environment
(`~/userpy/bin`) first on the login PATH and the user's own package folder (`~/userlib`, holding
`parity_mark.py`) on PYTHONPATH, and a Git name and email so Stu does not stop to ask.

It reuses stu-reminder-walk.py's class and its first steps; only these are this walk's:
  userpy      in the guest, before the relaunch: the user's Python environment is made
              (`python3 -m venv ~/userpy`), so the relaunched app's login environment finds it
  job         stu-reminder-walk.py's, with this walk's words: a teammate writes the output of
              `python3 -c 'import sys,parity_mark;print(sys.executable,parity_mark.MARK)'` into
              python.txt, commits it and lands it
  attribution every commit the team made in the Acme repository after first-run, read from the
              repository itself: none carries a co-author line (`Co-Authored-By`, any case)
  python      python.txt as committed names the interpreter in ~/userpy, not the app's runtime,
              and the user's package's mark: the agent's python3 imported the user's package

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
StepFailed = base.StepFailed

STEPS = ['identity', 'first-run', 'userpy', 'relaunch', 'job', 'attribution', 'python']
JOB = ('Please add a file named python.txt to the Acme repository whose whole content is the exact output '
       "of running the shell command python3 -c 'import sys,parity_mark;print(sys.executable,parity_mark.MARK)' "
       'in the repository, then commit it and land it.')
OFF = {'commit': '', 'pr': ''}
# The user's own Python package, on the user's PYTHONPATH (second review rv-20261030T220254Z, P2):
# the job's command imports it, so python.txt names it only when the agent's python3 has the
# user's package environment, as in the terminal.
MARK = 'user-packages'


def seed_home(home):
    home = Path(home)
    (home / '.claude').mkdir(parents=True, exist_ok=True)
    (home / '.claude' / 'settings.json').write_text(json.dumps({'attribution': OFF}) + '\n')
    (home / 'userlib').mkdir(parents=True, exist_ok=True)
    (home / 'userlib' / 'parity_mark.py').write_text('MARK = %r\n' % MARK)
    (home / '.zprofile').write_text('export PATH="$HOME/userpy/bin:$PATH"\nexport PYTHONPATH="$HOME/userlib"\n')
    (home / '.gitconfig').write_text('[user]\n\tname = QA\n\temail = qa@example.invalid\n')
    print('seeded %s' % home)


class ParityWalk(base.StuReminderWalk):
    def userpy(self):
        made = guest(self.vm, '/usr/bin/python3 -m venv %s/userpy && %s/userpy/bin/python3 -c %s' % (
            shlex.quote(self.home), shlex.quote(self.home),
            shlex.quote('import sys;print(sys.executable)')), 300).strip()
        if not made.endswith('/userpy/bin/python3'):
            raise StepFailed('the user Python environment was not made in the guest: %r' % made)
        self.facts['userpy'] = made
        self.save()
        return {'userpy': made}

    def repository(self):
        """The Acme folder first-run made a Git repository and added as the company's folder."""
        return self.company

    def attribution(self):
        repo = self.repository()
        log = guest(self.vm, 'git -C %s log --all --format=%%H%%x09%%an%%x09%%s%%n%%b%%x1e' % shlex.quote(repo), 60)
        (self.out / 'acme-log.txt').write_text(log)
        commits = [c.strip() for c in log.split('\x1e') if c.strip()]
        team = [c for c in commits if 'python.txt' in guest(self.vm, 'git -C %s show --name-only --format= %s' % (
            shlex.quote(repo), shlex.quote(c.split('\t', 1)[0])), 60)]
        if not team:
            raise StepFailed('no commit in the Acme repository adds python.txt; see acme-log.txt')
        signed = [c for c in commits if 'co-authored-by' in c.lower()]
        if signed:
            raise StepFailed('%d commit(s) carry a co-author line the user switched off: %s' % (len(signed), signed[:3]))
        return {'commits': len(commits), 'adding_python_txt': [c.split('\n', 1)[0] for c in team], 'co_authored': 0}

    def python(self):
        repo = self.repository()
        said = guest(self.vm, 'git -C %s log --all -1 --format=%%H -- python.txt' % shlex.quote(repo), 60).strip()
        text = guest(self.vm, 'git -C %s show %s:python.txt' % (shlex.quote(repo), shlex.quote(said)), 60).strip()
        want = self.facts.get('userpy', '')
        evidence = {'commit': said, 'python_txt': text, 'user_python': want, 'user_package': MARK}
        (self.out / 'python.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if not want or text != '%s %s' % (want, MARK):
            raise StepFailed("the team's python3 said %r, not the user's own %r with the user's package %r"
                             % (text, want, MARK))
        return evidence


def main():
    if len(sys.argv) == 3 and sys.argv[1] == '--seed-home':
        seed_home(sys.argv[2])
        return 0
    base.STEPS = STEPS
    base.JOB = JOB
    base.StuReminderWalk = ParityWalk
    return base.main()


if __name__ == '__main__':
    sys.exit(main())
