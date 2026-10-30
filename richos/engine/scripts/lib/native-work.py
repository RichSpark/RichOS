#!/usr/bin/env python3
"""Run native builds with one machine worker, one shared native build lane and
explicit compiler/JVM caps. A healthy external circuit breaker is required on Mac.
No environment variable exempts a caller from admission.

HOW MANY CORES (2026-10-02). From September 24 every native build ran on one core
(one Gradle worker, one Swift/Xcode job, CARGO_BUILD_JOBS=1, every JVM told it had one
processor) whatever the Mac had free. The protection that one-core rule was written for
is kept by the rest of this file and the watchdog: one compiler lane, the machine worker
budget, admission below the 80 percent line, and cpu_guard's per-process runaway stop.
A build now gets the cores free at admission below a target that leaves the CEO room:

    cores = max(1, floor(cpus * (target - busy) / 100 / sharers))
    target  = the admission line (80) minus CEO_HEADROOM_PERCENT (20) = 60 percent
    busy    = total CPU (user + system) measured at admission, in percent
    sharers = machine worker permits held at admission, this build's own included

On a 10-core Mac: alone and quiet (busy 5) 5 cores; four workers admitted at once 1
each; busy 40 percent 2 cores; busy 59 percent or more 1 core. The most any build gets
is cpus * 60 / 100 (6 here), so a lone build at its allowance leaves the Mac at about
60 percent, 20 points (2 cores here) below the line where admission closes. An
unmeasured host gets 1. The decision is printed on stderr for every admitted build.

A BUILD IS JUDGED BY ITS GRANT (2026-10-02). native-work registers each admitted build with
the CPU watchdog as a native-build root carrying its grant and its workspace (cpu_guard
NATIVE_ROLE). While the build's whole process tree, plus the workspace's warm Gradle daemon,
stays within the grant, no process in it is held to the watchdog's per-process line
(JOB_CORES, 3 cores for 10 s); above the grant that line applies again, and anything
unregistered keeps it. That line had kept every JVM at one processor: a Gradle JVM given
four ran at 6.08 cores and was stopped (cpu-guard events.jsonl, pid 3022).
  - Gradle gets the grant: its daemon JVM's processors and --max-workers are
    gradle_processors(cores) = cores - JVM_OVERHEAD_CORES (at least 1), because a JVM runs
    up to four cores above its processor count in JIT and GC threads (measured: 1 processor
    peaks at 1.2-2.3 cores, 4 at 6.08 on 2026-10-02, and a NEW 2-processor daemon at 5.72 on
    2026-10-03, which the breaker stopped inside a 4-core grant, killing the speed watch's
    test-copy build; cpu-guard events.jsonl, pid 42485). Every other JVM (JAVA_TOOL_OPTIONS: the
    Gradle client, test and worker JVMs) keeps JVM_CORES (1).
  - Cargo keeps at most PROCESS_CORES (2) jobs and release Swift/Xcode compiles 1 job, as
    before (not part of this change). Debug Swift and Xcode builds get the allowance.

GRADLE STAYS WARM WITHIN A JOB (2026-10-02). The first Gradle build in a workspace starts a
daemon in that workspace's private registry; the supervisor releases it at the build's end
instead of ending it, and later builds in the same workspace reuse it. It has Gradle's
2-minute idle timeout and ends when the workspace is landed or discarded. Ownership, CPU
accounting and every way it ends: gradle_daemons.py. RICHOS_GRADLE_DAEMON=0 turns it off
(every Gradle build cold, --no-daemon, as before) and RICHOS_GRADLE_MAX_PROCESSORS=N caps the
daemon's processors; neither changes anything about admission, and neither can raise a grant.
"""
import os
from pathlib import Path
import subprocess
import sys
import time
import worker_tokens
import cpu_guard
import gradle_daemons
from cpu_policy import DEFAULT_MAX_CPU

CEO_HEADROOM_PERCENT = 20
JVM_CORES = 1
JVM_OVERHEAD_CORES = 4
PROCESS_CORES = 2
GRADLE_OVERRIDES = ('--max-workers', '-Dorg.gradle.workers.max', '-Dorg.gradle.jvmargs',
                    '-Dkotlin.daemon.jvm.options', '-Dorg.gradle.daemon')


def allowance(cpus, busy, sharers, limit=DEFAULT_MAX_CPU):
    """Cores one native build may use. `busy` None (not measured) is always 1."""
    if busy is None or cpus < 1:
        return 1
    free = cpus * (limit - CEO_HEADROOM_PERCENT - busy) / 100.0
    return max(1, int(free // max(1, sharers)))


def one_process(cores):
    """Cargo's jobs: one rustc takes every job token (see PROCESS_CORES)."""
    return max(1, min(int(cores), PROCESS_CORES))


def gradle_processors(cores):
    """The Gradle daemon's processors and workers within a grant of `cores` (JVM_OVERHEAD_CORES).
    RICHOS_GRADLE_MAX_PROCESSORS lowers it, never raises it: a brake for a watchdog that does
    not yet judge builds by their grant (the live controller before this change is deployed)."""
    processors = max(1, int(cores) - JVM_OVERHEAD_CORES)
    ceiling = os.environ.get('RICHOS_GRADLE_MAX_PROCESSORS', '')
    return max(1, min(processors, int(ceiling))) if ceiling.isdigit() else processors


def is_gradle(command):
    return os.path.basename(command[0]) in ('gradlew', 'gradle')


def project_dir(command):
    """The Gradle project the command builds: its -p/--project-dir, else the working directory."""
    args = list(command[1:])
    for i, arg in enumerate(args):
        if arg in ('-p', '--project-dir') and i + 1 < len(args):
            return args[i + 1]
        if arg.startswith('--project-dir='):
            return arg.split('=', 1)[1]
    return os.getcwd()


def daemons_enabled():
    return os.environ.get('RICHOS_GRADLE_DAEMON', '1') != '0'


def release_build(command):
    """A Swift or Xcode compile in a release configuration (whole-module, multithreaded)."""
    args = [a.lower() for a in command[1:]]
    for flag in ('-c', '--configuration', '-configuration'):
        if flag in args and args.index(flag) + 1 < len(args) and args[args.index(flag) + 1] == 'release':
            return True
    return '--configuration=release' in args or 'archive' in args


def capped(command, cores=1, daemon=None):
    command = list(command)
    cores = max(1, int(cores))
    name = os.path.basename(command[0])
    if name == 'emulator' or name.startswith('qemu-system-') or (
            name in ('simctl', 'xcrun') and (name == 'simctl' or 'simctl' in command)
            and any(a in command for a in ('boot', 'diagnose'))):
        raise ValueError('native-work does not admit devices or diagnostic dumps; use testdevices for device leases')
    if name in ('gradlew', 'gradle'):
        if any(a.startswith(GRADLE_OVERRIDES) or a in ('--parallel', '--daemon', '--no-daemon', '--foreground')
               for a in command[1:]):
            raise ValueError('worker/JVM/daemon overrides are not allowed through native admission')
        if daemon is None:
            # Cold: one JVM processor, no daemon (RICHOS_GRADLE_DAEMON=0, or a refusal check).
            command += ['--no-daemon', '--no-parallel', '--max-workers=%d' % JVM_CORES,
                        '-Dorg.gradle.jvmargs=-Xmx1536m -XX:ActiveProcessorCount=%d -Dfile.encoding=UTF-8' % JVM_CORES,
                        '-Pkotlin.compiler.execution.strategy=in-process']
        else:
            # Warm: the workspace's own daemon (gradle_daemons.prepare), at the processors the
            # grant allows. Its JVM arguments must be identical for Gradle to reuse it.
            processors = int(daemon['processors'])
            command += ['--daemon', '--no-parallel', '--max-workers=%d' % processors,
                        '-Dorg.gradle.jvmargs=-Xmx1536m -XX:ActiveProcessorCount=%d -Dfile.encoding=UTF-8' % processors,
                        '-Dorg.gradle.daemon.registry.base=%s' % daemon['registry'],
                        '-Dorg.gradle.daemon.idletimeout=%d' % gradle_daemons.IDLE_TIMEOUT_MS,
                        '-Pkotlin.compiler.execution.strategy=in-process']
    elif name == 'swift' and len(command) > 1 and command[1] in ('build', 'test'):
        if any(a in ('-j', '--jobs', '--parallel') or a.startswith('--jobs=') for a in command[2:]):
            raise ValueError('parallelism is owned by native admission')
        command += ['--jobs', '1' if release_build(command) else str(cores)]
    elif name == 'xcodebuild':
        if any(a in command for a in ('-jobs', '-parallel-testing-enabled', '-collect-test-diagnostics', '-enablePerformanceTestsDiagnostics')):
            raise ValueError('parallelism is owned by native admission')
        # Plain repetition otherwise continues after a crash, accumulating macOS
        # crash dialogs. Reliability checks should stop at their first failure.
        # Keep an explicitly chosen Xcode repetition mode intact.
        if '-test-iterations' in command and not any(a in command for a in (
                '-retry-tests-on-failure', '-run-tests-until-failure')):
            index = command.index('-test-iterations')
            if index + 1 < len(command) and command[index + 1].isdigit() and int(command[index + 1]) > 1:
                command.append('-run-tests-until-failure')
        # Compile jobs follow the allowance; parallel TESTING stays off, because it
        # clones simulators and a run holds exactly one leased device.
        command += ['-jobs', '1' if release_build(command) else str(cores), '-parallel-testing-enabled', 'NO',
                    '-collect-test-diagnostics', 'never', '-enablePerformanceTestsDiagnostics', 'NO']
    return command


def wait_for_headroom(reserve, timeout=1800):
    """Wait for admission; return the total CPU percent of the admitting sample."""
    deadline = time.monotonic() + timeout
    announced = None
    while True:
        try:
            # macOS can return identical cached counters for a subsecond read.
            # Use reserve's full interval and retry an unavailable measurement.
            sample = reserve.host_sample()
        except BlockingIOError:
            sample = None
        why = reserve._refusal(sample, reserve.DEFAULT_MAX_CPU, 16) if sample is not None else ''
        if sample is not None and not why:
            return sample['cpu_user_percent'] + sample['cpu_system_percent']
        # A nightly waiting for its test-VM guest holds every admission (reserve.py) and says so.
        held = (sample or {}).get('held_for_nightly', '')
        if time.monotonic() >= deadline:
            raise TimeoutError('native build admission timed out waiting for measurable CPU/memory headroom'
                               + (f'; last refusal: {why}' if why else ''))
        if announced != (held or 'headroom'):
            print('native-work: ' + (f'held: {held}' if held else
                                     'waiting for measurable CPU/memory headroom'), file=sys.stderr, flush=True)
            announced = held or 'headroom'
        time.sleep(3)


def sharers(directory):
    """Machine worker permits held now, this build's own included (at least 1)."""
    try:
        return max(1, worker_tokens.Budget(directory, runner=True).held())
    except (OSError, ValueError):
        return 1


def needs_compiler_lane(command):
    # A prebuilt UI test uses a leased device and a machine worker, but does
    # not compile. It must not block an unrelated Android or Swift build.
    builds = {'build', 'build-for-testing', 'test', 'archive', 'install', 'analyze', 'clean'}
    return not (os.path.basename(command[0]) == 'xcodebuild' and 'test-without-building' in command
                and not builds.intersection(command[1:]))


def announce_admission(started):
    """Tell the caller the command was ADMITTED, so a caller's time limit can start now and
    never count the wait for a worker, the compiler lane or CPU headroom. A caller that sets
    RICHOS_NATIVE_ADMITTED_FILE gets that file (atomically) holding the seconds waited. Every
    caller already sees the same fact on stderr, at the end of the admission line run() prints
    ("admitted after N s"), so this prints no second line."""
    waited = round(time.monotonic() - started, 1)
    target = os.environ.get('RICHOS_NATIVE_ADMITTED_FILE')
    if target:
        temporary = target + '.tmp'
        with open(temporary, 'w') as handle:
            handle.write(str(waited))
        os.replace(temporary, target)


def run(command):
    cpu_guard.require_managed_ancestor()
    if sys.platform == 'darwin' and not cpu_guard.healthy():
        raise RuntimeError('CPU watchdog is not healthy; native work refused. Run cpu_guard.py status.')
    capped(command)  # a refused form is refused before any wait
    if os.path.basename(command[0]) == 'xcodebuild' and any(a in command for a in ('test', 'test-without-building')):
        cpu_guard.require_ios()
    directory = worker_tokens.machine_directory()
    lane_dir = Path(directory).parent / 'native-build-v1'
    worker_tokens.init(lane_dir, 1)
    lane = token = None
    started = time.monotonic()
    try:
        # ONE ORDER, THE PROOF WORKERS' OWN: the machine worker first, the compiler lane
        # second. A proof worker already holds its slot when it reaches a build and then
        # waits for the lane. Taking the lane first here, then waiting for a slot, made a
        # circle whenever every slot belonged to proof workers waiting for this lane
        # (P5-21); only a budget timeout broke it. The lane holder always holds a slot, so
        # in this order somebody can always finish.
        # A nested check borrows exactly its parent's slot. Acquiring another
        # machine token here deadlocks when every proof worker reaches a build.
        free = os.environ.get('RICHOS_WORKER_BORROW_LOCK') if os.environ.get('RICHOS_WORKER_SLOT_HELD') == '1' else None
        token = worker_tokens.Budget(directory, runner=True).acquire(free=free)
        if needs_compiler_lane(command):
            lane = worker_tokens.Budget(lane_dir, shared=False, resource='native-build').acquire()
        busy = None
        if sys.platform == 'darwin':
            sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'app/scripts/testvm'))
            import reserve
            busy = wait_for_headroom(reserve)
            if not cpu_guard.healthy():
                raise RuntimeError('CPU watchdog stopped during admission')
        # Unmeasured (not macOS) is 1 core whoever else is admitted.
        cpus, held = os.cpu_count() or 1, sharers(directory) if busy is not None else 1
        cores = allowance(cpus, busy, held)
        gradle = is_gradle(command)
        workspace = gradle_daemons.workspace_of(project_dir(command) if gradle else os.getcwd())

        def register(pid, label, granted):
            cpu_guard.register(pid, label, cpu_guard.NATIVE_ROLE, grant={'cores': granted, 'group': workspace})
        plan = None
        if gradle and daemons_enabled():
            plan = gradle_daemons.prepare(workspace, gradle_daemons.gradle_home(), gradle_processors(cores))
        if not gradle:
            gradle_line = ''
        elif plan:
            gradle_line = ', Gradle %s daemon %d processor%s' % (
                'warm' if plan['reused'] else 'new', plan['processors'], '' if plan['processors'] == 1 else 's')
        else:
            gradle_line = ', Gradle cold 1 processor'
        # Admitted after: the wait for a machine worker, the compiler lane and CPU headroom,
        # so a build's own time can be told from its time in the queue.
        print('native-work: %d core%s; Cargo %d, a JVM %d%s (%d cpus, %s, %d worker%s admitted, target %d%%, '
              'admitted after %.1f s)' % (
                  cores, '' if cores == 1 else 's', one_process(cores), JVM_CORES, gradle_line,
                  cpus, 'CPU not measured' if busy is None else '%.0f%% busy at admission' % busy,
                  held, '' if held == 1 else 's', DEFAULT_MAX_CPU - CEO_HEADROOM_PERCENT,
                  time.monotonic() - started), file=sys.stderr, flush=True)
        command = capped(command, cores, daemon=plan)
        env = {**os.environ, 'RICHOS_MACHINE_WORKERS': directory,
               'JAVA_TOOL_OPTIONS': (os.environ.get('JAVA_TOOL_OPTIONS', '') + ' -XX:ActiveProcessorCount=%d' % JVM_CORES).strip(),
               'CARGO_BUILD_JOBS': str(one_process(cores)), 'SWIFTPM_MAX_CONCURRENT_OPERATIONS': str(cores)}
        announce_admission(started)
        # The build and the workspace's warm daemon are judged together against this grant.
        register(os.getpid(), 'native build: ' + os.path.basename(command[0]), cores)
        if plan:
            gradle_daemons.regrant(workspace, cores, register)
        rc = worker_tokens.run_command(command, token, env, release=plan['registry'] if plan else None)
        if plan:
            try:
                kept = gradle_daemons.adopt(workspace, plan, cores, register)
                if kept:
                    print('native-work: Gradle daemon %s kept warm for %s (idle timeout %d s; ended at land/discard)' % (
                        ', '.join(str(p) for p in sorted(kept)), workspace, gradle_daemons.IDLE_TIMEOUT_MS // 1000),
                        file=sys.stderr, flush=True)
            except (OSError, ValueError, RuntimeError) as exc:
                print('native-work: Gradle daemon not recorded (%s); its 2-minute idle timeout ends it' % exc,
                      file=sys.stderr, flush=True)
        return rc
    finally:
        # Reverse of the admission order: the lane first, the worker last.
        if lane: lane.release()
        if token: token.release()


if __name__ == '__main__':
    try:
        if len(sys.argv) < 3 or sys.argv[1] != '--':
            raise ValueError('usage: native-work.py -- COMMAND ...')
        sys.exit(run(sys.argv[2:]))
    except (ValueError, RuntimeError, TimeoutError) as exc:
        print('native-work: ' + str(exc), file=sys.stderr)
        sys.exit(2)
