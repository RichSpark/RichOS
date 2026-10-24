#!/usr/bin/env python3
"""Persistent, per-user CPU circuit breaker. This is not a kernel CPU quota.

Ownership is explicit PID + birth identity, then observed ancestry, retained across
reparenting. Names never authorize signals. Sample CPU *time deltas*, not ps's
smoothed %cpu. Never signal a registered session, an unrelated process or a reused PID.

THIS FILE IS THE CONTROLLER LAUNCHD RUNS, not the one beside it. `cpu_guard.py` on main
carries the verification controller that was rolled back on 2026-09-27; this is cpu_guard.py
as of 9154bf51 (the bytes restored at that rollback) plus the release-build window below.
`cpu_guard_live_deploy.py` puts it in place and takes it out again. A change to the
per-process rule is made in BOTH files until the verification controller is re-qualified.
"""
import argparse
import contextlib
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import plistlib
import pwd
import re
import shlex
import shutil
import signal
import subprocess
import sys
import time
from cpu_policy import DEFAULT_MAX_CPU, admission_open

import machine_settings as M
STATE = M.state('cpu-guard', 'RICHOS_CPU_GUARD_STATE')
INTERVAL = 2.0
WINDOW = 10.0
JOB_CORES = 3.0
# A RELEASE BUILD'S COMPILER IS NOT A RUNAWAY FOR USING THE CORES A COMPILE NEEDS.
# Identical to cpu_guard.py's rule: a process whose OBSERVED ANCESTRY reaches a root
# registered with this role (nightly-local.py registers its build step's own supervisor,
# by PID and birth, never by name) may stay above JOB_CORES for BUILD_WINDOW seconds
# instead of WINDOW; longer than that and it is stopped exactly as before.
# Either way the stop needs a host that is out of room (DEFAULT_MAX_CPU for WINDOW seconds).
BUILD_ROLE = 'release-build'
BUILD_WINDOW = 600.0
# A NATIVE BUILD IS JUDGED BY THE CORES IT WAS GRANTED (2026-10-02). Identical to
# cpu_guard.py's rule. native-work.py admits a build with a core grant and registers its own
# process with this role, the grant (`cores`) and a `group` (its workspace); a Gradle daemon
# it keeps warm for that workspace is registered the same way. Every owned process whose
# CURRENT parent chain reaches such a root counts toward its group, and while the group's
# total stays within its grant no process in it is held to JOB_CORES: a Gradle JVM given
# four processors ran at 6.08 cores and was stopped here (2026-10-02 00:21Z, pid 3022)
# although its build was inside what admission had granted it. A group above its grant,
# and anything outside a group, is judged exactly as before.
NATIVE_ROLE = 'native-build'
LABEL = 'com.richos.cpu-guard'
IOS_FIRST_BOOT_SECONDS = 180
IOS_WARM_BOOT_SECONDS = 120


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + '.%s.new' % os.getpid())
    tmp.write_text(json.dumps(value))
    os.chmod(tmp, 0o600)
    os.replace(tmp, path)


def read_json(path, default=None):
    try:
        return json.loads(Path(path).read_text())
    except FileNotFoundError:
        return default


def seconds(value):
    days, _, rest = value.partition('-')
    total = int(days) * 86400 if rest else 0
    parts = (rest or days).split(':')
    for i, part in enumerate(reversed(parts)):
        total += float(part) * 60 ** i
    return total


def processes():
    result = subprocess.run(['ps', '-ax', '-o', 'uid=,pid=,ppid=,time=,lstart=,comm='],
                            capture_output=True, text=True, check=True, timeout=5,
                            env={**os.environ, 'LC_ALL': 'C', 'TZ': 'UTC0'})
    rows = {}
    for line in result.stdout.splitlines():
        f = line.split(None, 9)
        if len(f) != 10 or int(f[0]) != os.getuid():
            continue
        rows[int(f[1])] = dict(parent=int(f[2]), cpu=seconds(f[3]),
                              birth=' '.join(f[4:9]), name=f[9])
    return rows


def grant_fields(role, grant):
    """The grant a NATIVE_ROLE root carries: {'cores': > 0, 'group': non-empty}. Shared shape
    with cpu_guard.py; any other role carries none."""
    if role != NATIVE_ROLE:
        if grant:
            raise ValueError('only a %s root carries a core grant' % NATIVE_ROLE)
        return {}
    try:
        cores, group = float(grant['cores']), str(grant['group'])
    except (TypeError, KeyError, ValueError):
        raise ValueError('a %s root needs a grant with cores and group' % NATIVE_ROLE)
    if not cores > 0 or cores != cores or not group:
        raise ValueError('a %s grant needs positive cores and a group' % NATIVE_ROLE)
    return dict(cores=cores, group=group)


def register(pid, label, role='session', grant=None):
    rows = processes()
    if pid not in rows:
        raise ValueError('owner process is not alive or not owned by this user')
    record = dict(pid=pid, birth=rows[pid]['birth'], label=label, role=role, **grant_fields(role, grant))
    write_json(STATE / 'roots' / ('%s.json' % pid), record)
    return record


def grant_groups(owned, rows, rates, roots):
    """{group: {'cores': grant, 'used': cores now, 'members': [pid]}} for every registered
    native build. A process belongs to the nearest NATIVE_ROLE root on its CURRENT parent
    chain (a process that left the tree is no longer the build's). Shared with cpu_guard.py."""
    grants = {p: r for p, r in roots.items() if r.get('role') == NATIVE_ROLE and r.get('cores', 0) > 0}
    groups = {}
    if not grants:
        return groups
    for pid in owned:
        seen, at = set(), pid
        while at in rows and at not in seen and at not in grants:
            seen.add(at)
            at = rows[at]['parent']
        if at not in grants:
            continue
        name = grants[at].get('group') or 'root:%s' % at
        group = groups.setdefault(name, {'cores': 0.0, 'used': 0.0, 'members': []})
        group['cores'] = max(group['cores'], grants[at]['cores'])
        group['used'] += rates.get(pid, 0)
        group['members'].append(pid)
    return groups


def note(message, **details):
    """Record an event in the history without raising it as the latest alert."""
    record = dict(at=time.time(), message=message, **details)
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / 'events.jsonl').open('a') as out:
        out.write(json.dumps(record) + '\n')
    return record


def event(message, **details):
    record = note(message, **details)
    write_json(STATE / 'alert.json', record)


HOST_PRESSURE = 'Host CPU is high; unregistered processes reported without signals'


def host_pressure_note(busy, processes):
    """High host CPU with nothing of ours stopped is an OBSERVATION: it goes to the event
    history only, never to alert.json, so no hook shows it to the lead or the person. Its
    list of unowned processes does not say who consumed the host (orchestration
    reliability R6, richos-hq docs/prds/2026-10-23-orchestration-reliability.md); what
    changes the work (a workload or simulator stopped, admission recovered, a failure,
    an unhealthy guard) is still raised through event()."""
    return note(HOST_PRESSURE, host_busy=round(busy, 1), processes=processes)


def shown_alert():
    """alert.json as the hooks and `notice` show it. The host-pressure observation an earlier
    controller wrote there is never shown (2026-10-24: a session that started at 13:32:50Z was
    shown one written at 12:38:01Z); it stays in alert.json and events.jsonl."""
    alert = read_json(STATE / 'alert.json')
    return None if alert and alert.get('message') == HOST_PRESSURE else alert


def healthy():
    row = read_json(STATE / 'heartbeat.json', {})
    devices = read_json(STATE / 'devices-heartbeat.json', {})
    return (time.time() - row.get('at', 0) < 12 and row.get('ok') is True
            and time.time() - devices.get('at', 0) < 25 and devices.get('ok') is True)


def ios_block():
    return read_json(STATE / 'ios-block.json')


def ios_records():
    registry = Path(os.environ.get('RICHOS_TEST_DEVICES_DIR', str(Path.home() / '.claude/state/test-devices')))
    return [rec for path in registry.glob('*.json')
            if (rec := read_json(path, {})).get('kind') == 'ios-simulator']


def registered_ios():
    return bool(ios_records())


@contextlib.contextmanager
def ios_policy_lock():
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / 'ios-policy.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def block_ios(reason, automatic=False):
    with ios_policy_lock():
        old = ios_block()
        # An operator stop may strengthen an automatic cooldown, never the reverse.
        if old and (automatic or old.get('mode') != 'cooldown'):
            return
        now = time.time()
        record = dict(at=now, reason=reason, mode='manual')
        if automatic:
            incidents = [t for t in read_json(STATE / 'ios-incidents.json', []) if now - t < 3600]
            incidents.append(now)
            delay = (30, 120, 600)[min(len(incidents) - 1, 2)]
            write_json(STATE / 'ios-incidents.json', incidents)
            record.update(mode='cooldown', retry_after=now + delay, incidents_last_hour=len(incidents))
        write_json(STATE / 'ios-block.json', record)
        event('Local iOS simulator stopped', incident=record)


def ios_refusal():
    """Read-only admission explanation, including old incident files."""
    blocked = ios_block()
    if not blocked:
        return None
    reason = blocked['reason']
    if blocked.get('mode') != 'cooldown':
        return reason + '. Explicit recovery required: cpu_guard.py recover-ios REASON'
    remaining = blocked['retry_after'] - time.time()
    if remaining > 0:
        return reason + '. Automatic cooldown: %s seconds remaining' % int(remaining + 1)
    if not healthy() or not admission_open(read_json(STATE / 'heartbeat.json', {}).get('host_busy', float('nan'))):
        return reason + '. Cooldown elapsed; waiting for healthy monitoring and CPU headroom'
    if registered_ios():
        return reason + '. Cooldown elapsed; waiting for exact-device cleanup'
    return None


def clear_ios_block(reason):
    blocked = ios_block()
    if blocked:
        write_json(STATE / 'ios-last-incident.json', blocked)
        (STATE / 'ios-block.json').unlink()
        event('Local iOS simulator admission recovered; no device booted', reason=reason)


def recover_ios(reason):
    if not reason.strip():
        raise ValueError('recovery requires a reason')
    with ios_policy_lock():
        if not healthy() or not admission_open(read_json(STATE / 'heartbeat.json', {}).get('host_busy', float('nan'))):
            raise RuntimeError('recovery requires healthy monitoring and current CPU headroom')
        if registered_ios():
            raise RuntimeError('finish exact-device cleanup before recovery')
        clear_ios_block(reason)


def require_ios():
    with ios_policy_lock():
        reason = ios_refusal()
        if reason:
            raise RuntimeError('Local iOS simulator admission is closed: ' + reason)
        clear_ios_block('Cooldown elapsed with healthy monitoring, CPU headroom and completed cleanup')


class IOSWatch:
    """Busy CPU alone is not a failed simulator. No process enumeration here."""
    def __init__(self):
        self.distress_since = None

    def sample(self, busy, now, wall):
        records = ios_records()
        for rec in records:
            boot = rec.get('boot', {})
            if boot.get('phase') == 'starting' and wall >= boot['deadline']:
                block_ios('Simulator startup exceeded its %ss deadline: %s' %
                          (boot['deadline'] - boot['started'], rec['id']), automatic=True)
                return
        heartbeat = read_json(STATE / 'heartbeat.json', {})
        degraded = heartbeat.get('ok') is not True or wall - heartbeat.get('at', 0) >= 12
        # The independent worker remains able to shed exact registered devices
        # when host pressure actually prevents the CPU sampler from functioning.
        if records and busy >= 85 and degraded:
            self.distress_since = now if self.distress_since is None else self.distress_since
            if now - self.distress_since >= WINDOW:
                block_ios('Host CPU >=85% with failed/stale process monitoring for >=10s', automatic=True)
        else:
            self.distress_since = None


def device_cycle(engine):
    """Independent of ps: an overloaded process table must not delay shutdown."""
    args = [sys.executable, '-B', str(Path(engine) / 'scripts/lib/testdevices.py'), 'expire-leases']
    if ios_block():
        args.append('--pressure')
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=18)
        if result.returncode:
            raise RuntimeError('exit %s: %s' % (result.returncode, (result.stderr or result.stdout)[-4000:]))
        write_json(STATE / 'devices-heartbeat.json', dict(at=time.time(), ok=True, pid=os.getpid()))
    except Exception as exc:
        event('Device lease collector failed', error=str(exc))
        write_json(STATE / 'devices-heartbeat.json', dict(at=time.time(), ok=False, error=str(exc)))


def watch_devices(engine):
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / 'devices.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        before, watcher = host_ticks(), IOSWatch()
        while True:
            # Mach host counters do not enumerate processes. Pressure detection
            # remains available even if ps and the CPU worker are stalled.
            ticks = host_ticks()
            now = time.monotonic()
            if ticks is not None and before is not None:
                delta = [(b-a) % 2**32 for a,b in zip(before, ticks)]
                busy = 100 * (1 - delta[2]/sum(delta)) if sum(delta) else 0
                watcher.sample(busy, now, time.time())
            else:
                watcher.sample(0, now, time.time())
            before = ticks
            device_cycle(engine)
            time.sleep(INTERVAL)


def host_ticks():
    if sys.platform != 'darwin':
        return None
    library = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
    library.mach_host_self.restype = ctypes.c_uint
    ticks = (ctypes.c_uint * 4)()
    count = ctypes.c_uint(4)
    if library.host_statistics(library.mach_host_self(), 3, ctypes.byref(ticks), ctypes.byref(count)):
        raise RuntimeError('host CPU counters unavailable')
    return list(ticks)


class Watch:
    def __init__(self):
        self.owned = read_json(STATE / 'owned.json', {})
        self.previous = {}
        self.last = None
        self.over = {}
        self.pending = {}
        self.host_since = None
        self.pressure_since = None
        self.reported_at = 0
        self.unowned_rates = {}
        self.windows = {}
        self.grants = {}

    def grant_of(self, pid):
        """The grant group a process was judged in at the last sample, for the stop record."""
        for name, group in self.grants.items():
            if pid in group['members']:
                return dict(group=name, cores=group['cores'], used=round(group['used'], 2))
        return None

    @staticmethod
    def in_release_build(pid, rows, build_roots):
        """True when the process's CURRENT parent chain reaches a registered build root."""
        seen = set()
        while pid in rows and pid not in seen:
            if pid in build_roots:
                return True
            seen.add(pid)
            pid = rows[pid]['parent']
        return False

    def sample(self, rows, now, host_busy=0):
        roots = {}
        for path in (STATE / 'roots').glob('*.json'):
            rec = read_json(path)
            if rec and rows.get(rec['pid'], {}).get('birth') == rec['birth']:
                roots[rec['pid']] = rec
            elif rec:
                path.unlink(missing_ok=True)
        owned = {int(p): rec for p, rec in self.owned.items()
                 if rows.get(int(p), {}).get('birth') == rec['birth']}
        for pid, rec in roots.items():
            owned[pid] = dict(birth=rec['birth'], owner=rec['label'], root=pid)
        changed = True
        while changed:
            changed = False
            for pid, row in rows.items():
                if pid not in owned and row['parent'] in owned:
                    owned[pid] = {**owned[row['parent']], 'birth': row['birth']}
                    changed = True
        # Emulator launchers detach between samples. Their registry supplies the
        # exact generation; registry names or executable names alone do not.
        registry = Path(os.environ.get('RICHOS_TEST_DEVICES_DIR', str(Path.home() / '.claude/state/test-devices')))
        for path in registry.glob('*.json'):
            rec = read_json(path, {})
            gen = rec.get('generation') or {}
            pid = gen.get('pid')
            if rec.get('kind') == 'android-emulator' and pid in rows and rows[pid]['birth'] == gen.get('start'):
                owned[pid] = dict(birth=gen['start'], owner=rec.get('id'), root=pid)
        self.owned = {str(p): rec for p, rec in owned.items()}
        elapsed = now - self.last if self.last is not None else 0
        rates = {}
        for pid, rec in owned.items():
            old = self.previous.get(pid)
            if old and old['birth'] == rec['birth'] and elapsed > 0:
                rates[pid] = max(0, rows[pid]['cpu'] - old['cpu']) / elapsed
        self.unowned_rates = {pid: max(0, row['cpu'] - self.previous[pid]['cpu']) / elapsed
                              for pid, row in rows.items() if pid not in owned and elapsed > 0
                              and pid in self.previous and self.previous[pid]['birth'] == row['birth']}
        self.previous, self.last = rows, now
        # Sessions stay alive, and so does a release build's own supervisor. A
        # registered workload root may itself be stopped.
        protected = {p for p, r in roots.items() if r['role'] in ('session', BUILD_ROLE)} | {os.getpid()}
        build_roots = {p for p, r in roots.items() if r['role'] == BUILD_ROLE}
        allowed = set(owned) - protected - set(self.pending)
        # Host pressure closes admission, not already admitted work. Killing
        # the largest process here repeatedly killed sub-core land checks and
        # capped Gradle builds while unrelated work saturated the host.
        candidates = []
        self.windows = {}
        # A core count alone is not a runaway: the machine may have plenty idle
        # (P5-22). A process is stopped only while it is a contributor to a host
        # that is itself out of room, the same 80 percent that closes admission,
        # held for WINDOW seconds. A host that is busy for another reason never
        # stops a process that is not over JOB_CORES (above), and a sub-window
        # spike never stops a long-running compile.
        if host_busy >= DEFAULT_MAX_CPU:
            self.pressure_since = now if self.pressure_since is None else self.pressure_since
        else:
            self.pressure_since = None
        pressured = self.pressure_since is not None and now - self.pressure_since >= WINDOW
        # A registered native build within its grant is not held to JOB_CORES (NATIVE_ROLE).
        self.grants = grant_groups(owned, rows, rates, roots)
        within = {p for g in self.grants.values() if g['used'] <= g['cores'] for p in g['members']}
        for pid in allowed:
            rate = rates.get(pid, 0)
            window = BUILD_WINDOW if self.in_release_build(pid, rows, build_roots) else WINDOW
            self.windows[pid] = window
            if rate > JOB_CORES and pid not in within:
                self.over.setdefault((pid, rows[pid]['birth']), now)
            else:
                self.over.pop((pid, rows[pid]['birth']), None)
            since = self.over.get((pid, rows[pid]['birth']))
            if since is not None and now - since >= window and pressured:
                candidates.append(pid)
        self.over = {k: v for k, v in self.over.items() if k[0] in allowed and rows[k[0]]['birth'] == k[1]}
        return sorted(set(candidates), key=lambda p: rates.get(p, 0), reverse=True), rates, protected

    def policy_targets(self, rows, protected):
        targets = set()
        registry = Path(os.environ.get('RICHOS_TEST_DEVICES_DIR', str(Path.home() / '.claude/state/test-devices')))
        if ios_block():
            for path in registry.glob('*.json'):
                rec = read_json(path, {})
                if rec.get('kind') != 'ios-simulator':
                    continue
                for owner in rec.get('owners', [rec.get('owner', {})]):
                    pid = owner.get('pid')
                    if pid in rows and pid not in protected and rows[pid]['birth'] == owner.get('start'):
                        # Only an observed agent workload, never a personal session.
                        if str(pid) in self.owned:
                            targets.add(pid)
        for key in self.owned:
            pid = int(key)
            if pid in protected or pid not in rows:
                continue
            if os.path.basename(rows[pid]['name']) == 'simctl':
                args = subprocess.run(['ps', '-ww', '-o', 'args=', '-p', str(pid)],
                                      capture_output=True, text=True, timeout=2)
                try:
                    words = shlex.split(args.stdout)
                except ValueError:
                    continue
                if 'diagnose' in words:
                    targets.add(pid)
        return targets

    def stop(self, pid, rows, protected, rates, reason='process exceeded sustained per-process CPU limit'):
        # Expand only observed descendants. Never killpg on a potentially shared group.
        targets = {pid}
        while True:
            more = {p for p, row in rows.items() if row['parent'] in targets}
            if more <= targets:
                break
            targets |= more
        targets -= protected
        fresh = processes()
        signalled = {}
        for p in targets:
            if fresh.get(p, {}).get('birth') != rows[p]['birth']:
                continue
            try:
                os.kill(p, signal.SIGTERM)
                signalled[p] = rows[p]['birth']
                self.pending[p] = (rows[p]['birth'], time.monotonic() + 3)
            except ProcessLookupError:
                pass
        if not signalled:
            return
        event('CPU circuit breaker stopped an owned workload', pid=pid,
              reason=reason,
              executable=rows[pid]['name'], owner=self.owned[str(pid)]['owner'],
              cores=round(rates.get(pid, 0), 2), sustained_seconds=self.windows.get(pid, WINDOW),
              grant=self.grant_of(pid),
              signalled=signalled)

    def reap(self, rows, now):
        for pid, row in rows.items():
            if pid not in self.pending and row['parent'] in self.pending:
                self.pending[pid] = (row['birth'], self.pending[row['parent']][1])
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        for pid, (birth, deadline) in list(self.pending.items()):
            if rows.get(pid, {}).get('birth') != birth:
                del self.pending[pid]
            elif now >= deadline:
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                del self.pending[pid]


def watch(engine):
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / 'watch.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        watcher = Watch()
        # Which controller is running, read from its own bytes, so a deploy can prove the
        # restarted watch is the file it put in place (cpu_guard_live_deploy.py).
        controller = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        ticks_before = host_ticks()
        while True:
            try:
                rows = processes()
                now = time.monotonic()
                ticks = host_ticks()
                busy = 0
                if ticks is not None and ticks_before is not None:
                    delta = [(b-a) % 2**32 for a,b in zip(ticks_before, ticks)]
                    busy = 100 * (1 - delta[2]/sum(delta)) if sum(delta) else 0
                ticks_before = ticks
                candidates, rates, protected = watcher.sample(rows, now, busy)
                watcher.reap(rows, now)
                for pid in watcher.policy_targets(rows, protected):
                    watcher.stop(pid, rows, protected, rates, 'local simulator incident containment')
                if candidates:
                    watcher.stop(candidates[0], rows, protected, rates)
                if busy >= 85:
                    watcher.host_since = watcher.host_since if watcher.host_since is not None else now
                    if not candidates and now-watcher.host_since >= WINDOW and now-watcher.reported_at >= 300:
                        top = sorted(watcher.unowned_rates, key=watcher.unowned_rates.get, reverse=True)[:3]
                        if top:
                            host_pressure_note(busy, [dict(pid=p, executable=rows[p]['name'],
                                               cores=round(watcher.unowned_rates[p], 2)) for p in top])
                            watcher.reported_at = now
                else:
                    watcher.host_since = None
                write_json(STATE / 'owned.json', watcher.owned)
                write_json(STATE / 'heartbeat.json', dict(at=time.time(), ok=True,
                           pid=os.getpid(), owned=len(watcher.owned), host_busy=round(busy, 1),
                           admission_open=admission_open(busy), admission_limit=DEFAULT_MAX_CPU,
                           controller=controller, grants={name: dict(cores=g['cores'], used=round(g['used'], 2),
                                                                     processes=len(g['members']))
                                                          for name, g in watcher.grants.items()}))
            except Exception as exc:
                event('CPU watchdog sampling failed', error=str(exc))
                write_json(STATE / 'heartbeat.json', dict(at=time.time(), ok=False, error=str(exc)))
            time.sleep(INTERVAL)


def install(engine):
    M.validate(STATE)
    runtime = STATE / 'runtime'
    runtime.mkdir(parents=True, exist_ok=True)
    shutil.copy2(Path(__file__).with_name('cpu_policy.py'), runtime / 'cpu_policy.py')
    shutil.copy2(Path(__file__).with_name('machine_settings.py'), runtime / 'machine_settings.py')
    target = runtime / 'cpu_guard.py'
    shutil.copy2(__file__, target)
    domain = 'gui/%s' % os.getuid()
    started = time.time()
    for label, action in [(LABEL + '.devices', 'watch-devices'), (LABEL, 'watch')]:
        agent = Path.home() / 'Library/LaunchAgents' / (label + '.plist')
        config = dict(Label=label, ProgramArguments=['/usr/bin/python3', '-B', str(target), action, str(Path(engine).resolve())],
                      RunAtLoad=True, KeepAlive=True, ThrottleInterval=5, ProcessType='Interactive',
                      EnvironmentVariables={'RICHOS_CPU_GUARD_STATE': str(STATE), 'LC_ALL': 'C'},
                      StandardOutPath='/dev/null', StandardErrorPath='/dev/null')
        agent.parent.mkdir(parents=True, exist_ok=True)
        with agent.open('wb') as out:
            plistlib.dump(config, out)
        subprocess.run(['launchctl', 'bootout', domain + '/' + label], capture_output=True)
        for attempt in range(20):
            boot = subprocess.run(['launchctl', 'bootstrap', domain, str(agent)], capture_output=True, text=True)
            if boot.returncode == 0:
                break
            time.sleep(.25)
        else:
            raise RuntimeError('launchd bootstrap failed: ' + boot.stderr.strip())
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if healthy() and read_json(STATE/'heartbeat.json', {}).get('at', 0) >= started:
            break
        time.sleep(.25)
    else:
        raise RuntimeError('watchdog installed but did not produce a healthy heartbeat; inspect launchctl print')
    # A standalone user hook also covers cached/older engine plugins. The
    # canonical engine dispatcher is deliberately not a second registration.
    settings = Path.home() / '.claude/settings.json'
    original = settings.read_bytes() if settings.exists() else b'{}'
    backup = STATE / ('claude-settings-before-' + time.strftime('%Y%m%dT%H%M%S') + '.json')
    backup.write_bytes(original)
    backup.chmod(0o600)
    data = json.loads(original)
    hooks = data.setdefault('hooks', {})
    for name, matcher, action in [('PreToolUse', 'Bash', 'hook'), ('SessionStart', None, 'notice-json'), ('Stop', None, 'notice-json')]:
        entries = hooks.setdefault(name, [])
        command = '/usr/bin/python3 -B ' + shlex.quote(str(target)) + ' ' + action
        if not any(h.get('command') == command for e in entries for h in e.get('hooks', [])):
            entry = {'hooks': [{'type': 'command', 'command': command, 'timeout': 10}]}
            if matcher: entry['matcher'] = matcher
            entries.append(entry)
    write_json(settings, data)
    print(str(agent))


def shell_text(command):
    # Data in a quoted heredoc is not shell syntax. Shell-fed heredocs are
    # checked separately; malformed quoting outside them remains a refusal.
    lines = command.splitlines(keepends=True)
    output, bodies, i = [], [], 0
    while i < len(lines):
        line = lines[i]
        output.append(line)
        matches = list(re.finditer(r"<<-?\s*(['\"]?)([A-Za-z_][A-Za-z_0-9]*)\1", line))
        i += 1
        for match in matches:
            body = []
            while i < len(lines) and lines[i].strip() != match.group(2):
                body.append(lines[i]); i += 1
            if i == len(lines):
                output.extend(body)
                break
            i += 1
            if re.search(r"(?:^|[;|&]\s*)(?:bash|sh|zsh)(?:\s|$)", line):
                bodies.append(''.join(body))
    return ''.join(output), bodies


def forbidden(command, cwd=None):
    """Conservative shell command-head check, not an arbitrary-code sandbox."""
    command, bodies = shell_text(command)
    for body in bodies:
        reason = forbidden(body, cwd)
        if reason: return reason
    try:
        lexer = shlex.shlex(command, posix=True, punctuation_chars=';&|()\n')
        lexer.whitespace = ' \t\r'
        lexer.whitespace_split = True
        words = list(lexer)
    except ValueError:
        return 'unparseable shell command'
    chunks, chunk = [], []
    for word in words + [';']:
        if word and all(c in ';&|()\n' for c in word):
            if chunk: chunks.append(chunk)
            chunk = []
        else:
            chunk.append(word)
    for part in chunks:
        while part and (('=' in part[0] and not part[0].startswith('/')) or part[0] in ('then', 'do', 'exec', 'command', 'env', 'nohup', 'time', '!')):
            part.pop(0)
        if not part: continue
        name = os.path.basename(part[0])
        if name == 'cd' and len(part) > 1:
            cwd = str(Path(cwd or os.getcwd()).joinpath(part[1]).resolve())
        entry = os.path.basename(part[1]) if name in ('bash', 'sh', 'zsh', 'python3', 'python') and len(part) > 1 else name
        if entry in ('proof-run.py', 'run-tests.sh', 'native-work.py', 'rios', 'randroid', 'simulator-tests.sh', 'native-ios-share.test.sh', 'native-ios-app.test.sh', 'native-ios-ui.test.sh'):
            executable = part[1] if entry != name else part[0]
            path = Path(cwd or os.getcwd()).joinpath(executable).resolve()
            for parent in path.parents:
                policy = parent / 'richos/engine/scripts/lib/testdevices.py'
                if policy.is_file():
                    if 'def acquire_ios(' not in policy.read_text() or not policy.with_name('cpu_policy.py').is_file():
                        return 'outdated native/proof entrypoint; update this checkout from main before running it'
                    break
        if ios_refusal():
            if entry in ('native-ios-app.test.sh', 'native-ios-ui.test.sh', 'native-ios-share.test.sh', 'simulator-tests.sh'):
                if not (entry == 'native-ios-ui.test.sh' and '--headless' in part):
                    return 'local iOS simulator suite (incident stop is active)'
            if entry == 'rios' and 'sim' in part and not any(p in part for p in ('stop', 'check-release')):
                return 'local iOS simulator (incident stop is active)'
        if name in ('bash', 'zsh', 'sh'):
            if '-c' in part:
                i = part.index('-c')
                if len(part) > i+1:
                    reason = forbidden(part[i+1], cwd)
                    if reason: return reason
            if len(part) > 1 and os.path.basename(part[1]) == 'gradlew': return 'gradlew'
        if name in ('nice', 'timeout'):
            rest = [p for p in part[1:] if not p.startswith('-') and not p.isdigit()]
            if rest:
                reason = forbidden(shlex.join(rest), cwd)
                if reason: return reason
        if name in ('gradle', 'gradlew', 'emulator') or name.startswith('qemu-system-'):
            if not any(p in part for p in ('--version', '-version', '--help', '-help', '-help-all', '--stop', '-list-avds')):
                return name
        if name == 'xcodebuild' and not any(p in part for p in ('-version', '-list', '-showsdks', '-showBuildSettings', '-help')):
            return name
        if name == 'swift' and len(part) > 1 and part[1] in ('build', 'test'): return 'swift ' + part[1]
        if name in ('xcrun', 'simctl') and (name == 'simctl' or 'simctl' in part):
            if 'boot' in part: return 'simctl boot'
            if 'diagnose' in part: return 'simctl diagnose (expensive host diagnostics)'
    return None


def hook():
    payload = json.load(sys.stdin)
    if payload.get('tool_name') != 'Bash':
        return 0
    # Fixture engine hooks must never enroll a real test runner or mutate real state.
    if os.environ.get('CLAUDE_CONFIG_DIR') and Path(os.environ['CLAUDE_CONFIG_DIR']).resolve() != (Path.home() / '.claude').resolve():
        return 0
    if Path.home().resolve() != Path(pwd.getpwuid(os.getuid()).pw_dir).resolve():
        return 0
    rows = processes()
    pid = os.getppid()
    for _ in range(30):
        row = rows.get(pid)
        if not row: break
        if os.path.basename(row['name']) == 'claude':
            register(pid, 'Claude session ' + str(payload.get('session_id', pid)))
            break
        pid = row['parent']
    reason = forbidden(payload.get('tool_input', {}).get('command', ''), payload.get('cwd'))
    if reason:
        if 'outdated' in reason:
            remedy = 'Update this checkout from main before running its native or proof tools.'
        elif 'diagnose' in reason:
            remedy = 'Automatic simulator diagnostic dumps are disabled after the overload incident.'
        elif ios_block() and 'simulator' in reason:
            remedy = ios_refusal() or 'Inspect cpu_guard.py status.'
        else:
            remedy = 'Use randroid, rios or native-work.py -- COMMAND so admission and cleanup apply.'
        print('CPU guard: %s is refused. %s' % (reason, remedy), file=sys.stderr)
        return 2
    alert = shown_alert()
    if alert:
        marker = STATE / 'notices' / (str(payload.get('session_id', 'unknown')).replace('/', '_') + '.json')
        previous = read_json(marker, {})
        if alert.get('at') != previous.get('at'):
            print(json.dumps({'hookSpecificOutput': {'hookEventName': 'PreToolUse',
                             'additionalContext': 'CPU GUARD ALERT: ' + json.dumps(alert)}}))
            write_json(marker, {'at': alert.get('at')})
    return 0


def notice_payload(payload):
    alert = shown_alert()
    message = ''
    if not healthy():
        message = 'CPU GUARD UNHEALTHY: native build and device admission is closed.'
    marker = STATE/'notices'/('visible-' + str(payload.get('session_id', 'unknown')).replace('/', '_') + '.json')
    seen = read_json(marker, {})
    if alert and seen.get('at') != alert.get('at'):
        message += ' CPU GUARD ALERT: ' + json.dumps(alert)
        write_json(marker, {'at':alert.get('at')})
    if not message:
        return None
    result = {'systemMessage':message.strip()}
    if payload.get('hook_event_name') == 'SessionStart':
        result['hookSpecificOutput'] = {'hookEventName':'SessionStart','additionalContext':message.strip()}
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['watch', 'watch-devices', 'block-ios', 'recover-ios', 'check-ios', 'install', 'register', 'hook', 'status', 'notice', 'notice-json'])
    parser.add_argument('args', nargs='*')
    a = parser.parse_args()
    if a.action == 'watch': return watch(a.args[0])
    if a.action == 'watch-devices': return watch_devices(a.args[0])
    if a.action == 'block-ios': return block_ios(' '.join(a.args) or 'Operator stopped local iOS simulators')
    if a.action == 'recover-ios': return recover_ios(' '.join(a.args))
    if a.action == 'check-ios': return require_ios()
    if a.action == 'install': return install(a.args[0])
    if a.action == 'register': print(json.dumps(register(int(a.args[0]), a.args[1], a.args[2] if len(a.args)>2 else 'session')))
    if a.action == 'hook': return hook()
    if a.action == 'status':
        print(json.dumps(dict(healthy=healthy(), heartbeat=read_json(STATE/'heartbeat.json'), devices=read_json(STATE/'devices-heartbeat.json'), ios_block=ios_block(), ios_admission_reason=ios_refusal(), ios_startups=[dict(id=r['id'], boot=r.get('boot')) for r in ios_records()], alert=read_json(STATE/'alert.json'))))
        return 0 if healthy() else 1
    if a.action == 'notice-json':
        result = notice_payload(json.load(sys.stdin))
        if result: print(json.dumps(result))
    if a.action == 'notice':
        if not healthy(): print('CPU GUARD UNHEALTHY: native build and device admission is closed.')
        alert = shown_alert()
        if alert: print('CPU GUARD ALERT: ' + json.dumps(alert))
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except Exception as exc:
        print('cpu-guard: ' + str(exc), file=sys.stderr)
        sys.exit(2)
