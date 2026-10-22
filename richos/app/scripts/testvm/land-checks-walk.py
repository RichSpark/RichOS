#!/usr/bin/env python3
"""land-checks-walk.py — an app land meets the repository's own merge check and its land fence.

Branch G of richos-hq docs/plans/2026-10-21-everything-for-every-user.md (item 6): "an app land
gets the same checks a terminal land gets in that repository, and takes the land lease where the
fence is on." Section 6's walk: one regular install, a fixture repository whose merge check
refuses and whose fence is on. The check and the fence are the REPOSITORY's settings: this walk
puts them into the fixture repository in the guest, never into the app.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the
clone. The fixture home must carry the teammate first (the leases read the team at launch):

  land-checks-walk.py --seed-team FIXTURE_HOME
  run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE --report REPORT -- \\
      land-checks-walk.py --out DIR --expect-sha SHA [--within SECONDS]

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  team       the seeded teammate `builder` is in the guest's <app data>/team/
  first-run  adopt-walk.py's: company "Acme" with a folder that is a Git repository
  fixture    the Acme package committed; then the repository's own settings: a pre-merge-commit
             check that copies what the land lease says and refuses while a flag file exists,
             and the land fence installed and switched on (operator-fences.sh, the guest's
             engine, the app's own CLAUDE_CONFIG_DIR). Positive control: a move of main by a
             process holding no lease is refused.
  connect    review-walk.py's: the folder connected as a repository
  job        JOB, typed into the composer once
  refused    the land is asked and the check refuses it, once (it clears its own flag as it
             refuses, and JOB asks for the same land once more after a refusal); the lease was
             held by the app's land while the check ran
  landed     the check ran a second time and passed; main is ONE merge commit on the fixture's
             base whose second parent is the reviewed commit, and there is one land record, so the
             refused attempt landed nothing; the checkout is clean; slugify works on main; the
             lease was the app's again and is gone; the fence refused nothing; the refusal's
             words reached the conversation; the assignment closed settled

In --out: report.json, verdict.json, receipts.json, facts.json, acme-log.txt, the lease as the
check saw it each time, the app's logs. Exit 0 only when every step passed. Every app instance is
quit by run-walk.py's stop.sh (CEO §54).
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

_spec = importlib.util.spec_from_file_location('review_walk', HERE / 'review-walk.py')
review_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(review_walk)
command_walk = review_walk.command_walk
StepFailed = command_walk.StepFailed
OPEN = command_walk.OPEN

STEPS = ['identity', 'team', 'first-run', 'fixture', 'connect', 'job', 'refused', 'landed']
BUILDER = ('---\nname: builder\ndescription: Python engineer for small library changes in a user\'s repository: '
           'implements a function with its tests and commits. Use for implementation jobs.\nmodel: sonnet\n'
           'tools: Read, Glob, Grep, Bash, Write, Edit\n---\n\n'
           'You are Builder, a careful Python engineer. Implement exactly what your brief asks, with tests, '
           'and commit your work in your assigned worktree with git -C and an absolute path. Say what you '
           'did.\n')
JOB = ('Please have builder add a function slugify(text) to acme/text.py in my Acme repository. It must turn the '
       'text to lowercase, replace every run of spaces with a single hyphen, and remove any leading or trailing '
       'hyphens, so slugify("  -Hello  World- ") returns "hello-world". Add a unittest for it in tests/test_text.py. '
       'Have it reviewed and land it when the review passes. My repository has its own merge check, and today it '
       'refuses the first land it sees. When it refuses, tell me exactly what it said, then ask for the same land '
       'once more, unchanged: it passes the second time. Do not change the work or the repository to get past it.')
REFUSAL = 'ACME MERGE CHECK: refused, CHANGES.md has no entry for this land'
CHECK = review_walk.CHECK


class LandChecksWalk(review_walk.ReviewWalk):
    def __init__(self, a):
        super().__init__(a)
        self.gate = self.payload + '/merge-check'
        self.flag = self.gate + '/refuse'
        self.asked = self.gate + '/asked.log'
        self.entity = self.payload + '/fence-entity'
        self.engine = self.payload + '/engine'

    def team(self):
        path = self.data + '/team/builder.md'
        if guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true').strip() != BUILDER.strip():
            raise StepFailed(f'{path} is not the seeded teammate: run land-checks-walk.py --seed-team FIXTURE_HOME '
                             'before run-walk.py boots (the leases read the team at launch)')
        return {'builder': path}

    # --- the repository's own settings ------------------------------------------------------
    def as_the_app(self, script, timeout=120):
        """A guest command in the app's own HOME and CLAUDE_CONFIG_DIR (run.sh launches it with
        both), so the fence is installed with the lease home the app's land resolves."""
        env = 'HOME=%s CLAUDE_CONFIG_DIR=%s ' % (shlex.quote(self.home), shlex.quote(self.home + '/.claude'))
        return guest(self.vm, env + script, timeout)

    def fixture(self):
        found = review_walk.ReviewWalk.fixture(self)
        c = shlex.quote(self.company)
        (self.out / 'fence-entity.config').write_text('OPERATOR_FENCES="on"\nLAND_LEASE_HOLDERS=""\nOPERATOR_FENCES_REPOS=""\n')
        guest(self.vm, 'mkdir -p %s %s && printf %s > %s/orchestration.config' % (
            shlex.quote(self.entity), shlex.quote(self.gate),
            shlex.quote((self.out / 'fence-entity.config').read_text()), shlex.quote(self.entity)))
        fences = shlex.quote(self.engine + '/scripts/operator-fences.sh')
        said = {}
        for verb in ('install', 'on'):
            said[verb] = self.as_the_app('bash %s %s --repo %s --entity %s 2>&1; echo "exit=$?"' % (
                fences, verb, c, shlex.quote(self.entity))).strip()
            if not said[verb].endswith('exit=0'):
                raise StepFailed(f'operator-fences.sh {verb} failed in the guest: {said[verb][-1500:]}')
        launcher = guest(self.vm, 'grep "^OPERATOR_FENCES_" %s/.git/hooks/reference-transaction' % c)
        conf = dict(line.split('=', 1) for line in launcher.splitlines() if '=' in line)
        conf = {k.replace('OPERATOR_FENCES_', ''): v.strip().strip('"') for k, v in conf.items()}
        if conf.get('STATE') != 'on':
            raise StepFailed('the fence is not on after operator-fences.sh on: ' + launcher)
        lease = conf['HOME'] + '/' + conf['KEY'] + '.lease'
        self.facts['fence'] = {'launcher': conf, 'lease': lease, 'said': said}
        # The repository's merge check: what the lease says while it runs, then refuse while the
        # flag exists. A copy per call, numbered, so each land attempt keeps its own reading.
        check = ('#!/bin/sh\n'
                 'n=0; [ -f {log} ] && n=$(wc -l < {log} | tr -d " ")\n'
                 'n=$((n + 1))\n'
                 'cp {lease} {gate}/lease-seen-$n.json 2>/dev/null || echo none > {gate}/lease-seen-$n.json\n'
                 'echo "asked $n" >> {log}\n'
                 'if [ -e {flag} ]; then rm -f {flag}; echo {refusal} >&2; exit 1; fi\n'
                 'exit 0\n').format(log=shlex.quote(self.asked), lease=shlex.quote(lease), gate=shlex.quote(self.gate),
                                    flag=shlex.quote(self.flag), refusal=shlex.quote(REFUSAL))
        guest(self.vm, 'printf %s > %s/.git/hooks/pre-merge-commit && chmod +x %s/.git/hooks/pre-merge-commit && : > %s'
              % (shlex.quote(check), c, c, shlex.quote(self.flag)))
        # POSITIVE CONTROL: the fence refuses a move of main by a process that holds no lease.
        control = guest(self.vm, 'cd %s && t=$(git commit-tree "HEAD^{tree}" -p HEAD -m stray) && '
                                 'if git update-ref refs/heads/main "$t" HEAD 2>/dev/null; then echo MOVED; '
                                 'git update-ref refs/heads/main HEAD~1; else echo REFUSED; fi' % c).strip()
        self.facts['fence_control'] = control
        self.facts['fence_refusals_after_control'] = self.git_state()['fence_refusals']
        self.save()
        if control != 'REFUSED':
            raise StepFailed('the fence did not refuse a move of main without the lease: ' + control)
        if guest(self.vm, 'git -C %s rev-parse HEAD' % c).strip() != found['base']:
            raise StepFailed('main moved during the fixture')
        return {**found, 'fence': 'on', 'lease_home': conf['HOME'], 'control': control}

    def connect(self):
        """The shared step (adopt-walk.py connect_folder), with what it types read in the guest
        first, and the chooser as it stood kept when the step fails: walks walk-9696d0b3a4f1 and
        walk-eb9f006fb025 ended there with Open disabled, and neither left a picture of why."""
        c = shlex.quote(self.company)
        self.facts['company_path'] = {'typed': self.company, 'guest': guest(
            self.vm, 'ls -ld %s 2>&1; [ -d %s/.git ] && echo "git repository" || echo "no .git"' % (c, c)).strip()}
        self.save()
        try:
            return super().connect()
        except StepFailed:
            for name, keep in (('connect-failure.png', lambda: self.shot('connect-failure.png')),
                               ('connect-failure-tree.txt', lambda: (self.out / 'connect-failure-tree.txt').write_text(
                                   command_walk.command([HERE / 'ax.sh', self.vm, 'tree', '--depth', '8'], 90)))):
                try:
                    keep()
                except (StepFailed, subprocess.TimeoutExpired, OSError) as exc:
                    self.facts.setdefault('connect_failure_evidence', {})[name] = 'not kept: %s' % str(exc)[-300:]
            self.save()
            raise

    def job(self):
        """THIS walk's JOB, the one that says the check refuses once and asks for the land again.
        Not ReviewWalk.job(), which sends review-walk.py's own request (mid-job review
        rv-20261021T072931Z-a4453faa-ffff, finding 2)."""
        sent = self.send(JOB)
        self.facts['sent_ms'] = sent
        self.facts['job'] = JOB
        self.save()
        return {'sent_between_guest_ms': [round(v) for v in sent], 'job': JOB}

    # --- reading the guest --------------------------------------------------------------------
    def asked_count(self):
        text = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(self.asked))
        return len([l for l in text.splitlines() if l.startswith('asked')])

    def lease_seen(self, n):
        text = guest(self.vm, 'cat %s/lease-seen-%d.json 2>/dev/null || true' % (shlex.quote(self.gate), n)).strip()
        (self.out / ('lease-seen-%d.json' % n)).write_text(text + '\n')
        try:
            holder = json.loads(text).get('holder') or {}
        except ValueError:
            return {'read': text[:200]}
        args = guest(self.vm, 'ps -o args= -p %d 2>/dev/null || true' % int(holder.get('pid') or 0)).strip()
        return {'kind': holder.get('kind'), 'pid': holder.get('pid'), 'process': args[:300]}

    def git_state(self):
        c = shlex.quote(self.company)
        return {'main': guest(self.vm, 'git -C %s rev-parse main' % c).strip(),
                'status': guest(self.vm, 'git -C %s status --porcelain --untracked-files=all' % c).strip(),
                'merge_head': guest(self.vm, 'git -C %s rev-parse -q --verify MERGE_HEAD || true' % c).strip(),
                'lease_left': guest(self.vm, '[ -e %s ] && echo yes || echo no' % shlex.quote(self.facts['fence']['lease'])).strip(),
                'fence_refusals': guest(self.vm, 'cat %s 2>/dev/null | wc -l' % shlex.quote(
                    self.facts['fence']['launcher']['HOME'] + '/fence-refusals.jsonl')).strip()}

    def until(self, done, what):
        end = time.monotonic() + self.a.within
        last = 0
        while time.monotonic() < end:
            try:
                if done():
                    return True
            except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError) as exc:
                self.facts.setdefault('observe_misses', []).append(str(exc)[:300])
            if time.monotonic() - last > 120:
                last = time.monotonic()
                rows = self.receipts()
                print(json.dumps({'waiting_for': what, 'guest_ms': round(self.clock()), 'asked': self.asked_count(),
                                  'receipts': [(r['request']['role'], r.get('status')) for r in rows]}), flush=True)
            time.sleep(15)
        raise StepFailed(f'{what} did not happen within {self.a.within:.0f} s')

    def told(self, text):
        """Did the user hear `text`: in the conversation's own reply (its AssistantDelta rows, joined,
        since a reply is streamed in pieces) or in a notice on an assignment record."""
        script = ('import glob,json,sys\n'
                  'said=[]\n'
                  'for line in open(sys.argv[1]+"/conversation-ledger.jsonl",errors="replace"):\n'
                  '    try: row=json.loads(line)\n'
                  '    except ValueError: continue\n'
                  '    if row.get("event")=="AssistantDelta": said.append(row.get("text") or "")\n'
                  'for p in glob.glob(sys.argv[1]+"/engine-state/assignments/*/*.json"):\n'
                  '    try: said += [n.get("text") or "" for n in json.load(open(p)).get("notices") or []]\n'
                  '    except Exception: pass\n'
                  'print("yes" if sys.argv[2] in "".join(said) else "no")\n')
        return guest(self.vm, 'python3 -c %s %s %s' % (shlex.quote(script), shlex.quote(self.data),
                                                         shlex.quote(text)), 60).strip() == 'yes'

    def land_records(self):
        conf = self.facts['fence']['launcher']
        path = conf['HOME'] + '/' + conf['KEY'] + '.lands'
        text = guest(self.vm, 'cat %s 2>/dev/null || true' % shlex.quote(path))
        return [json.loads(l) for l in text.splitlines() if l.strip().startswith('{')]

    # --- the land, refused, then landed --------------------------------------------------------
    def refused(self):
        self.until(lambda: self.asked_count() >= 1, 'the repository merge check being asked')
        state = self.git_state()
        seen = self.lease_seen(1)
        self.facts['refused'] = {'state_when_read': state, 'lease_seen': seen}
        self.save()
        # The retry may already be running when this reads (the check clears its flag as it
        # refuses), so what the refused attempt left is judged at the end, from the history: main's
        # parents and the one land record. Here, only what the check itself saw.
        if seen.get('kind') != 'app':
            raise StepFailed('the lease while the check ran was not the app land\'s: %s' % json.dumps(seen))
        return {'asked': self.asked_count(), 'lease_while_checking': seen}

    def landed(self):
        self.until(lambda: self.asked_count() >= 2 and self.git_state()['main'] != self.facts['base'],
                   'the second land')
        rows = self.receipts()
        record = None
        end = time.monotonic() + 600
        while time.monotonic() < end:
            record = self.ours(self.facts['sent_ms']) or record
            if record and record.get('state') not in OPEN:
                break
            time.sleep(15)
        record = self.settled_read(record)
        rows = self.receipts()
        (self.out / 'receipts.json').write_text(json.dumps([{k: v for k, v in r.items() if k != '_brief'} for r in rows],
                                                           indent=2) + '\n')
        c = shlex.quote(self.company)
        (self.out / 'acme-log.txt').write_text(guest(self.vm, 'git -C %s log --format="%%h %%p %%s" main' % c))
        state = self.git_state()
        seen = self.lease_seen(2)
        parents = guest(self.vm, 'git -C %s rev-list --parents -n 1 main' % c).split()[1:]
        works = guest(self.vm, 'cd %s && python3 -c %s %s 2>&1 || true' % (c, shlex.quote(CHECK), c)).strip()
        lines = judge(rows, record, base=self.facts['base'], parents=parents, works=works, asked=self.asked_count(),
                      seen=seen, state=state, refusals_before=self.facts.get('fence_refusals_after_control'),
                      records=self.land_records(), told=self.told('CHANGES.md'))
        verdict = {'lines': lines, 'refused': self.facts.get('refused')}
        (self.out / 'verdict.json').write_text(json.dumps(verdict, indent=2) + '\n')
        failed = [l for l in lines if l['outcome'] != 'PASS']
        if failed:
            raise StepFailed('; '.join(f"{l['line']}: {l['detail']}" for l in failed))
        return {'lines': [(l['line'], l['outcome']) for l in lines]}


def judge(rows, record, *, base, parents, works, asked, seen, state, refusals_before, records, told):
    """The pass list from what was read in the guest (see the module doc)."""
    integrated = [r for r in rows if r['request']['role'] == 'worker' and (r.get('integration') or {}).get('verified')]
    reviewed = integrated[0]['integration']['commit'] if integrated else ''
    lines = []

    def line(name, ok, detail):
        lines.append({'line': name, 'outcome': 'PASS' if ok else 'FAIL', 'detail': detail})

    line('1 the repository merge check ran for each land attempt', asked == 2, 'asked %d times' % asked)
    line('2 main is the land\'s merge commit of the reviewed work', bool(reviewed) and parents == [base, reviewed],
         'parents %s, base %s, reviewed %s' % (parents, base[:12], reviewed[:12]))
    line('3 slugify works on main', works == "'hello-world'", works[:300])
    line('4 the app land held the lease while the check ran', seen.get('kind') == 'app', json.dumps(seen))
    line('5 the lease is gone after the land', state['lease_left'] == 'no', state['lease_left'])
    line('6 the fence refused nothing of the app land', state['fence_refusals'] == refusals_before,
         'fence-refusals.jsonl rows: %s, %s after the fixture\'s control' % (state['fence_refusals'], refusals_before))
    line('7 the refused attempt landed nothing: one land record, and it is main',
         len(records) == 1 and records[0].get('commit') == state['main'],
         json.dumps([(r.get('before', '')[:12], r.get('commit', '')[:12]) for r in records]))
    line('8 the checkout is clean', not state['status'] and not state['merge_head'], repr(state['status']))
    line('9 the check\'s refusal reached the user', told,
         'searched the conversation\'s reply and the assignment notices for CHANGES.md')
    line('10 the assignment closed settled', bool(record) and record.get('state') == 'settled',
         'state %s' % (record or {}).get('state'))
    return lines


def seed_team(home):
    folder = Path(home) / 'Library/Application Support/com.richos.app/team'
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'builder.md').write_text(BUILDER)
    print(folder / 'builder.md')
    return 0


def main():
    if sys.argv[1:2] == ['--seed-team']:
        return seed_team(sys.argv[2])
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True, help='the commit the bundle under test was built from')
    p.add_argument('--within', type=float, default=3000, help='seconds for each land attempt to happen')
    p.add_argument('--approvals', type=int, default=6, help='most Approve presses (each is counted)')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = LandChecksWalk(a)
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
