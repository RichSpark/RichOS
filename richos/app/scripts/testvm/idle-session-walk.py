#!/usr/bin/env python3
"""idle-session-walk.py — an idle conversation's claude process is ended, and his next message revives it.

Seed the fixture home BEFORE run-walk.py boots (the setting is read when the app opens its config):

  idle-session-walk.py --seed-config EMPTY_DIR [--minutes 1]

then run it under run-walk.py, which holds one guest slot for this run only, boots the guest and
removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      idle-session-walk.py --out DIR --expect-sha SHA [--within SECONDS]

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION (richos-hq docs/plans/follow-ups.md, "a nightly's VM boot proof cannot get a guest
slot", decision 3): "Retire a session after an idle timeout and revive it on demand." The spine's
`retire_idle_front_desks` ends a parked conversation's `claude` child past `idle_session_minutes`;
the conversation's next turn resumes the thread's own saved session. Unit tests prove the spine's
decision with a double; this walk proves the real app and the real provider.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme" and its first conversation (A)
  setting    config.json in the guest carries the seeded idle_session_minutes
  remember   in A: a code word unique to this run, to be remembered. PASS on TurnCompleted; A's
             session id is read from the turn's TurnStarted row and a `claude` process carrying
             it is seen running
  switch     ⌘N, a second conversation (B), one message, TurnCompleted. B is now on screen and A
             is parked
  retired    within --minutes*60 + 30 + --margin seconds of A's last reply: the app logs that it
             ended A's session, and no process carries A's session id any more
  revive     A opened from the rail, asked for the code word. PASS needs TurnCompleted, the code
             word in the reply, the turn started on A's ORIGINAL session id (resumed, not a new
             session) and a process carrying that id running again. The seconds from send to the
             first streamed reply are recorded beside the same figure for `remember`
Evidence: <out>/<step>.json, the guest logs (keep_logs) and report.json.

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import secrets
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
cw = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cw)
StepFailed = cw.StepFailed

STEPS = ['identity', 'first-run', 'setting', 'remember', 'switch', 'retired', 'revive']
ENDED = ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'TurnFailed')
DATA = 'Library/Application Support/com.richos.app'


def seed_config(home, minutes):
    """Write config.json with only the idle timeout into an EMPTY fixture home. Every other
    preference keeps its default, so the first run is the ordinary first run."""
    path = Path(home) / DATA / 'config.json'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({'idle_session_minutes': minutes}) + '\n')
    return path


class Walk(cw.CommandWalk):
    def ledger(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        out = []
        for line in rows.splitlines():
            try:
                out.append(json.loads(line))
            except ValueError:
                continue
        return out

    def turn(self, thread, mark, sent_ms, within):
        """Wait for the turn whose prompt carries `mark` on `thread` to end. Returns its rows."""
        end = time.monotonic() + within
        found = {}
        while time.monotonic() < end:
            rows = self.ledger()
            prompt = next((r for r in rows if r.get('event') == 'PromptReceived' and r.get('thread_id') == thread
                           and mark in str(r.get('text', ''))), None)
            if prompt:
                tid = prompt.get('turn_id')
                deltas = [r for r in rows if r.get('event') == 'AssistantDelta' and r.get('turn_id') == tid]
                started = [r for r in rows if r.get('event') == 'TurnStarted' and r.get('turn_id') == tid]
                ended = next((r for r in rows if r.get('turn_id') == tid and r.get('event') in ENDED), None)
                found = {'turn_id': tid, 'prompt_at': prompt.get('at'),
                         'sessions': [r.get('session_id') for r in started if r.get('session_id')],
                         'reply': ''.join(str(r.get('text', '')) for r in deltas),
                         'first_reply_at': min((r.get('at', 0) for r in deltas), default=None),
                         'ended': ended.get('event') if ended else None}
                if ended:
                    break
            time.sleep(3)
        if found.get('first_reply_at') and sent_ms:
            found['first_reply_after_send_s'] = round((found['first_reply_at'] - sent_ms) / 1000, 1)
        return found

    def processes(self, session):
        """Every process in the guest whose arguments carry `session`, with its arguments."""
        rows = []
        for line in guest(self.vm, 'ps -axo pid=,args=', 60).splitlines():
            if session and session in line:
                pid, _, args = line.strip().partition(' ')
                rows.append({'pid': int(pid), 'args': args[:400]})
        return rows

    def logs(self):
        names = ['app.log'] + guest(self.vm, 'cd ' + shlex.quote(self.payload)
                                    + ' && ls relaunch-*.log 2>/dev/null || true').split()
        return '\n'.join(guest(self.vm, 'cat ' + shlex.quote(self.payload + '/' + n) + ' 2>/dev/null || true', 60)
                         for n in names)

    def record(self, name, evidence):
        (self.out / f'{name}.json').write_text(json.dumps(evidence, indent=2) + '\n')

    # --- steps --------------------------------------------------------------------------------
    def setting(self):
        text = guest(self.vm, 'cat ' + shlex.quote(self.data + '/config.json'))
        value = json.loads(text).get('idle_session_minutes')
        if value != self.a.minutes:
            raise StepFailed(f'config.json in the guest says idle_session_minutes={value!r}, not {self.a.minutes}; '
                             'was the fixture home seeded with --seed-config?')
        return {'idle_session_minutes': value}

    def remember(self):
        thread = self.facts.get('thread')
        if not thread:
            raise StepFailed('first-run must have run (no thread on record)')
        word = 'PELICAN-' + str(secrets.randbelow(9000) + 1000)
        mark = f'[idle-walk {word[-4:]}]'
        sent = self.send(f'{mark} Please remember this code word for later in this conversation: {word}. '
                         'Reply with just OK.')
        turn = self.turn(thread, mark, sent[0], self.a.within)
        session = (turn.get('sessions') or [None])[-1]
        alive = self.processes(session)
        evidence = {'word': word, 'mark': mark, 'turn': turn, 'session': session, 'processes': alive}
        self.record('remember', evidence)
        if turn.get('ended') != 'TurnCompleted':
            raise StepFailed(f"the remember turn ended {turn.get('ended')}; see remember.json")
        if not session or not alive:
            raise StepFailed('no running process carries A\'s session id after its turn; see remember.json')
        self.facts.update(word=word, session_a=session, remember_done_ms=self.clock(),
                          remember_first_reply_s=turn.get('first_reply_after_send_s'))
        self.save()
        return {'session': session, 'processes': len(alive), 'first_reply_after_send_s': turn.get('first_reply_after_send_s')}

    def switch(self):
        before = {t['thread_id'] for t in self.threads()}
        self.keys('tell application "System Events" to keystroke "n" using command down')
        time.sleep(3)
        mark = '[idle-walk B]'
        sent = self.send(f'{mark} This is a second conversation. Reply with just OK.')
        end = time.monotonic() + 60
        fresh = []
        while time.monotonic() < end and not fresh:
            fresh = [t['thread_id'] for t in self.threads() if t['thread_id'] not in before]
            time.sleep(2)
        if not fresh:
            raise StepFailed('⌘N and a send made no new conversation')
        turn = self.turn(fresh[0], mark, sent[0], self.a.within)
        self.record('switch', {'thread_b': fresh[0], 'turn': turn})
        if turn.get('ended') != 'TurnCompleted':
            raise StepFailed(f"B's turn ended {turn.get('ended')}; see switch.json")
        self.facts['thread_b'] = fresh[0]
        self.save()
        return {'thread_b': fresh[0]}

    def threads(self):
        return [r for r in self.ledger() if r.get('event') == 'ThreadCreated']

    def retired(self):
        thread, session = self.facts.get('thread'), self.facts.get('session_a')
        if not session or not self.facts.get('thread_b'):
            raise StepFailed('remember and switch must have run')
        within = self.a.minutes * 60 + 30 + self.a.margin
        end = time.monotonic() + within
        line, gone = None, None
        while time.monotonic() < end:
            line = next((l for l in self.logs().splitlines() if 'idle sessions: ended' in l and thread in l), None)
            gone = not self.processes(session)
            if line and gone:
                break
            time.sleep(10)
        evidence = {'log_line': line, 'process_gone': gone, 'processes': self.processes(session),
                    'waited_up_to_s': within}
        self.record('retired', evidence)
        if not line:
            raise StepFailed(f'the app did not log ending A\'s session within {within} s; see retired.json')
        if not gone:
            raise StepFailed('the app logged ending A\'s session and a process still carries its id; see retired.json')
        return {'log_line': line.strip()[-300:]}

    def title_of(self, thread):
        """The conversation's title as the rail shows it (nav.rs `display_title`): his rename,
        then Haiku's title, then the title it was created with. The app keeps the first two in
        navigation.json (reap-walk.py `shown_title`); reading any other file found neither, and
        walk-14f83e95b656 then pressed "Onboarding" after Haiku had renamed the row."""
        try:
            nav = json.loads(guest(self.vm, 'cat ' + shlex.quote(self.data + '/navigation.json')
                                   + ' 2>/dev/null || true') or '{}')
        except ValueError:
            nav = {}
        for key in ('renamed_threads', 'auto_titles'):
            shown = ((nav or {}).get(key) or {}).get(thread)
            if shown:
                return shown
        titles = [r.get('title') for r in self.ledger() if r.get('thread_id') == thread and r.get('title')]
        return titles[-1] if titles else self.facts.get('thread_title')

    def revive(self):
        thread, session, word = self.facts.get('thread'), self.facts.get('session_a'), self.facts.get('word')
        if not word:
            raise StepFailed('remember must have run')
        title = self.title_of(thread)
        # Waited for before it is pressed: a press is never asked again (axwait.py).
        self.wait_for(title, seconds=30)
        self.press(title)
        self.wait_for('Message to Rich', role='AXTextArea', seconds=30)
        time.sleep(2)
        mark = '[idle-walk again]'
        sent = self.send(f'{mark} What was the code word I asked you to remember earlier in this conversation? '
                         'Reply with just the code word.')
        turn = self.turn(thread, mark, sent[0], self.a.within)
        alive = self.processes(session)
        evidence = {'title_pressed': title, 'turn': turn, 'original_session': session, 'processes': alive,
                    'first_reply_after_send_s': {'revive': turn.get('first_reply_after_send_s'),
                                                 'remember': self.facts.get('remember_first_reply_s')}}
        self.record('revive', evidence)
        failures = []
        if turn.get('ended') != 'TurnCompleted':
            failures.append(f"the turn ended {turn.get('ended')}")
        if word not in turn.get('reply', ''):
            failures.append('the reply does not carry the code word')
        if session not in turn.get('sessions', []):
            failures.append(f"the turn ran on {turn.get('sessions')}, not A's session {session}")
        if not alive:
            failures.append('no process carries A\'s session id after the revive')
        if failures:
            raise StepFailed('; '.join(failures) + '; see revive.json')
        resumed = any(re.search(r'--resume\s', p['args']) for p in alive)
        return {'reply': turn['reply'][:200], 'session': session, 'resumed_flag_seen': resumed,
                'first_reply_after_send_s': evidence['first_reply_after_send_s']}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm', nargs='?')
    p.add_argument('--seed-config', type=Path, help='write the idle timeout into this EMPTY fixture home and exit')
    p.add_argument('--out', type=Path)
    p.add_argument('--expect-sha')
    p.add_argument('--minutes', type=int, default=1, help='idle_session_minutes seeded and expected')
    p.add_argument('--within', type=float, default=240, help='seconds for each turn to end')
    p.add_argument('--margin', type=float, default=90, help='seconds past timeout + one 30 s check to wait')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    if a.seed_config:
        print(seed_config(a.seed_config, a.minutes))
        return 0
    if not (a.vm and a.out and a.expect_sha):
        p.error('vm, --out and --expect-sha are required for a walk')
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = Walk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'minutes': a.minutes, 'steps': []}
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
