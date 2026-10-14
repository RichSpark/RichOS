#!/usr/bin/env python3
"""run-walk.py --wait reaches the guest-slot admission; the default still refuses at once;
a command that cannot run, or the retired hold-walk.py, is refused before any slot is asked for.

No guest, no lock, no CPU sample: `guest_slot` is replaced by a stub that records what it
was asked for and refuses, so main() stops before anything is booted."""
import contextlib
import importlib.util
import io
import os
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('run_walk', HERE / 'run-walk.py')
run_walk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(run_walk)
failures = []


def check(name, cond, detail=''):
    print(('  ok    ' if cond else '  FAIL  ') + name + ('' if cond else '\n        ' + str(detail)))
    if not cond:
        failures.append(name)


asked = []


@contextlib.contextmanager
def stub_slot(**kw):
    asked.append(kw)
    raise BlockingIOError('stub: refused before anything boots')
    yield  # pragma: no cover


run_walk.guest_slot = stub_slot


def main_with(*extra, command=('true',), bundle=('--bundle', 'b.zip')):
    with tempfile.TemporaryDirectory() as t:
        sys.argv = ['run-walk.py', *bundle, '--home', t, '--engine', 'e.tar.gz',
                    '--report', str(Path(t) / 'r.json'), *extra, '--', *command]
        err = io.StringIO()
        try:
            with contextlib.redirect_stderr(err):
                run_walk.main()
        except BlockingIOError:
            return 'refused', err.getvalue()
        except SystemExit as exc:
            return 'exit %s' % exc.code, err.getvalue()
    return 'ran', ''


os.environ['TESTVM_ROOT'] = '/nonexistent/testvm-root'
asked.clear()
outcome, _ = main_with()
check('without --wait the slot is asked for no wait (refuse at once, as before)',
      outcome == 'refused' and asked and asked[-1].get('wait_seconds', 0) == 0, asked)
asked.clear()
outcome, _ = main_with('--wait', '900')
check('--wait 900 reaches the guest-slot admission as wait_seconds=900',
      outcome == 'refused' and asked and asked[-1].get('wait_seconds') == 900, asked)
check('...under the TESTVM_ROOT the walk runs in', asked and str(asked[-1].get('root')) == '/nonexistent/testvm-root',
      asked)
asked.clear()
outcome, said = main_with(command=(str(HERE / 'hold-walk.py'), '--out', '/tmp/x'))
check('hold-walk.py is refused before a slot is asked for, and the refusal says what to do instead',
      outcome == 'exit 2' and not asked and 'retired' in said and 'script' in said, (outcome, asked, said))
asked.clear()
outcome, said = main_with(command=('/nonexistent/walk.py',))
check('a command that cannot run is refused before a slot is asked for',
      outcome == 'exit 2' and not asked and 'not an executable file' in said, (outcome, asked, said))
asked.clear()
outcome, said = main_with('--no-app')
check('--no-app beside --bundle is refused before a slot is asked for',
      outcome == 'exit 2' and not asked and 'exactly one' in said, (outcome, asked, said))
asked.clear()
outcome, said = main_with(bundle=())
check('neither --bundle nor --no-app is refused before a slot is asked for',
      outcome == 'exit 2' and not asked and 'exactly one' in said, (outcome, asked, said))
asked.clear()
outcome, _ = main_with('--no-app', bundle=())
check('--no-app alone reaches the guest-slot admission (run-suite.sh boots a guest this way)',
      outcome == 'refused' and asked, (outcome, asked))
noapp = run_walk.boot_argv('vm1', None, 'home', '', False)
check('with no bundle, run.sh is asked for --no-app, never a --bundle, and no tailnet join',
      '--no-app' in noapp and '--bundle' not in noapp and '--no-tailnet' in noapp, noapp)

# The tailnet join is the phone path's alone: run.sh exits 1 when a join fails, so a walk
# that never touches the phone must not ask for one (audit 2026-09-29 part 2, section 09).
asked.clear()
plain = run_walk.boot_argv('vm1', 'b.zip', 'home', 'e.tar.gz', False)
check('a walk that does not pair a phone boots with --no-tailnet', '--no-tailnet' in plain, plain)
phone = run_walk.boot_argv('vm1', 'b.zip', 'home', 'e.tar.gz', True)
check('--tailnet keeps the join for a walk that does pair a phone', '--no-tailnet' not in phone, phone)
check('a walk boots with the guest sound device by default (no --no-audio)', '--no-audio' not in plain, plain)
quiet = run_walk.boot_argv('vm1', None, 'home', '', False, False)
check('audio=False asks run.sh for --no-audio: a call walk never reaches the host microphone or speakers',
      '--no-audio' in quiet and '--no-app' in quiet, quiet)
outcome, said = main_with(command=(str(HERE / 'delta-walk.py'), '--out', '/tmp/x'))
check('delta-walk.py (the phone walk) without --tailnet is refused before a slot is asked for',
      outcome == 'exit 2' and not asked and '--tailnet' in said, (outcome, asked, said))
outcome, _ = main_with('--tailnet', command=(str(HERE / 'delta-walk.py'), '--out', '/tmp/x'))
check('delta-walk.py with --tailnet reaches the slot admission', outcome == 'refused' and asked, (outcome, asked))

print(f'run-walk-wait.test.py: {len(failures)} failed')
sys.exit(1 if failures else 0)
