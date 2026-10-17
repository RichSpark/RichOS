#!/usr/bin/env python3
"""Run the operator back-end probes in a disposable test VM guest, never on the host.

Operator back-end spec r3 §5 (richos-hq docs/plans/2026-09-24-operator-back-end-spec-r3.md):
fifteen probes, each one short `claude` session with its control in the same run, run in
the test VM (CEO ruling §65, §68 point 5). This file is the host half: it takes one of
the test VM's two guest slots and the CPU admission (CEO ruling §77, the same
`slots.guest_slot` the walk runner uses, held for this run only and released when it
ends), boots ONE fresh clone with `testvm/run.sh --no-app` (the host's claude synced in,
the host's login pushed, no app launched), copies in this harness and a `git archive` of
the engine at a named commit, runs `guest_probes.py` inside the guest in the foreground,
pulls the results back, and deletes the clone however the run ends (§54).

  run-probes.py --out DIR [--only P1,P2,...] [--engine-rev REV] [--wait SECONDS] [--survey]
                [--walk-binary PATH] [--w3-cells W3,...] [--work-walk-binary BIN --runtime DIR]
                [--w4-cells baseline,P5,...]

W4's per-assignment runner has been removed. The probe is retired through the existing
registry, with its old code and pure grading retained for historical captures. Its
legacy work-walk/runtime/cell arguments do not provide current Stu recovery coverage.

Exit: the guest driver's exit code (0 when every probe that ran PASSED; 1 when any FAILED
or its premise was false; 2 for a harness failure), or 75 when admission was refused.

Results are private evidence: they go to the caller's --out, and from there to richos-hq
`docs/verification/`, never into this public repository. The guest driver records frames
with account data removed and environment NAMES only, never values.
"""
import argparse
import contextlib
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import uuid

HERE = Path(__file__).resolve().parent
TESTVM = HERE.parent / 'testvm'
REPO = HERE.parents[3]
sys.path.insert(0, str(TESTVM))
from slots import guest_slot  # noqa: E402
sys.path.insert(0, str(HERE.parent / 'lib'))
import runtime_cache  # noqa: E402


def sh(args, timeout=None, **kw):
    return subprocess.run(args, text=True, capture_output=True, timeout=timeout, **kw)


def guest(vm, command, timeout=120):
    result = sh([str(TESTVM / 'guest.sh'), vm, command], timeout=timeout)
    if result.returncode:
        raise RuntimeError(f'guest command failed ({result.returncode}): {command}\n{result.stderr}{result.stdout}')
    return result.stdout.strip()


def running_clones():
    listing = sh(['bash', '-c', '. "$1/lib.sh"; preflight_tart >/dev/null; tart list --format json', 'probes', str(TESTVM)],
                 timeout=30)
    if listing.returncode:
        raise RuntimeError('tart could not be asked what is running: ' + listing.stderr)
    base = os.environ.get('TESTVM_BASE_VM', 'richos-base')
    return [r.get('Name') for r in json.loads(listing.stdout)
            if r.get('Source') == 'local' and r.get('Running') and r.get('Name') != base]


STATUSLINE = Path.home() / '.claude' / 'statusline-payload.json'


def five_hour_used():
    """His five-hour quota reading (ruling §87's source), or None when it cannot be read."""
    try:
        return float(json.loads(STATUSLINE.read_text())['rate_limits']['five_hour']['used_percentage'])
    except (OSError, ValueError, KeyError, TypeError):
        return None


def engine_archive(rev, into):
    """The engine exactly as committed at `rev`, with the marketplace file that makes it a
    directory marketplace, as his terminal registers it (`~/.claude/settings.json`)."""
    sha = sh(['git', '-C', str(REPO), 'rev-parse', rev], timeout=30)
    if sha.returncode:
        raise RuntimeError(f'{rev} is not a commit in {REPO}')
    sha = sha.stdout.strip()
    tar = into / 'engine.tar'
    made = sh(['git', '-C', str(REPO), 'archive', '--format=tar', '-o', str(tar), sha,
               '.claude-plugin', 'richos/engine'], timeout=300)
    if made.returncode:
        raise RuntimeError('git archive failed: ' + made.stderr)
    return sha, tar


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--out', type=Path, required=True, help='host folder the results are pulled into')
    p.add_argument('--only', default='', help='comma-separated probe ids (default: all)')
    p.add_argument('--engine-rev', default='HEAD', help='the engine commit the guest runs (default HEAD)')
    p.add_argument('--wait', type=float, default=0,
                   help='wait for a free guest slot and CPU admission for at most this many seconds (slots.py)')
    p.add_argument('--survey', action='store_true', help='record the guest toolset only, run no probe')
    p.add_argument('--probe-timeout', type=int, default=900, help='seconds per probe inside the guest')
    p.add_argument('--walk-binary', type=Path, default=None,
                   help='the operator_walk example binary (cargo build --example operator_walk), for the W2 walk')
    p.add_argument('--work-walk-binary', type=Path, default=None,
                   help='the work_walk example binary (cargo build --features crash-points --example work_walk), for W4')
    p.add_argument('--runtime', type=Path, default=runtime_cache.cache_path(Path.home() / '.richos-nightly',
                                                                          HERE.parent / 'runtime-sources.json'),
                   help='a delivered runtime (delivery.json and its files) for W4\'s work lease; read, never written')
    p.add_argument('--w4-cells', default='',
                   help='comma-separated work crash-matrix cells for W4 (baseline,P4-carry,P4-sent,P5,P4e,P7; default '
                        'all), so one cell can be retried alone')
    p.add_argument('--w3-cells', default='',
                   help='comma-separated crash-matrix cells for W3 (baseline,W1b,W2,W3,W4; default all), so one '
                        'cell can be rerun alone')
    p.add_argument('--quota-ceiling', type=float, default=85.0,
                   help='stop the run when his five-hour quota reading reaches this percent (default 85), so a '
                        'probe never carries the account to ruling §87\'s 93%% pause for every other session')
    a = p.parse_args()

    # An unknown probe id is refused here, before a guest is booted and set up to run nothing.
    # guest_probes.py refuses it again inside the guest; this is the cheap copy of that check.
    import importlib.util
    spec = importlib.util.spec_from_file_location('guest_probes', HERE / 'guest_probes.py')
    gp = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gp)
    unknown = gp.unknown_probe_ids(a.only)
    if unknown:
        p.error('not a probe: %s (known: %s)' % (', '.join(unknown), ', '.join(pid for pid, _ in gp.PROBES)))

    out = a.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    vm = 'probes-' + uuid.uuid4().hex[:12]
    root = Path(os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm')))
    report = {'vm': vm, 'started': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'outcome': 'harness failure'}

    def interrupted(signum, frame):
        raise KeyboardInterrupt('interrupted by signal ' + str(signum))
    for sig in (signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, interrupted)

    def clean():
        # Idempotent: inside the slot when the run ends, again from the outer finally
        # for a failure before the slot was ever held. The slot is released only after
        # its guest is gone (slots.py: a guest never outlives its slot).
        if (root / 'run' / vm).exists():
            stopped = sh([str(TESTVM / 'stop.sh'), vm], timeout=300)
            report['cleanup'] = {'exit': stopped.returncode, 'said': (stopped.stdout + stopped.stderr)[-2000:]}
            try:
                report['clone_left'] = vm in running_clones()
            except Exception as error:  # noqa: BLE001
                report['clone_left'] = 'unknown: ' + repr(error)

    try:
        with guest_slot(root=root, wait_seconds=a.wait, purpose='run-probes ' + vm), \
                contextlib.ExitStack() as in_slot:
            in_slot.callback(clean)
            with tempfile.TemporaryDirectory(prefix='operator-probes-', dir=os.environ.get('TMPDIR')) as scratch:
                scratch = Path(scratch)
                home = scratch / 'home'
                home.mkdir()
                sha, engine_tar = engine_archive(a.engine_rev, scratch)
                report['engine_commit'] = sha
                bundle = scratch / 'probes.tar'
                with tarfile.open(bundle, 'w') as t:
                    t.add(str(HERE / 'guest_probes.py'), arcname='probes/guest_probes.py')
                    t.add(str(engine_tar), arcname='probes/engine.tar')
                    if a.walk_binary:
                        t.add(str(a.walk_binary.resolve()), arcname='probes/operator_walk')
                        report['walk_binary_sha256'] = __import__('hashlib').sha256(a.walk_binary.read_bytes()).hexdigest()
                    if a.work_walk_binary:
                        if not (a.runtime / 'delivery.json').is_file():
                            raise RuntimeError('W4 needs a delivered runtime; %s has no delivery.json' % a.runtime)
                        t.add(str(a.work_walk_binary.resolve()), arcname='probes/work_walk')
                        report['work_walk_binary_sha256'] = __import__('hashlib').sha256(
                            a.work_walk_binary.read_bytes()).hexdigest()
                        # Symlinks stay links (delivery.json lists them); the guest checks every hash.
                        t.add(str(a.runtime.resolve()), arcname='probes/runtime')
                        report['runtime_delivery_sha256'] = __import__('hashlib').sha256(
                            (a.runtime / 'delivery.json').read_bytes()).hexdigest()
                boot = subprocess.Popen([str(TESTVM / 'run.sh'), '--no-app', '--no-tailnet', '--vm', vm, '--home', str(home)],
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, start_new_session=True)
                try:
                    boot_out, boot_err = boot.communicate(timeout=480)
                except subprocess.TimeoutExpired:
                    os.killpg(boot.pid, signal.SIGTERM)
                    raise
                (out / 'boot.log').write_text(boot_err + '\n' + boot_out)
                if boot.returncode:
                    raise RuntimeError(f'run.sh --no-app failed ({boot.returncode}); see {out / "boot.log"}')
                payload = (root / 'run' / vm / 'payload').read_text().strip()
                report['payload'] = payload
                pushed = sh([str(TESTVM / 'guest.sh'), vm, '--push', str(bundle), payload + '/'], timeout=300)
                if pushed.returncode:
                    raise RuntimeError('the harness could not be copied in: ' + pushed.stderr)
                guest(vm, f'cd {shlex.quote(payload)} && tar -xf probes.tar && rm -f probes.tar')
                args = ['/usr/bin/python3', f'{payload}/probes/guest_probes.py', '--payload', payload,
                        '--engine-commit', sha, '--probe-timeout', str(a.probe_timeout)]
                if a.only:
                    args += ['--only', a.only]
                if a.survey:
                    args += ['--survey']
                if a.walk_binary:
                    args += ['--walk-binary', f'{payload}/probes/operator_walk']
                if a.work_walk_binary:
                    args += ['--work-walk-binary', f'{payload}/probes/work_walk', '--runtime', f'{payload}/probes/runtime']
                if a.w4_cells:
                    args += ['--w4-cells', a.w4_cells]
                if a.w3_cells:
                    args += ['--w3-cells', a.w3_cells]
                command = ' '.join(shlex.quote(x) for x in args)
                began = time.monotonic()
                report['quota_at_start'] = five_hour_used()
                if report['quota_at_start'] is not None and report['quota_at_start'] >= a.quota_ceiling:
                    raise BlockingIOError('five-hour quota at %s%%, at or over the %s%% ceiling'
                                          % (report['quota_at_start'], a.quota_ceiling))
                with open(out / 'guest-driver.log', 'w') as log:
                    # Owned: this Popen is the only process stopped here, and only by its own pid group.
                    driver = subprocess.Popen([str(TESTVM / 'guest.sh'), vm, command], stdout=log,
                                              stderr=subprocess.STDOUT, text=True, start_new_session=True)
                    deadline = time.monotonic() + a.probe_timeout * 20 + 600
                    while driver.poll() is None:
                        used = five_hour_used()
                        if used is not None and used >= a.quota_ceiling:
                            report['stopped_at_quota'] = used
                            os.killpg(driver.pid, signal.SIGTERM)
                            driver.wait(timeout=60)
                            break
                        if time.monotonic() > deadline:
                            os.killpg(driver.pid, signal.SIGTERM)
                            driver.wait(timeout=60)
                            report['stopped_at_deadline'] = True
                            break
                        time.sleep(30)
                report['quota_at_end'] = five_hour_used()
                report['driver_seconds'] = round(time.monotonic() - began, 1)
                report['driver_exit'] = driver.returncode
                pulled = sh([str(TESTVM / 'guest.sh'), vm, '--pull', f'{payload}/results', str(out)], timeout=600)
                if pulled.returncode:
                    raise RuntimeError('the results could not be copied out: ' + pulled.stderr)
                report['outcome'] = 'completed'
                return driver.returncode
    except BlockingIOError as refused:
        report['outcome'] = 'admission refused: ' + str(refused)
        print(report['outcome'], file=sys.stderr)
        return 75
    except BaseException as failure:  # noqa: BLE001 - recorded, then the clone still goes
        report['error'] = repr(failure)
        print('probe run failed: ' + repr(failure), file=sys.stderr)
        return 2
    finally:
        clean()
        report['ended'] = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
        (out / 'run-report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report, indent=2))


if __name__ == '__main__':
    sys.exit(main())
