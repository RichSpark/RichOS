#!/usr/bin/env python3
"""token-waste-walk.py — in the regular app, a finished Stu job closes although a teammate Stu lined
up never started, and no "gone quiet" notice is told for that teammate (nightly 80).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE_TARBALL --report REPORT -- \\
      token-waste-walk.py --out DIR --expect-sha SHA

It reuses stu-reminder-walk.py's class and its first steps; only the job's words and the last two
steps are this walk's.

STEPS, in the guest, never on the host's screen (CEO ruling 65):
  identity, first-run, relaunch   as stu-reminder-walk.py; first-run also seeds a git name and email
              in the fixture home's ~/.gitconfig, so Stu does not stop to ask who he is (job seeds it too)
  job         typed: a job for the back end (Stu) that also asks him to line up (prepare) a second
              teammate and then NOT send it. PASS needs the job SETTLED, as before.
  idle        Stu's turn ended (the lead file says so)
  receipts    the app's own dispatch receipts (<app data>/engine-state/work-receipts). PASS needs a
              receipt the app never admitted (no tool_use_id, no agent_id) that is SETTLED with a
              reason, and no unsettled one left. NOT REACHED (a failure that says so) when Stu
              prepared no such teammate: the model is not told to, and a walk does not invent it.
  quiet       the bundle's own stall-watch tick over a fixture registry in the guest (never the app's
              registry): the started-and-silent control is reported SILENT, and the prepared and the
              refused teammates are NOT.

Not reachable in a guest, and not claimed: the scheduled-cleanup escalation (a host launchd job).
Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
"""
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

_spec = importlib.util.spec_from_file_location('stu_reminder_walk', HERE / 'stu-reminder-walk.py')
base = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(base)
command_walk = base.command_walk
StepFailed = base.StepFailed

STEPS = ['identity', 'first-run', 'relaunch', 'job', 'idle', 'receipts', 'quiet']
JOB = ('Please add a file named notes.md whose whole content is the line "Notes for the walk test." '
       'to the Acme repository and commit it, and land it. While you plan, line up (prepare) a second '
       'teammate for a file named extra.md in the same repository, then decide it is not needed and '
       'never send it. Finish the job without it.')
GIT_NAME = 'QA'
GIT_EMAIL = 'qa@example.invalid'
FIXTURE_SESSION ='beadfeed-0000-4000-8000-00000000w001'
SILENT_CONTROL = 'control-silent'
NEVER_STARTED = ('lined-up-never-started', 'refused-never-started')


def judge_receipts(rows):
    """(verdict, why) over the guest's receipt rows. verdict: PASS, FAIL or NOT REACHED."""
    if any(r.get('unreadable') for r in rows):
        return 'FAIL', 'a receipt could not be read'
    unadmitted = [r for r in rows if not r.get('admitted')]
    if not unadmitted:
        return 'NOT REACHED', 'Stu prepared no teammate that he never sent, so there was nothing to settle'
    open_ones = [r for r in unadmitted if not r.get('settled')]
    if open_ones:
        return 'FAIL', 'a receipt the app never admitted is not settled: %s' % json.dumps(
            [{k: r.get(k) for k in ('name', 'status', 'request_id')} for r in open_ones])
    unreasoned = [r for r in unadmitted if not (r['settled'].get('reason') or '').strip()]
    if unreasoned:
        return 'FAIL', 'a settled receipt carries no reason: %s' % json.dumps([r.get('name') for r in unreasoned])
    return 'PASS', '%d never-started receipt(s) settled with a reason' % len(unadmitted)


def judge_quiet(done):
    """(verdict, why) over the guest's stall-watch tick."""
    out = done.get('out', '')
    if done.get('rc') != 0:
        return 'FAIL', 'stall-watch --tick exited %s: %s' % (done.get('rc'), out[-300:])
    if SILENT_CONTROL not in out or '[SILENT]' not in out:
        return 'FAIL', 'the control (started, silent for an hour) was not reported SILENT, so the check measured nothing'
    told = [n for n in NEVER_STARTED if n in out]
    if told:
        return 'FAIL', 'stall-watch told a "gone quiet" notice for a teammate that never started: %s' % ', '.join(told)
    return 'PASS', 'the silent control was told; the prepared and the refused teammates were not'


class TokenWasteWalk(base.StuReminderWalk):
    def seed_git_identity(self):
        """Give the fixture home a git name and email, as a user's own ~/.gitconfig does. The base
        walk's -c flags cover only its seed commit; Stu's commit in the job runs in the app's home,
        and with no identity Stu stops to ask "What name should go on your team's saved work?" and
        the job stalls for its whole window (run 1, 2026-10-29: 1200 s). Idempotent, so --steps can
        run job alone against a guest that already has it."""
        gitconfig = shlex.quote(self.home + '/.gitconfig')
        guest(self.vm, 'git config --file {g} user.name {n} && git config --file {g} user.email {e}'.format(
            g=gitconfig, n=shlex.quote(GIT_NAME), e=shlex.quote(GIT_EMAIL)), 60)

    def first_run(self):
        result = super().first_run()
        self.seed_git_identity()
        return result

    def job(self):
        self.seed_git_identity()
        return super().job()

    def receipts(self):
        made = self.payload + '/token-waste'
        guest(self.vm, 'mkdir -p ' + shlex.quote(made), 60)
        command_walk.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'token-waste-guest.py', made + '/'], 300)
        rows = json.loads(guest(self.vm, '/usr/bin/python3 %s receipts %s' % (
            shlex.quote(made + '/token-waste-guest.py'), shlex.quote(self.data)), 120) or '[]')
        (self.out / 'receipts.json').write_text(json.dumps(rows, indent=2) + '\n')
        verdict, why = judge_receipts(rows)
        if verdict != 'PASS':
            raise StepFailed('%s: %s; see receipts.json' % (verdict, why))
        return {'receipts': len(rows), 'why': why}

    def quiet(self):
        made = self.payload + '/token-waste'
        guest(self.vm, 'mkdir -p ' + shlex.quote(made), 60)
        command_walk.command([HERE / 'guest.sh', self.vm, '--push', HERE / 'token-waste-guest.py', made + '/'], 300)
        text = guest(self.vm, '/usr/bin/python3 %s quiet %s %s %s' % (
            shlex.quote(made + '/token-waste-guest.py'), shlex.quote(self.payload + '/engine'),
            FIXTURE_SESSION, shlex.quote(made + '/quiet-box')), 180)
        done = json.loads(text)
        (self.out / 'quiet.json').write_text(json.dumps(done, indent=2) + '\n')
        verdict, why = judge_quiet(done)
        if verdict != 'PASS':
            raise StepFailed('%s; see quiet.json' % why)
        return {'why': why, 'stall_watch': done['out']}


def main():
    base.STEPS = STEPS
    base.JOB = JOB
    base.StuReminderWalk = TokenWasteWalk
    return base.main()


if __name__ == '__main__':
    sys.exit(main())
