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
# A GUEST HAS NO SOUND DEVICE UNLESS ITS CALLER ASKS FOR ONE BY NAME (the CEO, 2026-11-15: "I've
# heard the start sound well over 40 times now"): run.sh's default is no device, so a default boot
# asks for none, and --audio is the only way to one.
check('a walk boots with no guest sound device by default (no --audio)', '--audio' not in plain, plain)
loud = run_walk.boot_argv('vm1', None, 'home', '', False, audio=True)
check('audio=True, and only that, asks run.sh for --audio (which mutes it before anything launches)',
      '--audio' in loud and '--no-app' in loud, loud)


def booted_with(*extra, command=('true',)):
    """The run.sh command line main() would boot with: the slot is granted, and the boot itself
    is the stub's refusal, so nothing runs."""
    seen = []

    @contextlib.contextmanager
    def granted(**kw):
        yield Path('/nonexistent/slot')

    def popen(argv, **kw):
        seen.append(argv)
        raise BlockingIOError('stub: the boot is not run')
    saved = (run_walk.guest_slot, run_walk.subprocess.Popen)
    run_walk.guest_slot, run_walk.subprocess.Popen = granted, popen
    try:
        outcome, said = main_with(*extra, command=command)
    finally:
        run_walk.guest_slot, run_walk.subprocess.Popen = saved
    return outcome, said, (seen[0] if seen else None)


_, _, default_boot = booted_with()
check('main() boots a plain walk with no --audio', default_boot is not None and '--audio' not in default_boot,
      default_boot)
for name in sorted(run_walk.VOICE_WALKS):
    outcome, said = main_with(command=(str(HERE / name), '--out', '/tmp/x'))
    check(name + ' (it starts voice) without --audio is refused before a slot, and says --audio',
          outcome == 'exit 2' and '--audio' in said, (outcome, said))
    _, _, voice_boot = booted_with('--audio', command=(str(HERE / name), '--out', '/tmp/x'))
    check(name + ' with --audio boots run.sh --audio', voice_boot is not None and '--audio' in voice_boot, voice_boot)
for name in sorted(run_walk.CALL_WALKS):
    outcome, said = main_with('--audio', command=(str(HERE / name), '--out', '/tmp/x'))
    check(name + ' (a call on its own virtual devices) is refused --audio', outcome == 'exit 2' and 'never --audio' in said,
          (outcome, said))
    _, _, call_boot = booted_with('--no-audio', command=(str(HERE / name), '--out', '/tmp/x'))
    check(name + ' boots with no guest sound device', call_boot is not None and '--audio' not in call_boot, call_boot)
outcome, said = main_with('--audio', '--no-audio')
check('--audio beside --no-audio is refused', outcome == 'exit 2' and 'contradict' in said, (outcome, said))
outcome, said = main_with(command=(str(HERE / 'delta-walk.py'), '--out', '/tmp/x'))
check('delta-walk.py (the phone walk) without --tailnet is refused before a slot is asked for',
      outcome == 'exit 2' and not asked and '--tailnet' in said, (outcome, asked, said))
outcome, _ = main_with('--tailnet', command=(str(HERE / 'delta-walk.py'), '--out', '/tmp/x'))
check('delta-walk.py with --tailnet reaches the slot admission', outcome == 'refused' and asked, (outcome, asked))

print(f'run-walk-wait.test.py: {len(failures)} failed')
sys.exit(1 if failures else 0)
