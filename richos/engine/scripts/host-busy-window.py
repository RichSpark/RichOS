#!/usr/bin/env python3
"""host-busy-window.py — how busy the Mac was over a window, and how many
processes it created, measured the cheap way.

    python3 host-busy-window.py [SECONDS]      (default 120)

CPU comes from the kernel's own tick counters (host_statistics
HOST_CPU_LOAD_INFO), read once a second: user (with nice), system and idle,
as a share of all cores. Process creation comes from the PID counter: once a
second this tool starts /usr/bin/true and reads the PID it was given; macOS
hands PIDs out in order, so the step between two samples is how many
processes were created in that second (wrapping past 99998). The sampler's
own one process a second is subtracted.

Its cost is one tiny process and two syscalls a second, which is why it can be
left running beside the thing it measures. It prints one JSON object.
"""
import ctypes
import json
import os
import sys
import time

PID_MAX = 99998     # kern.maxpid on macOS; PIDs wrap back to 100 after it.


def ticks():
    lib = ctypes.CDLL('/usr/lib/libSystem.B.dylib')
    lib.mach_host_self.restype = ctypes.c_uint
    cpu = (ctypes.c_uint * 4)()
    count = ctypes.c_uint(4)
    if lib.host_statistics(lib.mach_host_self(), 3, ctypes.byref(cpu), ctypes.byref(count)):
        raise SystemExit('host CPU counters unavailable')
    return list(cpu)          # user, system, idle, nice


def next_pid():
    pid = os.posix_spawn('/usr/bin/true', ['true'], {})
    os.waitpid(pid, 0)
    return pid


def main():
    seconds = int(sys.argv[1]) if len(sys.argv) > 1 else 120
    start_ticks, last_pid = ticks(), next_pid()
    created, began = 0, time.time()
    for _ in range(seconds):
        time.sleep(1)
        pid = next_pid()
        step = pid - last_pid if pid >= last_pid else (PID_MAX - last_pid) + (pid - 100) + 1
        created += max(step - 1, 0)          # minus the sampler's own process
        last_pid = pid
    end_ticks, elapsed = ticks(), time.time() - began
    delta = [(b - a) % 2**32 for a, b in zip(start_ticks, end_ticks)]
    total = sum(delta) or 1
    user, system, idle = 100.0 * (delta[0] + delta[3]) / total, 100.0 * delta[1] / total, 100.0 * delta[2] / total
    print(json.dumps({'seconds': round(elapsed, 1), 'busy_percent': round(user + system, 1),
                      'user_percent': round(user, 1), 'system_percent': round(system, 1),
                      'idle_percent': round(idle, 1),
                      'processes_created_per_second': round(created / elapsed, 1)}))


if __name__ == '__main__':
    main()
