#!/usr/bin/env python3
"""openings-walk.py's arithmetic and refusals, without a guest: which opening "a different one
each day" expects on a day (the guest's LOCAL day, as the app counts it), that what the walk
calls his after 430 days is what the catalog's own rules give, and that an unknown step and a
missing VM state are refused before anything is driven."""
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
WALK = HERE.parent / 'openings-walk.py'
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


# The module reads the VM's state when a Walk is made, never at import.
spec = importlib.util.spec_from_file_location('openings_walk', WALK)
walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(walk)

DAY = 86400
his = walk.HIS
# 2026-11-14 12:00:00 UTC is day 20,406 since 1970-01-01.
noon = 20406 * DAY + 12 * 3600
check('the day is counted in the guest\'s own time: noon UTC is the same day in UTC',
      walk.daily_opening(noon, '+0000', his) == (20406, his[20406 % len(his)]))
# 23:30 in US Pacific (UTC-8) is 07:30 the next day in UTC: still the Pacific day.
late = 20407 * DAY + 7 * 3600 + 1800
check('late evening in US Pacific is still that day there, though UTC is on the next',
      walk.daily_opening(late, '-0800', his)[0] == 20406 and walk.daily_opening(late, '+0000', his)[0] == 20407)
check('a half-hour zone is read (+0530)', walk.daily_opening(20406 * DAY + 19 * 3600, '+0530', his)[0] == 20407)
run = [walk.daily_opening(noon + d * DAY, '+0000', his)[1] for d in range(len(his) + 1)]
check('six days in a row are his six openings, each once, in the catalog\'s order, then the first again',
      sorted(run[:-1]) == sorted(his) and run[-1] == run[0] and run[1] == his[(20406 + 1) % len(his)], run)

# What 430 days earn, by the catalog's own rows: every "start" opening and every "months" rule of
# 14 or fewer, and nothing that needs money, jobs, hours, an update or a thousand starts.
source = (HERE.parents[2] / 'ui' / 'openings-catalog.js').read_text()
catalog = json.loads(re.sub(r';\s*$', '', source[source.index('=', source.index('window.RichOpeningsCatalog')) + 1:].strip()))
by_rule = [o['id'] for o in catalog['openings'] if o['earn'].get('start') or 0 < o['earn'].get('months', 0) <= 14]
check('the walk\'s six are what the catalog gives a user 430 days in, in the catalog\'s order', by_rule == his, (by_rule, his))
names = {o['id']: o['name'] for o in catalog['openings']}
check('and it calls them what the catalog calls them', all(walk.NAMES[o] == names[o] for o in his))
others = [o['name'] for o in catalog['openings'] if o['id'] not in his]
check('every name it expects NOT to see belongs to an opening that is not his',
      all(any(n in other for other in others) for n in walk.NOT_HIS), walk.NOT_HIS)

with tempfile.TemporaryDirectory() as tmp:
    env = dict(os.environ, HOME=tmp, TESTVM_ROOT=tmp + '/testvm')
    r = subprocess.run([sys.executable, str(WALK), 'no-such-vm', '--out', tmp + '/out', '--steps', 'gallery,juggle'],
                       capture_output=True, text=True, env=env)
    check('an unknown step is refused by name, before a guest is touched', r.returncode == 2 and 'juggle' in r.stderr, r.stderr[-300:])
    r = subprocess.run([sys.executable, str(WALK), 'no-such-vm', '--out', tmp + '/out'], capture_output=True, text=True, env=env)
    check('a VM this run does not own (no recorded state) is not driven', r.returncode != 0 and 'PASS' not in r.stdout, r.stdout[-300:] + r.stderr[-300:])

print(('PASS' if not failures else 'FAIL') + ': openings-walk, ' + str(len(failures)) + ' failure(s)')
sys.exit(1 if failures else 0)
