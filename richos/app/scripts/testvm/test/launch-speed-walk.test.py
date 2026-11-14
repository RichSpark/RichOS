#!/usr/bin/env python3
"""launch-speed-walk.py's arithmetic, without a guest: the milliseconds read off log lines, the
p95 verdict against the CEO's 380 ms, the sentence the nightly prints, the start-up checks,
and the receipt it writes for the updated route, whose tree digest must be the updater's own."""
import importlib.util
import os
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
# THE SPLASH'S OPENING ON THE GLASS IS THE SPLASH PLAYING (Sage's plan 2026-11-14, "the splash the
# rules picked is playing within 380 ms of the click", section 6): with "its opening used", the judged
# moment is the first frame, not the page.
opened_line = line.replace('; splash shown;', '; splash shown, round-11/v2 (picked at the last quit), its opening used '
                           '(30 later pictures), the page joined it at 165 ms;')
opened = walk.measure({'label': 'rule-switch:judged', 'pid': 501, 't_open': t_open, 'expect_splash': 'round-11/v2',
                       'lines': [{'t': 0, 'text': opened_line}]}, events)
check("with its opening used, the judged moment is the opening's first picture on the glass, plus the allowance",
      opened.get('splash') == 245 + A and opened.get('splash_from') == "the splash's opening on the glass"
      and opened.get('splash_id') == 'round-11/v2' and opened.get('opening') == 30 and opened.get('joined_ms') == 165, opened)
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

check("the limit is the CEO's number", walk.LIMIT_MS == 380)
check('p95 by nearest rank: the 19th of 20, the slowest of fewer',
      walk.p95(list(range(1, 21))) == 19 and walk.p95([5, 1, 9]) == 9 and walk.p95([7]) == 7)
rows = [{'label': 'ordinary:judged', 'splash': ms} for ms in range(300, 320)] + \
       [{'label': 'cold:judged', 'splash': ms} for ms in (350, 360, 370)] + \
       [{'label': 'ordinary:first', 'splash': 1200}]
summary = walk.verdict(rows)
check('the p95 of every judged launch together, within 380 ms, passes',
      summary['p95_ms'] == 360 and summary['launches'] == 23 and summary['refused'] is False, summary)
check("each route's own p95 and median are stated",
      summary['routes']['cold'] == {'p95_ms': 370, 'median_ms': 360, 'max_ms': 370, 'launches': 3}, summary)
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
slow = walk.verdict([{'label': 'ordinary:judged', 'splash': ms} for ms in [300] * 18 + [390, 400]])
check('a p95 over 380 ms refuses, however good the median', slow['p95_ms'] == 390 and slow['refused'] is True, slow)
check('nothing measured refuses', walk.verdict([])['refused'] is True)
said = walk.sentence(summary)
check('the sentence states the p95 against 380 ms and every route',
      'p95 of 380 ms' in said and 'p95 360 ms over 23 launches' in said and 'cold p95 370 ms' in said
      and 'Dictionary' not in said, said)

# Missing evidence never passes (review rv-20261114T115101Z-10e7c6ac-7689, finding 1).
fast = [{'label': 'ordinary:judged', 'splash': 300} for _ in range(20)]
untimed = walk.verdict(fast + [{'label': 'ordinary:judged'}])
check('a judged launch with no timing refuses', untimed['refused'] is True and untimed['missing'] == ['ordinary:judged'],
      untimed)
gone = walk.verdict(fast, {'ordinary': 20, 'updated-from-applications': 20})
check('a planned route with no launches refuses, however fast the rest', gone['refused'] is True
      and gone['missing'].count('updated-from-applications:not launched') == 20, gone)
few = walk.verdict(fast[:19])
check('fewer than 20 judged launches refuse', few['refused'] is True and few['incomplete'] is True, few)
whole = walk.verdict(fast, {'ordinary': 20})
check('a complete fast measurement passes', whole['refused'] is False and whole['missing'] == [], whole)
check('the sentence says the measurement was incomplete', 'INCOMPLETE' in walk.sentence(gone), walk.sentence(gone))

# The guest is slower than his Mac: the limit is scaled by Dictionary's slowness there, LIKE FOR LIKE
# and BY MEDIANS (the CEO, 2026-11-14): its median from the launch request to its window in the guest,
# plus the Dock's part measured on his Mac, against its median from the Dock click on his Mac.
check("his Mac's Dictionary median and Dock part are the audit's measurements",
      (walk.MAC_DICTIONARY_MEDIAN_MS, walk.DOCK_PART_MS) == (318, 43.5))
# Ten guest launches, one of them a cold outlier at 1,900 ms: the median ignores it, a p95 would not.
windows = (500, 510, 520, 530, 540, 550, 560, 570, 580, 1900)
calib = [{'label': 'dictionary:calibration', 'window': ms, 'splash': ms + A} for ms in windows]
limit, ratio, inputs = walk.guest_limit(calib)
want = (545 + 43.5) / 318
check("the limit is 380 ms times (the guest's median from the request + the Dock part) over his Mac's median",
      ratio == round(want, 3) and limit == round(380 * round(want, 3)) and inputs['guest_median_from_request_ms'] == 545
      and inputs['guest_median_from_click_ms'] == 588.5 and inputs['mac_median_from_click_ms'] == 318, (limit, ratio, inputs))
check('a single cold outlier does not move the limit', walk.guest_limit(calib[:-1] + [dict(calib[-1], window=590)])[1]
      == ratio, walk.guest_limit(calib[:-1] + [dict(calib[-1], window=590)]))
check('no calibration keeps 380 ms', walk.guest_limit([]) == (380, None, None))
# THE COLD ROUTE AGAINST COLD DICTIONARY (the lead's decision on esc-20261114T164747Z-54632204):
# its own limit from Dictionary after the same purge, and it is not pooled with the warm routes.
cold_calib = [{'label': 'dictionary:cold', 'window': ms} for ms in (1500, 1600, 1700)]
cold_limit, cold_ratio, cold_inputs = walk.guest_limit(cold_calib, 'dictionary:cold')
check('the cold limit comes from cold Dictionary only', cold_inputs['guest_median_from_request_ms'] == 1600
      and cold_limit == round(380 * cold_ratio) and walk.guest_limit(cold_calib)[1] is None, (cold_limit, cold_ratio))
own = {'cold': {'limit_ms': cold_limit, 'ratio': cold_ratio, 'inputs': cold_inputs}}
cold_rows = [{'label': 'ordinary:judged', 'splash': 300} for _ in range(20)] + \
            [{'label': 'cold:judged', 'splash': 1900} for _ in range(20)]
judged_own = walk.verdict(cold_rows, {'ordinary': 20, 'cold': 20}, 380, None, None, own)
check('a cold route within its own cold limit passes, and is not pooled with the warm routes',
      judged_own['refused'] is False and judged_own['p95_ms'] == 300 and judged_own['launches'] == 20, judged_own)
slow_cold = walk.verdict(cold_rows[:20] + [{'label': 'cold:judged', 'splash': 9000} for _ in range(20)],
                         {'ordinary': 20, 'cold': 20}, 380, None, None, own)
check('a cold route over its own limit refuses', slow_cold['refused'] is True and slow_cold['routes_over'] == ['cold'],
      slow_cold)
check('the sentence names the cold route\'s own limit', 'cold judged on its own against %d ms' % cold_limit
      in walk.sentence(judged_own), walk.sentence(judged_own))
# Publication re-derives the limit from the recorded ratio as round(380 * ratio): the walk's own
# limit is that same number for every guest median (review rv-20261114T153837Z-b8423a44-96f8).
mismatched = [m for m in range(300, 900) if (lambda l, r, _i: l != round(380 * r))(
    *walk.guest_limit([{'label': 'dictionary:calibration', 'window': m}]))]
check('the limit is the one publication re-derives from the recorded ratio, for every median', mismatched == [], mismatched[:5])
check('a guest faster than his Mac never tightens the limit',
      walk.guest_limit([{'label': 'dictionary:calibration', 'window': 200}])[:2] == (380, 1.0))
scaled = walk.verdict(fast + calib, {'ordinary': 20}, limit, ratio, inputs)
check('the calibration launches are not judged, and the scaled limit is used',
      scaled['launches'] == 20 and scaled['limit_ms'] == limit and scaled['refused'] is False, scaled)
said_scaled = walk.sentence(scaled)
check('the sentence records the ratio and its inputs beside the limit', ('x %.2f' % ratio) in said_scaled
      and '588.5 ms from the Dock click here' in said_scaled and 'against 318 ms on his Mac' in said_scaled, said_scaled)

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

print('%d failure(s)' % len(failures))
sys.exit(1 if failures else 0)
