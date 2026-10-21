#!/usr/bin/env python3
"""Guest-side deadline for AX calls. Kill the owned process group, including helpers."""
import os
import json
from pathlib import Path
import tempfile
import shutil
import signal
import subprocess
import sys
import time


def phase_records():
    try:
        return [json.loads(line) for line in Path(os.environ['TESTVM_AX_PHASE_FILE']).read_text().splitlines()]
    except (KeyError, OSError, ValueError):
        return []


def host_phase():
    records = phase_records()
    phase = records[-1]['phase'] if records else 'preflight'
    if phase == 'transport':
        try:
            if '"ax_phase": "guest"' in Path(os.environ['TESTVM_AX_LOG_FILE']).read_text():
                return 'guest'
        except (KeyError, OSError):
            pass
    return phase


def timing(start, host, err):
    elapsed = time.monotonic() - start
    if not host:
        return {'ax_guest_seconds': round(elapsed, 4)}
    phases = phase_records() + [{'phase': 'done', 'at': time.monotonic()}]
    record = {'ax_timing': True, 'total_seconds': round(elapsed, 4)}
    for before, after in zip(phases, phases[1:]):
        record[before['phase'] + '_seconds'] = round(after['at'] - before['at'], 4)
    for line in err.decode(errors='replace').splitlines():
        try:
            guest = json.loads(line).get('ax_guest_seconds')
            if guest is not None:
                record['guest_seconds'] = guest
        except (ValueError, AttributeError):
            pass
    if 'transport_seconds' in record and 'guest_seconds' in record:
        # Residual includes remote startup and shell overhead, not pure SSH time.
        record['transport_overhead_seconds'] = round(max(0, record['transport_seconds'] - record['guest_seconds']), 4)
    return record


def timeout_record(host, err=b''):
    phase = host_phase() if host else 'guest'
    if phase == 'transport' and b'"ax_phase": "guest"' in err:
        phase = 'guest'
    return {'error': phase + '_deadline', 'phase': phase, 'status': 'effect_unknown',
            'detail': 'AX deadline reached; owned command group terminated; inspect before retrying'}


# ===========================================================================
# AN OVERDUE READ IS PHOTOGRAPHED WHILE IT IS STILL OVERDUE
# ===========================================================================
# esc-20261021T122950Z-b8b7362a: one read in most walks hung to this deadline at
# a different step each time while every other read answered in 0.01-0.5 s, and
# the only evidence left was "deadline reached". Whether the app's main thread was
# blocked or the guest was starved cannot be told after the kill, so a read still
# running at SLOW_SECONDS starts, once and in parallel, what decides it: `sample`
# of the app under test, of the read's own process (ax.py, the AX client) and of
# System Events (which still carries keystrokes and raw scripts), and the
# guest's own CPU and memory by `top`. The read itself is never retried and its
# deadline never moves; the snapshot only describes the read it rides on.
# 10 s, half the deadline: a `find` answers in 0.01-0.5 s, but a whole `tree` of
# 150-240 nodes took 3.0-8.1 s on a calm guest (walk-56dbcca781c4, 2026-10-21), and
# `sample` suspends what it samples, so a lower line slowed every healthy tree.
SLOW_SECONDS = float(os.environ.get('TESTVM_AX_SLOW_SECONDS', '10'))
SNAPSHOT_SAMPLE_SECONDS = 1
_snapshot_children = []


def _heaviest_path(text, limit=70):
    """The first thread of `sample`'s call graph (the main thread), heaviest branch only:
    sample sorts children by count, so the first frame at each deeper column is the path
    the thread spent most of its time on. [] when the text has no call graph."""
    lines = text.splitlines()
    try:
        start = lines.index('Call graph:') + 1
    except ValueError:
        return []
    path, column = [], -1
    for line in lines[start:]:
        if not line.strip():
            break
        digits = next((i for i, c in enumerate(line) if c.isdigit()), None)
        if digits is None:
            continue
        if path and line.startswith('    ') and line[4:5].isdigit():
            break  # the next thread
        if digits <= column:
            break  # the heaviest branch ended; what follows is a lighter sibling's
        path.append(line[digits:].split('  [0x')[0].strip()[:160])
        column = digits
        if len(path) >= limit:
            path.append('...')
            break
    return path


def _top_summary(text):
    """The second `top -l 2` sample (the first has no CPU deltas): header lines and rows."""
    blocks = text.split('Processes:')
    if len(blocks) < 2:
        return text.strip().splitlines()[-12:]
    keep = []
    for line in ('Processes:' + blocks[-1]).splitlines():
        if line.startswith(('Processes:', 'Load Avg', 'CPU usage', 'PhysMem', 'VM:', 'Swap')) or \
           line[:1].isdigit() or line.lstrip().startswith('PID'):
            keep.append(line.rstrip()[:160])
    return keep


class Snapshot:
    def __init__(self, pid, at, client=None):
        self.at = at
        self.dir = tempfile.mkdtemp(prefix='testvm-ax-slow-')
        self.jobs = []
        pids = {}
        if pid:
            pids['app under test'] = str(pid)
        # The read's own process: ax.py calls the AX API itself, so it is the AX client.
        # System Events stays sampled: keystrokes and the raw osascript reads still use it.
        if client:
            pids['AX client'] = str(client)
        # A web view's accessibility is answered by its WebContent process, not the app's.
        for label, pattern in (('System Events', ['-x', 'System Events']),
                               ('WebContent', ['-f', 'com.apple.WebKit.WebContent'])):
            try:
                found = subprocess.run(['pgrep', *pattern], capture_output=True,
                                       text=True, timeout=2).stdout.split()
            except (OSError, subprocess.SubprocessError):
                found = []
            for p in found[:2]:
                pids[label + (' ' + str(len(pids)) if label in pids else '')] = p
        self._start('top', ['top', '-l', '2', '-s', '1', '-n', '10', '-o', 'cpu',
                            '-stats', 'pid,command,cpu,state,th,mem'])
        for label, p in pids.items():
            self._start(label + ' pid ' + p, ['sample', p, str(SNAPSHOT_SAMPLE_SECONDS), '10', '-mayDie'])
        # Every process by CPU, at once: top shows ten, and a starved guest had 60 running.
        self._start('processes', ['ps', '-A', '-r', '-o', 'pid=,ppid=,pcpu=,stat=,rss=,comm='])
        self._start('memory', ['sysctl', 'vm.loadavg', 'kern.memorystatus_vm_pressure_level',
                               'kern.memorystatus_level', 'vm.swapusage'])

    def _start(self, label, argv):
        out = open(os.path.join(self.dir, str(len(self.jobs))), 'wb')
        try:
            child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=out,
                                     stderr=subprocess.STDOUT, start_new_session=True)
        except OSError as exc:
            out.write(str(exc).encode())
            child = None
        out.close()
        if child:
            _snapshot_children.append(child)
        self.jobs.append((label, child, out.name))

    def report(self, until):
        """Wait for the snapshot no later than `until`, print it, delete its files."""
        lines = []
        for label, child, path in self.jobs:
            unfinished = False
            if child is not None:
                try:
                    child.wait(timeout=max(0, until - time.monotonic()))
                except subprocess.TimeoutExpired:
                    unfinished = True
                    try:
                        os.killpg(child.pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    child.wait()
            try:
                text = open(path, errors='replace').read()
            except OSError:
                text = ''
            if label == 'top':
                body = _top_summary(text)
            elif ' pid ' in label:
                body = _heaviest_path(text)
                total = next((l.strip() for l in text.splitlines() if l.startswith('Analysis of sampling')), '')
                body = ([total] if total else []) + ['main thread, heaviest path:'] + body if body else text.strip().splitlines()[-3:]
            elif label == 'processes':
                rows = text.strip().splitlines()
                running = sum(1 for r in rows if len(r.split()) > 3 and r.split()[3].startswith('R'))
                body = ['%d processes, %d runnable (R); the 25 busiest by ps %%cpu:' % (len(rows), running)]
                body += [r[:150] for r in rows[:25]]
            else:
                body = text.strip().splitlines()
            lines.append('--- ' + label + (' (UNFINISHED at the deadline: the guest could not finish it)' if unfinished else ''))
            lines.extend('  ' + l for l in body)
        shutil.rmtree(self.dir, ignore_errors=True)
        print(json.dumps({'ax_slow_read': True, 'snapshot_started_at_seconds': round(self.at, 2)}),
              file=sys.stderr, flush=True)
        print('\n'.join(lines), file=sys.stderr, flush=True)


def host_reading():
    """The host's own admission reading and the guests' VM processes, taken WHILE a read is
    overdue (at SLOW_SECONDS on the host's clock, beside the guest's snapshot), as a JSON line.
    Diagnostics only; never raises."""
    try:
        sys.dont_write_bytecode = True
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        import reserve
        s = reserve.host_sample(0.5)
        record = {'ax_host_reading': True, 'busy_percent': round(reserve.busy_percent(s), 1),
                  'detail': reserve.describe(s)}
    except Exception as exc:  # noqa: BLE001 - a diagnostic must not change the read's outcome
        record = {'ax_host_reading': True, 'unavailable': str(exc)}
    try:
        table = subprocess.run(['ps', '-A', '-o', '%cpu=,rss=,comm='], capture_output=True, text=True, timeout=5).stdout
        record['vm_processes'] = [' '.join(l.split()[:2]) + ' cpu%,rss-kb' for l in table.splitlines()
                                  if l.rstrip().endswith('com.apple.Virtualization.VirtualMachine')]
    except (OSError, subprocess.SubprocessError):
        pass
    record['at_seconds'] = round(time.monotonic() - float(os.environ.get('TESTVM_AX_BEGAN', time.monotonic())), 2)
    return json.dumps(record)


def run(command, source, seconds, host=False, pid=None):
    began = time.monotonic()
    deadline = began + seconds
    if host:
        os.environ['TESTVM_AX_DEADLINE']=str(deadline)
    else:
        print(json.dumps({'ax_phase': 'guest'}), file=sys.stderr, flush=True)
    child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, start_new_session=True)
    def stop(signum, _frame):
        for owned in [child] + _snapshot_children:
            try:
                os.killpg(owned.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            owned.wait()
        raise SystemExit(128 + signum)
    previous = {s: signal.signal(s, stop) for s in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP)}
    snapshot = reading = None
    os.environ['TESTVM_AX_BEGAN'] = str(began)
    try:
        try:
            allowance = max(.01, seconds - 1)
            if allowance > SLOW_SECONDS:
                try:
                    out, err = child.communicate(source, timeout=SLOW_SECONDS)
                except subprocess.TimeoutExpired:
                    if host:
                        reading = host_reading()
                    else:
                        snapshot = Snapshot(pid, time.monotonic() - began, client=child.pid)
                    out, err = child.communicate(timeout=max(.01, began + allowance - time.monotonic()))
            else:
                out, err = child.communicate(source, timeout=allowance)
            if snapshot:
                snapshot.report(began + allowance)
            if reading:
                print(reading, file=sys.stderr, flush=True)
            sys.stdout.buffer.write(out)
            sys.stdout.buffer.flush()
            sys.stderr.buffer.write(err)
            sys.stderr.buffer.flush()
            print(json.dumps(timing(began, host, err)), file=sys.stderr, flush=True)
            return child.returncode if child.returncode >= 0 else 128 - child.returncode
        except subprocess.TimeoutExpired:
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            out, err = child.communicate()
            # Classification precedes partial output and process listings, also with head.
            print(json.dumps(timeout_record(host, err)), flush=True)
            sys.stdout.buffer.write(out)
            sys.stdout.buffer.flush()
            sys.stderr.buffer.write(err)
            sys.stderr.buffer.flush()
            print(json.dumps(timing(began, host, err)), file=sys.stderr, flush=True)
            if host:
                print(reading or host_reading(), file=sys.stderr, flush=True)
            if snapshot:
                snapshot.report(deadline - 1)
            # Process state is independent of a possibly blocked accessibility server.
            try:
                if host:
                    return 124
                state = subprocess.run(['ps', '-axo', 'pid,stat,comm'], capture_output=True,
                                       text=True, timeout=max(.01, deadline-time.monotonic()))
                for line in state.stdout.splitlines():
                    if any(x in line for x in ('SecurityAgent', 'richos-tauri', 'Safari')):
                        print(line, file=sys.stderr)
            except subprocess.TimeoutExpired:
                pass
            return 124
    finally:
        for s, handler in previous.items():
            signal.signal(s, handler)
        for owned in [child] + _snapshot_children:
            if owned.poll() is None:
                os.killpg(owned.pid, signal.SIGKILL)
                owned.wait()


if __name__ == '__main__':
    host = sys.argv[1:2] == ['--host']
    if host: del sys.argv[1]
    pid = None
    if sys.argv[1:2] == ['--pid']:
        pid = int(sys.argv[2]) if sys.argv[2].isdigit() and int(sys.argv[2]) > 0 else None
        del sys.argv[1:3]
    seconds = float(sys.argv[1])
    if not 1 <= seconds <= 300:
        raise SystemExit('AX timeout must be between 1 and 300 seconds')
    command = sys.argv[2:] or ['osascript', '-l', 'JavaScript', '-']
    source = sys.stdin.buffer.read()
    if host:
        # Ephemeral diagnostics only. Never use the agent registry or a durable budget.
        with tempfile.TemporaryDirectory(prefix='testvm-ax-', dir=os.environ.get('TMPDIR')) as scratch:
            os.environ['TESTVM_AX_PHASE_FILE'] = str(Path(scratch) / 'phases.jsonl')
            os.environ['TESTVM_AX_LOG_FILE'] = str(Path(scratch) / 'guest.stderr')
            Path(os.environ['TESTVM_AX_PHASE_FILE']).write_text(json.dumps({'phase': 'preflight', 'at': time.monotonic()}) + '\n')
            raise SystemExit(run(command, source, seconds, host=True))
    raise SystemExit(run(command, source, seconds, pid=pid))
