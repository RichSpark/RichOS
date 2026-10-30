#!/usr/bin/env python3
"""The test VM's two guest slots: each is held ONLY while one run executes in a guest.

  slots.py status                           who holds which slot, right now
  slots.py run [--wait SECONDS] -- COMMAND  hold one slot for exactly COMMAND's run
  slots.py check                            (run.sh) print the slot, exit 0, only inside a held slot

THE RULE (the CEO, 2026-09-27): *"do I want the work to be BLOCKED AND PISSED AWAY like this
because the current worker might need the VM for a 5-second-long fart? Do I want HOURS OF
DEVELOPMENT TIME TO BE PISSED AWAY FOR EVERY 5-SECOND FART???"*

  * A slot is held only while a run executes in the guest: boot, the run's steps, cleanup.
    It is released the instant that run ends.
  * No reservations. Nothing keeps a slot "for later", across a job, a build or a debugging
    session. Anything that needs a guest again takes a free slot again.
  * A walk that truly needs one guest across several steps (a 30-minute observation) is one
    run, and holds a slot for that run's length only.
  * A caller waits only while every slot is executing somebody's run.

WHY TWO. With one guest lock, a job that needed the guest for a few minutes of a multi-hour
task queued everybody else behind it for hours. A guest takes a fixed share of the host's cores
and memory (lib.sh), and Apple's Virtualization framework runs at most two macOS guests at once
on one host. Two is therefore both the most that can fit and the ceiling: TESTVM_SLOTS may lower it to 1, never raise it.

SLOT 1 KEEPS THE OLD NAME, `<TESTVM_ROOT>/guest.lock`, on purpose: a run-walk.py from a
checkout older than this file still takes that lock, so it still counts as one slot, and the
new code can never put a third guest beside it. Slot 2 is `guest-2.lock`.

ADMISSION, in order, every attempt: a slot that is free (a probe that holds nothing, so no
sample is spent while every slot executes a run); fewer guests running than there are slots
(tart's own list, so a guest booted by an old checkout is counted); reserve.py's CPU and memory
rule (CEO ruling §77); then THE GUEST'S OWN MEMORY. ONLY THEN is a slot taken: a caller being
sampled, refused or waiting holds no slot, so no walk queues behind one that is not admitted
(a slot held through the one-second CPU sample would make a walk arriving in that second be
refused as if both slots ran). `--wait SECONDS` bounds the whole admission;
0, the default, refuses at once.
From the first refusal until the caller is admitted or refused for good, the wait is RECORDED
(the engine's resource_waits.py, through reserve.py), so the lead's turn end is refused once
any caller has waited past ten minutes (the engine's guard-resource-waits.sh).

THE GUEST'S OWN MEMORY: two guests admitted a moment apart both pass reserve.py's rule,
because a guest takes its memory only as it boots; shortly after, the host can be CPU-saturated
and swapping heavily although it looked mostly free at admission. reserve.py samples what is
already used; a guest is its whole RAM about to be used.
So a guest is admitted only when the kernel's available memory, less what every running guest
may still take (its RAM less what its VM process already holds) and less this guest's whole
RAM, leaves the memory floor for everything else on the Mac.

THE FLOOR IS A MACHINE SETTING, not a constant: a fixed floor refused a guest for an hour on a
machine whose own margin was lower. TESTVM_MEMORY_FLOOR_MB, from the environment or this machine's private
limits file (engine machine_settings.py `limit`), else GUEST_MEMORY_FLOOR_MB below, the stated
default for a Mac nobody has measured. Admission still reads live available memory every time;
the floor is only the margin a guest leaves for everything else. A refusal names the setting it
used, and the kernel's own leak (reserve.py LEAK_ZONE) when that, not an app, holds the memory.

`slots.py fits` answers "would one more guest fit now?" at once, for the nightly to ask before
its gates and again on entering its VM phase (nightly-local.py), instead of finding out at the
end of an hour's slot wait.

A NIGHTLY WAITING FOR ITS SLOT GOES FIRST. A caller with TESTVM_NIGHTLY_WAIT set (the nightly's
vm-boot-proof and new-user-walk) records its wait in <TESTVM_ROOT>/nightly-guest-wait.json from
its first refusal until it is admitted or refused for good; meanwhile reserve.py refuses every
other admission, test, proof and guest walk alike (reserve.py's docstring).

A GUEST NEVER OUTLIVES ITS SLOT. run.sh records the slot it booted under in its run state,
and refuses to boot at all unless a slot is held by one of its own ancestors (`check`). When a
slot is released, any guest still recorded against it is stopped with stop.sh first: a caller
that forgot its cleanup cannot turn a run into a hold.
"""
import argparse
from contextlib import contextmanager
import ctypes
import fcntl
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import reserve  # noqa: E402  (it also puts engine/scripts/lib on the path)
import machine_settings  # noqa: E402

SLOT_FILES = ('guest.lock', 'guest-2.lock')
MAX_SLOTS = len(SLOT_FILES)
SLOT_POLL_SECONDS = 2      # a flock attempt is a syscall; checking often costs nothing
CLONE_POLL_SECONDS = 10    # a running-guest count is one `tart list`
ENV = 'TESTVM_SLOT'
NIGHTLY_WAIT_ENV = 'TESTVM_NIGHTLY_WAIT'
FLOOR_SETTING = 'TESTVM_MEMORY_FLOOR_MB'
# The DEFAULT for what must stay available for the Mac itself once every admitted guest has all
# its RAM, used only where no machine sets FLOOR_SETTING. The 18:56Z measurement above swapped
# hard with about 3 GB projected to be left, so 4 GB.
GUEST_MEMORY_FLOOR_MB = 4096
VM_PROCESS = 'com.apple.Virtualization.VirtualMachine'
# Docker Desktop's VM runs in a VM process of its own; the app list counts it as Docker's.
DOCKER_VM_COMMAND = '/Applications/Docker.app/Contents/MacOS/com.docker.virtualization'


def _say(message):
    """stderr, but never at the price of a cleanup: a caller that died and took our stderr pipe
    with it must not turn the next line into a BrokenPipeError halfway through a release."""
    try:
        print(message, file=sys.stderr, flush=True)
    except (OSError, ValueError):
        pass


def _record_wait(recorded, reason):
    """The caller's wait, recorded for the lead's turn-end gate from its first refusal: opened
    once, its reason kept current. Never breaks admission: an engine without the recorder, or
    a directory it cannot write, is announced once and the wait goes on unrecorded."""
    rw = getattr(reserve, 'resource_waits', None)
    if rw is None:
        return None
    try:
        if recorded is None:
            return rw.waiting(rw.VM, reason).start()
        recorded.update(reason)
    except Exception as exc:  # the record is never a reason to refuse a guest
        _say(f'slot wait not recorded: {exc}')
    return recorded


def guest_ram_mb():
    """TESTVM_RAM_MB, else lib.sh's own default: one number, declared once, that run.sh gives the
    guest and this file admits it by."""
    if os.environ.get('TESTVM_RAM_MB'):
        return int(os.environ['TESTVM_RAM_MB'])
    found = re.search(r'^TESTVM_RAM_MB="\$\{TESTVM_RAM_MB:-(\d+)\}"', (HERE / 'lib.sh').read_text(), re.M)
    if not found:
        raise ValueError('lib.sh no longer declares TESTVM_RAM_MB\'s default where slots.py reads it')
    return int(found.group(1))


def _vm_processes():
    """({pid: MB} of the test guests' own VM processes, {pid: MB} of Docker Desktop's VM processes)
    by read-only `ps` and `lsof`. Every Virtualization VM process is a candidate; only one holding
    a tart guest's disk.img open is a test guest, so nothing else is charged to a guest."""
    table = subprocess.run(['ps', '-A', '-o', 'pid=,rss=,comm='], capture_output=True, text=True, timeout=10).stdout
    found = {}
    for line in table.splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[0].isdigit() and parts[1].isdigit() and parts[2].endswith('/' + VM_PROCESS):
            found[parts[0]] = int(parts[1]) / 1024
    if not found:
        return {}, {}
    held = subprocess.run(['lsof', '-nP', '-Fpn', '-p', ','.join(found)], capture_output=True, text=True, timeout=20).stdout
    guests, docker, pid = {}, {}, None
    for line in held.splitlines():
        if line.startswith('p'):
            pid = line[1:]
        elif line.startswith('n') and pid in found:
            if '/tart/vms/' in line and line.endswith('/disk.img'):
                guests[pid] = found[pid]
            elif 'com.docker.docker' in line:
                docker[pid] = found[pid]
    return guests, {pid: mb for pid, mb in docker.items() if pid not in guests}


def vm_resident_mb():
    """Resident memory of each running test guest's own VM process, in MB."""
    return list(_vm_processes()[0].values())


def host_memory_mb():
    return reserve._sysctl('hw.memsize', ctypes.c_uint64()).value / 1048576


def process_memory_mb():
    """[(full command, MB)] for this user's processes other than the test guests, by `top`'s own
    memory figure (what the process holds including what is compressed or swapped out; `ps rss`
    undercounts Chrome and WebStorm badly). Docker Desktop's VM process is listed as Docker's.
    Read-only; [] when `top` cannot answer."""
    try:
        shown = subprocess.run(['top', '-l', '1', '-o', 'mem', '-n', '300', '-stats', 'pid,mem'],
                               capture_output=True, text=True, timeout=30).stdout
        paths = subprocess.run(['ps', '-A', '-o', 'pid=,uid=,command='], capture_output=True, text=True,
                               timeout=10).stdout
        guests, docker = _vm_processes()
    except (OSError, subprocess.SubprocessError):
        return []
    path_of = {}
    for line in paths.splitlines():
        parts = line.split(None, 2)
        if len(parts) == 3 and parts[0].isdigit() and parts[1] == str(os.getuid()) and parts[0] not in guests:
            path_of[parts[0]] = DOCKER_VM_COMMAND if parts[0] in docker else parts[2]
    units = {'B': 1 / 1048576, 'K': 1 / 1024, 'M': 1, 'G': 1024, 'T': 1024 * 1024}
    found = []
    for line in shown.splitlines():
        parts = line.split()
        match = re.fullmatch(r'(\d+(?:\.\d+)?)([BKMGT])[+-]?', parts[1]) if len(parts) == 2 else None
        if match and parts[0] in path_of:
            found.append((path_of[parts[0]], float(match.group(1)) * units[match.group(2)]))
    return found


def top_apps(processes, count=3):
    """The `count` apps holding the most memory, [(app name, MB)]. Only his own apps are named: a
    process inside an `X.app` under /Applications or ~/Applications, helpers grouped under the app."""
    totals = {}
    roots = '|'.join(re.escape(r) for r in ('/Applications/', str(Path.home() / 'Applications') + '/'))
    for command, mb in processes:
        app = re.match(r'(?:%s)(?:[^/]+/)*?([^/]+)\.app(?:/|$)' % roots, command)
        if app:
            totals[app.group(1)] = totals.get(app.group(1), 0.0) + mb
    return sorted(totals.items(), key=lambda item: -item[1])[:count]


def guest_memory_floor():
    """(MB, where it came from): the machine's TESTVM_MEMORY_FLOOR_MB, else the stated default."""
    return machine_settings.limit(FLOOR_SETTING, GUEST_MEMORY_FLOOR_MB)


def _accounting(sample, running, resident, total_mb, ram_mb, floor_mb):
    """(ram, total, floor, where the floor came from, available, owed to running guests, left)."""
    ram_mb = ram_mb or guest_ram_mb()
    total_mb = total_mb or host_memory_mb()
    floor_from = 'given'
    if floor_mb is None:
        floor_mb, floor_from = guest_memory_floor()
    resident = vm_resident_mb() if resident is None else resident
    available = sample['memory_free_percent'] / 100 * total_mb
    owed = sum(max(0.0, ram_mb - r) for r in resident[:len(running)])
    owed += ram_mb * max(0, len(running) - len(resident))
    return ram_mb, total_mb, floor_mb, floor_from, available, owed, available - owed - ram_mb


def memory_refusal(sample, running, resident=None, total_mb=None, ram_mb=None, floor_mb=None,
                   processes=None, zone_text=None):
    """'' when one more guest fits, else why not. `sample` is reserve's (memory_free_percent is
    the kernel's own available level); `running` the running guests; `resident` the MB each VM
    process already holds. A running guest with no visible process is owed its whole RAM.
    `floor_mb` None reads the machine's setting; `zone_text` None reads `zprint` (reserve.py)."""
    ram_mb, total_mb, floor_mb, floor_from, available, owed, left = _accounting(
        sample, running, resident, total_mb, ram_mb, floor_mb)
    if left >= floor_mb:
        return ''
    text = (f'a guest needs {ram_mb} MB: {available:.0f} MB available, {owed:.0f} MB still owed to '
            f'{len(running)} running guest(s), would leave {left:.0f} MB (floor {floor_mb} MB, {floor_from}); '
            f'{floor_mb - left:.0f} MB more must be free')
    # The kernel's own leak first, when it is a large share: no app closed gives that back.
    leak = reserve.kernel_leak_note(total_mb, zone_text)
    if leak:
        text += '; ' + leak
    apps = top_apps(process_memory_mb() if processes is None else processes)
    if apps:
        text += '; largest apps by memory: ' + ', '.join(f'{name} {mb:.0f} MB' for name, mb in apps)
    return text


def guest_fit(sample=None, running=None, resident=None, total_mb=None, ram_mb=None, floor_mb=None,
              processes=None, zone_text=None):
    """Would one more guest fit NOW? {'fits', 'reason', 'waitable', 'running', 'needed_mb',
    'freeable_mb'}, from one memory reading (no CPU sample, no slot touched).

    `waitable` is True when what processes hold could cover the shortfall: running guests
    (their RAM comes back when they end), other agents' builds and checks that end within
    minutes, or apps somebody can close. Then the slot wait is worth having, and the caller says
    so at once. It is False only when the shortfall is more than every process on the machine holds
    together: the kernel holds it (reserve.py LEAK_ZONE), and only a restart returns it."""
    sample = sample or reserve.memory_sample()
    running = running_guests() if running is None else running
    resident = vm_resident_mb() if resident is None else resident
    total_mb = total_mb or host_memory_mb()
    processes = process_memory_mb() if processes is None else processes
    reason = memory_refusal(sample, running, resident, total_mb, ram_mb, floor_mb, processes, zone_text)
    if not reason:
        return {'fits': True, 'reason': '', 'waitable': False, 'running': list(running),
                'needed_mb': 0, 'freeable_mb': 0}
    _, _, floor_mb, _, _, owed, left = _accounting(sample, running, resident, total_mb, ram_mb, floor_mb)
    needed = floor_mb - left
    freeable = owed + sum(resident[:len(running)]) + sum(mb for _, mb in processes)
    return {'fits': False, 'reason': reason, 'waitable': needed <= freeable, 'running': list(running),
            'needed_mb': round(needed), 'freeable_mb': round(freeable)}


def root_dir(root=None):
    return reserve.testvm_root(root)


def slot_count():
    raw = os.environ.get('TESTVM_SLOTS', str(MAX_SLOTS))
    try:
        n = int(raw)
    except ValueError:
        raise ValueError(f'TESTVM_SLOTS must be a whole number from 1 to {MAX_SLOTS}, not {raw!r}') from None
    if not 1 <= n <= MAX_SLOTS:
        raise ValueError(f'TESTVM_SLOTS must be from 1 to {MAX_SLOTS} (Apple runs at most two macOS guests '
                         f'per host), not {n}')
    return n


def slot_paths(root=None, count=None):
    root = root_dir(root)
    return [root / name for name in SLOT_FILES[:count or slot_count()]]


def running_guests():
    """Names of the running local clones (never the base image), from tart's own list."""
    base = os.environ.get('TESTVM_BASE_VM', 'richos-base')
    listing = subprocess.run(['bash', '-c', '. "$1/lib.sh"; preflight_tart >/dev/null; tart list --format json',
                              'slots', str(HERE)], capture_output=True, text=True, timeout=30)
    if listing.returncode:
        raise BlockingIOError('tart could not say which guests are running; admission refused: '
                              + listing.stderr.strip()[-300:])
    return [r.get('Name') for r in json.loads(listing.stdout)
            if r.get('Source') == 'local' and r.get('Running') and r.get('Name') != base]


def _owned_dir(path):
    parent = path.parent
    parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if parent.is_symlink() or parent.stat().st_uid != os.getuid():
        raise ValueError('slot directory must be owned by the current user and not a symlink: ' + str(parent))


def _try(path):
    """The open, flocked file for `path`, or None when somebody else holds it."""
    _owned_dir(path)
    handle = path.open('a+')
    try:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        handle.close()
        return None
    return handle


def holder(path):
    """What a slot's holder wrote about itself, or {} (a free slot, or an older checkout's
    run-walk.py, which holds guest.lock without writing anything)."""
    try:
        return json.loads(path.read_text() or '{}')
    except (OSError, ValueError):
        return {}


def is_held(path):
    """True when some process holds the flock on `path`. The probe lock is dropped at once."""
    if not path.exists():
        return False
    with path.open('a') as probe:
        try:
            fcntl.flock(probe, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return True
        fcntl.flock(probe, fcntl.LOCK_UN)
        return False


def _write(handle, record):
    """Replace the slot file's holder record (None clears it). Only its flock holder writes."""
    handle.seek(0)
    handle.truncate()
    if record:
        handle.write(json.dumps(record) + '\n')
    handle.flush()


def own_holder(paths, owner=None):
    """(pid, slot name) of a slot held by a run of the SAME owner as this caller, else None. The owner is
    $RICHOS_AGENT_OWNER (one agent's name for itself); no owner, no match. A caller that queues
    behind its own earlier walk can only wait for itself."""
    owner = os.environ.get('RICHOS_AGENT_OWNER', '') if owner is None else owner
    if not owner:
        return None
    for path in paths:
        if is_held(path):
            h = holder(path)
            if h.get('owner') == owner and h.get('pid') and h['pid'] != os.getpid():
                return h['pid'], path.name
    return None


def describe(path):
    """One slot, in words: free, admitting, running (whose, for how long), or an older checkout's."""
    if not is_held(path):
        return f'{path.name}: free'
    h = holder(path)
    if not h.get('pid'):
        return f'{path.name}: held by an older checkout (no holder record)'
    minutes = (time.time() - h.get('since', time.time())) / 60
    doing = 'admitting' if h.get('state') == 'admitting' else 'running'
    return f"{path.name}: {doing}, pid {h['pid']} for {minutes:.1f} min ({h.get('purpose') or 'a run'})"


def describe_holders(paths):
    return '; '.join(describe(p) for p in paths if is_held(p)) or 'none'


class GuestNotStopped(RuntimeError):
    """A guest recorded against a slot survived its stop: the slot is released, the guest may still hold RAM."""

    def __init__(self, survivors):
        self.survivors = survivors
        super().__init__('guest(s) still recorded against the released slot after a failed stop: '
                         + ', '.join(f'{n} ({why})' for n, why in survivors)
                         + f'. Stop by hand: {HERE}/stop.sh <name>')


def _stop_leftovers(path, root):
    """Stop every guest whose run state says it booted under `path`: the slot's run is over.
    Returns the names stopped; raises GuestNotStopped, after trying every guest, for any whose
    stop.sh failed or timed out (a surviving guest is not a finished cleanup)."""
    run = root / 'run'
    stopped, failed = [], []
    if not run.is_dir():
        return stopped
    for state in sorted(run.iterdir()):
        try:
            recorded = (state / 'slot').read_text().strip()
        except OSError:
            continue
        if recorded != str(path):
            continue
        _say(f'slot {path.name} released with guest {state.name} still recorded against it; '
             f'stopping it (a guest never outlives its slot)')
        # SIGPIPE stays ignored in stop.sh (run-walk.py says why): a dead reader of our stderr
        # must not end a cleanup halfway.
        try:
            done = subprocess.run([str(HERE / 'stop.sh'), state.name], timeout=300, check=False, restore_signals=False)
            code = getattr(done, 'returncode', 0)
            if code:
                failed.append((state.name, f'stop.sh exited {code}'))
                continue
        except subprocess.TimeoutExpired:
            failed.append((state.name, 'stop.sh timed out after 300s'))
            continue
        stopped.append(state.name)
    if failed:
        raise GuestNotStopped(failed)
    return stopped


@contextmanager
def guest_slot(root=None, wait_seconds=0, purpose='', max_cpu=reserve.DEFAULT_MAX_CPU,
               guests=None, admit=None, memory=None, clock=time.monotonic, sleep=time.sleep):
    """Hold one guest slot for exactly the body of the `with`. Yields the slot's path.

    `guests`, `admit` and `memory` are replaceable so the suite can drive every refusal offline."""
    guests = guests or running_guests
    memory = memory or memory_refusal
    if admit is None:
        def admit():
            return reserve.cpu_admission(max_cpu, 0)
    if not 0 <= wait_seconds <= reserve.MAX_WAIT_SECONDS:
        raise ValueError(f'admission wait must be between 0 and {reserve.MAX_WAIT_SECONDS} seconds')
    root = root_dir(root)
    count = slot_count()
    paths = slot_paths(root, count)
    started = clock()
    slept = 0.0
    last_reason = None
    recorded = None
    nightly = os.environ.get(NIGHTLY_WAIT_ENV, '')
    announced = False
    try:
        while True:
            handle = path = None
            pause = SLOT_POLL_SECONDS
            # Admission first, holding nothing: a caller being sampled for CPU or memory, or
            # refused, never holds a slot another walk could have run in. A slot is taken only
            # once the run is admitted. The probe below holds nothing and spends no sample.
            if all(is_held(candidate) for candidate in paths):
                mine = own_holder(paths)
                if mine:
                    raise BlockingIOError(f'guest slot refused: your own run (pid {mine[0]}, {mine[1]}) holds it, '
                                          f'same owner; end that run first, it would only wait for itself')
                reason = f'every slot is executing a run ({describe_holders(paths)})'
            else:
                try:
                    busy = guests()
                    if len(busy) >= count:
                        reason = f'{len(busy)} guest(s) already running ({", ".join(busy)})'
                        pause = CLONE_POLL_SECONDS
                    else:
                        sample = admit()
                        reason = memory(sample, busy)
                        if not reason:
                            for candidate in paths:
                                handle = _try(candidate)
                                if handle:
                                    path = candidate
                                    break
                            if handle:
                                break
                            # Admitted, but every slot was taken while this caller was sampled.
                            reason = f'every slot is executing a run ({describe_holders(paths)})'
                        else:
                            pause = reserve.MIN_RETRY_SECONDS
                except BlockingIOError as refused:
                    reason = str(refused)
                    pause = reserve.MIN_RETRY_SECONDS
            elapsed = clock() - started
            remaining = wait_seconds - elapsed
            if remaining <= 0:
                raise BlockingIOError(f'guest slot refused after {elapsed:.0f}s: {reason}')
            if reason != last_reason:
                _say(f'slot waiting: {reason}; up to {remaining:.0f}s more')
                last_reason = reason
            recorded = _record_wait(recorded, reason)
            if nightly and not announced:
                # From the first refusal: every other admission now waits behind this one.
                try:
                    reserve.write_nightly_guest_wait(nightly, root)
                    _say(f'slot waiting as the nightly\'s {nightly}: new test and proof admissions are '
                         'held until this run has its guest (reserve.py)')
                except (OSError, ValueError) as exc:
                    _say(f'nightly slot wait not recorded, other admissions are not held: {exc}')
                announced = True
            sleep(min(pause, remaining))
            slept += min(pause, remaining)
    finally:
        if announced:
            reserve.clear_nightly_guest_wait(root)
        if recorded is not None:
            recorded.close()
    waited = clock() - started
    _write(handle, {'pid': os.getpid(), 'since': time.time(), 'purpose': purpose, 'slot': path.name,
                    'state': 'running', 'owner': os.environ.get('RICHOS_AGENT_OWNER', '')})
    previous = os.environ.get(ENV)
    os.environ[ENV] = str(path)
    # `waited` is time spent waiting for a slot or for admission; the checks themselves (one
    # `tart list`, one 1-second CPU sample) are reported apart, so "admitted at once" reads 0.
    _say(f'slot held: {path} (waited {slept:.0f}s; admission checks {waited - slept:.1f}s; '
         f"{reserve.describe(sample) if isinstance(sample, dict) and 'swap_used_mb' in sample else 'admitted'})")
    try:
        yield path
    finally:
        try:
            _stop_leftovers(path, root)
        finally:
            if previous is None:
                os.environ.pop(ENV, None)
            else:
                os.environ[ENV] = previous
            _write(handle, None)
            fcntl.flock(handle, fcntl.LOCK_UN)
            handle.close()
            _say(f'slot released: {path.name} after {clock() - started - waited:.0f}s')


def _ancestors():
    table = subprocess.run(['ps', '-A', '-o', 'pid=,ppid='], capture_output=True, text=True, timeout=10).stdout
    parent = {}
    for line in table.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0].isdigit() and parts[1].isdigit():
            parent[int(parts[0])] = int(parts[1])
    chain, pid = [], os.getppid()
    while pid > 1 and pid not in chain:
        chain.append(pid)
        pid = parent.get(pid, 0)
    return chain


def check(root=None):
    """'' when the caller runs inside a slot held by one of its ancestors, else why not."""
    named = os.environ.get(ENV, '')
    paths = slot_paths(root)
    if not named:
        return 'no guest slot is held for this run'
    path = Path(named)
    if path not in paths:
        return f'{ENV}={named} is not one of this root\'s slots ({", ".join(str(p) for p in paths)})'
    if not is_held(path):
        return f'slot {path.name} is not held'
    pid = holder(path).get('pid')
    if not pid or pid not in _ancestors():
        return f'slot {path.name} is held by another run (pid {pid}), not by this one'
    return ''


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest='verb', required=True)
    sub.add_parser('status', help='each slot: free, or who holds it and for how long')
    sub.add_parser('fits', help='one JSON line: would one more guest fit in memory now (exit 0), or why '
                                'not (exit 75) and whether only running guests stand in the way')
    sub.add_parser('check', help='exit 0 and print the slot only when an ancestor of the caller holds the slot '
                                 'named in $' + ENV)
    r = sub.add_parser('run', help='hold one slot for exactly one command')
    r.add_argument('--wait', type=float, default=0, metavar='SECONDS',
                   help=f'wait for a free slot and for admission for at most SECONDS (0-{reserve.MAX_WAIT_SECONDS}; '
                        'default 0: refuse at once)')
    r.add_argument('--purpose', default='', help='one line for `status` to show')
    r.add_argument('command', nargs=argparse.REMAINDER)
    a = p.parse_args()
    if a.verb == 'status':
        for path in slot_paths():
            print(describe(path))
        return 0
    if a.verb == 'fits':
        report = guest_fit()
        print(json.dumps(report), flush=True)
        return 0 if report['fits'] else 75
    if a.verb == 'check':
        why = check()
        if why:
            print(why, file=sys.stderr)
            return 1
        print(os.environ[ENV])
        return 0
    command = a.command[1:] if a.command[:1] == ['--'] else a.command
    if not command:
        p.error('a command is required after --')
    with guest_slot(wait_seconds=a.wait, purpose=a.purpose or ' '.join(command)[:120]):
        child = subprocess.Popen(command, start_new_session=True)

        def stop(signum, frame):
            try:
                os.killpg(child.pid, signal.SIGTERM)
                child.wait(timeout=30)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()
            except ProcessLookupError:
                pass
            raise SystemExit(128 + signum)
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, stop)
        try:
            return child.wait()
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait()


if __name__ == '__main__':
    try:
        sys.exit(main())
    except BlockingIOError as exc:
        print(str(exc), file=sys.stderr)
        sys.exit(75)
    except ValueError as exc:
        print('slots.py: ' + str(exc), file=sys.stderr)
        sys.exit(2)
