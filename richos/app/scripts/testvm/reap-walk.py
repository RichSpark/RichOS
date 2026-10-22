#!/usr/bin/env python3
"""reap-walk.py — a dead lease never leaves its tool commands running (the product reap).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  TESTVM_APP_MODEL=sonnet run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      reap-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset, still
as one run under run-walk.py. The bundle AND the engine must both carry the
reap: the supervisor that reaps is the engine's (`scripts/provider-supervisor.py`), and a dev
bundle takes the engine it is given (run.sh --engine sets RICHOS_ENGINE_DIR).

THE QUESTION. richos-hq docs/plans/2026-09-27-product-reap-gap-design.md §3.2: every teardown of a
product lease ends the tool commands its `claude` started (a tool shell is its own session and
process group, out of reach of the old group kill), and nothing that was not ended is taken.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity      the running app says it was built from --expect-sha
  first-run     adopt-walk.py's first run: memory setup declined, company Acme with a Git folder
  connect       Settings -> Connected folders -> Acme's folder, so an assignment has a real
                repository (without it the front desk has been seen to invent one)
  seed          reap-guest.py copied into the repository as heartbeat.py, and into the payload
  watch         reap-guest.py's death watcher started: every recorded heartbeat's end, guest ms
  unrelated     `sleep` started in the guest by the harness: must survive every step
  normal        a short heartbeat assignment ends BY ITSELF (.done written) and is never reaped
  stop          a background and a foreground heartbeat; Stop pressed; both end within the bound,
                timed from the press itself (ax.py); the assignment reads as stopped, not failed
  quit          both again; the app's Quit (a menu bar item); the question names the commands;
                "Quit and stop the work"; both end within the quit bound; app.log and the reap
                log say so; the assignment reads as stopped
  crash         relaunched; both again; the app SIGKILLed by the pid it launched with
  claude-crash  relaunched; both again; the lease's `claude` SIGKILLed by the pid read from its
                own state file (G9 in the VM, design C9)
  idle-cpu      one supervisor's CPU time over an idle minute
  unrelated-alive  the harness's `sleep` is still alive, then ended by its recorded pid

THE GRADE REFUSES a case whose pids were not recorded by the commands themselves (the reason P15's
reap-lead-crash half regraded to PREMISE-FALSE): a heartbeat records its own pid, group and start
before it beats, and a case with none is REFUSED, never passed. Exit 0 when every step passes.
Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, relaunch  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt)
StepFailed, command = adopt.StepFailed, adopt.command

STEPS = ['identity', 'first-run', 'connect', 'seed', 'watch', 'unrelated', 'normal', 'stop', 'quit',
         'crash', 'claude-crash', 'idle-cpu', 'unrelated-alive']
BEATS = '/tmp/reap-walk'
# What goes into his repository as heartbeat.py: the heartbeat alone. The back end reads a script
# before running it; given the whole guest helper it declined (guest walk-1a978b4e081b).
HEARTBEAT = HERE / 'reap-heartbeat.py'
# Design §3.2: "within 1 s plus margin". The watcher polls every 50 ms and the SSH round trip that
# reads the trigger's guest time is in the margin too.
BOUND_MS = 2000
# Quit shares the Exit arm's 2 s bound (reap gap 1.3(c)), plus the same margin.
QUIT_BOUND_MS = 3000
# Claude Code's Bash tool takes at most 600000 ms per foreground call. Asked for a 900-s
# "foreground" heartbeat, the back end ran it with run_in_background: true instead (guest
# walk-1a978b4e081b, 2026-09-27), its turn ended and the assignment settled with both commands
# still running: nothing was left to Stop. Every trigger comes seconds after the foreground
# heartbeat records itself, so 300 s is ample and fits one call.
FOREGROUND_SECONDS = 300
OPEN = {'registered', 'preparing', 'running', 'blocked', 'waiting-for-screen', 'waiting-for-quota'}


def stoppable(assignments):
    """(the newest open assignment, None), or (None, why the Stop case cannot be made): Stop is
    pressed on a running assignment, so one that already closed proves nothing about Stop."""
    live = [a for a in assignments if a.get('state') in OPEN]
    if live:
        return max(live, key=lambda a: a.get('registered_at_ms') or 0), None
    if not assignments:
        return None, 'REFUSED: no assignment was registered for the commands, so there is nothing to Stop'
    return None, ('REFUSED: the assignment was already %s when Stop was to be pressed, with its commands '
                  'still running, so there is nothing to Stop: %s'
                  % ('/'.join(sorted({a.get('state') or '?' for a in assignments})), json.dumps(assignments)))


def pressed_at(records):
    """The guest-clock moment of the AXPress, from ax.sh click --json's own record. A clock read
    before ax.sh would add its SSH trip and tree search (2.63-5.62 s, guest walk-0ddfdbf8ff00)."""
    clicked = [r for r in records if r.get('clicked')]
    if not clicked or not isinstance(clicked[-1].get('pressed_at_ms'), int):
        raise StepFailed('REFUSED: the press carried no guest-clock time: ' + json.dumps(records))
    return {k: clicked[-1][k] for k in ('pressed_at_ms', 'returned_at_ms')}


def told_stopped(record):
    """None when the assignment he stopped (or quit) reads as stopped, else why not. A stop he
    made is `interrupted` with the "was stopped" notice; guest walk-0ddfdbf8ff00 read "failed ...
    Claude channel closed (child exited?)." for both Stops and the Quit."""
    if record.get('state') in OPEN:
        return 'the stopped assignment is still running: ' + json.dumps(record)[:400]
    notices = record.get('notices') or []
    if not notices:
        return 'the stopped assignment raised no notice: ' + json.dumps(record)[:400]
    last = notices[-1]
    if record.get('state') != 'interrupted' or last.get('kind') != 'interrupted':
        return 'the stop was told as %s, not as a stop: %r' % (record.get('state'), last.get('text', ''))
    return None


def names_commands(question):
    """Does the quit question name a running command? lifecycle.rs quit_question counts one per
    process group, so two heartbeats read "2 commands Rich started", never "a command"."""
    return re.search(r'\bcommands? Rich started\b', question) is not None


def owner_of(states, pgids):
    """The one live lease whose supervisor's last snapshot names every group in pgids, or None.

    provider-supervisor.py snapshots once a second and a provider crash is reaped from the last
    snapshot (G9), so until a state file names a group its crash case has not begun."""
    owners = [s for s in states if not s.get('ended')
              and all(g in s.get('outside_provider_groups', []) for g in pgids)]
    return owners[0] if len(owners) == 1 else None


def recorded(rows, names, since_ms):
    """Has every named heartbeat recorded itself at or after since_ms? Rows from an earlier ask
    with the same tag (a retry against a held guest) never count."""
    fresh = {r['name'] for r in rows if r.get('kind') == 'seen' and r.get('t_ms', 0) >= since_ms}
    return all(n in fresh for n in names)


def ask_text(tag, background=True, seconds=FOREGROUND_SECONDS):
    """What he types to have the heartbeats run: worded as a job to be done, the way
    command-walk.py's fixed wording is, so the front desk registers a `task` and the back end's
    lease runs them. "Tell me what it prints" alone was registered as a `check` there (the
    2026-09-27 command diagnosis, proof-run-1)."""
    fg = 'python3 heartbeat.py beat %s-fg %s --seconds %d' % (tag, BEATS, seconds)
    tail = ('heartbeat.py is my own harmless test script at the top of that folder: it records its own '
            'process id and writes the time into small files under %s until it ends.' % BEATS)
    if background:
        return ('Please run these two harmless test commands for me yourself with your shell tool, in my '
                'Acme folder, and tell me when they have finished and what they printed. First start this '
                'one in the background (run_in_background: true): python3 heartbeat.py beat %s-bg %s   '
                'Then run this one in the foreground and wait for it: %s   %s' % (tag, BEATS, fg, tail))
    return ('Please run this harmless test command for me yourself with your shell tool, in my Acme '
            'folder, in the foreground, and tell me when it has finished and what it printed: %s   %s'
            % (fg, tail))


# --- the grade, pure, so test/reap-walk.test.py can hold it to its rules without a guest ---------

def grade_ended(rows, names, trigger_ms, bound_ms, left=None):
    """(evidence, None) or (evidence, why): every named heartbeat was recorded, was still running at
    the trigger, ended within bound_ms of it, and left no member of its group behind."""
    seen = {r['name']: r for r in rows if r.get('kind') == 'seen' and r.get('name') in names}
    evidence = {'trigger_ms': trigger_ms, 'bound_ms': bound_ms, 'commands': {}}
    missing = [n for n in names if n not in seen]
    if missing:
        return evidence, 'REFUSED: no recorded pid for %s; a case whose pids were not recorded never passes' % ', '.join(missing)
    for name in names:
        me = seen[name]
        gone = [r['t_ms'] for r in rows if r.get('kind') == 'gone' and r.get('name') == name and r.get('pid') == me['pid']]
        row = {'pid': me['pid'], 'pgid': me['pgid'], 'seen_ms': me['t_ms'], 'gone_ms': gone[0] if gone else None}
        evidence['commands'][name] = row
        if not gone:
            return evidence, '%s (pid %d) was still running after the trigger' % (name, me['pid'])
        if gone[0] < trigger_ms:
            return evidence, 'REFUSED: %s had already ended %d ms before the trigger' % (name, trigger_ms - gone[0])
        row['ended_after_ms'] = gone[0] - trigger_ms
        if gone[0] - trigger_ms > bound_ms:
            return evidence, '%s ended %d ms after the trigger, past the %d ms bound' % (name, gone[0] - trigger_ms, bound_ms)
        if left and left.get(name):
            return evidence, 'members of %s\'s group %d survived: %s' % (name, me['pgid'], left[name])
    return evidence, None


def grade_normal(rows, name, done_ms):
    """A command that finishes by itself is never cut short: it was recorded, it wrote .done, and it
    ended no earlier than that."""
    seen = [r for r in rows if r.get('kind') == 'seen' and r.get('name') == name]
    evidence = {'name': name, 'done_ms': done_ms}
    if not seen:
        return evidence, 'REFUSED: %s never recorded its pid' % name
    gone = [r['t_ms'] for r in rows if r.get('kind') == 'gone' and r.get('name') == name]
    evidence['gone_ms'] = gone[0] if gone else None
    if done_ms is None:
        return evidence, '%s never finished by itself (no .done): it was ended, or never ran to the end' % name
    if gone and gone[0] + 100 < done_ms:
        return evidence, '%s was gone %d ms before it finished' % (name, done_ms - gone[0])
    return evidence, None


def cpu_seconds(text):
    """`ps -o time=` as seconds: `m:ss.xx`, or `h:mm:ss.xx` past an hour."""
    parts = text.strip().split(':')
    seconds = float(parts[-1])
    for i, part in enumerate(reversed(parts[:-1])):
        seconds += int(part) * 60 ** (i + 1)
    return seconds


class Walk(adopt.Walk):
    def __init__(self, a):
        super().__init__(a)
        self.helper = self.payload + '/reap-guest.py'
        self.engine_state = self.data + '/engine-state'
        self.watch_out = self.payload + '/reap-watch.jsonl'

    # --- guest helpers ----------------------------------------------------------------------
    def now(self):
        return int(self.clock())

    def rows(self):
        text = guest(self.vm, 'cat ' + shlex.quote(self.watch_out) + ' 2>/dev/null || true', 60)
        return [json.loads(r) for r in text.splitlines() if r.startswith('{')]

    def file_ms(self, path):
        text = guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true').strip()
        return int(text) if text.isdigit() else None

    def left_in_groups(self, names):
        rows = {r['name']: r for r in self.rows() if r.get('kind') == 'seen' and r['name'] in names}
        return {name: guest(self.vm, 'python3 %s group %d' % (shlex.quote(self.helper), r['pgid'])).split()
                for name, r in rows.items()}

    def assignments_since(self, sent_ms):
        """Every assignment registered since sent_ms: id, kind and state, so the evidence says
        which lease ran the commands (a `task` is the back end's work lease)."""
        script = ('import json,glob,sys\n'
                  'rows=[]\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/assignments/*/*.json"):\n'
                  '    try: r=json.load(open(p))\n'
                  '    except Exception: continue\n'
                  '    if r.get("registered_at_ms",0)>=int(sys.argv[2]):\n'
                  '        rows.append({k:r.get(k) for k in ("id","kind","state","title","registered_at_ms")})\n'
                  'print(json.dumps(sorted(rows,key=lambda r:r.get("registered_at_ms") or 0)))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s %d' % (shlex.quote(script), shlex.quote(self.data), sent_ms), 60))

    def closed_record(self, assignment_id, within=20):
        """The assignment's own record once it has closed and raised a notice (or as it is
        after `within` seconds): the state he is shown after a stop."""
        script = ('import json,glob,sys\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/assignments/*/"+sys.argv[2]+".json"):\n'
                  '    print(open(p).read())\n')
        end, record = time.monotonic() + within, {}
        while time.monotonic() < end:
            text = guest(self.vm, 'python3 -c %s %s %s' % (shlex.quote(script), shlex.quote(self.data),
                                                            shlex.quote(assignment_id)), 60)
            record = json.loads(text) if text.strip() else {}
            if record.get('state') not in OPEN and record.get('notices'):
                break
            time.sleep(1)
        return record

    def approve_pending(self):
        """Open the work summary when something waits for him, and approve every request there."""
        if not self.present('waiting for you'):
            return 0
        self.press('waiting for you')
        time.sleep(1)
        approved = 0
        while self.present('Approve '):
            self.press('Approve ')
            approved += 1
            time.sleep(1)
        if self.present('back to Rich'):
            self.press('back to Rich')
        return approved

    def ask(self, text):
        self.type_into(text, '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')

    def start(self, tag, background=True, seconds=FOREGROUND_SECONDS, within=300):
        """Ask for the heartbeats and approve their requests until each has recorded itself."""
        names = [tag + '-fg'] + ([tag + '-bg'] if background else [])
        sent = self.now()
        self.ask(ask_text(tag, background, seconds))
        end = time.monotonic() + within
        approved = 0
        while time.monotonic() < end:
            if recorded(self.rows(), names, sent):
                return {'names': names, 'approved': approved, 'sent_ms': sent, 'assignments': self.assignments_since(sent)}
            approved += self.approve_pending()
            time.sleep(3)
        raise StepFailed('prerequisite unavailable: %s never recorded itself within %d s (approved %d request(s))'
                         % (', '.join(names), within, approved))

    def verdict(self, names, trigger_ms, bound_ms):
        time.sleep(bound_ms / 1000 + 1)
        evidence, failure = grade_ended(self.rows(), names, trigger_ms, bound_ms, self.left_in_groups(names))
        if failure:
            raise StepFailed(failure)
        return evidence

    def relaunched(self):
        model = self.a.model
        return relaunch(self.vm, environment={'ANTHROPIC_MODEL': model} if model else None)

    def enter(self):
        # Every caller has just relaunched the app, and a launch with a company opens on the HOME
        # screen (asked-again-walk.py waits up to 90 s for it). Its door is waited for, not
        # checked once: checked before home was drawn, it was skipped, and the walk then waited
        # on the home screen for a conversation row (walk-45ed47dcaab0's screenshot,
        # esc-20261021T213902Z-b513879e).
        self.wait_for('Talk to Rich', seconds=90)
        self.press('Talk to Rich')
        # The door opens the company's last conversation itself: walk-5a4e3ff2c768 showed
        # "Acme / Onboarding" with its composer and the sidebar closed, so the row the walk then
        # waited for was not on screen. The row is pressed only when the door opened another one.
        self.wait_for('Message to Rich', role='AXTextArea')
        title = self.facts.get('thread_title') or 'Running'
        # The crumb is static text, whose words are its AXValue (walk-08aeea74dab4's tree:
        # {"role": "AXStaticText", "title": "", "value": "Onboarding"}), never its title.
        try:
            if self.ax('find', '--value', title, '--role', 'AXStaticText', '--first'):
                return
        except StepFailed as exc:
            if not adopt.axwait.not_yet(exc, 'find'):
                raise
        # Waited for before it is pressed: a press is never asked again (axwait.py), and right
        # after a relaunch the app may not answer one read (esc-20261021T205850Z-48c70d7d).
        self.wait_for(title)
        self.press(title)

    # --- steps --------------------------------------------------------------------------------
    def connect(self):
        """The folder through the shared connect step (adopt-walk.py connect_folder)."""
        self.connect_folder()
        # Leases started before the connection carry the old --add-dir list; start fresh ones.
        self.facts['app'] = self.relaunched()
        self.save()
        self.enter()
        return {'connected': self.company}

    def seed(self):
        command([HERE / 'guest.sh', self.vm, '--push', HERE / 'reap-guest.py', self.helper], 60)
        command([HERE / 'guest.sh', self.vm, '--push', HEARTBEAT, self.company + '/heartbeat.py'], 60)
        guest(self.vm, 'cd {c} && git add heartbeat.py && git -c user.name=QA -c user.email=qa@example.invalid '
                       'commit -q -m "heartbeat test helper" && rm -rf {b}'.format(c=shlex.quote(self.company), b=BEATS))
        return {'helper': self.company + '/heartbeat.py'}

    def watch(self):
        guest(self.vm, 'nohup python3 {h} watch {b} {o} --seconds 5400 >/dev/null 2>&1 &'.format(
            h=shlex.quote(self.helper), b=BEATS, o=shlex.quote(self.watch_out)))
        time.sleep(2)
        if not any(r.get('kind') == 'watch-start' for r in self.rows()):
            raise StepFailed('the death watcher did not start in the guest')
        return {'watching': BEATS}

    def unrelated(self):
        pid = guest(self.vm, 'nohup /bin/sleep 5400 >/dev/null 2>&1 & echo $!').strip()
        self.facts['unrelated'] = int(pid)
        self.save()
        return {'pid': int(pid)}

    def normal(self):
        started = self.start('normal', background=False, seconds=8)
        end = time.monotonic() + 120
        while time.monotonic() < end and self.file_ms(BEATS + '/normal-fg.done') is None:
            self.approve_pending()
            time.sleep(3)
        evidence, failure = grade_normal(self.rows(), 'normal-fg', self.file_ms(BEATS + '/normal-fg.done'))
        if failure:
            raise StepFailed(failure)
        return dict(evidence, approved=started['approved'], assignments=started['assignments'])

    def stop(self):
        started = self.start('stop')
        names = started['names']
        target, why = stoppable(self.assignments_since(started['sent_ms']))
        if why:
            raise StepFailed(why)
        # The work chip names the job's state ("1 assignment starting", "... running", "... waiting
        # to start", "1 waiting for you"; main.js renderDrillChip) and always ends its label with
        # "Open the work summary." Walk walk-21f83423b356 pressed the fixed words "assignment
        # running" while the chip said another state; the chip's own ending is pressed instead.
        self.wait_for('Open the work summary')
        self.press('Open the work summary')
        # The row's Stop is labeled "Stop <its title>" (work-summary.js); name this one.
        label = 'Stop ' + (target.get('title') or '')[:60]
        press = pressed_at(self.press(label))
        evidence = self.verdict(names, press['pressed_at_ms'], BOUND_MS)
        record = self.closed_record(target['id'])
        told = told_stopped(record)
        if told:
            raise StepFailed(told)
        return dict(evidence, press=press, started=started, stopped=target['id'],
                    told=record['notices'][-1]['text'])

    def quit(self):
        started = self.start('quit')
        names = started['names']
        target, why = stoppable(self.assignments_since(started['sent_ms']))
        if why:
            raise StepFailed(why)
        # The app's own Quit item (main.rs, MENU_QUIT) is in the menu bar, which is in no window.
        self.ax('click', '--title', 'Quit RichOS', '--role', 'AXMenuItem', '--in', 'menubar', '--first')
        self.wait_for('Quit and stop the work', seconds=20)
        question = command([HERE / 'ax.sh', self.vm, 'tree', '--in', 'dialog', '--max', '80'], 60)
        if not names_commands(question):
            raise StepFailed('the quit question did not name the running command')
        press = pressed_at(self.press('Quit and stop the work'))
        evidence = dict(self.verdict(names, press['pressed_at_ms'], QUIT_BOUND_MS), press=press)
        app_log = guest(self.vm, 'cat ' + shlex.quote(self.payload) + '/app.log ' + shlex.quote(self.payload)
                        + '/relaunch-*.log 2>/dev/null | grep "\\[richos\\] quit:" | tail -1 || true')
        reap_log = guest(self.vm, 'cat ' + shlex.quote(self.engine_state + '/provider-reap.log') + ' 2>/dev/null || true')
        groups = {c['pgid'] for c in evidence['commands'].values()}
        if not app_log or not any('killed group %d' % g in reap_log for g in groups):
            raise StepFailed('the quit left no reap lines: app log %r, reap log %r' % (app_log, reap_log[-400:]))
        record = self.closed_record(target['id'])
        told = told_stopped(record)
        if told:
            raise StepFailed(told)
        return dict(evidence, app_log=app_log, question=question[-800:], started=started, stopped=target['id'],
                    told=record['notices'][-1]['text'])

    def crash(self):
        self.facts['app'] = self.relaunched()
        self.save()
        self.enter()
        started = self.start('crash')
        names = started['names']
        pid = self.facts['app']['pid']
        trigger = int(guest(self.vm, 'python3 -c "import os,time; t=time.time(); os.kill(%d, 9); print(int(t*1000))"' % pid))
        return dict(self.verdict(names, trigger, BOUND_MS), app_pid=pid, started=started)

    def claude_crash(self):
        self.facts['app'] = self.relaunched()
        self.save()
        self.enter()
        started = self.start('claude')
        names = started['names']
        mine = {r['name']: r['pgid'] for r in self.rows()
                if r.get('kind') == 'seen' and r['name'] in names and r.get('t_ms', 0) >= started['sent_ms']}
        pgids = [mine[n] for n in names]
        end, owner, states = time.monotonic() + 10, None, '[]'
        while owner is None and time.monotonic() < end:
            states = guest(self.vm, 'python3 -c "import glob,json,sys; print(json.dumps([json.load(open(f)) for f in '
                                    'glob.glob(sys.argv[1] + \'/provider-leases/*.json\')]))" ' + shlex.quote(self.engine_state))
            owner = owner_of(json.loads(states), pgids)
            if owner is None:
                time.sleep(0.5)
        if owner is None:
            raise StepFailed('REFUSED: no single live lease state file named groups %s within 10 s: %s' % (pgids, states[:400]))
        provider = owner['provider']
        trigger = int(guest(self.vm, 'python3 -c "import os,time; t=time.time(); os.kill(%d, 9); print(int(t*1000))"' % provider))
        return dict(self.verdict(names, trigger, BOUND_MS), provider=provider, started=started)

    def idle_cpu(self):
        rows = guest(self.vm, 'ps -axo pid=,time=,command= | grep "provider-supervisor.py --reap" | grep -v grep')
        pid = int(rows.split()[0])
        def cpu():
            return cpu_seconds(guest(self.vm, 'ps -p %d -o time=' % pid))
        before = cpu()
        time.sleep(60)
        return {'supervisor': pid, 'cpu_seconds_per_idle_minute': round(cpu() - before, 3)}

    def unrelated_alive(self):
        pid = self.facts.get('unrelated')
        if not pid:
            raise StepFailed('the unrelated step did not run')
        alive = guest(self.vm, 'kill -0 %d 2>/dev/null && echo alive || true' % pid).strip() == 'alive'
        guest(self.vm, 'kill %d 2>/dev/null || true' % pid)
        if not alive:
            raise StepFailed('the unrelated process (pid %d) did not survive the walk' % pid)
        return {'pid': pid, 'alive': True}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--model', default='sonnet', help='ANTHROPIC_MODEL for a relaunched app (default sonnet)')
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
        except (StepFailed, RuntimeError, ValueError, StopIteration, subprocess.TimeoutExpired) as exc:
            row['outcome'] = 'REFUSED' if str(exc).startswith('REFUSED') else 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok:
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
