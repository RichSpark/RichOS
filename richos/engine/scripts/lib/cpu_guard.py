#!/usr/bin/env python3
"""Persistent, per-user CPU circuit breaker. This is not a kernel CPU quota.

Ownership is explicit PID + birth identity, then observed ancestry, retained across
reparenting. Names never authorize signals. Sample CPU *time deltas*, not ps's
smoothed %cpu. Never signal a registered session, an unrelated process or a reused PID.
"""
import argparse
import contextlib
import ctypes
import fcntl
import hashlib
import importlib.util
import json
import math
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
import uuid
import operator_fences
from cpu_policy import DEFAULT_MAX_CPU, admission_open, VerificationPressure
import machine_settings as M

def canonical_state():
    # The installed controller's actual-user anchor stays authoritative during
    # migration. A test HOME/selector must never hide active verification policy.
    home = Path(pwd.getpwuid(os.getuid()).pw_dir)
    agent = home / 'Library/LaunchAgents/com.richos.cpu-guard.plist'
    if agent.is_file():
        with agent.open('rb') as source:
            installed = plistlib.load(source).get('EnvironmentVariables', {}).get('RICHOS_CPU_GUARD_STATE')
        if installed:
            return M.validate(installed)
    return M.state('cpu-guard', 'RICHOS_CPU_GUARD_STATE', environment={'HOME': str(home)})


CANONICAL_STATE = canonical_state()
# Preserve the existing kernel lock inodes across rollout and older worktrees.
CANONICAL_WORKERS = Path(pwd.getpwuid(os.getuid()).pw_dir) / '.richos-nightly/worker-budget-v1'
CANONICAL_ENGINE_PASS = CANONICAL_STATE.parent / 'engine-pass-v1'


def installed_verification_policy():
    """Consult the machine location, never an override that could hide it."""
    if (CANONICAL_STATE / 'verification-enabled.json').exists():
        return True
    try:
        with (CANONICAL_STATE / 'verification-pressure.json').open() as source:
            return json.load(source).get('protocol') == 1
    except FileNotFoundError:
        return False


def governed_directory(path, canonical):
    """Private pools require an explicit fixture root and private HOME/config.

    This distinguishes a declared test seam from a silent production override.
    It is not an OS sandbox or an authorization boundary against arbitrary shell.
    """
    target, canonical = Path(path).resolve(), Path(canonical).resolve()
    if target == canonical or not installed_verification_policy():
        return target
    root = os.environ.get('RICHOS_VERIFICATION_FIXTURE_ROOT')
    if os.environ.get('RICHOS_VERIFICATION_MODE') != 'fixture' or not root:
        raise ValueError('managed verification refuses a private capacity/controller path; '
                         'tests need explicit fixture mode and private HOME/config: ' + str(target))
    root = Path(root).resolve()
    actual_home = Path(pwd.getpwuid(os.getuid()).pw_dir).resolve()
    home = Path(os.environ.get('HOME', str(actual_home))).resolve()
    config = Path(os.environ.get('CLAUDE_CONFIG_DIR', str(home / '.claude'))).resolve()
    if (not root.is_dir() or root == Path(root.anchor) or home == actual_home
            or not home.is_relative_to(root) or not config.is_relative_to(root)
            or not target.is_relative_to(root) or root == actual_home
            or canonical.is_relative_to(root)):
        raise ValueError('verification fixture override escapes its private root/HOME/config: ' + str(target))
    return target


STATE = governed_directory(M.state('cpu-guard', 'RICHOS_CPU_GUARD_STATE', default=CANONICAL_STATE), CANONICAL_STATE)
INTERVAL = 2.0
VERIFICATION_PROTOCOL = 1
CALIBRATION_SECONDS = 120
LAUNCHER_FILES = (
    'richos/app/scripts/proof-run.py', 'richos/app/scripts/lib/proof_evidence.py',
    'richos/app/scripts/run-tests.sh', 'richos/app/scripts/nightly-local.py',
    'richos/app/scripts/lib/simulator_budget.py', 'richos/app/scripts/testvm/reserve.py',
    'richos/engine/scripts/ci-shard.sh', 'richos/engine/scripts/run-all-tests.sh',
    'richos/engine/scripts/lib/ci-receipts.py', 'richos/engine/scripts/lib/engine_pass.py',
    'richos/engine/scripts/lib/worker_tokens.py', 'richos/engine/scripts/lib/proc_tree.py',
    'richos/engine/scripts/lib/cpu_guard.py', 'richos/engine/scripts/lib/cpu_policy.py',
    'richos/engine/scripts/lib/operator_fences.py', 'richos/engine/scripts/lib/native-work.py',
    'richos/engine/scripts/lib/testdevices.py', 'richos/engine/scripts/lib/mutation-harness.sh',
    'richos/mobile/native-ios/bin/rios', 'richos/mobile/native-android/bin/randroid',
    'richos/mobile/native-ios/Release/simulator-tests.sh',
)
WINDOW = 10.0
JOB_CORES = 3.0
# A RELEASE BUILD'S COMPILER IS NOT A RUNAWAY FOR USING THE CORES A COMPILE NEEDS.
# rustc's LLVM phase for the app binary runs one thread per jobserver token, and on
# 2026-09-28 (run 20260928T190111Z-40a16163) the breaker stopped it at 4.52 cores
# after 10 s, after every gate had passed. A process whose OBSERVED ANCESTRY reaches a
# root registered with this role (nightly-local.py registers its build step's own
# supervisor, by PID and birth, never by name) may stay above JOB_CORES for
# BUILD_WINDOW seconds instead of WINDOW. The limit is still a limit: a process that
# stays above JOB_CORES for longer is stopped exactly as before, the grant ends with the
# registered supervisor, and a process that leaves the tree (reparented) is judged by
# the ordinary 10-second rule again. Everything else is unchanged.
BUILD_ROLE = 'release-build'
BUILD_WINDOW = 600.0
# A NATIVE BUILD IS JUDGED BY THE CORES IT WAS GRANTED (2026-10-02). native-work.py admits a
# build with a core grant and registers its own process with this role, the grant (`cores`)
# and a `group` (its workspace); a Gradle daemon it keeps warm for that workspace is
# registered the same way. Every owned process whose CURRENT parent chain reaches such a
# root counts toward its group, and while the group's total stays within its grant no
# process in it is held to JOB_CORES: a Gradle JVM given four processors ran at 6.08 cores
# and was stopped by the per-process rule (2026-10-02 00:21Z, pid 3022) although its build
# was inside what admission had granted it. A group above its grant, and anything outside a
# group, is judged exactly as before. The same rule is in cpu_guard_live.py.
NATIVE_ROLE = 'native-build'
LABEL = 'com.richos.cpu-guard'
IOS_FIRST_BOOT_SECONDS = 180
IOS_WARM_BOOT_SECONDS = 120


def write_json(path, value, durable=False):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + '.%s.new' % os.getpid())
    with tmp.open('w') as stream:
        json.dump(value, stream)
        if durable:
            stream.flush()
            os.fsync(stream.fileno())
    os.chmod(tmp, 0o600)
    os.replace(tmp, path)
    if durable:
        directory = os.open(path.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)


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
    import proc_tree
    result = subprocess.run(['ps', '-ax', '-o', 'uid=,pid=,ppid=,time=,rss=,lstart=,comm='],
                            capture_output=True, text=True, check=True, timeout=5,
                            env={**os.environ, 'LC_ALL': 'C', 'TZ': 'UTC0'})
    parsed = {}
    for line in result.stdout.splitlines():
        f = line.split(None, 10)
        if len(f) != 11 or int(f[0]) != os.getuid():
            continue
        parsed[int(f[1])] = f
    rows = {}
    for pid, native in proc_tree.native_processes(parsed).items():
        f = parsed[pid]
        rows[pid] = dict(parent=native['ppid'] if native else int(f[2]), cpu=seconds(f[3]),
                              rss_mb=int(f[4]) / 1024, birth=' '.join(f[5:10]), name=f[10],
                              generation=native['start'] if native else None)
    return rows


def grant_fields(role, grant):
    """The grant a NATIVE_ROLE root carries: {'cores': > 0, 'group': non-empty}. Shared shape
    with cpu_guard_live.py; any other role carries none."""
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


def grant_groups(owned, rows, rates, roots):
    """{group: {'cores': grant, 'used': cores now, 'members': [pid]}} for every registered
    native build. A process belongs to the nearest NATIVE_ROLE root on its CURRENT parent
    chain (a process that left the tree is no longer the build's). Shared with cpu_guard_live.py."""
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


def register(pid, label, role='session', verification=None, grant=None):
    rows = processes()
    if pid not in rows:
        raise ValueError('owner process is not alive or not owned by this user')
    generation = rows[pid].get('generation')
    if not generation:
        raise ValueError('owner process generation is unavailable')
    record = dict(pid=pid, birth=rows[pid]['birth'], generation=generation, label=label, role=role,
                  **grant_fields(role, grant))
    if role == 'verification':
        if not verification or not re.fullmatch('[0-9a-f]{64}', verification.get('input_key', '')):
            raise ValueError('verification registration requires its input identity')
        if verification.get('priority') not in ('integration', 'background'):
            raise ValueError('verification registration requires a qualified local priority')
        result = Path(verification.get('result', ''))
        if not result.is_absolute() or not result.parent.is_dir():
            raise ValueError('verification registration requires its existing result directory')
        reason = verification_recovery(verification['input_key'])['blocked']
        if reason:
            raise RuntimeError(reason)
        record['verification'] = verification
    write_json(STATE / 'roots' / ('%s.json' % pid), record)
    return record


def verification_recovery(input_key, charge=None, details=None):
    """Pressure and resource faults never spend the assertion-failure budget.

    The per-input lock serializes registration and controller updates across
    runner restarts. A renamed run or changed plan cannot reset this record.
    Resource recalibration/recovery is explicit; there is no automatic reset.
    """
    if not re.fullmatch('[0-9a-f]{64}', input_key):
        raise ValueError('invalid verification input identity')
    directory = STATE / 'verification-recovery'
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / (input_key + '.lock')).open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = directory / (input_key + '.json')
        record = read_json(path, {'schema': 1, 'containment': [], 'resource': []})
        if (not isinstance(record, dict) or record.get('schema') != 1 or
                any(not isinstance(record.get(kind), list) for kind in ('containment', 'resource'))):
            raise ValueError('corrupt verification recovery record: ' + input_key)
        blocked = ('scheduler-starvation: three pressure containments for unchanged inputs'
                   if len(record['containment']) >= 3 else
                   'resource-envelope-exceeded: measured recalibration required' if record['resource'] else None)
        if charge:
            if charge not in ('containment', 'resource'):
                raise ValueError('unknown recovery budget')
            if blocked:
                raise RuntimeError(blocked)
            record[charge].append({'at': time.time(), **(details or {})})
            write_json(path, record, durable=True)
        return {**record, 'blocked': blocked}


class RecoveryExhausted(RuntimeError):
    """A known exhausted policy cannot be repaired by waiting for capacity."""


def request_verification_recovery(key, result_path):
    """One recorded policy change: retry under exclusive measured calibration.

    Original counters are immutable history. The alternate policy has its own
    bounded counters and can never create another alternate. This gives resource
    faults a real remedy without relabelling them as assertion failures or
    repeatedly resetting the same concurrent policy.
    """
    history = verification_recovery(key)
    if not history['blocked']:
        raise ValueError('resource recovery requires an exhausted policy')
    directory = STATE / 'verification-policies'
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / (key + '.lock')).open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = directory / (key + '.json')
        existing = read_json(path)
        if existing:
            if existing.get('original') != key:
                raise RecoveryExhausted(history['blocked'] + '; exclusive recovery exhausted')
            return existing
        result = read_json(result_path, {})
        events = history['resource'] or history['containment']
        if (result.get('input_key') != key or result.get('cleanup') != 'complete'
                or events[-1].get('result') != str(result_path)
                or result.get('status') not in ('contained', 'resource-envelope-exceeded')):
            raise BlockingIOError('resource recovery waits for matching completed owned cleanup')
        variant = hashlib.sha256((key + ':exclusive-calibration-v1').encode()).hexdigest()
        record = {'protocol': VERIFICATION_PROTOCOL, 'original': key, 'variant': variant,
                  'mode': 'exclusive-calibration-v1', 'reason': history['blocked'],
                  'result': str(result_path), 'at': time.time()}
        # Publish the terminal alternate first. A crash between these writes is
        # repaired by repeating this same deterministic publication, never by
        # granting another recovery episode.
        write_json(directory / (variant + '.json'), record, durable=True)
        write_json(path, record, durable=True)
        note('Verification resource recovery requires exclusive calibration', policy=record)
        return record


def resolve_verification_policy(context):
    key = context['input_key']
    history = verification_recovery(key)
    record = read_json(STATE / 'verification-policies' / (key + '.json'))
    if not record and history['blocked']:
        events = history['resource'] or history['containment']
        result = events[-1].get('result')
        if not result:
            raise RecoveryExhausted(history['blocked'] + '; matching cleanup evidence is unavailable')
        record = request_verification_recovery(key, result)
    if record:
        expected = hashlib.sha256((record['original'] + ':exclusive-calibration-v1').encode()).hexdigest()
        if (record.get('protocol') != VERIFICATION_PROTOCOL or record.get('variant') != expected
                or key not in (record['original'], record['variant'])
                or record.get('mode') != 'exclusive-calibration-v1'):
            raise ValueError('invalid recorded verification recovery policy')
        context.update(input_key=record['variant'], original_input_key=record['original'],
                       resource_policy=record['mode'])
    blocked = verification_recovery(context['input_key'])['blocked']
    if blocked:
        raise RecoveryExhausted(blocked + '; exclusive recovery exhausted')
    return context


def verification_enabled():
    governed_directory(STATE, CANONICAL_STATE)
    expected = read_json(STATE / 'verification-enabled.json')
    if expected is not None:
        if expected.get('protocol') != VERIFICATION_PROTOCOL:
            raise RuntimeError('installed verification protocol requires a current launcher')
        if (expected.get('launchers') and STATE.resolve() == CANONICAL_STATE.resolve()
                and str(Path(__file__).resolve().parent) != expected.get('runtime')):
            mismatch = launcher_mismatch(Path(__file__).resolve().parents[4], expected['launchers'])
            if mismatch:
                raise RuntimeError('unqualified verification launcher; update from the installed version: ' + mismatch)
        return True
    return read_json(STATE / 'verification-pressure.json', {}).get('protocol') == VERIFICATION_PROTOCOL


def launcher_inventory(root):
    return {name: hashlib.sha256((Path(root) / name).read_bytes()).hexdigest() for name in LAUNCHER_FILES}


def launcher_mismatch(root, expected):
    if set(expected) != set(LAUNCHER_FILES):
        return 'installed launcher inventory is incomplete'
    for name, digest in expected.items():
        try:
            actual = hashlib.sha256((Path(root) / name).read_bytes()).hexdigest()
        except OSError:
            return name + ' is missing or unreadable'
        if actual != digest:
            return name + ' differs'
    return None


def require_managed_ancestor():
    if verification_enabled() and not inherited_verification_owner(os.environ.get('RICHOS_VERIFICATION_OWNER')):
        raise ValueError('REFUSED before resource wait: use the current proof-run plan or its live admitted ancestor')


def inherited_verification_owner(claim, pid=None):
    """An environment hint is valid only for a live, registered ancestor.

    A PID alone, an old process generation or a different live verification
    session must never suppress registration/admission of another workload.
    """
    if not claim:
        return None
    try:
        number, generation = claim.split(':', 1)
        owner = int(number)
    except (ValueError, AttributeError):
        return None
    if owner <= 0 or not generation:
        return None
    record = read_json(STATE / 'roots' / ('%s.json' % owner), {})
    if (record.get('pid') != owner or record.get('generation') != generation
            or record.get('role') != 'verification'
            or record.get('verification', {}).get('protocol') != VERIFICATION_PROTOCOL):
        return None
    import proc_tree
    rows = proc_tree.process_rows()
    if owner not in rows or rows[owner][2] != generation:
        return None
    current = os.getpid() if pid is None else pid
    seen = set()
    while current in rows and current not in seen:
        if current == owner:
            # Re-read at the authorization point, after ancestry observation.
            return record if proc_tree.identity(owner) == generation else None
        seen.add(current)
        current = rows[current][0]
    return None


def verification_admission():
    """Machine pressure gates expansion; borrowing an already held slot is separate.

    An absent managed-policy record is the legacy boundary during migration.
    Once present, missing/stale monitoring refuses admission rather than silently
    returning to token-only execution. Mandatory enrollment is enforced by the
    production launcher, not inferred from a private HOME path here.
    """
    installed = read_json(STATE / 'verification-enabled.json', {})
    if installed.get('activation', 'active') != 'active':
        return 'verification installation is incomplete; admission remains closed'
    record = read_json(STATE / 'verification-pressure.json')
    if record is None:
        return 'verification controller is unavailable' if verification_enabled() else None
    if record.get('protocol') != VERIFICATION_PROTOCOL:
        return 'verification controller protocol requires a current launcher'
    if not healthy() or not 0 <= time.time() - record.get('at', 0) < 12:
        return 'verification controller is unhealthy or stale'
    if not record.get('admission_open'):
        return 'verification pressure: ' + record.get('stage', 'unknown')
    return None


class VerificationClient:
    """Supervisor enrollment and typed completion for one verification attempt.

    The launcher supplies a durable input key and a private attempt directory.
    Nested supervisors inherit the owner but do not register duplicate demand.
    The child remains behind its exec pipe until its native identity is recorded.
    """
    def __init__(self, context):
        self.context = read_json(context)
        if not isinstance(self.context, dict) or self.context.get('protocol') != VERIFICATION_PROTOCOL:
            raise ValueError('unsupported verification controller protocol')
        resolve_verification_policy(self.context)
        self.pid = os.getpid()
        native = operator_fences.proc(self.pid, precise=True)
        if not native or native['zombie']:
            raise RuntimeError('verification supervisor generation unavailable')
        self.generation = native['start']
        self.started = time.monotonic()
        self.last_health = 0
        self.lease = None
        inherited = self.context.get('reservation')
        if inherited:
            fd = inherited['fd']
            path = STATE / 'verification-reservations' / (inherited['id'] + '.json')
            actual, expected = os.fstat(fd), path.with_suffix('.lock').stat()
            record = read_json(path)
            if ((actual.st_dev, actual.st_ino) != (expected.st_dev, expected.st_ino)
                    or record['input_key'] != self.context['input_key']):
                raise ValueError('inherited verification reservation does not match this input')
            self.lease = (fd, record['envelope'], str(path))
            record.update(root_pid=self.pid, root_generation=self.generation,
                          seed={'pid': self.pid, 'generation': self.generation})
            write_json(path, record, durable=True)
        self.check_health(force=True)

    def check_health(self, force=False):
        now = time.monotonic()
        if force or now - self.last_health >= 1:
            record = read_json(STATE / 'verification-pressure.json', {})
            if (not healthy() or record.get('protocol') != VERIFICATION_PROTOCOL
                    or not 0 <= time.time() - record.get('at', 0) < 12):
                raise RuntimeError('verification controller unavailable; stopping owned work')
            self.last_health = now

    def start(self, child):
        self.check_health(force=True)
        reason = verification_admission()
        if reason:
            raise RuntimeError(reason)
        native = operator_fences.proc(child, precise=True)
        if not native or native['zombie']:
            raise RuntimeError('verification child generation unavailable before exec')
        context = {**self.context, 'seed': {'pid': child, 'generation': native['start']}}
        reason = verification_recovery(context['input_key'])['blocked']
        if reason:
            raise BlockingIOError(reason)
        if self.lease is None:
            self.lease = reserve_verification(context, self.pid, self.generation)
        else:
            record = read_json(self.lease[2])
            record.update(root_pid=self.pid, root_generation=self.generation, seed=context['seed'])
            write_json(self.lease[2], record, durable=True)
        context['reservation'] = self.lease[1]
        context['started_monotonic'] = time.monotonic()
        register(self.pid, context['label'], 'verification', context)

    def finish(self, rc, survivors, cpu_seconds):
        path = self.context['result']
        record = read_json(path, {})
        if record and (record.get('input_key') != self.context['input_key']
                       or record.get('root_generation') != self.generation):
            raise RuntimeError('verification result belongs to another execution')
        record.update(input_key=self.context['input_key'], root_pid=self.pid,
                      root_generation=self.generation, exit=rc,
                      cleanup='failed' if survivors else 'complete', survivors=survivors,
                      reaped_cpu_seconds=cpu_seconds, elapsed_seconds=time.monotonic() - self.started,
                      finished_at=time.time())
        record.setdefault('status', 'completed' if rc not in (75, 124, 125, 127, 130, 143) else 'incomplete')
        measurement = read_json(STATE / 'verification-measurements' / (str(self.pid) + '.json'), {})
        if measurement.get('root_generation') == self.generation:
            record['measurement'] = measurement
        record['cost'] = record_verification_cost(self.context, record)
        write_json(path, record, durable=True)
        if self.lease:
            try:
                if measurement.get('root_generation') == self.generation:
                    if record['status'] == 'completed' and rc == 0 and not survivors:
                        qualify_demand(self.context['input_key'], measurement, record)
            finally:
                os.close(self.lease[0])
                self.lease = None
        return record


def record_verification_cost(context, completion):
    """Retain the first qualified cost as the baseline; never ratchet it upward.

    A comparison key binds command, execution recipe, tools and platform while
    allowing source changes. Growth is a diagnostic, not an assertion verdict.
    Queue time and repeated attempts stay in the runner's separate total costs.
    """
    comparison = context.get('cost_comparison')
    if not comparison:
        return {'status': 'unqualified', 'reason': 'execution inputs have no comparable cost recipe'}
    key = comparison['key']
    if not re.fullmatch('[0-9a-f]{64}', key):
        raise ValueError('invalid cost comparison identity')
    if completion.get('status') != 'completed' or completion.get('exit') != 0 or completion.get('cleanup') != 'complete':
        return {'status': 'incomplete', 'reason': 'failed or interrupted execution is not a cost baseline'}
    values = {'cpu_seconds': completion.get('reaped_cpu_seconds'),
              'elapsed_seconds': completion.get('elapsed_seconds')}
    if any(not isinstance(v, (int, float)) or not math.isfinite(v) or v < 0 for v in values.values()):
        return {'status': 'unknown', 'reason': 'complete finite CPU/elapsed measurement is unavailable'}
    current = {**values, 'at': time.time(), 'input_key': context['input_key'],
               'result': context['result'], 'predicted_seconds': comparison['predicted_seconds'],
               'load': completion.get('measurement', {}).get('load')}
    directory = STATE / 'verification-costs'
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / (key + '.lock')).open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = directory / (key + '.json')
        record = read_json(path)
        if record is None:
            # Retain one self-contained baseline inside the existing bounded
            # history. Run rotation must not leave an opaque input hash and a
            # dead path as the only explanation of the original measurement.
            # Copy before finish() attaches this cost record to completion.
            evidence = json.loads(json.dumps({'context': context,
                'completion': {k: v for k, v in completion.items() if k != 'cost'}}))
            record = {'schema': 1, 'comparison': comparison,
                      'baseline': {**current, 'evidence': evidence}, 'recent': []}
        if record.get('schema') != 1 or record.get('comparison', {}).get('key') != key:
            raise ValueError('invalid verification cost history: ' + str(path))
        baseline = record['baseline']
        def load_qualified(row):
            load = row.get('load')
            return load and load['samples'] >= 3 and not load['pressure_seen'] and not load['unknown']
        elapsed_comparable = bool(load_qualified(current) and load_qualified(baseline)
            and current['load']['owners_peak'] == baseline['load']['owners_peak']
            and abs(current['load']['busy_total'] / current['load']['samples']
                    - baseline['load']['busy_total'] / baseline['load']['samples']) <= 10)
        increases = {metric: {'baseline': baseline[metric], 'actual': value,
                          'increase_seconds': value - baseline[metric]}
                  for metric, value in values.items()
                  if value > 1.2 * baseline[metric] and value - baseline[metric] >= 10}
        growth = {metric: value for metric, value in increases.items()
                  if metric == 'cpu_seconds' or elapsed_comparable}
        unconfirmed = {metric: value for metric, value in increases.items() if metric not in growth}
        status = ('growth' if growth else 'uncertain-growth' if unconfirmed else
                  'within-baseline' if elapsed_comparable else 'partial')
        record['recent'] = [*record['recent'], {**current, 'status': status}][-20:]
        record['latest'] = {'status': status, 'growth': growth, 'actual': current,
                            'unconfirmed_growth': unconfirmed,
                            'baseline': baseline, 'history': str(path),
                            'elapsed_comparable': elapsed_comparable,
                            'uncertainty': [] if elapsed_comparable else
                                ['elapsed comparison needs matching sampled host load and concurrency without pressure']}
        write_json(path, record, durable=True)
        return record['latest']


def previous_verification_cost(key):
    if not re.fullmatch('[0-9a-f]{64}', key):
        raise ValueError('invalid cost comparison identity')
    record = read_json(STATE / 'verification-costs' / (key + '.json'))
    return record['latest'] if record else None


def demand_capacity(heartbeat):
    """Leave explicit host/service headroom, independently of worker counts."""
    sample = heartbeat.get('verification_resources', {})
    cores = heartbeat.get('cpu_count', 0)
    busy = heartbeat.get('host_busy', float('nan'))
    free, total = sample.get('memory_free_mb', 0), sample.get('memory_total_mb', 0)
    owned_memory = heartbeat.get('verification_rss_mb', 0)
    unknown = heartbeat.get('unattributed_cores', float('nan'))
    if not all(math.isfinite(v) and v >= 0 for v in (cores, busy, free, total, unknown, owned_memory)) or not cores or not total:
        raise BlockingIOError('CPU/memory demand measurement unavailable')
    # At least 20% for unowned host/system work, another 20% below saturation.
    # Observed unowned demand above that reserve reduces capacity immediately.
    return {'cores': max(0, .8 * cores - max(.2 * cores, unknown + .1 * cores)),
            'rss_mb': max(0, min(total, free + owned_memory) - .25 * total)}


def reserve_verification(context, pid, generation):
    """Atomically reserve a measured envelope; one unknown calibrates at a time.

    The existing supervisor holds the lease through cleanup. A crashed owner
    with native surviving members still occupies its reservation. No lock file
    is resized and nested children share the tree's reservation.
    """
    resolve_verification_policy(context)
    directory = STATE / 'verification-reservations'
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / 'admission.lock').open('a') as admission:
        fcntl.flock(admission, fcntl.LOCK_EX)
        capacity = demand_capacity(read_json(STATE / 'heartbeat.json', {}))
        profile = read_json(STATE / 'verification-demand' / (context['input_key'] + '.json'))
        if profile and profile.get('protocol') != VERIFICATION_PROTOCOL:
            raise BlockingIOError('stale verification demand envelope requires calibration')
        envelope = dict(profile['envelope']) if profile else {
            **capacity, 'calibration': True, 'max_seconds': CALIBRATION_SECONDS}
        if context.get('resource_policy') == 'exclusive-calibration-v1':
            envelope['exclusive'] = True
        if any(not math.isfinite(envelope.get(k, 0)) or envelope.get(k, 0) <= 0 for k in ('cores', 'rss_mb')):
            raise BlockingIOError('no measured CPU/memory capacity for verification')
        active = []
        for path in directory.glob('*.json'):
            record = read_json(path)
            fd = os.open(path.with_suffix('.lock'), os.O_CREAT | os.O_RDWR, 0o600)
            try:
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    active.append(record)
                    continue
                result = read_json(record['result'], {})
                if (result.get('root_generation') == record['root_generation']
                        and result.get('cleanup') == 'complete'):
                    path.unlink()
                    path.with_suffix('.lock').unlink()
                    continue
                measurement = read_json(STATE / 'verification-measurements' / (str(record['root_pid']) + '.json'), {})
                members = {str(record['root_pid']): record['root_generation'],
                           str(record['seed']['pid']): record['seed']['generation']}
                if measurement.get('root_generation') == record['root_generation']:
                    members.update(measurement['members'])
                for raw, birth in members.items():
                    native = operator_fences.proc(int(raw), precise=True)
                    if native and not native['zombie'] and native['start'] == birth:
                        active.append(record)
                        break
                    if not native:
                        # Native-unreadable live work cannot free capacity.
                        import proc_tree
                        if proc_tree._alive([int(raw)]):
                            raise BlockingIOError('orphan reservation generation unavailable')
                else:
                    path.unlink()
                    path.with_suffix('.lock').unlink()
            finally:
                os.close(fd)
        if (envelope.get('calibration') or envelope.get('exclusive')) and active:
            raise BlockingIOError('unknown demand waits for exclusive bounded calibration')
        if any(r['input_key'] == context['input_key'] for r in active):
            raise BlockingIOError('identical verification inputs already own a reservation')
        if any(r['envelope'].get('calibration') or r['envelope'].get('exclusive') for r in active):
            raise BlockingIOError('a verification demand calibration is active')
        for key in ('cores', 'rss_mb'):
            if sum(r['envelope'][key] for r in active) + envelope[key] > capacity[key] + .001:
                raise BlockingIOError('measured verification capacity is full: ' + key)
        path = directory / (uuid.uuid4().hex + '.json')
        fd = os.open(path.with_suffix('.lock'), os.O_CREAT | os.O_RDWR, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            record = {**context, 'root_pid': pid, 'root_generation': generation, 'envelope': envelope}
            write_json(path, record, durable=True)
            return fd, envelope, str(path)
        except BaseException:
            os.close(fd)
            raise


def qualify_demand(key, measurement, completion):
    # Short checks have no representative concurrent samples. Keep them in
    # bounded calibration until an explicit fixture baseline qualifies demand.
    if measurement.get('samples', 0) < 3 or measurement.get('sample_seconds', 0) < 4:
        return
    elapsed = completion['elapsed_seconds']
    sampled_cpu = measurement['cpu_seconds']
    reaped_cpu = completion['reaped_cpu_seconds']
    # Count short-lived work missed by the sampler in addition to its peak.
    unseen = max(0, reaped_cpu - sampled_cpu) / max(.001, elapsed)
    peak = max(measurement['peak_cores'], reaped_cpu / max(.001, elapsed)) + unseen
    envelope = {'cores': max(1.0, 1.25 * peak + .25),
                'rss_mb': max(256.0, 1.25 * measurement['peak_rss_mb'] + 64), 'calibration': False}
    policy = read_json(STATE / 'verification-policies' / (key + '.json'))
    if policy:
        history = verification_recovery(policy['original'])
        for event in history['resource'] + history['containment']:
            envelope['cores'] = max(envelope['cores'], 1.25 * event.get('cores', 0) + .25)
            envelope['rss_mb'] = max(envelope['rss_mb'], 1.25 * event.get('rss_mb', 0) + 64)
        envelope['exclusive'] = True
    write_json(STATE / 'verification-demand' / (key + '.json'),
        {'protocol': VERIFICATION_PROTOCOL, 'envelope': envelope, 'measurement': measurement,
         'completion': completion, 'at': time.time()}, durable=True)


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


class HostMemory:
    """Use the existing reserve adapter without its blocking admission interval."""
    def __init__(self, engine):
        provider = Path(__file__).with_name('reserve.py')
        if not provider.exists():
            provider = Path(engine).resolve().parent / 'app/scripts/testvm/reserve.py'
        spec = importlib.util.spec_from_file_location('verification_host_resources', provider)
        self.adapter = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.adapter)
        self.previous = None

    def sample(self, now):
        api = self.adapter
        _, swapouts = api._counters()
        page = api._sysctl('hw.pagesize', ctypes.c_int()).value
        level = api._sysctl('kern.memorystatus_vm_pressure_level', ctypes.c_int()).value
        free = api._sysctl('kern.memorystatus_level', ctypes.c_int()).value
        total = api._sysctl('hw.memsize', ctypes.c_uint64()).value / 1048576
        rate = 0.0
        if self.previous:
            before, at = self.previous
            if now <= at or swapouts < before:
                raise RuntimeError('memory sample counters did not advance monotonically')
            rate = (swapouts - before) * page / 1048576 / (now - at)
        self.previous = swapouts, now
        if level not in api.PRESSURE_NAMES or not 0 <= free <= 100 or total <= 0:
            raise RuntimeError('invalid memory pressure or capacity sample')
        return {'memory_pressure': api.PRESSURE_NAMES[level], 'swapout_mb_per_s': rate,
                'memory_free_mb': total * free / 100, 'memory_total_mb': total}


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
        self.verification_pressure = VerificationPressure()
        self.verification_measurements = {}
        self.verification = {}
        self.containment = None
        self.resource_over = {}
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
        """True when the process's CURRENT parent chain reaches a registered build root.

        Ancestry is read from this sample's rows, not from the retained ownership
        record: a descendant that detached from the build is no longer a compile step.
        """
        seen = set()
        while pid in rows and pid not in seen:
            if pid in build_roots:
                return True
            seen.add(pid)
            pid = rows[pid]['parent']
        return False

    @staticmethod
    def same_process(record, row):
        if not row or row.get('birth') != record.get('birth'):
            return False
        if record.get('generation') is not None or record.get('role') == 'verification':
            return record.get('generation') is not None and row.get('generation') == record['generation']
        return True  # Existing session/device records retain their legacy format.

    def sample(self, rows, now, host_busy=0):
        roots = {}
        for path in (STATE / 'roots').glob('*.json'):
            rec = read_json(path)
            seed = (rec.get('verification') or {}).get('seed') if rec else None
            seed_row = rows.get(seed['pid']) if seed else None
            if (rec and rec.get('role') == 'verification' and rec['pid'] in rows
                    and not rows[rec['pid']].get('generation')):
                raise RuntimeError('registered verification generation unavailable: %s' % rec['pid'])
            if seed_row and not seed_row.get('generation'):
                raise RuntimeError('verification seed generation unavailable')
            if rec and (self.same_process(rec, rows.get(rec['pid'])) or
                        seed_row and seed_row['generation'] == seed['generation']):
                roots[rec['pid']] = rec
            elif rec:
                path.unlink(missing_ok=True)
        for p, rec in self.owned.items():
            if rec.get('role') == 'verification' and int(p) in rows and not rows[int(p)].get('generation'):
                raise RuntimeError('owned verification generation unavailable: ' + p)
        owned = {int(p): rec for p, rec in self.owned.items() if self.same_process(rec, rows.get(int(p)))}
        for pid, rec in roots.items():
            owned[pid] = dict(birth=rec['birth'], generation=rec.get('generation'),
                              role=rec['role'], owner=rec['label'], root=pid,
                              root_generation=rec.get('generation'),
                              verification=rec.get('verification'))
            seed = (rec.get('verification') or {}).get('seed')
            if seed and seed['pid'] in rows:
                row = rows[seed['pid']]
                if not row.get('generation'):
                    raise RuntimeError('verification seed generation unavailable')
                if row['generation'] == seed['generation']:
                    owned[seed['pid']] = {**owned[pid], 'birth': row['birth'], 'generation': row['generation']}
        changed = True
        while changed:
            changed = False
            for pid, row in rows.items():
                if (pid not in roots and row['parent'] in owned and
                        (pid not in owned or owned[pid]['root'] != owned[row['parent']]['root'])):
                    parent = owned[row['parent']]
                    if parent.get('role') == 'verification' and not row.get('generation'):
                        raise RuntimeError('verification descendant generation unavailable: %s' % pid)
                    owned[pid] = {**parent, 'birth': row['birth'], 'generation': row.get('generation')}
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
            if pid in rows and old and self.same_process(rec, old) and elapsed > 0:
                rates[pid] = max(0, rows[pid]['cpu'] - old['cpu']) / elapsed
        self.unowned_rates = {pid: max(0, row['cpu'] - self.previous[pid]['cpu']) / elapsed
                              for pid, row in rows.items() if pid not in owned and elapsed > 0
                              and pid in self.previous and self.previous[pid]['birth'] == row['birth']}
        self.previous, self.last = rows, now
        self.verification = self.verification_groups(rows, rates, elapsed)
        # Sessions stay alive, and so does a release build's own supervisor. A
        # registered workload root may itself be stopped.
        protected = {p for p, r in roots.items() if r['role'] in ('session', BUILD_ROLE)} | {os.getpid()}
        build_roots = {p for p, r in roots.items() if r['role'] == BUILD_ROLE}
        allowed = {pid for pid, rec in owned.items() if rec.get('role') != 'verification'} - protected - set(self.pending)
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

    def verification_groups(self, rows, rates, elapsed):
        """Aggregate disjoint observed trees, including retained detached members.

        CPU is sampled live-process demand, not a claim to capture every process
        that exits between observations. Final reaped CPU and unobserved demand
        need separate accounting when qualifying a workload envelope.
        """
        groups = {}
        for raw, record in self.owned.items():
            pid = int(raw)
            if record.get('role') != 'verification' or pid not in rows:
                continue
            root = str(record['root']) + ':' + record['root_generation']
            group = groups.setdefault(root, {**record['verification'], 'owner': record['owner'],
                                            'root_pid': record['root'], 'root_generation': record['root_generation'],
                                            'members': {}, 'cores': 0.0, 'rss_mb': 0.0})
            group['members'][raw] = record['generation']
            group['cores'] += rates.get(pid, 0.0)
            group['rss_mb'] += rows[pid].get('rss_mb', 0.0)
        for root, group in groups.items():
            measurement_key = (root, group['input_key'])
            measurement = self.verification_measurements.setdefault(measurement_key,
                {'cpu_seconds': 0.0, 'peak_cores': 0.0, 'peak_rss_mb': 0.0, 'samples': 0, 'sample_seconds': 0.0})
            measurement['cpu_seconds'] += group['cores'] * max(0, elapsed)
            measurement['sample_seconds'] += max(0, elapsed)
            measurement['peak_cores'] = max(measurement['peak_cores'], group['cores'])
            measurement['peak_rss_mb'] = max(measurement['peak_rss_mb'], group['rss_mb'])
            measurement['samples'] += 1
            group.update(measurement)
            write_json(STATE / 'verification-measurements' / (str(group['root_pid']) + '.json'), group)
        self.verification_measurements = {key: value for key, value in self.verification_measurements.items()
                                          if key[0] in groups}
        return groups

    def verification_cycle(self, rows, now, busy, memory_pressure='normal', swapout_mb_per_s=0):
        for key, owner in self.verification.items():
            measurement = self.verification_measurements[(key, owner['input_key'])]
            load = measurement.setdefault('load', {'samples': 0, 'busy_total': 0.0, 'busy_peak': 0.0,
                'owners_peak': 0, 'pressure_seen': False, 'unknown': False})
            load['samples'] += 1
            if isinstance(busy, (int, float)) and math.isfinite(busy) and 0 <= busy <= 100:
                load['busy_total'] += busy
                load['busy_peak'] = max(load['busy_peak'], busy)
            else:
                load['unknown'] = True
            load['unknown'] |= not math.isfinite(swapout_mb_per_s)
            load['owners_peak'] = max(load['owners_peak'], len(self.verification))
            load['pressure_seen'] |= memory_pressure != 'normal' or swapout_mb_per_s > 0 or busy >= 80
            owner['load'] = load
            write_json(STATE / 'verification-measurements' / (str(owner['root_pid']) + '.json'), owner)
        if self.containment is None:
            # Recover an already charged intervention after a controller restart.
            for key, owner in self.verification.items():
                saved = read_json(owner['result'], {})
                if (saved.get('status') in ('contained', 'resource-envelope-exceeded')
                        and saved.get('root_generation') == owner['root_generation']):
                    self.containment = key, saved
                    self.verification_pressure.pending = key
                    self.verification_pressure.pending_since = now - max(0, time.time() - saved['at'])
                    self.verification_pressure.closed = True
                    break
        decision = self.verification_pressure.observe(now, busy, self.verification,
                                                       memory_pressure, swapout_mb_per_s)
        breached = []
        for key, owner in self.verification.items():
            envelope = owner.get('reservation')
            if envelope and envelope.get('calibration'):
                started = owner.get('started_monotonic')
                if (not isinstance(started, (int, float)) or not math.isfinite(started)
                        or started > now or now - started >= CALIBRATION_SECONDS):
                    owner['resource_fault'] = 'unknown-demand calibration exceeded its 120-second window or has no valid start'
                    breached.append(key)
                    continue
            exceeds = envelope and not envelope.get('calibration') and any(
                owner[metric] > envelope[metric] for metric in ('cores', 'rss_mb'))
            if exceeds:
                self.resource_over.setdefault(key, now)
                if now - self.resource_over[key] >= 4:
                    breached.append(key)
            else:
                self.resource_over.pop(key, None)
        cause = 'host-pressure'
        if breached and self.containment is None:
            key = VerificationPressure.contributor({key: self.verification[key] for key in breached}) or breached[0]
            decision = {'admission_open': False, 'stage': 'contain', 'target': key}
            self.verification_pressure.pending, self.verification_pressure.pending_since = key, now
            self.verification_pressure.closed = True
            cause = 'resource-envelope-exceeded'
        if self.containment:
            key, record = self.containment
            latest = read_json(record['result'], {})
            for field in ('exit', 'survivors', 'reaped_cpu_seconds', 'elapsed_seconds', 'finished_at'):
                if field in latest:
                    record[field] = latest[field]
            if key not in self.verification:
                record.update(cleanup='complete', cleanup_completed_at=time.time())
                write_json(record['result'], record)
                note('Verification containment cleanup complete', input_key=record['input_key'],
                     owner=record['owner'])
                self.containment = None
            elif now - self.verification_pressure.pending_since >= 15:
                # The supervisor normally completes its own bounded cleanup.
                # If it is wedged or gone, stop only the retained native
                # generations, including newly observed owned descendants.
                escalated = record.setdefault('escalated', {})
                for raw, generation in self.verification[key]['members'].items():
                    if escalated.get(raw) == generation:
                        continue
                    current = operator_fences.proc(int(raw), precise=True)
                    if current and current['start'] == generation and not current['zombie']:
                        try:
                            os.kill(int(raw), signal.SIGKILL)
                            escalated[raw] = generation
                        except ProcessLookupError:
                            pass
                write_json(record['result'], record)
            if key in self.verification and decision['stage'] == 'containment-failed' and record['cleanup'] != 'failed':
                record.update(cleanup='failed', survivors=self.verification[key]['members'])
                write_json(record['result'], record)
                event('Verification containment exceeded 30 seconds', input_key=record['input_key'],
                      owner=record['owner'], survivors=record['survivors'])
        if decision['stage'] == 'contain':
            owner = self.verification[decision['target']]
            # Charge before signalling. The supervisor's ordinary SIGTERM path
            # retains worker leases until its owned descendants have been cleaned.
            kind = 'resource' if cause == 'resource-envelope-exceeded' else 'containment'
            budget = verification_recovery(owner['input_key'], kind,
                {'owner': owner['owner'], 'sampled_cpu_seconds': owner['cpu_seconds'],
                 'cores': owner['cores'], 'rss_mb': owner['rss_mb'], 'host_busy': busy,
                 'result': owner['result'], 'resource_fault': owner.get('resource_fault')})
            record = {**owner, 'status': cause if kind == 'resource' else 'contained', 'cause': cause,
                      'at': time.time(), 'cleanup': 'pending', 'budget_used': len(budget[kind])}
            write_json(owner['result'], record, durable=True)
            self.containment = decision['target'], record
            root = owner['root_pid']
            members = {root: owner['root_generation']} if root in rows else {
                int(pid): generation for pid, generation in owner['members'].items()}
            signalled = []
            for pid, generation in members.items():
                current = operator_fences.proc(pid, precise=True)
                if current and current['start'] == generation and not current['zombie']:
                    try:
                        os.kill(pid, signal.SIGTERM)
                        signalled.append(pid)
                    except ProcessLookupError:
                        pass
            note('Verification contained: ' + cause, owner=owner['owner'],
                 input_key=owner['input_key'], signalled=signalled, budget_used=record['budget_used'])
        write_json(STATE / 'verification-pressure.json', {**decision, 'at': time.time(),
            'protocol': VERIFICATION_PROTOCOL,
            'host_busy': busy, 'memory_pressure': memory_pressure,
            'owners': [{key: owner[key] for key in ('owner', 'input_key', 'priority', 'cores',
                       'rss_mb', 'cpu_seconds', 'peak_cores', 'peak_rss_mb', 'samples')}
                       for owner in self.verification.values()]})
        return decision

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
            if (not rows[p].get('generation') or
                    fresh.get(p, {}).get('generation') != rows[p]['generation']):
                continue
            try:
                os.kill(p, signal.SIGTERM)
                signalled[p] = rows[p]['birth']
                self.pending[p] = (rows[p]['generation'], time.monotonic() + 3)
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
            if (pid not in self.pending and row['parent'] in self.pending
                    and rows.get(row['parent'], {}).get('generation') == self.pending[row['parent']][0]
                    and row.get('generation')):
                self.pending[pid] = (row['generation'], self.pending[row['parent']][1])
                try:
                    current = operator_fences.proc(pid, precise=True)
                    if current and current['start'] == row['generation']:
                        os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        for pid, (birth, deadline) in list(self.pending.items()):
            if rows.get(pid, {}).get('generation') != birth:
                del self.pending[pid]
            elif now >= deadline:
                try:
                    current = operator_fences.proc(pid, precise=True)
                    if current and current['start'] == birth:
                        os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                del self.pending[pid]


def watch(engine):
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / 'watch.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        watcher = Watch()
        memory = HostMemory(engine) if sys.platform == 'darwin' else None
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
                resources = memory.sample(now) if memory else {'memory_pressure': 'normal', 'swapout_mb_per_s': 0}
                watcher.verification_cycle(rows, now, busy, resources['memory_pressure'], resources['swapout_mb_per_s'])
                for pid in watcher.policy_targets(rows, protected):
                    watcher.stop(pid, rows, protected, rates, 'local simulator incident containment')
                if candidates:
                    watcher.stop(candidates[0], rows, protected, rates)
                if busy >= 85:
                    watcher.host_since = watcher.host_since if watcher.host_since is not None else now
                    if not candidates and now-watcher.host_since >= WINDOW and now-watcher.reported_at >= 300:
                        top = sorted(watcher.unowned_rates, key=watcher.unowned_rates.get, reverse=True)[:3]
                        if top:
                            event('Host CPU is high; unregistered processes reported without signals',
                                  host_busy=round(busy,1), processes=[dict(pid=p,executable=rows[p]['name'],
                                  cores=round(watcher.unowned_rates[p],2)) for p in top])
                            watcher.reported_at = now
                else:
                    watcher.host_since = None
                write_json(STATE / 'owned.json', watcher.owned)
                write_json(STATE / 'heartbeat.json', dict(at=time.time(), ok=True,
                           pid=os.getpid(), owned=len(watcher.owned), host_busy=round(busy, 1),
                           admission_open=admission_open(busy), admission_limit=DEFAULT_MAX_CPU,
                           cpu_count=os.cpu_count(),
                           grants={name: dict(cores=g['cores'], used=round(g['used'], 2), processes=len(g['members']))
                                   for name, g in watcher.grants.items()},
                           verification_rss_mb=sum(g['rss_mb'] for g in watcher.verification.values()),
                           unattributed_cores=max(sum(watcher.unowned_rates.values()),
                               busy / 100 * os.cpu_count() - sum(g['cores'] for g in watcher.verification.values())),
                           verification_resources=resources))
            except Exception as exc:
                event('CPU watchdog sampling failed', error=str(exc))
                write_json(STATE / 'heartbeat.json', dict(at=time.time(), ok=False, error=str(exc)))
            time.sleep(INTERVAL)


def stage_runtime(engine, runtime):
    """Copy every native observation/recovery dependency into the installed runtime."""
    runtime = Path(runtime)
    runtime.mkdir(parents=True, exist_ok=True)
    for name in ('cpu_policy.py', 'operator_fences.py', 'proc_tree.py', 'machine_settings.py'):
        shutil.copy2(Path(__file__).with_name(name), runtime / name)
    shutil.copy2(Path(engine).resolve().parent / 'app/scripts/testvm/reserve.py', runtime / 'reserve.py')
    target = runtime / 'cpu_guard.py'
    shutil.copy2(__file__, target)
    return target


def install(engine):
    M.validate(STATE)
    engine = Path(engine).resolve()
    root = engine.parents[1]
    inventory = launcher_inventory(root)
    if Path(__file__).resolve() != engine / 'scripts/lib/cpu_guard.py':
        raise ValueError('install must use the candidate checkout controller')
    revision = subprocess.check_output(['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip()
    subprocess.run(['git', '-C', str(root), 'diff', '--quiet', 'HEAD', '--', *LAUNCHER_FILES], check=True)
    # Parse and preserve user settings before making the machine policy active.
    settings = Path.home() / '.claude/settings.json'
    original = settings.read_bytes() if settings.exists() else b'{}'
    data = json.loads(original)
    hooks = data.setdefault('hooks', {})
    bundle = hashlib.sha256(json.dumps(inventory, sort_keys=True).encode()).hexdigest()
    target = stage_runtime(engine, STATE / 'runtimes' / bundle)
    previous = read_json(STATE / 'verification-enabled.json', {})
    backup = STATE / ('claude-settings-before-' + time.strftime('%Y%m%dT%H%M%S') + '.json')
    backup.write_bytes(original)
    backup.chmod(0o600)
    policy = dict(protocol=VERIFICATION_PROTOCOL, installed_at=time.time(), revision=revision,
                  launchers=inventory, runtime=str(target.parent), engine=str(engine), bundle=bundle,
                  settings_backup=str(backup), activation='starting')
    # The marker survives failed bootstrap or rollback. No failure can reopen
    # token-only admission; restoration needs the same healthy managed protocol.
    write_json(STATE / 'verification-enabled.json', policy, durable=True)
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
        if (healthy() and read_json(STATE/'heartbeat.json', {}).get('at', 0) >= started
                and read_json(STATE/'devices-heartbeat.json', {}).get('at', 0) >= started
                and read_json(STATE/'verification-pressure.json', {}).get('protocol') == VERIFICATION_PROTOCOL):
            break
        time.sleep(.25)
    else:
        raise RuntimeError('watchdog installed but did not produce a healthy heartbeat; inspect launchctl print')
    # A standalone user hook also covers cached/older engine plugins. The
    # canonical engine dispatcher is deliberately not a second registration.
    for name, matcher, action in [('PreToolUse', 'Bash', 'hook'), ('SessionStart', None, 'notice-json'), ('Stop', None, 'notice-json')]:
        entries = hooks.setdefault(name, [])
        old_targets = {str(STATE / 'runtime/cpu_guard.py')}
        if previous.get('runtime'):
            old_targets.add(str(Path(previous['runtime']) / 'cpu_guard.py'))
        old_commands = {'/usr/bin/python3 -B ' + shlex.quote(path) + ' ' + action for path in old_targets}
        for entry in entries:
            entry['hooks'] = [h for h in entry.get('hooks', []) if h.get('command') not in old_commands]
        entries[:] = [entry for entry in entries if entry.get('hooks')]
        command = '/usr/bin/python3 -B ' + shlex.quote(str(target)) + ' ' + action
        if not any(h.get('command') == command for e in entries for h in e.get('hooks', [])):
            entry = {'hooks': [{'type': 'command', 'command': command, 'timeout': 10}]}
            if matcher: entry['matcher'] = matcher
            entries.append(entry)
    write_json(settings, data)
    policy.update(activation='active', activated_at=time.time())
    write_json(STATE / 'verification-enabled.json', policy, durable=True)
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
        if entry in ('proof-run.py', 'run-tests.sh', 'ci-shard.sh', 'run-all-tests.sh', 'nightly-local.py',
                     'worker_tokens.py', 'simulator_budget.py', 'mutation-harness.sh', 'testdevices.py',
                     'native-work.py', 'rios', 'randroid', 'simulator-tests.sh',
                     'native-ios-share.test.sh', 'native-ios-app.test.sh', 'native-ios-ui.test.sh'):
            executable = part[1] if entry != name else part[0]
            path = Path(cwd or os.getcwd()).joinpath(executable).resolve()
            for parent in path.parents:
                policy = parent / 'richos/engine/scripts/lib/testdevices.py'
                if policy.is_file():
                    installed = read_json(STATE / 'verification-enabled.json', {})
                    if installed.get('launchers'):
                        mismatch = launcher_mismatch(parent, installed['launchers'])
                        if mismatch:
                            return 'outdated verification entrypoint: ' + mismatch
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
    alert = read_json(STATE / 'alert.json')
    if alert:
        marker = STATE / 'notices' / (str(payload.get('session_id', 'unknown')).replace('/', '_') + '.json')
        previous = read_json(marker, {})
        if alert.get('at') != previous.get('at'):
            print(json.dumps({'hookSpecificOutput': {'hookEventName': 'PreToolUse',
                             'additionalContext': 'CPU GUARD ALERT: ' + json.dumps(alert)}}))
            write_json(marker, {'at': alert.get('at')})
    return 0


def notice_payload(payload):
    alert = read_json(STATE/'alert.json')
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
        print(json.dumps(dict(healthy=healthy(), verification_policy=read_json(STATE/'verification-enabled.json'), verification_admission=verification_admission(), heartbeat=read_json(STATE/'heartbeat.json'), devices=read_json(STATE/'devices-heartbeat.json'), ios_block=ios_block(), ios_admission_reason=ios_refusal(), ios_startups=[dict(id=r['id'], boot=r.get('boot')) for r in ios_records()], alert=read_json(STATE/'alert.json'))))
        return 0 if healthy() else 1
    if a.action == 'notice-json':
        result = notice_payload(json.load(sys.stdin))
        if result: print(json.dumps(result))
    if a.action == 'notice':
        if not healthy(): print('CPU GUARD UNHEALTHY: native build and device admission is closed.')
        alert = read_json(STATE/'alert.json')
        if alert: print('CPU GUARD ALERT: ' + json.dumps(alert))
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except Exception as exc:
        print('cpu-guard: ' + str(exc), file=sys.stderr)
        sys.exit(2)
