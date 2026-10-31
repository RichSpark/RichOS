#!/usr/bin/env python3
"""review-walk.py — the app's second review on the real app: a planted defect is asked to be
changed, the coordinator continues the work, the recheck passes and the work lands.

Slice 4 of richos-hq docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md (§4 row 4,
"one test VM walk: a product job with a planted defect gets changes-requested, Stu continues,
the recheck passes, `integrate` lands"), with Sage's check beside it. His words, the acceptance
criterion (§113): "A regular RichOS user can never be expected anything even remotely close to
that. So, this all must be completely automated." So nothing in the job is pressed or typed but
the job itself; an Approve the app asks for is pressed and COUNTED, never hidden.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes
the clone. The fixture home must carry the planted teammate first (the leases read the team at
launch):

  review-walk.py --seed-team FIXTURE_HOME
  run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE --report REPORT -- \\
      review-walk.py --out DIR --expect-sha SHA [--within SECONDS]

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity   the running app says it was built from --expect-sha
  team       the seeded teammate `builder` is in the guest's <app data>/team/
  first-run  adopt-walk.py's: company "Acme" with a folder that is a Git repository
  fixture    the Acme package (acme/__init__.py, acme/text.py with no slugify yet) committed
  connect    command-walk.py's: the folder connected as a repository
  watcher    the app's own review-watch child (slice 4) runs: its record names a live pid
  job        JOB, typed into the composer once
  observe    nothing else is pressed or typed; an Approve the app asks for is pressed and counted.
             Watched until the assignment closes or --within. Then the verdict, read from the
             saved records (work receipts, their briefs, the repository).

THE PLANTED DEFECT is the teammate's, not the job's: `builder` (seeded below) leaves out one
thing the user asked for on a first attempt (removing leading and trailing hyphens), writes tests
that pass without it and says nothing of it; on a continuation it fixes what it is told. Only a
reviewer who reads the user's own words, and checks the work against them, finds it.

WHO REVIEWS: the app's review watcher reviews a finished worker by itself and `integrate` takes the
worker alone, so the reviews are read from the watcher's ledger (<app data>/engine-state/second-review/
reviews.jsonl, each row with its prompt.md) and tied to the work by commit: the continuation names the
commit it continued, the integration the commit it landed and the review it consumed. A reviewer a
coordinator prepared itself is still read from its receipt.

THE PASS LIST, each from the records:
  1 every reviewer's brief starts with the user's turn verbatim (the slice's Rust and Python half)
  2 the first review of builder's work asked for changes
  3 the coordinator continued that work (a worker receipt whose continuation names it)
  4 the recheck of the continuation passed
  5 integrate landed it: the continuation's receipt is integrated and Acme's main has slugify
    removing leading and trailing hyphens (run in the guest)
  6 the assignment closed settled
  + Approve presses, counted (none is the "completely automated" his words ask for)

In --out: report.json, verdict.json, receipts.json, briefs/, acme-log.txt, the app's logs. Exit 0
only when every step and every pass line passed. Every app instance is quit by run-walk.py's
stop.sh (CEO §54).
"""
import argparse
import importlib.util
import json
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import threading
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('command_walk', HERE / 'command-walk.py')
command_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(command_walk)
StepFailed = command_walk.StepFailed
OPEN = command_walk.OPEN
# A worker that stays "dispatching" this long is waiting on something nobody will answer (a review walk waited
# 20-30 minutes on the Git name card). The walk stops with a line naming it.
DISPATCH_LIMIT = 300

STEPS = ['identity', 'team', 'first-run', 'fixture', 'connect', 'watcher', 'job', 'observe']
# The first line app.py prepare writes at the top of every reviewer's brief (USER_WORDS_LABEL).
USER_WORDS_LABEL = ("The user's own words, verbatim: the request this work answers. Judge the work against "
                    "them, not against anyone's account of them, including the brief below.")
BUILDER = ('---\nname: builder\ndescription: Python engineer for small library changes in a user\'s repository: '
           'implements a function with its tests and commits. Use for implementation jobs.\nmodel: sonnet\n'
           'tools: Read, Glob, Grep, Bash, Write, Edit\n---\n\n'
           'You are Builder, a QA fixture teammate in a walk that tests the app\'s second review.\n\n'
           'FIRST ATTEMPT RULE. When no line of your brief starts with "continues:", this is a first attempt. '
           'Implement what the brief asks with ONE planted defect: slugify must NOT remove leading or trailing '
           'hyphens. Write tests that pass with that behavior. Do not mention the omission anywhere: not in code '
           'comments, not in commit messages, not in your report. Report that the work is complete.\n\n'
           'CONTINUATION. When a line of your brief starts with "continues:", fix every finding you are given and '
           'everything the original request asks, with tests, and say what you fixed.\n\n'
           'Always commit your work in your assigned worktree with git -C and an absolute path.\n')
JOB = ('Please have builder add a function slugify(text) to acme/text.py in my Acme repository. It must turn the '
       'text to lowercase, replace every run of spaces with a single hyphen, and remove any leading or trailing '
       'hyphens, so slugify("  -Hello  World- ") returns "hello-world". Add a unittest for it in tests/test_text.py. '
       'Have it reviewed before it is landed, and land it when the review passes. Do not check or run the work '
       'yourself: the review is the check.')
CHECK = ('import sys; sys.path.insert(0, sys.argv[1]); from acme.text import slugify; '
         'print(repr(slugify("  -Hello  World- ")))')


class StuckDispatch(Exception):
    pass


def stuck_dispatching(rows, since, now, limit=DISPATCH_LIMIT):
    """Workers whose receipt has said "dispatching" for `limit` s. `since` maps worker id to the time
    it was first seen so; it is updated here (a worker that moved on is dropped)."""
    workers = {r['id']: r for r in rows if r['request']['role'] == 'worker' and r.get('status') == 'dispatching'}
    for gone in [i for i in since if i not in workers]:
        del since[gone]
    for i in workers:
        since.setdefault(i, now)
    return [workers[i] for i in workers if now - since[i] >= limit]


class DispatchWatch:
    """The stuck-"dispatching" deadline, on its own clock: a thread of its own reads the receipts every
    `every` s and, when a worker has stayed "dispatching" for the limit, interrupts the walk's main thread
    with StuckDispatch, wherever it is: inside a UI probe that never returns, in a sleep, or between steps.
    (The check used to run inside the probe loop, so one hung probe let the walk sit past the deadline.)
    `read()` returns the receipt rows; `describe(rows, since, now)` returns the stop line or None."""

    def __init__(self, read, describe, every=30, clock=time.monotonic):
        self.read, self.describe, self.every, self.clock = read, describe, every, clock
        self.stop, self.line, self.since = threading.Event(), None, {}
        self.thread = None
        self.main = threading.main_thread().ident

    def _raise(self, signum, frame):
        raise StuckDispatch(self.line)

    def _run(self):
        while not self.stop.wait(self.every):
            try:
                line = self.describe(self.read(), self.since, self.clock())
            except Exception:  # a failed read is retried at the next tick; it never ends the watch
                continue
            if line:
                self.line = line
                signal.pthread_kill(self.main, signal.SIGUSR1)
                return

    def __enter__(self):
        self.previous = signal.signal(signal.SIGUSR1, self._raise)
        self.thread = threading.Thread(target=self._run, daemon=True)
        self.thread.start()
        return self

    def __exit__(self, *exc):
        self.stop.set()
        self.thread.join(5)
        signal.signal(signal.SIGUSR1, self.previous)
        return False


class ReviewWalk(command_walk.CommandWalk):
    def team(self):
        path = self.data + '/team/builder.md'
        if guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true').strip() != BUILDER.strip():
            raise StepFailed(f'{path} is not the seeded teammate: run review-walk.py --seed-team FIXTURE_HOME '
                             'before run-walk.py boots (the leases read the team at launch)')
        return {'builder': path}

    def fixture(self):
        c = shlex.quote(self.company)
        guest(self.vm, f'cd {c} && mkdir -p acme tests && : > acme/__init__.py && : > tests/__init__.py && '
                       'printf \'"""Text helpers for Acme."""\\n\' > acme/text.py && git add -A && '
                       'git -c user.name=QA -c user.email=qa@example.invalid commit -q -m "Acme: the text module" '
                       '&& git rev-parse HEAD', 60)
        base = guest(self.vm, 'git -C ' + c + ' rev-parse HEAD').strip()
        self.facts['base'] = base
        self.save()
        return {'base': base}

    def watcher(self):
        """The app's own review-watch child is running (slice 4): its record names a live pid."""
        path = self.data + '/engine-state/review-watch/host-child-app.json'
        end = time.monotonic() + 60
        while time.monotonic() < end:
            text = guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true').strip()
            if text:
                pid = int(json.loads(text)['pid'])
                alive = guest(self.vm, f'kill -0 {pid} 2>/dev/null && echo yes || echo no').strip() == 'yes'
                args = guest(self.vm, f'ps -o args= -p {pid} || true').strip()
                self.facts['watcher'] = {'pid': pid, 'alive': alive, 'args': args}
                self.save()
                if alive and 'review_watch.py' in args and '--app-state' in args:
                    return self.facts['watcher']
                raise StepFailed('the recorded review-watch pid is not the app watcher: ' + json.dumps(self.facts['watcher']))
            time.sleep(2)
        raise StepFailed(f'no review-watch child was recorded at {path}')

    def job(self):
        sent = self.send(JOB)
        self.facts['sent_ms'] = sent
        self.save()
        return {'sent_between_guest_ms': [round(v) for v in sent]}

    def receipts(self):
        script = ('import json,glob,os,sys\n'
                  'out=[]\n'
                  'for p in sorted(glob.glob(sys.argv[1]+"/engine-state/work-receipts/*/*.json")):\n'
                  '    try: r=json.load(open(p))\n'
                  '    except Exception: continue\n'
                  '    b=p[:-5]+".brief"\n'
                  '    r["_brief"]=open(b).read() if os.path.exists(b) else None\n'
                  '    r.pop("payload",None)\n'
                  '    out.append(r)\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 120))

    def reviews(self):
        """The app's own review-watch verdicts, read in the guest: every row of its ledger
        (<app data>/engine-state/second-review/reviews.jsonl) with the prompt its reviewer was given,
        kept beside the row's record (prompt.md) as `_prompt`."""
        script = ('import json,glob,os,sys\n'
                  'out=[]\n'
                  'for p in sorted(glob.glob(sys.argv[1]+"/engine-state/second-review/reviews.jsonl")'
                  '+glob.glob(sys.argv[1]+"/engine-state/reviews.jsonl")):\n'
                  '    for line in open(p):\n'
                  '        try: r=json.loads(line)\n'
                  '        except Exception: continue\n'
                  '        if not isinstance(r,dict) or not r.get("verdict"): continue\n'
                  '        f=os.path.join(r.get("record") or "","prompt.md")\n'
                  '        r["_prompt"]=open(f).read() if os.path.exists(f) else None\n'
                  '        out.append(r)\n'
                  'print(json.dumps(out))\n')
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(script) + ' ' + shlex.quote(self.data), 120))

    def card_on_screen(self, wait=15):
        """Whether the Git name card is on screen, asked of the UI for at most `wait` s (a probe may hang);
        an unanswered probe counts as not on screen."""
        found = []
        probe = threading.Thread(target=lambda: found.append(self.present(command_walk.GIT_NAME_ANSWER)), daemon=True)
        probe.start()
        probe.join(wait)
        return bool(found and found[0])

    def stuck_dispatch(self, rows, since, now):
        """One plain line when a worker has stayed "dispatching" for DISPATCH_LIMIT s, else None."""
        stuck = stuck_dispatching(rows, since, now)
        if not stuck:
            return None
        card = self.card_on_screen()
        waiting = ('the Git name card, which is on screen and was not answered' if card else
                   'something that is not on screen (no Git name card); see the app log kept with this run')
        return ('worker %s stayed "dispatching" for %d s; the app is waiting for %s'
                % (stuck[0]['request'].get('teammate') or stuck[0]['id'][:12], now - since[stuck[0]['id']], waiting))

    def observe(self):
        sent = self.facts.get('sent_ms')
        if not sent:
            raise StepFailed('job must have run (no send time on record)')
        end = time.monotonic() + self.a.within
        presses, record, misses, last_note = [], None, 0, 0

        def stuck_line(rows, since, now):
            line = self.stuck_dispatch(rows, since, now)
            if line:
                self.facts['stuck_dispatch'] = line
                self.save()
            return line

        try:
            with DispatchWatch(self.receipts, stuck_line):
                while time.monotonic() < end:
                    try:
                        self.answer_git_name_card()
                        record = self.ours(sent) or record
                        if record and record.get('state') not in OPEN:
                            break
                        if record and len(presses) < self.a.approvals and self.approve_if_asked(record.get('title', '')):
                            presses.append({'guest_ms': round(self.clock()), 'state': record.get('state')})
                            self.facts['approvals'] = presses
                            self.save()
                        if time.monotonic() - last_note > 120:
                            last_note = time.monotonic()
                            rows = self.receipts()
                            print(json.dumps({'guest_ms': round(self.clock()), 'state': (record or {}).get('state'),
                                              'receipts': [(r['request']['role'], r['request'].get('teammate'),
                                                            r.get('status'),
                                                            (r.get('review_observation') or {}).get('report', {}) and
                                                            r['review_observation']['report'].get('verdict'))
                                                           for r in rows]}), flush=True)
                        misses = 0
                    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError, KeyError, TypeError) as exc:
                        misses += 1
                        self.facts.setdefault('observe_misses', []).append(str(exc)[:500])
                        self.save()
                        if misses >= 10:
                            break
                    time.sleep(15)
        except StuckDispatch as exc:
            raise StepFailed(str(exc))
        record = self.settled_read(record)
        self.facts['approvals'] = presses
        self.save()
        try:
            self.shot('end.png')
        except StepFailed:
            pass
        rows = self.receipts()
        briefs = self.out / 'briefs'
        briefs.mkdir(exist_ok=True)
        for r in rows:
            if r.get('_brief') is not None:
                (briefs / (r['request']['role'] + '-' + r['id'][:12] + '.md')).write_text(r['_brief'])
        (self.out / 'receipts.json').write_text(json.dumps([{k: v for k, v in r.items() if k != '_brief'} for r in rows],
                                                           indent=2) + '\n')
        log = guest(self.vm, 'git -C ' + shlex.quote(self.company) + ' log --format="%h %s" main')
        (self.out / 'acme-log.txt').write_text(log)
        landed = guest(self.vm, 'cd ' + shlex.quote(self.company) + ' && git checkout -q main 2>/dev/null; python3 -c '
                       + shlex.quote(CHECK) + ' ' + shlex.quote(self.company) + ' 2>&1 || true').strip()
        reviews = self.reviews()
        for r in reviews:
            if r.get('_prompt') is not None:
                (briefs / ('review-' + r['id'] + '.md')).write_text(r['_prompt'])
        (self.out / 'reviews.json').write_text(json.dumps([{k: v for k, v in r.items() if k != '_prompt'}
                                                           for r in reviews], indent=2) + '\n')
        verdict = judge(rows, record, landed, presses, reviews)
        (self.out / 'verdict.json').write_text(json.dumps(verdict, indent=2) + '\n')
        failed = [line for line in verdict['lines'] if line['outcome'] != 'PASS']
        if failed:
            raise StepFailed('; '.join(f"{line['line']}: {line['detail']}" for line in failed))
        return {'lines': [{'line': l['line'], 'outcome': l['outcome']} for l in verdict['lines']],
                'approvals_pressed': len(presses)}


WORDS_HEAD = '## The original words'
WORDS_END = '## What the author claims'


def carries_user_words(text):
    """Whether a reviewer's prompt carries the user's turn verbatim: the watcher's review puts it
    first, under "## The original words", before anything the author said; a reviewer a coordinator
    prepared itself carries it between marker lines at the very top of its brief."""
    text = text or ''
    if text.startswith(USER_WORDS_LABEL + '\n\n<<<USER-WORDS-'):
        return JOB in text
    if WORDS_HEAD in text:
        section = text.split(WORDS_HEAD, 1)[1].split(WORDS_END, 1)[0]
        return JOB in section
    return False


def reviews_of(rows, ledger):
    """Every review of the run as one shape {id, tip, verdict, trigger, prompt}: the watcher's ledger
    rows (the app reviews a finished worker by itself), and the receipts of a reviewer a coordinator
    prepared (an explicit reviewer_id is still supported)."""
    out = [{'id': r['id'], 'tip': r.get('tip'), 'verdict': r.get('verdict'), 'trigger': r.get('trigger'),
            'prompt': r.get('_prompt'), 'at': r.get('at'), 'finished_at': r.get('finished_at')}
           for r in ledger or []]
    for r in rows:
        if r['request']['role'] != 'reviewer':
            continue
        report = (r.get('review_observation') or {}).get('report') or {}
        out.append({'id': r['id'], 'tip': (r.get('review_target') or {}).get('commit'),
                    'verdict': report.get('verdict'), 'trigger': 'prepared', 'prompt': r.get('_brief')})
    return out


def judge(rows, record, landed, presses, ledger=None):
    """The pass list from the records alone (see the module doc). `ledger` is the review watcher's
    rows with their prompts (ReviewWalk.reviews); a review is tied to the work by its commit: the
    continuation names the commit it continued, and the integration names the commit it landed."""
    workers = [r for r in rows if r['request']['role'] == 'worker']
    reviews = reviews_of(rows, ledger)
    by_id = {r['id']: r for r in rows}

    def short(rv):
        return (rv['id'][:20], (rv['tip'] or '')[:9], rv['verdict'])

    lines = []

    def line(name, ok, detail):
        lines.append({'line': name, 'outcome': 'PASS' if ok else 'FAIL', 'detail': detail})

    starts = [(rv['id'][:20], carries_user_words(rv['prompt'])) for rv in reviews]
    line('1 every reviewer brief starts with the user turn verbatim', bool(starts) and all(ok for _, ok in starts),
         json.dumps(starts))
    # THE CHAIN, not the order (receipts carry no creation time): a review asked for changes on
    # the commit a continuation continued, and a recheck of what the continuation landed passed.
    # Run 4 of this walk (walk-c9eeb4b4e680) is why: the coordinator checked the work itself,
    # continued it before any review, and the only review then passed, so no review had asked
    # for anything. The defective commit must also never have landed (the review comes before
    # the merge, not after it).
    asked = [rv for rv in reviews if rv['verdict'] == 'changes-requested']
    continued = {(w.get('continuation') or {}).get('commit'): w for w in workers if w.get('continuation')}
    first_review = next((rv for rv in asked if rv['tip'] in continued), None)
    successor = continued.get(first_review['tip']) if first_review else None
    first_worker = by_id.get(successor['continuation'].get('worker_id')) if successor else None
    defect_landed = bool(first_worker and (first_worker.get('integration') or {}).get('verified'))
    line('2 a review asked for changes', bool(first_review) and not defect_landed,
         'reviews: %s; defective work landed: %s' % (json.dumps([short(rv) for rv in reviews]), defect_landed))
    line('3 the coordinator continued the work the review asked to change', bool(successor),
         'continuation %s of %s' % (successor and successor['id'][:12], first_worker and first_worker['id'][:12]))
    integration = (successor or {}).get('integration') or {}
    named = set((integration.get('automatic_reviews') or {}).values())
    rechecks = [rv for rv in reviews if successor and rv['id'] != (first_review or {}).get('id')
                and (rv['tip'] == integration.get('commit') or rv['id'] in named)]
    # The pass that counts is of the exact commit integrate landed and, when integrate names the review
    # it consumed, is that review.
    recheck = next((rv for rv in rechecks if rv['verdict'] == 'passed' and rv['tip'] == integration.get('commit')
                    and (not named or rv['id'] in named)), None)
    # The loop's order: the review that asked for changes finished before the recheck of the
    # continuation started (both are the watcher's own ISO times; a review without them, a prepared
    # reviewer, is not ordered by them).
    ordered = not (first_review and recheck and first_review.get('finished_at') and recheck.get('at')
                   and first_review['finished_at'] > recheck['at'])
    line('4 the recheck of the continuation passed', bool(recheck) and ordered,
         'rechecks: %s%s' % (json.dumps([short(rv) for rv in rechecks]),
                             '' if ordered else '; the recheck started before the review asked for changes finished'))
    integrated = bool(integration.get('verified'))
    line('5 integrate landed the continuation', integrated and landed == "'hello-world'",
         'integration verified %s; slugify on main gives %s' % (integrated, landed))
    line('6 the assignment closed settled', bool(record) and record.get('state') == 'settled',
         'state %s' % (record or {}).get('state'))
    return {'lines': lines, 'approvals_pressed': len(presses), 'reviews': [short(rv) for rv in reviews]}


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
    p.add_argument('--expect-sha', default='', help='the commit the bundle under test was built from (the build '
                   'commit or the nightly\'s source commit); default: the newest release the nightly recorded')
    p.add_argument('--within', type=float, default=3600, help='seconds for the assignment to close')
    p.add_argument('--approvals', type=int, default=6, help='most Approve presses (each is counted)')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    a.expect_sha = a.expect_sha or command_walk.adopt_walk.latest_build_commit()
    if not a.expect_sha:
        p.error('--expect-sha is needed: the nightly recorded no release to default to')
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = ReviewWalk(a)
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
