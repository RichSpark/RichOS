#!/usr/bin/env python3
"""Admit heavy work by measured CPU and memory. Example: reserve.py -- COMMAND ARGS...

CEO ruling §77 (2026-09-22): test runs and development builds, including land
checks, are admitted by the CPU rule alone and do not take the nightly
release.lock. Pass --release-lock only for a command that writes nightly or
release state; the nightly/release commands hold that lock themselves.

Admission is one cheap in-process sample (the kernel's own counters, read with
host_statistics and sysctl: no top, no lsof, no subprocess). It refuses when
total CPU (user plus system) is at or above --max-cpu (default 80%) or when the machine is swapping
hard (kernel pressure CRITICAL, or swap-out at or above --max-swapout-mb-s
during the sample). --wait N retries for at most N seconds, one sample every
--retry-every seconds (at least 30), and reports how long it waited.

--low-priority is CEO ruling §78's mode for the 30-hour native build: the CPU
line is not applied, the memory rule and the backoff still are, and the command
runs under nice -n 10. Every use is announced. When that build ends, the 80%
line applies again: stop passing the flag.

A NIGHTLY WAITING FOR ITS GUEST GOES FIRST (2026-10-29, nightly 80). While a
nightly's vm-boot-proof or new-user-walk waits for a test-VM guest slot,
slots.py writes <TESTVM_ROOT>/nightly-guest-wait.json, and every admission
sampled here (this command, proof-run.py's checks, every other guest walk's
slot) is refused, naming that wait, until the nightly is admitted or gives up.
Only that one waiting process is exempt. It is narrower than release.lock,
which §77 keeps away from proof runs, and it holds nothing: a record whose
process is gone, or older than the longest slot wait, is ignored.
"""
import argparse
from contextlib import contextmanager, ExitStack
import ctypes
import fcntl
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import math

# Before the import below: cpu_policy and what it imports live under engine/scripts/lib, and
# compiling them here wrote lib/__pycache__ into the checkout on every admitted run (2026-09-30).
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'engine/scripts/lib'))
from cpu_policy import DEFAULT_MAX_CPU, admission_open, busy_percent
try:
    # A wait that lasts is recorded where the lead's turn-end gate reads it
    # (engine scripts/lib/resource_waits.py). Admission never depends on it.
    import resource_waits
except ImportError:  # pragma: no cover - an engine older than the recorder
    resource_waits = None

MIN_RETRY_SECONDS = 30
LOW_PRIORITY_NICE = 10
MAX_WAIT_SECONDS = 3600
PRESSURE_NAMES = {1: 'normal', 2: 'warn', 4: 'critical'}
NIGHTLY_GUEST_WAIT = 'nightly-guest-wait.json'
# A record older than this is a leftover, whatever its pid says: no slot wait outlasts
# MAX_WAIT_SECONDS, and the margin covers the samples taken around it.
NIGHTLY_GUEST_WAIT_STALE = MAX_WAIT_SECONDS + 600


# THE KERNEL'S OWN LEAK (2026-10-29, Otto's read-only trace). With no guest running this Mac had
# 12.57 GiB wired, about 9.3 GiB of it one kernel zone, data.kalloc.1024, growing about 46
# elements/s under agent workloads; only a restart gives it back. A memory refusal that named
# only apps sent people closing apps for memory no app held. `zprint ZONE` needs no sudo; without
# it the size columns read 0K, so the zone's size is its elements in use times the element size.
LEAK_ZONE = 'data.kalloc.1024'
LEAK_SHARE = 0.10           # named once the zone holds this share of physical memory
_ZONE_CACHE = {}


def zone_in_use_mb(text, zone=LEAK_ZONE):
    """MB in use by `zone` in `zprint`'s output, else None. Columns: name, element size, cur size,
    max size, cur #elts, max #elts, cur inuse, alloc size, alloc count."""
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 7 and parts[0] == zone and parts[1].isdigit() and parts[6].isdigit():
            return int(parts[1]) * int(parts[6]) / 1048576
    return None


def kernel_zone_text(zone=LEAK_ZONE, now=None):
    """`zprint ZONE`'s output, read-only and at most once a minute per process; '' if unreadable."""
    now = time.monotonic() if now is None else now
    cached = _ZONE_CACHE.get(zone)
    if cached and now - cached[0] < 60:
        return cached[1]
    try:
        text = subprocess.run(['/usr/bin/zprint', zone], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        text = ''
    _ZONE_CACHE[zone] = (now, text)
    return text


def kernel_leak_note(total_mb=None, text=None):
    """'' unless the leaking kernel zone holds LEAK_SHARE or more of this Mac's memory, else the
    clause that names it. `text` is zprint's output (read here when None)."""
    held = zone_in_use_mb(kernel_zone_text() if text is None else text)
    if held is None:
        return ''
    try:
        total_mb = total_mb or _sysctl('hw.memsize', ctypes.c_uint64()).value / 1048576
    except OSError:
        return ''
    if held < LEAK_SHARE * total_mb:
        return ''
    return f'kernel leak {LEAK_ZONE} holds {held / 1024:.1f} GB; only a restart returns it'


def testvm_root(root=None):
    """The test VM's state directory (slots.py's root_dir): TESTVM_ROOT, else ~/.richos-testvm."""
    return Path(root or os.environ.get('TESTVM_ROOT', str(Path.home() / '.richos-testvm')))


def _alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def nightly_guest_wait(root=None, now=None):
    """The record of a nightly phase waiting for a guest slot right now, else None. A record
    that cannot be read, whose process has ended or that is older than any slot wait is None."""
    path = testvm_root(root) / NIGHTLY_GUEST_WAIT
    try:
        record = json.loads(path.read_text())
        pid, since = int(record['pid']), float(record['since'])
    except (OSError, ValueError, KeyError, TypeError):
        return None
    if pid <= 0 or not _alive(pid) or (now or time.time()) - since > NIGHTLY_GUEST_WAIT_STALE:
        return None
    return {**record, 'pid': pid, 'since': since}


def nightly_hold(root=None, now=None, me=None):
    """'' when no nightly waits for a guest (or the one waiting is this process), else the
    sentence that refuses a new test or proof admission."""
    waiting = nightly_guest_wait(root, now)
    if not waiting or waiting['pid'] == (me or os.getpid()):
        return ''
    minutes = ((now or time.time()) - waiting['since']) / 60
    return (f"a nightly's {waiting.get('phase') or 'VM phase'} has waited {minutes:.0f} min for a test-VM "
            f"guest slot (pid {waiting['pid']}); new test and proof admissions are held until it has "
            f"its guest or gives up ({NIGHTLY_GUEST_WAIT}, reserve.py)")


def write_nightly_guest_wait(phase, root=None):
    """Record that this process, a nightly's `phase`, is waiting for a guest slot. Atomic."""
    path = testvm_root(root) / NIGHTLY_GUEST_WAIT
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.parent.is_symlink() or path.parent.stat().st_uid != os.getuid():
        raise ValueError('testvm directory must be owned by the current user and not a symlink: '
                         + str(path.parent))
    partial = path.with_name(f'.{path.name}.{os.getpid()}')
    partial.write_text(json.dumps({'pid': os.getpid(), 'since': time.time(), 'phase': phase}) + '\n')
    os.replace(partial, path)
    return path


def clear_nightly_guest_wait(root=None):
    """Remove the record, only if it is this process's own."""
    path = testvm_root(root) / NIGHTLY_GUEST_WAIT
    try:
        if int(json.loads(path.read_text()).get('pid', 0)) == os.getpid():
            path.unlink()
    except (OSError, ValueError, TypeError, AttributeError):
        pass


class _CpuLoad(ctypes.Structure):
    # host_cpu_load_info: ticks[CPU_STATE_MAX]; USER=0, SYSTEM=1, IDLE=2, NICE=3
    _fields_ = [('ticks', ctypes.c_uint * 4)]


class _Vm64(ctypes.Structure):
    # vm_statistics64 (<mach/vm_statistics.h>), HOST_VM_INFO64_COUNT = 38 words
    _fields_ = ([(n, ctypes.c_uint) for n in ('free_count', 'active_count', 'inactive_count', 'wire_count')]
                + [(n, ctypes.c_uint64) for n in ('zero_fill_count', 'reactivations', 'pageins', 'pageouts',
                                                  'faults', 'cow_faults', 'lookups', 'hits', 'purges')]
                + [(n, ctypes.c_uint) for n in ('purgeable_count', 'speculative_count')]
                + [(n, ctypes.c_uint64) for n in ('decompressions', 'compressions', 'swapins', 'swapouts')]
                + [(n, ctypes.c_uint) for n in ('compressor_page_count', 'throttled_count',
                                                'external_page_count', 'internal_page_count')]
                + [('total_uncompressed_pages_in_compressor', ctypes.c_uint64)])


class _SwapUsage(ctypes.Structure):
    _fields_ = [('total', ctypes.c_uint64), ('avail', ctypes.c_uint64), ('used', ctypes.c_uint64),
                ('pagesize', ctypes.c_uint32), ('encrypted', ctypes.c_int)]


_KERNEL = {}


def _kernel():
    """libSystem and one host port, opened once per process."""
    if not _KERNEL:
        lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib', use_errno=True)
        lib.mach_host_self.restype = ctypes.c_uint
        _KERNEL.update(lib=lib, host=lib.mach_host_self())
    return _KERNEL['lib'], _KERNEL['host']


def _sysctl(name, value):
    lib, _ = _kernel()
    size = ctypes.c_size_t(ctypes.sizeof(value))
    if lib.sysctlbyname(name.encode(), ctypes.byref(value), ctypes.byref(size), None, ctypes.c_size_t(0)):
        raise OSError(ctypes.get_errno(), 'sysctl ' + name)
    return value


def _counters():
    lib, host = _kernel()
    cpu, n = _CpuLoad(), ctypes.c_uint(4)
    if lib.host_statistics(host, 3, ctypes.byref(cpu), ctypes.byref(n)):          # HOST_CPU_LOAD_INFO
        raise OSError('host_statistics(HOST_CPU_LOAD_INFO) failed')
    vm, m = _Vm64(), ctypes.c_uint(ctypes.sizeof(_Vm64) // 4)
    if lib.host_statistics64(host, 4, ctypes.byref(vm), ctypes.byref(m)):          # HOST_VM_INFO64
        raise OSError('host_statistics64(HOST_VM_INFO64) failed')
    return list(cpu.ticks), vm.swapouts


def host_sample(interval=1.0):
    """One admission sample: CPU split and swap activity over `interval`, plus the
    kernel's memory-pressure level, its free-memory percentage and swap in use.
    Fails closed: anything unreadable raises BlockingIOError."""
    try:
        ticks0, out0 = _counters()
        time.sleep(interval)
        ticks1, out1 = _counters()
        page = _sysctl('hw.pagesize', ctypes.c_int()).value
        level = _sysctl('kern.memorystatus_vm_pressure_level', ctypes.c_int()).value
        free = _sysctl('kern.memorystatus_level', ctypes.c_int()).value
        swap = _sysctl('vm.swapusage', _SwapUsage())
    except (OSError, AttributeError) as exc:
        raise BlockingIOError('CPU/memory measurement unavailable; admission refused: ' + str(exc)) from exc
    delta = [(b - a) % 2**32 for a, b in zip(ticks0, ticks1)]
    total = sum(delta)
    if total <= 0:
        raise BlockingIOError('CPU measurement unavailable; the tick counters did not advance')
    pct = [100.0 * d / total for d in delta]
    sample = {'cpu_user_percent': pct[0] + pct[3], 'cpu_system_percent': pct[1], 'cpu_idle_percent': pct[2],
              'swapout_mb_per_s': (out1 - out0) * page / 1048576 / interval,
              'memory_pressure': PRESSURE_NAMES.get(level, str(level)), 'memory_free_percent': free,
              'swap_used_mb': swap.used / 1048576}
    held = nightly_hold()
    if held:
        # Carried in the sample, so every caller that refuses on it (_refusal) also says why
        # (describe), proof-run.py's checks included.
        sample['held_for_nightly'] = held
    return sample


def memory_sample():
    """The memory half of `host_sample`, at once: no CPU interval, no swap-out rate. For a
    question about memory alone (slots.py `fits`). Fails closed like host_sample."""
    try:
        level = _sysctl('kern.memorystatus_vm_pressure_level', ctypes.c_int()).value
        free = _sysctl('kern.memorystatus_level', ctypes.c_int()).value
        swap = _sysctl('vm.swapusage', _SwapUsage())
    except (OSError, AttributeError) as exc:
        raise BlockingIOError('memory measurement unavailable: ' + str(exc)) from exc
    return {'memory_pressure': PRESSURE_NAMES.get(level, str(level)), 'memory_free_percent': free,
            'swap_used_mb': swap.used / 1048576}


def cpu_busy_percent():
    """Total CPU utilization, including user, nice and system time."""
    return busy_percent(host_sample())


def _refusal(s, max_cpu, max_swapout, cpu_rule=True):
    if s.get('held_for_nightly'):
        return s['held_for_nightly']
    if cpu_rule and not admission_open(busy_percent(s), max_cpu):
        return f"total CPU is {busy_percent(s):.1f}% (limit {max_cpu:g}%)"
    if s['memory_pressure'] == 'critical':
        why = 'kernel memory pressure is CRITICAL'
    elif s['swapout_mb_per_s'] >= max_swapout:
        why = f"swapping out {s['swapout_mb_per_s']:.1f} MB/s (limit {max_swapout:g} MB/s)"
    else:
        return ''
    leak = kernel_leak_note()
    return f'{why}; {leak}' if leak else why


def describe(s):
    return ((f"HELD: {s['held_for_nightly']}; " if s.get('held_for_nightly') else '')
            + f"user CPU {s['cpu_user_percent']:.1f}%, system {s['cpu_system_percent']:.1f}%, "
            f"idle {s['cpu_idle_percent']:.1f}%; memory pressure {s['memory_pressure']}, "
            f"free {s['memory_free_percent']}%, swap used {s['swap_used_mb']:.0f} MB, "
            f"swap-out {s['swapout_mb_per_s']:.1f} MB/s")


def _record_wait(why, holding):
    """The wait, recorded for the lead's turn-end gate from its first refusal; None if unrecordable."""
    if resource_waits is None:
        return None
    try:
        return resource_waits.waiting(resource_waits.CPU, why, holding=holding).start()
    except Exception as exc:  # never let the record break admission
        print(f'admission wait not recorded: {exc}', file=sys.stderr, flush=True)
        return None


def cpu_admission(max_cpu=DEFAULT_MAX_CPU, wait_seconds=0, retry_every=MIN_RETRY_SECONDS, max_swapout=16, cpu_rule=True,
                  holding=()):
    """Bounded admission before work or a paid send; never infer jobs from load.
    Retries at most every `retry_every` (>= 30) seconds, never spins.
    `cpu_rule=False` is the low-priority mode (CEO ruling §78): the CPU line is
    not applied, the memory rule still is.

    From the first refusal until admission or the final refusal the wait is
    recorded (resource_waits.py), with `holding` naming any lock the caller holds
    while it waits, so the lead is stopped at turn end once it passes ten minutes
    and a lock held by a waiter is shown as held and NOT in use."""
    if not math.isfinite(max_cpu) or not 0 < max_cpu <= 100:
        raise ValueError('max_cpu must be greater than 0 and at most 100')
    if not math.isfinite(wait_seconds) or not 0 <= wait_seconds <= MAX_WAIT_SECONDS:
        raise ValueError(f'admission wait must be between 0 and {MAX_WAIT_SECONDS} seconds')
    if not math.isfinite(retry_every) or retry_every < MIN_RETRY_SECONDS:
        raise ValueError(f'samples are at least {MIN_RETRY_SECONDS} seconds apart')
    if not math.isfinite(max_swapout) or max_swapout <= 0:
        raise ValueError('max_swapout must be a positive number of MB/s')
    started = time.monotonic()
    samples = 0
    recorded = None
    try:
        while True:
            s = host_sample()
            samples += 1
            elapsed = time.monotonic() - started
            why = _refusal(s, max_cpu, max_swapout, cpu_rule)
            if not why:
                return {**s, 'cpu_busy_percent': busy_percent(s), 'load': os.getloadavg()[0],
                        'admission_wait_seconds': elapsed, 'admission_samples': samples}
            remaining = wait_seconds - elapsed
            if remaining <= 0:
                raise BlockingIOError(f'{why}; admission refused after {elapsed:.0f}s and {samples} sample(s). '
                                      f'{describe(s)}. Load average alone does not identify another running job.')
            if recorded is None:
                recorded = _record_wait(why, holding)
            else:
                recorded.update(why)
            print(f'admission waiting: {why}; next sample in {min(retry_every, remaining):.0f}s', file=sys.stderr,
                  flush=True)
            time.sleep(min(retry_every, remaining))
    finally:
        if recorded is not None:
            recorded.close()


@contextmanager
def exclusive_lock(path):
    """Hold one nonblocking exclusive flock on `path` for the whole interval."""
    path = Path(path)
    parent = path.parent
    parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if parent.is_symlink() or parent.stat().st_uid != os.getuid():
        raise ValueError('lock directory must be owned by the current user and not a symlink: ' + str(parent))
    with path.open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise BlockingIOError('reservation unavailable: ' + str(path)) from None
        print('reservation held: ' + str(path.resolve()), file=sys.stderr, flush=True)
        try:
            yield path.resolve()
        finally:
            print('reservation released', file=sys.stderr, flush=True)


@contextmanager
def reservation(state=None, max_load=None, max_cpu=DEFAULT_MAX_CPU, release_lock=False, lock=None,
                wait_seconds=0, retry_every=MIN_RETRY_SECONDS, max_swapout=16, low_priority=False):
    """Admission for heavy work (CEO ruling §77).

    `low_priority=True` is CEO ruling §78's explicit mode for the 30-hour native
    build: the CPU line is not applied (the memory rule and the backoff still
    are) and the caller runs the command under `nice -n LOW_PRIORITY_NICE`. It
    is announced on every use; it is never a default.

    By default this is the CPU rule alone (with the memory rule beside it, see
    `cpu_admission`): nothing is locked and nothing under
    the nightly state directory is touched, so test runs and development builds
    never queue behind one another or behind a nightly. `release_lock=True`
    additionally holds `<state>/release.lock`, the lock `nightly-local.py`'s
    `exclusive()` takes; use it only for work that writes nightly/release state.
    `lock` holds a different single-owner resource lock (the test VM guest).
    Locks are taken first, so a held lock refuses without spending a sample."""
    if release_lock and state is None:
        raise ValueError('release_lock needs the nightly state directory')
    with ExitStack() as held:
        paths = []
        if release_lock:
            state = Path(state)
            paths.append(held.enter_context(exclusive_lock(state / 'release.lock')))
        if lock is not None:
            paths.append(held.enter_context(exclusive_lock(lock)))
        if max_load is not None:
            # Explicit opt-in for a controlled benchmark's historical load criterion.
            if not math.isfinite(max_load) or max_load <= 0 or os.getloadavg()[0] >= max_load:
                raise BlockingIOError('load is above the explicitly requested limit; admission refused')
        else:
            sample = cpu_admission(max_cpu, wait_seconds, retry_every, max_swapout, cpu_rule=not low_priority,
                                   holding=paths)
            print(f"admission: {describe(sample)}; waited {sample['admission_wait_seconds']:.0f}s over "
                  f"{sample['admission_samples']} sample(s); load {sample['load']:.2f} is informational",
                  file=sys.stderr, flush=True)
            if low_priority:
                over = not admission_open(busy_percent(sample), max_cpu)
                print(f"LOW PRIORITY (CEO ruling §78): the {max_cpu:g}% CPU line was NOT applied"
                      f"{' and total CPU is over it' if over else ''}; the command runs under "
                      f"nice -n {LOW_PRIORITY_NICE}. The memory rule was applied.", file=sys.stderr, flush=True)
        if not paths:
            print('admitted without a lock (CEO ruling §77)', file=sys.stderr, flush=True)
        yield paths[0] if paths else None


def _killpg_if_present(pid, sig):
    """Signal the command's group; a group that is gone or going is not an error.

    macOS raises PermissionError (EPERM) as well as ProcessLookupError for a group whose
    members are all exiting. Nothing else is swallowed.
    """
    try:
        os.killpg(pid, sig)
    except (ProcessLookupError, PermissionError):
        pass


class StopRequested(BaseException):
    """Raised by the signal handler to unwind out of child.wait() before the group is stopped.

    The handler must not call child.wait() itself: it runs inside the main thread's own
    child.wait(), which holds Popen's non-reentrant wait lock, so a nested wait can never
    reap the child (it waited out its timeout, then blocked forever after SIGKILL).
    """

    def __init__(self, signum):
        super().__init__(signum)
        self.signum = signum


STOP_GRACE = 10  # seconds the command's group gets after SIGTERM before SIGKILL
KILL_GRACE = 5   # seconds to wait for the group to vanish after SIGKILL


def _group_present(child):
    """True while any process of the command's group exists.

    The leader (the Popen child) exiting does not mean the group is gone: a descendant in the
    same group can outlive it and ignore SIGTERM. The leader is reaped first so its zombie
    does not count; EPERM (macOS, members exiting) counts as present, under a bounded wait.
    """
    child.poll()
    try:
        os.killpg(child.pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def _wait_group_gone(child, seconds):
    end = time.monotonic() + seconds
    while _group_present(child):
        if time.monotonic() >= end:
            return False
        time.sleep(0.05)
    return True


def stop_group(child, signum):
    """SIGTERM the command's group, SIGKILL whatever of it remains after STOP_GRACE, exit 128+signum.

    Completion is judged on the whole group, never on the leader alone (a descendant that
    ignores SIGTERM survives the leader's exit and needs the SIGKILL).
    """
    _killpg_if_present(child.pid, signal.SIGTERM)
    if not _wait_group_gone(child, STOP_GRACE):
        _killpg_if_present(child.pid, signal.SIGKILL)
        _wait_group_gone(child, KILL_GRACE)
    child.wait()
    raise SystemExit(128+signum)


def main():
    sys.dont_write_bytecode = True
    # Engineers also invoke Cargo directly through this admission wrapper.
    # Apply isolation inside RichOS worktrees without changing resource policy.
    cargo_library = Path(__file__).resolve().parents[1] / 'lib'
    if (cargo_library / 'cargo_identity.py').is_file():
        sys.path.insert(0, str(cargo_library))
        import cargo_identity
        os.environ.update(cargo_identity.enable_environment(os.environ))
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--state-dir', type=Path, default=Path.home()/'.richos-nightly',
                   help='same --state-dir as nightly-local.py; read only with --release-lock')
    p.add_argument('--release-lock', action='store_true',
                   help='also hold <state-dir>/release.lock: only for work that writes nightly/release state')
    p.add_argument('--max-load', type=float, help='opt in to a strict load-average gate for controlled measurements')
    p.add_argument('--max-cpu', type=float, default=DEFAULT_MAX_CPU, help='maximum measured total CPU percentage (default: 80)')
    p.add_argument('--max-swapout-mb-s', type=float, default=16,
                   help='refuse while swapping out at least this many MB/s during the sample (default: 16)')
    p.add_argument('--wait', type=float, default=0, metavar='SECONDS',
                   help=f'retry admission for at most SECONDS (0-{MAX_WAIT_SECONDS}; default 0: refuse at once)')
    p.add_argument('--retry-every', type=float, default=MIN_RETRY_SECONDS, metavar='SECONDS',
                   help=f'seconds between samples while waiting (at least {MIN_RETRY_SECONDS})')
    p.add_argument('--low-priority', action='store_true',
                   help=f'CEO ruling §78, the 30-hour native build only: skip the CPU line (memory rule and '
                        f'backoff still apply) and run the command under nice -n {LOW_PRIORITY_NICE}; announced')
    p.add_argument('command', nargs=argparse.REMAINDER)
    args = p.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    if not command:
        p.error('a command is required')
    if args.low_priority:
        command = ['/usr/bin/nice', '-n', str(LOW_PRIORITY_NICE), *command]
    with reservation(args.state_dir, args.max_load, args.max_cpu, release_lock=args.release_lock,
                     wait_seconds=args.wait, retry_every=args.retry_every, max_swapout=args.max_swapout_mb_s,
                     low_priority=args.low_priority):
        # Any lock FD stays in the supervisor until the command is reaped.
        child = subprocess.Popen(command, start_new_session=True)
        def stop(signum, frame):
            raise StopRequested(signum)
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, stop)
        try:
            try:
                return child.wait()
            except StopRequested as requested:
                for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
                    signal.signal(sig, signal.SIG_IGN)
                stop_group(child, requested.signum)
        finally:
            if _group_present(child):
                _killpg_if_present(child.pid, signal.SIGKILL)
                _wait_group_gone(child, KILL_GRACE)
            child.wait()


if __name__ == '__main__':
    try:sys.exit(main())
    except BlockingIOError as exc:print(str(exc),file=sys.stderr);sys.exit(75)
    except ValueError as exc:print('reserve.py: '+str(exc),file=sys.stderr);sys.exit(2)
