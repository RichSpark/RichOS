#!/usr/bin/env python3
"""CPU admission for one command, using the engine's own admission policy.

    admit.py --wait SECONDS -- COMMAND [ARGS...]

Samples the host CPU the way the engine's CPU guard does (cpu_guard.host_ticks),
asks cpu_policy whether total CPU (user + system) is under the limit, and waits up
to SECONDS for the Mac to quiet. Then it runs COMMAND in place. Exit 75 when the
wait ends with the Mac still busy; the command is not run.

Both modules live beside this skill in the installed engine (scripts/lib), so
nothing outside the engine is needed.
"""
import os
import sys
import time

LIB = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'scripts', 'lib')
sys.path.insert(0, LIB)
from cpu_policy import DEFAULT_MAX_CPU, admission_open, busy_percent  # noqa: E402
import cpu_guard  # noqa: E402

POLL = 5.0


def sample():
    """One CPU sample over one second, shaped as busy_percent expects."""
    before = cpu_guard.host_ticks()
    time.sleep(1)
    after = cpu_guard.host_ticks()
    if before is None or after is None:
        raise BlockingIOError('host CPU counters unavailable')
    user, system, idle, nice = [(b - a) % 2 ** 32 for a, b in zip(before, after)]
    total = user + system + idle + nice
    if total <= 0:
        raise BlockingIOError('host CPU counters did not advance')
    return {'cpu_user_percent': 100.0 * (user + nice) / total,
            'cpu_system_percent': 100.0 * system / total,
            'cpu_idle_percent': 100.0 * idle / total}


def main(argv):
    if len(argv) < 4 or argv[0] != '--wait' or '--' not in argv:
        print('usage: admit.py --wait SECONDS -- COMMAND [ARGS...]', file=sys.stderr)
        return 64
    wait = float(argv[1])
    command = argv[argv.index('--') + 1:]
    if not command:
        print('admit: no command given', file=sys.stderr)
        return 64
    deadline = time.monotonic() + wait
    announced = False
    while True:
        try:
            busy = busy_percent(sample())
        except BlockingIOError as error:
            reason = str(error)
        else:
            reason = 'host CPU %.0f%% busy, limit %d%%' % (busy, DEFAULT_MAX_CPU)
            if admission_open(busy, DEFAULT_MAX_CPU):
                os.execvp(command[0], command)
        if time.monotonic() >= deadline:
            print('admit: refused: %s' % reason, file=sys.stderr)
            return 75
        if not announced:
            print('admit: waiting for the Mac to quiet (%s)' % reason, file=sys.stderr)
            announced = True
        time.sleep(POLL)


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
