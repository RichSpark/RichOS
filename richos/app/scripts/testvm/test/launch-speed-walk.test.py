#!/usr/bin/env python3
"""launch-speed-walk.py's arithmetic, without a guest: the milliseconds read off log lines, the
p95 verdict against the CEO's 500 ms, the sentence the nightly prints, the start-up checks,
and the receipt it writes for the updated route, whose tree digest must be the updater's own."""
import importlib.util
import os
import shlex
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('launch_speed_walk', HERE.parent / 'launch-speed-walk.py')
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


# The system log's compact lines are the guest's local time; +0100 is an hour ahead of UTC.
log = ('2026-11-14 10:00:00.250 Df richos-tauri[501:1a] [com.apple.AppKit:Window] order window front conditionally\n'
       '2026-11-14 10:00:00.400 Df richos-tauri[501:1a] [com.apple.WebKit:Loading] didCommitLoadForFrame\n'
       '2026-11-14 10:00:00.600 Df richos-tauri[501:1a] [com.apple.WebKit:Loading] didFinishDocumentLoadForFrame\n'
       '2026-11-14 10:00:00.300 Df Dictionary[777:2b] [com.apple.AppKit:Window] order window front conditionally\n')
events = walk.syslog_events(log, walk.utc_offset('+0100'))
t_open = events[0][0] - 0.250  # the app's `open` started 250 ms before its window
check('the guest offset is read as seconds', walk.utc_offset('+0100') == 3600 and walk.utc_offset('-0700') == -25200)

started_ms = int((t_open + 0.080) * 1000)
line = ('[richos] launch timing: update step 15 ms, window 160 ms, first frame 165 ms, start-up finished 250 ms, '
        'page took over 330 ms after the process started; splash shown; update put in place: no; bundle check none '
        '(trusted as written at its update), 1 ms; engine check none (trusted as verified when it was written), 3 ms; '
        'process started at %d' % started_ms)
row = walk.measure({'label': 'ordinary:judged', 'pid': 501, 't_open': t_open, 'lines': [{'t': 0, 'text': line}]}, events)
check('AppKit and WebKit milestones in ms after open', (row.get('window'), row.get('page_committed'),
      row.get('document_loaded')) == (250, 400, 600), row)
A = walk.ALLOWANCE_MS
check("the judged moment is the page's splash playing, plus the allowance",
      row.get('splash') == 410 + A and row.get('splash_from') == "the page's splash playing"
      and row.get('first_frame') == 245, row)
check('the launch line says which checks ran', row.get('bundle_check', '').startswith('bundle check none (trusted')
      and row.get('engine_check', '').startswith('none (trusted'), row)
# With "its opening used", the judged moment is the first frame (Sage's plan 2026-11-14, section 6).
opened_line = line.replace('; splash shown;', '; splash shown, round-11/v2 (picked at the last quit), its opening used '
                           '(30 later pictures), the page joined it at 165 ms;')
opened = walk.measure({'label': 'rule-switch:judged', 'pid': 501, 't_open': t_open, 'expect_splash': 'round-11/v2',
                       'lines': [{'t': 0, 'text': opened_line}]}, events)
check("with its opening used, the judged moment is the opening's first picture on the glass, plus the allowance",
      opened.get('splash') == 245 + A and opened.get('splash_from') == "the splash's opening on the glass"
      and opened.get('splash_id') == 'round-11/v2' and opened.get('opening') == 30 and opened.get('joined_ms') == 165, opened)
native_line = line.replace('; splash shown;', '; splash shown, round-11/v2 (native pack);')
native_anchor = '[richos] native splash: frame zero scheduled 188 ms after process start'
native_run = {'label': 'ordinary:judged', 'pid': 501, 't_open': t_open,
              'lines': [{'t': 0, 'text': native_anchor}, {'t': 0, 'text': native_line}]}
native = walk.measure(native_run, events)
check('a native launch is timed from frame zero rather than the window or WebView',
      native.get('splash') == 268 + A and native.get('native_frame_zero') == 268, native)
no_anchor = walk.measure({**native_run, 'lines': native_run['lines'][1:]}, events)
check('a native candidate without a clock report supplies no timing verdict', 'splash' not in no_anchor, no_anchor)
fell_back_line = line.replace('; splash shown;', '; splash shown, round-11/v1 (picked by the rules at this start), its '
                              'opening not used: nothing was saved at the last quit (the first start after an install '
                              'or an update);')
fell_back = walk.measure({'label': 'fresh:first', 'pid': 501, 't_open': t_open, 'lines': [{'t': 0, 'text': fell_back_line}]}, events)
check("without its opening, the page's splash is the judged moment, and the launch line says why",
      fell_back.get('splash') == 410 + A and fell_back.get('opening') is None
      and fell_back.get('opening_not_used', '').startswith('nothing was saved at the last quit'), fell_back)
wrong = walk.measure({'label': 'rule-switch:judged', 'pid': 501, 't_open': t_open, 'expect_splash': 'round-11/v1',
                      'lines': [{'t': 0, 'text': opened_line}]}, events)
check('the wrong splash is not the splash: no time is judged for it', 'splash' not in wrong
      and wrong.get('wrong_splash') == 'expected round-11/v1, the launch line says round-11/v2', wrong)
older_build = walk.measure({'label': 'ordinary:judged', 'pid': 501, 't_open': t_open, 'expect_splash': 'round-11/v1',
                            'lines': []}, events)
check('a build older than the launch line cannot name its splash, and is timed rather than refused for it',
      'wrong_splash' not in older_build and older_build.get('splash') == 600 + A, older_build)
wrong_verdict = walk.verdict([{'label': 'ordinary:judged', 'splash': 300} for _ in range(20)] + [wrong])
check('a launch that showed the wrong splash refuses the measurement, by name', wrong_verdict['refused'] is True
      and wrong_verdict['missing'] == ['rule-switch:judged (expected round-11/v1, the launch line says round-11/v2)'],
      wrong_verdict)
per_splash = walk.verdict([{'label': 'ordinary:judged', 'splash': 300, 'splash_id': 'round-11/v1'} for _ in range(20)]
                          + [{'label': 'rule-switch:judged', 'splash': 320, 'splash_id': 'round-11/v2'} for _ in range(20)])
check("every splash's own p95 is stated", per_splash['splashes'] == {
    'round-11/v1': {'p95_ms': 300, 'median_ms': 300, 'launches': 20},
    'round-11/v2': {'p95_ms': 320, 'median_ms': 320, 'launches': 20}} and 'round-11/v2 p95 320 ms (20 launches)'
      in walk.sentence(per_splash), per_splash)
check('the rule the walk expects is the shell\'s: start 2 shows #2, every other start #1',
      walk.SPLASH_FOR_START == {2: 'round-11/v2'} and walk.SPLASH_OTHERWISE == 'round-11/v1')

old = walk.measure({'label': 'ordinary:judged', 'pid': 501, 't_open': t_open, 'lines': []}, events)
check("an older build with no launch line falls back to WebKit's document loaded",
      old.get('splash') == 600 + A and old.get('splash_from') == "WebKit's document loaded", old)

check("the limit is the CEO's number", walk.LIMIT_MS == 500)
check('p95 by nearest rank: the 19th of 20, the slowest of fewer',
      walk.p95(list(range(1, 21))) == 19 and walk.p95([5, 1, 9]) == 9 and walk.p95([7]) == 7)
rows = [{'label': 'ordinary:judged', 'splash': ms} for ms in range(300, 320)] + \
       [{'label': 'cold:judged', 'splash': ms} for ms in (350, 360, 370)] + \
       [{'label': 'ordinary:first', 'splash': 1200}]
summary = walk.verdict(rows)
check('the p95 of every judged launch together, within 500 ms, passes',
      summary['p95_ms'] == 360 and summary['launches'] == 23 and summary['refused'] is False, summary)
check("each route's own p95 and median are stated",
      summary['routes']['cold'] == {'p95_ms': 370, 'median_ms': 360, 'max_ms': 370, 'launches': 3, 'over': 0}, summary)
check('first launches are reported, not judged', summary['reported'] == {'ordinary:first': 1200}, summary)
# Each judged route on its own (review rv-20261114T143754Z-7a96691c-3106, finding 2): a slow route
# refuses even when the pooled p95 is fast.
hidden = walk.verdict([{'label': 'ordinary:judged', 'splash': 300} for _ in range(40)]
                      + [{'label': 'updated-from-applications:judged', 'splash': ms} for ms in [300] * 18 + [700, 700]])
check('one slow route refuses, however fast the rest pooled', hidden['p95_ms'] == 300 and hidden['refused'] is True
      and hidden['routes_over'] == ['updated-from-applications'] and 'OVER THE LIMIT ON ITS OWN' in walk.sentence(hidden),
      hidden)
# A build that writes the launch line and did not report its splash is missing evidence, never
# timed by WebKit's document loaded (finding 3).
unreported = line.replace('page took over 330 ms', 'page took over: not reported (the ceiling ended the wait)')
silent = walk.measure({'label': 'ordinary:judged', 'pid': 501, 't_open': t_open, 'lines': [{'t': 0, 'text': unreported}]}, events)
check('a current build whose page never reported has no timing', 'splash' not in silent and 'document_loaded' in silent, silent)
slow = walk.verdict([{'label': 'ordinary:judged', 'splash': ms} for ms in [300] * 18 + [501, 502]])
check('a p95 over 500 ms refuses, however good the median', slow['p95_ms'] == 501 and slow['refused'] is True, slow)
check('nothing measured refuses', walk.verdict([])['refused'] is True)
said = walk.sentence(summary)
check('the sentence states the p95 against 500 ms and every route',
      'p95 of 500 ms' in said and 'p95 360 ms over 23 launches' in said and 'cold p95 370 ms' in said
      and 'Dictionary' not in said, said)

# Missing evidence never passes (review rv-20261114T115101Z-10e7c6ac-7689, finding 1).
fast = [{'label': 'ordinary:judged', 'splash': 300} for _ in range(20)]
untimed = walk.verdict(fast + [{'label': 'ordinary:judged'}])
check('a judged launch with no timing refuses', untimed['refused'] is True and untimed['missing'] == ['ordinary:judged'],
      untimed)
gone = walk.verdict(fast, {'ordinary': 20, 'updated-from-applications': 20})
check('a planned route with no launches refuses, however fast the rest', gone['refused'] is True
      and gone['missing'].count('updated-from-applications:not launched') == 20, gone)
# FIVE STARTS, TWENTY ONLY WHEN ONE FAILS (the CEO, 2026-11-15: "Reduce to 5. Only if one of the 5
# is outside what's required, only then do you need to extend to 20. Otherwise i.e. if the first 5
# are fine, it's a PASS.").
check('a route runs five starts first, and is taken to twenty', (walk.FIRST_STARTS, walk.FULL_STARTS) == (5, 20))
five = walk.verdict(fast[:5], {'ordinary': 5}, 506)
check('five starts, every one within the limit, pass', five['refused'] is False and five['routes_over'] == [], five)
one_of_five = walk.verdict(fast[:4] + [{'label': 'ordinary:judged', 'splash': 900}], {'ordinary': 5}, 506)
check('five starts with one over, never taken on to twenty, refuse', one_of_five['refused'] is True
      and one_of_five['routes_over'] == ['ordinary'] and one_of_five['routes']['ordinary']['over'] == 1, one_of_five)
one_of_twenty = walk.verdict(fast[:19] + [{'label': 'ordinary:judged', 'splash': 900}], {'ordinary': 20}, 506)
check('taken to twenty with one over, its p95 within the limit, it passes', one_of_twenty['refused'] is False
      and one_of_twenty['routes']['ordinary']['p95_ms'] == 300, one_of_twenty)
two_of_twenty = walk.verdict(fast[:18] + [{'label': 'ordinary:judged', 'splash': 900}] * 2, {'ordinary': 20}, 506)
check('taken to twenty with its p95 over the limit, it refuses', two_of_twenty['refused'] is True
      and two_of_twenty['routes_over'] == ['ordinary'], two_of_twenty)
check('only a route with a start over ITS limit is taken on, and a start without a timing takes nothing on',
      walk.needing_more([{'label': 'ordinary:judged', 'splash': 900}, {'label': 'cold:judged', 'splash': 3900},
                         {'label': 'day-change:judged'}, {'label': 'ordinary:first', 'splash': 5000},
                         {'label': 'rule-switch:judged', 'splash': 506}],
                        506, {'cold': {'limit_ms': 3920}}) == ['ordinary'])
whole = walk.verdict(fast, {'ordinary': 20})
check('a complete fast measurement passes', whole['refused'] is False and whole['missing'] == [], whole)
check('the sentence says the measurement was incomplete', 'INCOMPLETE' in walk.sentence(gone), walk.sentence(gone))

# THE GUEST IS SLOWER THAN HIS MAC, SO THE LIMIT IS SCALED, BY A FIXED RATIO, AND DICTIONARY IS
# NEVER LAUNCHED (the CEO, 2026-11-14: "if the VM is slower, then the measurements need to be
# adjusted accordingly", and "Stop all that retarded "launch-speed walk beside Dictionary"
# fuckshit!"; 2026-11-15: "What the fuck did I say about that????"). The ratios are the Dictionary
# measurements on record (the launch-speed record of run 20261115T061911Z-109b9887.1),
# like for like, by medians.
check("his Mac's Dictionary median and Dock part are the audit's measurements",
      (walk.MAC_DICTIONARY_MEDIAN_MS, walk.DOCK_PART_MS) == (318, 43.5))
check('the warm ratio is the record\'s: (median 278.5 ms of its 10 guest launches + 43.5) / 318, limit 506 ms',
      len(walk.GUEST_DICTIONARY_WARM_MS) == 10 and walk.WARM_INPUTS['guest_median_from_request_ms'] == 278.5
      and walk.WARM_RATIO == 1.013 and walk.WARM_LIMIT_MS == 506, (walk.WARM_RATIO, walk.WARM_LIMIT_MS))
check('the cold ratio is the record\'s: (median 2449.5 ms of its 10 launches after a purge + 43.5) / 318, limit 3920 ms',
      len(walk.GUEST_DICTIONARY_COLD_MS) == 10 and walk.COLD_INPUTS['guest_median_from_request_ms'] == 2449.5
      and walk.COLD_RATIO == 7.84 and walk.COLD_LIMIT_MS == 3920, (walk.COLD_RATIO, walk.COLD_LIMIT_MS))
check('each limit is the one publication re-derives from the recorded ratio',
      walk.WARM_LIMIT_MS == round(500 * walk.WARM_RATIO) and walk.COLD_LIMIT_MS == round(500 * walk.COLD_RATIO))
own = {'cold': {'limit_ms': walk.COLD_LIMIT_MS, 'ratio': walk.COLD_RATIO, 'inputs': walk.COLD_INPUTS}}
cold_rows = [{'label': 'ordinary:judged', 'splash': 300} for _ in range(20)] + \
            [{'label': 'cold:judged', 'splash': 1900} for _ in range(20)]
judged_own = walk.verdict(cold_rows, {'ordinary': 20, 'cold': 20}, walk.WARM_LIMIT_MS, walk.WARM_RATIO,
                          walk.WARM_INPUTS, own)
check('a cold route within its own cold limit passes, and is not pooled with the warm routes',
      judged_own['refused'] is False and judged_own['p95_ms'] == 300 and judged_own['launches'] == 20, judged_own)
slow_cold = walk.verdict(cold_rows[:20] + [{'label': 'cold:judged', 'splash': 9000} for _ in range(20)],
                         {'ordinary': 20, 'cold': 20}, walk.WARM_LIMIT_MS, walk.WARM_RATIO, walk.WARM_INPUTS, own)
check('a cold route over its own limit refuses', slow_cold['refused'] is True and slow_cold['routes_over'] == ['cold'],
      slow_cold)
said_scaled = walk.sentence(judged_own)
check('the sentence names both fixed limits and their ratios, and no Dictionary launch',
      'p95 of 506 ms' in said_scaled and 'x 1.013' in said_scaled and 'cold judged on its own against 3920 ms (500 x 7.84' in said_scaled
      and 'Dictionary' not in said_scaled, said_scaled)


# THE WALK ITSELF, AGAINST A FAKE GUEST: main() runs as the nightly runs it, every guest command and
# every push and pull answered here, and every launch given the time `time_for(label, n)` says
# (from the launch line, as a real build writes it). What the walk asked the guest to launch is
# read back from the plans it wrote.
def walk_in_fake_guest(time_for, extra=(), install_path_migrates=True):
    import contextlib
    import io
    import json
    import tarfile
    import types
    probes, counter = {}, [0]
    with tempfile.TemporaryDirectory() as root:
        root = Path(root)
        vm, payload = 'fake-vm', '/Users/admin/testvm/fake-vm'
        (root / 'run' / vm).mkdir(parents=True)
        (root / 'run' / vm / 'payload').write_text(payload + '\n')
        out = root / 'out'

        def probe(plan_file):
            name = Path(plan_file).stem
            results = []
            for step in json.loads((out / (name + '.json')).read_text()):
                counter[0] += 1
                n = counter[0]
                t_open = 1000.0 + 10 * n
                ms = time_for(step['label'], n)
                splash = step.get('expect_splash') or 'round-11/v1'
                text = ('[richos] launch timing: update step 15 ms, window 100 ms, first frame %d ms, page took over '
                        '%d ms after the process started; splash shown, %s (picked at the last quit), its opening used '
                        '(30 later pictures), the page joined it at 500 ms; update put in place: no; bundle check none '
                        '(trusted), 1 ms; engine check none (trusted), 3 ms; process started at %d'
                        % (ms - walk.ALLOWANCE_MS, ms + 400, splash, int(t_open * 1000)))
                if step['label'] == 'install-path-legacy':
                    text += ', after a hand-over from an older copy'
                results.append({'n': n, 'label': step['label'], 'app': step['app'], 't_open': t_open, 'pid': 5000 + n,
                                'quit': 'quit from its own menu', 'all_gone': True, 'events': [],
                                'lines': [{'t': t_open, 'text': text}]})
            probes[name] = results

        def guest(_vm, command, timeout=30):
            if 'launch-speed-guest.py --plan' in command:
                probe(shlex.split(command)[shlex.split(command).index('--plan') + 1])
                return 'probe exit 0'
            if 'echo migrated' in command:
                return 'migrated' if install_path_migrates else ''
            for needle, answer in (('PlistBuddy', '1.3.3'), ('ls -d', payload + '/launch-speed/candidate/RichOS.app'),
                                   ('sw_vers', '15.1'), ('hw.ncpu', '4'), ('date +%z', '+0000'),
                                   ('print(len(json.load', '0'), ('receipt_tree_sha256', 'f' * 64),
                                   ('pwd -P', '/Users/admin')):
                if needle in command:
                    return answer
            return ''

        def run(argv, **kwargs):
            if len(argv) > 2 and argv[2] == '--pull':
                if argv[3].endswith('/probe.json'):
                    plan = Path(argv[3]).parent.name
                    Path(argv[4]).write_text(json.dumps({'results': probes[plan]}))
                else:
                    with tarfile.open(argv[4], 'w:gz') as t:
                        for name, body in [('launch-speed/guest-system.log', b'')] + [
                                ('launch-speed/%s/probe.json' % n, json.dumps({'results': r}).encode()) for n, r in probes.items()]:
                            info = tarfile.TarInfo(name)
                            info.size = len(body)
                            t.addfile(info, io.BytesIO(body))
            return types.SimpleNamespace(returncode=0, stdout='', stderr='')
        saved = (sys.modules.get('relaunch'), walk.subprocess.run, sys.argv, os.environ.get('TESTVM_ROOT'))
        sys.modules['relaunch'] = types.SimpleNamespace(guest=guest)
        walk.subprocess.run, os.environ['TESTVM_ROOT'] = run, str(root)
        sys.argv = ['launch-speed-walk.py', vm, '--candidate', str(root / 'RichOS-1.3.3.zip'), '--out', str(out), *extra]
        try:
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                code = walk.main()
        finally:
            if saved[0] is None:
                sys.modules.pop('relaunch', None)
            else:
                sys.modules['relaunch'] = saved[0]
            walk.subprocess.run, sys.argv = saved[1], saved[2]
            if saved[3] is None:
                os.environ.pop('TESTVM_ROOT', None)
            else:
                os.environ['TESTVM_ROOT'] = saved[3]
        plans = {p.stem: json.loads(p.read_text()) for p in out.glob('*.json') if p.name != 'report.json'}
        report = json.loads((out / 'report.json').read_text())
    return code, report, plans


fast_code, fast_report, fast_plans = walk_in_fake_guest(lambda label, n: 300)
launched = [step for steps in fast_plans.values() for step in steps]
check('the walk launches no Dictionary, in any plan: only the app under test',
      launched and not [s for s in launched if 'Dictionary' in s['app'] or s['label'].startswith('dictionary')]
      and 'dictionary' not in fast_plans, sorted({s['app'] for s in launched}))
check('its limits are the fixed ones on record, recorded beside the verdict',
      fast_report['summary']['limit_ms'] == 506 and fast_report['summary']['guest_ratio'] == 1.013
      and fast_report['summary']['route_limits']['cold']['limit_ms'] == 3920
      and fast_report['summary']['route_limits']['cold']['ratio'] == 7.84, fast_report['summary'])
check('a walk whose every start is within the limits passes', fast_code == 0 and fast_report['summary']['refused'] is False,
      (fast_code, fast_report['sentence']))


def judged_in(plans, route, plan=None):
    return [s for name, steps in plans.items() if plan in (None, name) for s in steps if s['label'] == route + ':judged']


ROUTES = ('ordinary', 'cold', 'rule-switch', 'day-change', 'after-update')
PREVIOUS = ('--previous-app', '/Users/admin/testvm/fake-vm/previous/RichOS.app')
fast_code, fast_report, fast_plans = walk_in_fake_guest(lambda label, n: 300, PREVIOUS)
check('the installed path is qualified independently of the splash timing routes',
      fast_report['checks'][-1]['pass'] is True
      and fast_plans['install-path-direct'][0]['owns'] == ['/Applications/RichOS.app']
      and fast_plans['install-path-direct'][0]['env']['HOME'] == '/Users/admin')
split_code, split_report, _ = walk_in_fake_guest(lambda label, n: 300, PREVIOUS, install_path_migrates=False)
check('a stale Applications copy fails qualification even when all the timings pass',
      split_code == 1 and split_report['summary']['refused'] is False
      and split_report['checks'][-1]['pass'] is False)
check('every route, the rule switch too, runs five starts, and none is taken on when all five are within',
      {r: len(judged_in(fast_plans, r)) for r in ROUTES} == dict.fromkeys(ROUTES, 5)
      and not [n for n in fast_plans if n.startswith('more-')] and fast_code == 0
      and fast_report['facts']['taken_to_full'] == [], (sorted(fast_plans), fast_report['sentence']))
switch = judged_in(fast_plans, 'rule-switch')
check("the rule switch's five: start 2 and start 3 of fresh homes, each home removed after its last start",
      [s['expect_splash'] for s in switch] == ['round-11/v2', 'round-11/v1'] * 2 + ['round-11/v2']
      and len({s['env']['HOME'] for s in switch}) == 3
      and [bool(s.get('after')) for s in switch] == [False, True, False, True, True], switch)


def slow_at(route, which, ms=900):
    """The `which`-th judged starts (1-based) of `route` take `ms`; every other start 300 ms."""
    seen = [0]

    def time_for(label, n):
        if label == route + ':judged':
            seen[0] += 1
            if seen[0] in which:
                return ms
        return 300
    return time_for


for route in ROUTES:
    code, report, plans = walk_in_fake_guest(slow_at(route, {3}, 9000 if route == 'cold' else 900), PREVIOUS)
    counts = {r: len(judged_in(plans, r)) for r in ROUTES}
    check("one of %s's five over its limit takes that route, and only it, on to twenty, judged by its p95 (one over: "
          "passes)" % route, counts == {**dict.fromkeys(ROUTES, 5), route: 20}
          and len(judged_in(plans, route, 'more-' + route)) == 15 and report['facts']['taken_to_full'] == [route]
          and report['summary']['routes'][route]['launches'] == 20 and report['summary']['refused'] is False
          and code == 0, (counts, sorted(plans), report['sentence']))
code, report, plans = walk_in_fake_guest(slow_at('ordinary', {2, 4}), PREVIOUS)
check('two of twenty over the limit: the p95 is over, and the walk refuses', code == 1
      and report['summary']['routes_over'] == ['ordinary'] and report['summary']['routes']['ordinary']['launches'] == 20,
      report['sentence'])
code, report, plans = walk_in_fake_guest(slow_at('cold', {1}, 2900), PREVIOUS)
check('a cold start over the warm limit and within its own is not taken on', not judged_in(plans, 'cold', 'more-cold')
      and code == 0, report['sentence'])
_, _, plans = walk_in_fake_guest(slow_at('day-change', {1}), PREVIOUS)
more_days = judged_in(plans, 'day-change', 'more-day-change')
check('the day-change route goes on in the same home, no set-up again, its two openings still in turn',
      not [s for s in plans['more-day-change'] if s['label'] == 'day-change:prep']
      and [s['expect_splash'] for s in judged_in(plans, 'day-change')]
      == ['opening/midnight-suede', 'opening/ruled-standard'] * 10
      and {s['env']['HOME'] for s in more_days} == {s['env']['HOME'] for s in judged_in(plans, 'day-change', 'daily')},
      more_days[:2])
_, _, plans = walk_in_fake_guest(slow_at('rule-switch', {1}), PREVIOUS)
more_switch = judged_in(plans, 'rule-switch', 'more-rule-switch')
check('the rule switch goes on in new fresh homes, each one\'s start 2 and start 3 (the fifth was a home\'s start 2)',
      [s['expect_splash'] for s in judged_in(plans, 'rule-switch')]
      == ['round-11/v2', 'round-11/v1'] * 2 + ['round-11/v2'] + ['round-11/v2', 'round-11/v1'] * 7 + ['round-11/v2']
      and not {s['env']['HOME'] for s in more_switch} & {s['env']['HOME'] for s in judged_in(plans, 'rule-switch', 'switch')},
      more_switch[:2])
# The receipt's tree digest, on the tree whose digest the updater's own test pins
# (crates/richos-user-update/src/lib.rs, digests_are_spelled_as_sha2_0_10_spelled_them).
with tempfile.TemporaryDirectory() as d:
    os.mkdir(os.path.join(d, 'a'))
    os.chmod(os.path.join(d, 'a'), 0o755)
    for name, body, mode in (('a/x', b'hello', 0o644), ('b', b'world!', 0o755)):
        with open(os.path.join(d, name), 'wb') as f:
            f.write(body)
        os.chmod(os.path.join(d, name), mode)
    got = walk.receipt_tree_sha256(d)
check("the receipt's tree digest is the updater's", got == '6e96a12b30eb6d6763477bfec950ae61bc1d0a701a9421d529986c7a80ca58fa', got)

results = [
    {'label': 'checks:failing-step', 'events': [{'what': 'alert frame'}, {'what': 'exited after the alert'}],
     'lines': [{'text': '[richos] application startup: panicked at src/main.rs:1:1 — open ledger'}]},
    {'label': 'checks:quit-during-startup', 'events': [{'what': 'quit pressed during start-up'}, {'what': 'gone after the quit'}],
     'lines': []},
    {'label': 'checks:after', 'events': [], 'lines': [{'text': '[richos] boot complete — every line above'}]},
]
checks = walk.check_results(results)
check('the three start-up checks pass on what they must show', [c['pass'] for c in checks] == [True, True, True], checks)
stuck = walk.check_results([{'label': 'checks:failing-step', 'events': [{'what': 'alert frame'},
                             {'what': 'STILL RUNNING after the alert'}], 'lines': []}])
check('an app still running after its alert fails the check', stuck[0]['pass'] is False, stuck)

# THE DAY-CHANGE ROUTE (round 74, "one of mine, a different one each day"): the first start of a
# new day is judged like every other, on the opening the day before's quit drew for it.
steps = walk.day_change_plan(lambda label, home, owns, **extra: dict(label=label, home=home, **extra),
                             '/h', ['/Applications/RichOS.app'], 4, '/h/config.json')
judged = [s for s in steps if s['label'] == 'day-change:judged']
check('the day-change route: two starts to set it up, then every judged start a day after the last',
      [s['label'] for s in steps[:2]] == ['day-change:prep'] * 2 and 'before' not in steps[0]
      and '"opening_pick"]="daily"' in steps[1]['before'] and len(judged) == 4
      and all('c["opening_daily"][0]-=1' in s['before'] and '/h/config.json' in s['before'] for s in judged), steps)
check('each judged start must show the other opening than the one before it',
      [s['expect_splash'] for s in judged] == ['opening/midnight-suede', 'opening/ruled-standard'] * 2, judged)
day = [{'label': 'day-change:judged', 'splash': 300, 'splash_id': 'opening/midnight-suede'} for _ in range(20)]
check('a day-change start that drew its splash in the page refuses the route on its own',
      walk.verdict(fast + day[:19] + [{'label': 'day-change:judged', 'splash': 1726}] * 2,
                   {'ordinary': 20, 'day-change': 20})['routes_over'] == ['day-change'])
check('a day-change route that was planned and not launched refuses',
      walk.verdict(fast, {'ordinary': 20, 'day-change': 20})['refused'] is True)
check('the day-change route within the limit passes with the rest',
      walk.verdict(fast + day, {'ordinary': 20, 'day-change': 20})['refused'] is False)

# The nightly's budget (nightly-local.py LAUNCH_SPEED_BUDGET) is made from these two counts: every
# route passing at five starts, and every route taken on to twenty.
usual = sum(len(steps) for steps in fast_plans.values())
_, _, worst_plans = walk_in_fake_guest(lambda label, n: 9000 if label.endswith(':judged') else 300, PREVIOUS)
worst = sum(len(steps) for steps in worst_plans.values())
check('including two install path launches, the walk starts 41 times at five and at most 139 at twenty',
      (usual, worst) == (41, 139), (usual, worst))

print('%d failure(s)' % len(failures))
sys.exit(1 if failures else 0)
