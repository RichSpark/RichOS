#!/usr/bin/env python3
"""netcut-walk.py — a turn the lost internet connection interrupted finishes once the connection is back.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      netcut-walk.py --out DIR --expect-sha SHA [--lost-within 1500] [--hold-seconds 60] [--within 900]

THE QUESTION (CEO, 2026-10-21: "Of course, that must be built into the app. What else is the app good
for if it can't handle that?"). The internet dropped for a few minutes and a worker's turn ended on
Claude Code's connection error. The app now waits until the connection is back and carries the same
turn on (`richos-core` `connection.rs`, `native.rs` `after_api_error`). This walk cuts the GUEST's
internet while Rich's turn runs and restores it minutes later.

WHAT IT DOES, in the guest, never on the host (CEO rulings 65 and 54):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme" and its first conversation
  cut        a one-word message to Rich; the moment it is received, the guest's default route is
             removed, so every connection beyond the guest's own subnet fails at once while ssh from
             the host keeps working, and the API does not answer (checked with Claude Code's own
             preflight, `/api/hello`). Held until the app's log says the connection was lost (Claude
             Code's own retries ran out and ended the turn), then --hold-seconds more.
  restore    the default route is put back and the API answers again
  observe    within --within seconds of the restore: the turn completes, ONCE, with the one word;
             his message reached Rich once; the app's log says it lost the connection and carried
             the same turn on. Evidence: <out>/observe.json, the ledger and app.log tail.

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54), and
a route left removed dies with the guest, which run-walk.py deletes.
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
import axwait  # noqa: E402

_spec = importlib.util.spec_from_file_location('adopt_walk', HERE / 'adopt-walk.py')
adopt_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(adopt_walk)
StepFailed = adopt_walk.StepFailed
command = adopt_walk.command

STEPS = ['identity', 'first-run', 'cut', 'restore', 'observe']
WORDS = 'Reply with the single word: finished.'
LOST = 'the internet connection is lost'
CARRIED = 'the same turn carries on'
PREFLIGHT = ('curl -sS -o /dev/null -w "%{http_code}" --max-time 10 https://api.anthropic.com/api/hello '
             '2>/dev/null; true')


class NetcutWalk(adopt_walk.Walk):
    def ax(self, mode, *args, app=None, timeout=40):
        """A READ with no answer on a busy host is asked again, twice at most (axwait.ask_again).
        Only a read: it changes nothing, so asking again cannot act twice; a click or a paste that
        timed out is still a failure."""
        for attempt in range(3):
            try:
                return super().ax(mode, *args, app=app, timeout=timeout)
            except StepFailed as exc:
                if not axwait.ask_again(exc, mode) or attempt == 2:
                    raise
        raise AssertionError('unreachable')

    def ledger_rows(self):
        rows = guest(self.vm, 'cat ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' 2>/dev/null || true', 60)
        out = []
        for line in rows.splitlines():
            try:
                out.append(json.loads(line))
            except ValueError:
                pass
        return out

    def app_log(self, tail=None):
        path = shlex.quote(self.payload + '/app.log')
        return guest(self.vm, (f'tail -c {tail} ' if tail else 'cat ') + path + ' 2>/dev/null || true', 60)

    def api_answers(self):
        return guest(self.vm, PREFLIGHT, 30).strip()

    def cut(self):
        self.type_into(WORDS, '--role', 'AXTextArea', '--title', 'Message to Rich')
        self.press('Send')
        end = time.monotonic() + 120
        received = None
        while time.monotonic() < end and not received:
            received = next((r for r in self.ledger_rows() if r.get('event') == 'PromptReceived'
                             and r.get('text', '').strip() == WORDS), None)
            time.sleep(1)
        if not received:
            raise StepFailed('his message never reached Rich (no PromptReceived row within 120 s)')
        self.facts['turn'] = received['turn_id']
        # The guest loses its way to the internet: its default route is removed, so a connection out
        # fails at once (no route), as a dropped line does. The guest's own subnet (ssh from the host,
        # its DNS) and loopback stay. A packet filter that drops or "returns" was tried first and
        # left connections hanging (curl: "Connection timed out after 10005 ms"), and Claude Code
        # 2.1.305 waits about 75 s on each hanging attempt (measured 2026-10-21), so its retries
        # would take some 25 minutes instead of the minutes of a real outage.
        gateway = guest(self.vm, "route -n get default 2>/dev/null | awk '/gateway:/{print $2}'", 30).strip()
        if not gateway:
            raise StepFailed('the guest has no default route to remove')
        # Its IPv6 default goes too: the guest has one (via en0's link-local router) that reaches
        # nothing, so with only IPv4 cut a connection to the API's IPv6 address hung instead of
        # failing (walk-3ee8d591416e: IPv4 failed in 0 ms, IPv6 timed out at 8 s).
        gateway6 = guest(self.vm, "netstat -rn -f inet6 | awk '$1==\"default\" && $NF==\"en0\"{print $2; exit}'", 30).strip()
        self.facts.update({'gateway': gateway, 'gateway6': gateway6})
        self.save()
        guest(self.vm, 'sudo -n route -q -n delete default >/dev/null'
              + ('; sudo -n route -q -n delete -inet6 default >/dev/null' if gateway6 else ''), 30)
        self.facts['cut_at_ms'] = round(self.clock())
        subnet = 'the guest subnet and loopback'
        answered = self.api_answers()
        if answered == '200':
            raise StepFailed('the API still answered from the guest after the cut')
        # How a request fails now: at once, not left hanging.
        self.facts['cut_probe'] = guest(self.vm, PREFLIGHT.replace('2>/dev/null', '2>&1')
                                        .replace('%{http_code}', '%{http_code} %{time_total}s '), 30)
        self.facts.update({'cut_subnet_kept': subnet, 'api_after_cut': answered})
        self.save()
        # The cut is held until the app itself says Claude Code gave the turn up on the lost
        # connection (its own retries run first and their length is Claude Code's, not ours: a
        # fixed 300 s cut was outlasted by them on 2026-10-21), then --hold-seconds more, so the
        # app's wait for the connection is what carries the turn on.
        end = time.monotonic() + self.a.lost_within
        lost = []
        while time.monotonic() < end and not lost:
            lost = [line for line in self.app_log().splitlines() if LOST in line]
            if not lost:
                if any(r.get('turn_id') == self.facts['turn'] and r.get('event') in ('TurnCompleted', 'TurnInterrupted',
                                                                                    'TurnStopped') for r in self.ledger_rows()):
                    raise StepFailed('the turn ended during the cut before the app said the connection was lost')
                time.sleep(10)
        if not lost:
            raise StepFailed('the app never said the connection was lost within %g s of the cut' % self.a.lost_within)
        self.facts['lost_at_ms'] = round(self.clock())
        self.facts['lost_in_log_during_cut'] = lost
        self.save()
        time.sleep(self.a.hold_seconds)
        return {'turn': self.facts['turn'], 'api_after_cut': answered, 'subnet_kept': subnet,
                'lost_logged': lost, 'lost_after_cut_s': round((self.facts['lost_at_ms'] - self.facts['cut_at_ms']) / 1000)}

    def restore(self):
        gateway = self.facts.get('gateway')
        if not gateway:
            raise StepFailed('no default route was recorded to put back')
        gateway6 = self.facts.get('gateway6')
        guest(self.vm, 'sudo -n route -q -n add default ' + shlex.quote(gateway) + ' >/dev/null'
              + ('; sudo -n route -q -n add -inet6 default ' + shlex.quote(gateway6) + ' >/dev/null' if gateway6 else ''), 30)
        self.facts['restored_at_ms'] = round(self.clock())
        end = time.monotonic() + 60
        answered = ''
        while time.monotonic() < end and answered != '200':
            answered = self.api_answers()
        if answered != '200':
            raise StepFailed('the API did not answer from the guest after the restore: %r' % answered)
        self.save()
        return {'api_after_restore': answered}

    def observe(self):
        turn = self.facts['turn']
        end = time.monotonic() + self.a.within
        rows = []
        while time.monotonic() < end:
            rows = self.ledger_rows()
            if any(r.get('event') in ('TurnCompleted', 'TurnInterrupted', 'TurnStopped') and r.get('turn_id') == turn
                   for r in rows):
                break
            time.sleep(5)
        record = {
            'turn': turn,
            'prompts_with_his_words': sum(1 for r in rows if r.get('event') == 'PromptReceived'
                                          and r.get('text', '').strip() == WORDS),
            'endings': [r for r in rows if r.get('turn_id') == turn and r.get('event') in
                        ('TurnCompleted', 'TurnInterrupted', 'TurnStopped', 'UpstreamFailure', 'TurnSuperseded',
                         'TurnReplayed')],
            'reply': ''.join(r.get('text', '') for r in rows if r.get('event') == 'AssistantDelta'
                             and r.get('turn_id') == turn),
            'cut_at_ms': self.facts.get('cut_at_ms'),
            'restored_at_ms': self.facts.get('restored_at_ms'),
        }
        log = self.app_log()
        record['log_lost'] = [l for l in log.splitlines() if LOST in l]
        record['log_carried'] = [l for l in log.splitlines() if CARRIED in l]
        record['app_log_tail'] = self.app_log(6000)
        guest(self.vm, 'cp ' + shlex.quote(self.data + '/conversation-ledger.jsonl') + ' /tmp/netcut-ledger.jsonl', 30)
        command([HERE / 'guest.sh', self.vm, '--pull', '/tmp/netcut-ledger.jsonl', self.out / 'conversation-ledger.jsonl'], 60)
        self.shot('observe.png')
        (self.out / 'observe.json').write_text(json.dumps(record, indent=2) + '\n')
        completed = [e for e in record['endings'] if e.get('event') == 'TurnCompleted']
        other = [e for e in record['endings'] if e.get('event') != 'TurnCompleted']
        if record['prompts_with_his_words'] != 1:
            raise StepFailed('his message reached Rich %d times; see observe.json' % record['prompts_with_his_words'])
        if len(completed) != 1 or other:
            raise StepFailed('the turn did not finish exactly once: %s; see observe.json' % json.dumps(record['endings']))
        if 'finished' not in record['reply'].lower() or 'API Error' in record['reply']:
            raise StepFailed('the reply is not the one word asked for: %r' % record['reply'])
        if not record['log_lost'] or not record['log_carried']:
            raise StepFailed('the turn finished without the connection wait: lost=%d carried=%d (the cut was '
                             'shorter than Claude Code\'s own retries?)' % (len(record['log_lost']), len(record['log_carried'])))
        return {k: record[k] for k in ('turn', 'prompts_with_his_words', 'endings', 'reply', 'log_lost', 'log_carried')}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--lost-within', type=float, default=1500,
                   help='seconds after the cut for the app to say the connection was lost (Claude Code '
                        'retries first: 15 retries since 9b5a5cd4d, at least 319.5 s of waits)')
    p.add_argument('--hold-seconds', type=float, default=60,
                   help='how long the cut is held after that, so the app waits for the connection')
    p.add_argument('--within', type=float, default=900, help='seconds after the restore for the turn to end')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = NetcutWalk(a)
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
        if row['outcome'] != 'PASS':
            try:
                walk.shot('failed-%s.png' % step)
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                pass
            if step == 'cut':
                # Never leave the filter up for the remaining steps' commands to fight.
                try:
                    walk.restore()
                except (StepFailed, RuntimeError, subprocess.TimeoutExpired):
                    pass
            break
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
