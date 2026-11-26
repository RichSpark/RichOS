#!/usr/bin/env python3
"""open-idle-walk.py — in the regular app: opening it and changing Rich's model in Settings uses no model.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      open-idle-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

HIS WORDS (CEO, 2026-11-26): "Everything was either stopped, finished, ended or paused in the RichOS
app. So, unless I'm missing something major, there should have been absolutely zero usage after me
just opening the app." That morning the app, opened after two days closed with no job open, handed
the Codex reports written meanwhile to the team's session and then to Rich's: 23 requests on Fable
before he had typed a word. He had opened it to switch both rows in Settings from Fable to Opus, and
closed the window with Settings still open, which saved nothing.

WHAT IT DOES, in the guest, on fixture data, never on the host's screen (CEO ruling 65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme" with a folder that is a Git repository
  connect    a Codex connection of the first-run conversation, made with the engine's own helper
             (codex_channels.py connect --scope), no model involved
  closed     the app quit, then a report written on the connection while it is closed
  open       the app opened again; Settings opened by its button; the conversation's row set to
             Opus; Settings closed with Escape, which saves. Then --idle-seconds pass. PASS needs
             config.json holding rich_model=opus, the report NOT in the inbox, the app's log
             saying it waits, and not one new answer from any model in any Claude Code
             transcript since the app was closed
  window     Settings opened again; the team's row set to Opus; the WINDOW closed by its red
             button with Settings still open. PASS needs config.json holding stu_model=opus
  ask        the app running (opened again if the closed window ended it), the first-run
             conversation opened from the sidebar and a question typed in it. PASS needs the
             report in the inbox now, and every answer since the app was closed from an Opus model
  live       a second report written on the connection WHILE THE APP IS OPEN. PASS needs it in
             that conversation's inbox with nothing more typed (Codex's entries still reach the
             team by themselves), and still no answer from any model but Opus
  behind     the app closed, a report written, the app opened and left alone (the report waits),
             then one more report written while it is open. PASS needs BOTH in the inbox with
             nothing typed, the one from before first: a held report never holds back a new one

Both sessions are switched to Opus before either is started, so the walk itself uses no Fable.

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
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
import uuid

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest, quit_app, relaunch  # noqa: E402


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, HERE / filename)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


codex = load('open_idle_codex', 'codex-watch-walk.py')
StepFailed = codex.StepFailed
STEPS = ['identity', 'first-run', 'connect', 'closed', 'open', 'window', 'ask', 'live', 'behind']
WAITS = 'nothing newer was written since RichOS opened'

# Every answer a model gave, in every Claude Code transcript of this home, by model.
ANSWERS = r"""import glob,json,sys
home,data=sys.argv[1:3]
seen={}
ids=set()
paths=sorted(set(glob.glob(home+'/.claude/projects/**/*.jsonl',recursive=True)+glob.glob(data+'/**/projects/**/*.jsonl',recursive=True)))
for path in paths:
    for line in open(path,errors='replace'):
        try: row=json.loads(line)
        except ValueError: continue
        if row.get('type')!='assistant': continue
        message=row.get('message') or {}
        model=message.get('model')
        if model=='<synthetic>' or message.get('id') in ids: continue
        ids.add(message.get('id'))
        seen[str(model)]=seen.get(str(model),0)+1
print(json.dumps({'transcripts':len(paths),'answers':seen}))
"""


class OpenIdleWalk(codex.CodexWatchWalk):
    # lease-model-walk.py's three, on this walk's own StepFailed (a module loaded twice has two).
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
            self.until('the effort control in Settings', lambda: self.present_id('ef-rich-low'), 60)

    def answers(self):
        return json.loads(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(ANSWERS), shlex.quote(self.home), shlex.quote(self.data)), 120))

    def quiet(self, seconds=45, within=900):
        """The transcripts as they are once no model has answered for `seconds`."""
        end = time.monotonic() + within
        last, since = self.answers(), time.monotonic()
        while time.monotonic() < end:
            time.sleep(10)
            now = self.answers()
            if now != last:
                last, since = now, time.monotonic()
            elif time.monotonic() - since >= seconds:
                return last
        raise StepFailed('the models were still answering after %g s' % within)

    def config(self):
        text = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(self.data + '/config.json'), 60)
        return json.loads(text) if text.strip() else {}

    def channel(self, *args):
        """The engine's own channel helper, as Codex runs it."""
        command = shlex.join(['env', 'HOME=' + self.home, 'CODEX_COORDINATION_ROOT=' + self.coordination,
                              self.python, '-B', self.helper, *args])
        return json.loads(guest(self.vm, command, 120))

    def report(self, marker):
        return self.channel('send', '--locator', self.facts['locator'], '--id', marker, '--text',
                           'Harmless transport check %s. Nothing is requested and nothing needs doing.' % marker)

    def reached(self, marker):
        return [r for r in self.inboxes() if r['bridge']['message_id'] == marker]

    def model_row(self, row, model):
        """Press `model` in a Settings row (0: when he talks with you; 1: when he directs your team)."""
        self.ax('click', '--title', model, '--role', 'AXRadioButton', '--nth', str(row))

    def connect(self):
        thread = self.facts['thread']
        entity = next(r['entity_id'] for r in self.threads() if r['thread_id'] == thread)
        scope = self.payload + '/open-idle-scope.json'
        body = json.dumps({'version': 1, 'state_root': self.data + '/engine-state', 'entity_id': entity, 'thread_id': thread})
        guest(self.vm, 'printf %s ' + shlex.quote(body) + ' > ' + shlex.quote(scope), 60)
        made = self.channel('connect', '--scope', scope, '--text', 'A harmless transport test: nothing is requested.',
                           '--provenance', 'open-idle-walk fixture')
        self.facts.update(locator=made['locator'], entity=entity)
        self.save()
        return {'locator': made['locator'], 'conversation': thread, 'company': entity}

    def live(self):
        marker = 'live-' + uuid.uuid4().hex[:12]
        self.report(marker)
        rows = self.wait_fact('a report written while the app is open reaching the team by itself',
                              lambda: self.reached(marker), bool)
        if rows[0]['thread'] != self.facts['thread']:
            raise StepFailed('the report reached another conversation: ' + str(rows[0]))
        after = self.quiet()
        other = [model for model in after['answers'] if 'opus' not in model]
        if other:
            raise StepFailed('a model that is not Opus answered: ' + json.dumps(after))
        return {'marker': marker, 'typed_since_the_report': 0, 'in_the_inbox': len(rows), 'answers_once_quiet': after}

    def close_app(self):
        """Quit the app (it may have ended already, with its window) and wait for its Codex watcher.

        The watcher ends by itself once its app is gone, at its next look (every 15 s), and holds
        the receiver lock until then: an app opened inside that window starts no watcher until a
        session opens. He opened his after two days, so the walk waits for the watcher to be gone."""
        record = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(self.data + '/engine-state/codex-watch/host-child.json'), 60)
        watcher = str(json.loads(record).get('pid', '')) if record.strip() else ''
        quit_app(self.vm)
        if watcher.isdigit():
            self.until("the closed app's Codex watcher to end",
                       lambda: guest(self.vm, 'kill -0 %s 2>/dev/null && echo alive || echo gone' % watcher) == 'gone', 90)

    def behind(self):
        self.quiet()
        self.close_app()
        before, new = 'before-' + uuid.uuid4().hex[:12], 'new-' + uuid.uuid4().hex[:12]
        self.report(before)
        launch = relaunch(self.vm)
        self.after_relaunch()

        def waits():
            log = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(launch['log']), 60)
            return [line for line in log.splitlines() if WAITS in line]
        said = self.until('the app to say the report from before waits', waits, 120)
        if self.reached(before):
            raise StepFailed('the report written while the app was closed was handed over with nothing asked')
        self.report(new)
        rows = self.wait_fact('both reports reaching the team once a new one was written',
                              lambda: [r['bridge']['message_id'] for r in self.inboxes()
                                       if r['bridge']['message_id'] in (before, new)], lambda ids: len(ids) == 2)
        if rows != [before, new]:
            raise StepFailed('the two reports did not arrive once each, the one from before first: ' + str(rows))
        after = self.quiet()
        other = [model for model in after['answers'] if 'opus' not in model]
        if other:
            raise StepFailed('a model that is not Opus answered: ' + json.dumps(after))
        return {'held_until_a_new_report': before, 'new_report': new, 'log_said_it_waits': said[:1],
                'inbox_order': rows, 'typed_by_the_user': 0, 'answers_once_quiet': after}

    def closed(self):
        self.close_app()
        marker = 'closed-' + uuid.uuid4().hex[:12]
        self.report(marker)
        before = self.answers()
        self.facts.update(closed_marker=marker, answers_while_closed=before)
        self.save()
        return {'marker': marker, 'written_while_the_app_was': 'closed', 'answers_before_opening': before}

    def open(self):
        marker, before = self.facts['closed_marker'], self.facts['answers_while_closed']
        launch = relaunch(self.vm)
        self.facts['launch_log'] = launch['log']
        self.save()
        entered = self.after_relaunch()
        self.open_settings()
        self.model_row(0, 'Opus')
        self.shot('settings-opus.png')
        codex.stu.command_walk.adopt_walk.command([HERE / 'ax.sh', self.vm, '--key', '53'], 60)  # Escape closes Settings, which saves
        self.until('config.json to hold rich_model=opus', lambda: self.config().get('rich_model') == 'opus', 60)
        time.sleep(self.a.idle_seconds)
        after = self.answers()
        log = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(launch['log']), 60)
        (self.out / 'app-after-opening.log').write_text(log)
        (self.out / 'codex-watch-host-child.log').write_text(guest(
            self.vm, 'tail -n 80 %s 2>/dev/null || true' % shlex.quote(self.data + '/engine-state/codex-watch/host-child.log'), 60))
        # The watcher as it is: its process, its journals and the positions it has saved.
        (self.out / 'codex-watch-state.txt').write_text(guest(self.vm, (
            'ps -axo pid,ppid,etime,command | grep -E "codex_watch|host_child" | grep -v grep; cat %s 2>/dev/null; '
            'find %s -type f -exec ls -la {} + 2>/dev/null; '
            'find %s -type f -exec ls -la {} + 2>/dev/null; cat %s 2>/dev/null; true') % (
                shlex.quote(self.data + '/engine-state/codex-watch/host-child.json'), shlex.quote(self.coordination), shlex.quote(self.data + '/engine-state/codex-watch'),
                shlex.quote(self.data + '/engine-state/codex-watch/health.json')), 60))
        waits = [line for line in log.splitlines() if WAITS in line]
        evidence = {'home': entered, 'idle_seconds': self.a.idle_seconds, 'config': {k: v for k, v in self.config().items() if 'model' in k},
                    'answers_before_opening': before, 'answers_after': after,
                    'second_report_in_the_inbox': len(self.reached(marker)), 'log_lines_saying_it_waits': waits[:3]}
        (self.out / 'open.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if self.reached(marker):
            raise StepFailed('the report written while the app was closed was handed to the team with nothing asked; see open.json')
        if after != before:
            raise StepFailed('a model answered after the app was only opened and Settings changed: %s then %s' % (
                json.dumps(before), json.dumps(after)))
        if not waits:
            raise StepFailed('the app\'s log has no line saying the report waits ("%s")' % WAITS)
        return evidence

    def window(self):
        self.open_settings()
        self.model_row(1, 'Opus')
        self.shot('settings-team-opus.png')
        state = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm'))) / 'run' / self.vm
        pid = int((state / 'app.pid').read_text())
        # The window's own red button, pressed as an element of the owned app: no key is sent.
        self.keys('tell application "System Events"\n'
                  'set p to first process whose unix id is %d\n'
                  'click (first button of window 1 of p whose subrole is "AXCloseButton")\n'
                  'end tell' % pid)
        self.until('config.json to hold stu_model=opus after the window closed with Settings open',
                   lambda: self.config().get('stu_model') == 'opus', 60)
        return {'closed_by': "the window's red button, Settings open",
                'config': {k: v for k, v in self.config().items() if 'model' in k}}

    def ask(self):
        marker, before = self.facts['closed_marker'], self.facts['answers_while_closed']
        # The window closed with nothing running, which ends the app: it is opened again, as he would.
        self.close_app()
        relaunch(self.vm)
        self.after_relaunch()
        if self.present('Show the sidebar'):
            self.press('Show the sidebar')
        title = self.title(self.facts['thread'])
        self.ax('click', '--title', title, '--role', 'AXButton', '--in', 'sidebar', '--first')
        self.wait_for('Send', seconds=120)
        self.send('This is a transport test. Did any report from Codex arrive for this chat? One sentence is enough; do nothing else.')
        rows = self.wait_fact('the held report reaching the team after his message', lambda: self.reached(marker), bool,
                              self.a.answer_within)
        after = self.quiet()
        new = {model: count - before['answers'].get(model, 0) for model, count in after['answers'].items()
               if count != before['answers'].get(model, 0)}
        evidence = {'asked_in': title, 'second_report_in_the_inbox': len(rows), 'answers_since_the_app_was_closed': new,
                    'config': {k: v for k, v in self.config().items() if 'model' in k}}
        (self.out / 'ask.json').write_text(json.dumps(evidence, indent=2) + '\n')
        if len(rows) != 1 or rows[0]['thread'] != self.facts['thread']:
            raise StepFailed('the held report reached the wrong conversation or reached it twice: ' + str(rows))
        other = [model for model in new if 'opus' not in model]
        if not new or other:
            raise StepFailed('answers since the app was closed are not all from an Opus model: ' + json.dumps(new))
        return evidence


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=300, help='seconds for a report to reach the inbox')
    p.add_argument('--answer-within', type=float, default=300, help='seconds for the held report to be handed over once he asked')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--idle-seconds', type=float, default=90,
                   help='how long the opened app is left alone before its records are read (the watcher looks every few seconds)')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    a.out.mkdir(parents=True, exist_ok=True)
    walk = OpenIdleWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as exc:
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
