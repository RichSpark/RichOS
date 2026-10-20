#!/usr/bin/env python3
"""team-watch-walk.py — his team's quota-watch, stall-watch and codex-watch wake his lead in the app.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE --report REPORT -- \\
      team-watch-walk.py --out DIR --expect-sha SHA [--within SECONDS]

THE QUESTION. The CEO, 2026-10-20, about the RichOS app as his daily driver ("your setup as-is",
ruling 86): "how long do I have to wait for all that checking and doing to be completed?" His
team's lead in the app runs `claude --print`, where Claude Code starts no plugin monitor, so only
review-watch reached it. The host now runs quota-watch, stall-watch and codex-watch as its own
children and sends each notice to his team's lead (richos-core team_watch.rs). This walk shows
one notice from each reaching that lead, in the real app, on fixture data in a guest.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling 65):
  identity   the running app says it was built from --expect-sha
  first-run  adopt-walk.py's: company "Acme", the first conversation
  fixture    team-watch-guest.py setup: his team's fixture (the operator probes' entity and engine,
             the fences on, QUOTA_PAUSE_PERCENT=1) and its declaration in the app's data folder
  relaunch   command-walk.py's: the app quit and opened again, so it reads the declaration
  watchers   the app's own log names the three children's pids, each record names a live pid
  task       typed into the composer: ask his team to reply with one word, so a lead starts
  lead       his team's lead started (the operator log), its session named to the watchers
  notices    once his lead's turn has ended (it is idle, so only a watcher can wake it): Codex
             writes an entry to the lead's channel and a teammate's escalation is raised
  observe    within --within seconds his lead's own transcript holds, as messages it was sent:
             the QUOTA-THRESHOLD notice (kept while no lead ran, sent to it first at its start),
             the CODEX-WATCH entry and the ESCALATION-WATCH block. Evidence is pulled to --out.

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
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

REPO = HERE.parents[3]
PROBES = HERE.parent / 'operator-probes'
STEPS = ['identity', 'first-run', 'fixture', 'relaunch', 'watchers', 'task', 'lead', 'notices', 'observe']
TASK = ('Please ask my team to read the first line of CLAUDE.md in their repository and tell me what it says. '
        'Nothing else is needed.')
WATCHERS = ('quota-watch', 'stall-watch', 'codex-watch')
# What each watcher's notice starts with, as the lead is sent it.
MARKERS = {'quota-watch': 'QUOTA-THRESHOLD', 'codex-watch': 'CODEX-WATCH', 'stall-watch': 'ESCALATION-WATCH'}
CODEX_WORDS = 'READY TO LAND: codex/team-watch-walk, a fixture entry'
ESCALATION_ID = 'esc-team-watch-walk'


class TeamWatchWalk(command_walk.CommandWalk):
    def __init__(self, a):
        super().__init__(a)
        self.operator = self.data + '/operator'

    def read(self, path, limit=200000):
        return guest(self.vm, 'tail -c %d %s 2>/dev/null || true' % (limit, shlex.quote(path)), 60)

    def fixture(self):
        made = self.out / 'probes'
        made.mkdir(parents=True, exist_ok=True)
        tar = made / 'engine.tar'
        command_walk.command(['git', '-C', REPO, 'archive', '--format=tar', '-o', tar, self.a.expect_sha,
                              '.claude-plugin', 'richos/engine'], 300)
        for name in ('guest_probes.py',):
            (made / name).write_bytes((PROBES / name).read_bytes())
        (made / 'team-watch-guest.py').write_bytes((HERE / 'team-watch-guest.py').read_bytes())
        command_walk.command([HERE / 'guest.sh', self.vm, '--push', made, self.payload + '/'], 300)
        said = guest(self.vm, '/usr/bin/python3 %s setup %s' % (
            shlex.quote(self.payload + '/probes/team-watch-guest.py'), shlex.quote(self.payload)), 900)
        record = json.loads(said.splitlines()[-1])
        (self.out / 'fixture.json').write_text(json.dumps(record, indent=2) + '\n')
        fences = record.get('fence_status') or [1, '']
        if fences[0] != 0:
            raise StepFailed('the fixture fences are not on: ' + json.dumps(record.get('fence_install')) + ' '
                             + json.dumps(fences))
        self.facts.update({'entity': record['entity'], 'engine': record['engine'], 'channels': record['channels']})
        self.save()
        return {'entity': record['entity'], 'fences': fences[1][-200:], 'declaration': record['data'] + '/operator.json'}

    def watchers(self):
        log = self.read(json.loads((self.out / 'relaunch-observed.json').read_text())['relaunched']['log'])
        (self.out / 'app-after-relaunch.log').write_text(log + '\n')
        pids = {}
        for name in WATCHERS:
            line = next((l for l in log.splitlines() if 'his team: %s runs as pid' % name in l), '')
            if not line:
                raise StepFailed('the app never said it started %s; see app-after-relaunch.log' % name)
            pid = line.rsplit(' ', 1)[-1]
            record = json.loads(self.read(self.data + '/engine-state/team-watch/%s.json' % name) or '{}')
            alive = guest(self.vm, 'kill -0 %s 2>/dev/null && echo alive || true' % pid) == 'alive'
            if str(record.get('pid')) != pid or not alive:
                raise StepFailed('%s: pid %s, record %s, alive %s' % (name, pid, record, alive))
            pids[name] = int(pid)
        self.facts['watchers'] = pids
        self.save()
        return pids

    def lead(self):
        end = time.monotonic() + self.a.within
        while time.monotonic() < end:
            log = self.read(self.operator + '/operator.log')
            pointer = json.loads(self.read(self.operator + '/team-watch-lead.json') or '{}')
            if 'lead started for' in log and pointer.get('session'):
                self.facts['session'] = pointer['session']
                self.save()
                return {'session': pointer['session'],
                        'started': [l for l in log.splitlines() if 'lead started for' in l][-1]}
            time.sleep(3)
        (self.out / 'operator.log').write_text(self.read(self.operator + '/operator.log') + '\n')
        self.shot('lead-missing.png')
        raise StepFailed('no lead of his team started within %g s of the task; see operator.log' % self.a.within)

    def idle(self, path):
        """stall_watch.lead_is_idle's rule, on the guest's copy of the lead's transcript: the last
        main-thread row is the turn's end (a `turn_duration` system row)."""
        script = ('import json,sys\n'
                  'rows=[json.loads(l) for l in open(sys.argv[1]) if l.strip()]\n'
                  'for r in reversed(rows):\n'
                  '    if r.get("isSidechain"): continue\n'
                  '    if r.get("type") in ("user","assistant"): print("busy"); break\n'
                  '    if r.get("type")=="system" and r.get("subtype")=="turn_duration": print("idle"); break\n')
        return guest(self.vm, 'python3 -c %s %s 2>/dev/null || true' % (shlex.quote(script), shlex.quote(path)), 60) == 'idle'

    def notices(self):
        end = time.monotonic() + self.a.within
        path = ''
        while time.monotonic() < end:
            path = path or self.transcript()
            if path and self.idle(path):
                break
            time.sleep(5)
        else:
            raise StepFailed('his lead never went idle within %g s (transcript %s)' % (self.a.within, path or 'none'))
        channel = self.facts['channels'][0]
        entry = '## %s — fixture\n%s\n' % (time.strftime('%Y-%m-%dT%H:%MZ', time.gmtime()), CODEX_WORDS)
        guest(self.vm, 'printf %%s %s >> %s' % (shlex.quote(entry), shlex.quote(channel)))
        ledger = guest(self.vm, '/usr/bin/python3 %s ledger %s' % (
            shlex.quote(self.payload + '/probes/team-watch-guest.py'), shlex.quote(self.payload)), 60)
        raised = guest(self.vm, "date -u +%Y-%m-%dT%H:%M:%SZ")
        row = {'event': 'Escalation', 'id': ESCALATION_ID, 'raised': raised, 'teammate': 'fixture-mate',
               'worktree': self.facts['entity'] + '/.claude/worktrees/fixture-mate', 'branch': 'cc/fixture-mate',
               'repo': 'fixture', 'head': '', 'state': 'proceeding', 'for': 'lead',
               'title': 'Team-watch walk: a fixture escalation',
               'question': 'A fixture question, raised by the walk to show the escalation watch reaches the lead.',
               'tried': '', 'meanwhile': '', 'record': '', 'session_id': '', 'actor': 'team-watch-walk'}
        guest(self.vm, 'mkdir -p %s && printf %%s\\\\n %s >> %s' % (
            shlex.quote(str(Path(ledger).parent)), shlex.quote(json.dumps(row)), shlex.quote(ledger)))
        self.facts.update({'ledger': ledger, 'raised': raised})
        self.save()
        return {'channel': channel, 'ledger': ledger, 'raised': raised}

    def transcript(self):
        found = guest(self.vm, 'ls %s/.claude/projects/*/%s.jsonl 2>/dev/null || true' % (
            shlex.quote(self.home), shlex.quote(self.facts['session'])))
        return found.splitlines()[0] if found else ''

    def sent_to_lead(self, path):
        """Each watcher's notice, as a user message in his lead's own transcript: the first line of
        each, so the evidence carries no more of the fixture than it needs."""
        script = ('import json,sys\n'
                  'out={}\n'
                  'for l in open(sys.argv[1]):\n'
                  '    try: r=json.loads(l)\n'
                  '    except Exception: continue\n'
                  '    if r.get("type")!="user": continue\n'
                  '    c=(r.get("message") or {}).get("content")\n'
                  '    t=c if isinstance(c,str) else " ".join(x.get("text","") for x in c or [] if isinstance(x,dict))\n'
                  '    for k,m in json.loads(sys.argv[2]).items():\n'
                  '        if m in t and k not in out:\n'
                  '            line=next(s for s in t.splitlines() if m in s)\n'
                  '            out[k]={"timestamp":r.get("timestamp"),"line":line[:300]}\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c %s %s %s' % (
            shlex.quote(script), shlex.quote(path), shlex.quote(json.dumps(MARKERS))), 60) or '{}')

    def observe(self):
        end = time.monotonic() + self.a.within
        got, path = {}, ''
        while time.monotonic() < end:
            path = path or self.transcript()
            got = self.sent_to_lead(path) if path else {}
            if len(got) == len(MARKERS):
                break
            time.sleep(5)
        (self.out / 'operator.log').write_text(self.read(self.operator + '/operator.log') + '\n')
        for name in WATCHERS:
            (self.out / ('%s.log' % name)).write_text(self.read(self.data + '/engine-state/team-watch/%s.log' % name) + '\n')
        evidence = {'session': self.facts.get('session'), 'transcript': path, 'sent_to_lead': got}
        (self.out / 'observed.json').write_text(json.dumps(evidence, indent=2) + '\n')
        missing = [name for name in WATCHERS if name not in got]
        if missing:
            raise StepFailed('not in his lead\'s transcript within %g s: %s; see observed.json and the logs'
                             % (self.a.within, ', '.join(missing)))
        return got


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--task', default=TASK)
    p.add_argument('--within', type=float, default=300, help='seconds for a lead to start, and for the notices')
    p.add_argument('--relaunch-within', type=float, default=120)
    p.add_argument('--approvals', type=int, default=0)
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = TeamWatchWalk(a)
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
