#!/usr/bin/env python3
"""Machine-wide simulator limits: two live test devices and one boot at a time.

A simulator is an OS service tree, not one worker. Keep resource leases through
shutdown, across checkouts and independent proof/nightly runs. Boot admission
also measures the host; worker availability alone cannot admit a boot storm.
"""
import hashlib
import json
import subprocess
import os
from pathlib import Path
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "testvm"))
sys.path.insert(0, str(HERE.parents[2] / "engine/scripts/lib"))
import reserve
import worker_tokens


def boot_admitted(sample):
    return not reserve._refusal(sample, reserve.DEFAULT_MAX_CPU, 16)


def booted_devices(runner=subprocess.run):
    result = runner(["xcrun", "simctl", "list", "devices", "--json"],
                    capture_output=True, text=True, check=True, timeout=15)
    devices = json.loads(result.stdout)["devices"]
    if not isinstance(devices, dict):
        raise ValueError("simulator inventory is unreadable")
    import testdevices
    android = sum(d.get("state") == "Running" for d in testdevices.android_emulators(testdevices.records()))
    return android + sum(device["state"] == "Booted" for group in devices.values() for device in group)


def acquire(kind, timeout=1800, sampler=None, cache=None, inventory=None):
    import cpu_guard
    cpu_guard.require_managed_ancestor()
    name = kind if kind != "cache" else "cache-" + hashlib.sha256(os.path.realpath(cache).encode()).hexdigest()
    directory = Path(worker_tokens.machine_directory()).parent / ("simulator-" + name + "-v1")
    worker_tokens.init(directory, 2 if kind == "live" else 1)
    budget = worker_tokens.Budget(directory, runner=True, shared=False, resource='simulator-' + name)
    deadline = time.monotonic() + timeout
    sample = sampler or reserve.host_sample
    while time.monotonic() < deadline:
        token = budget.try_acquire()
        if token is None:
            time.sleep(.2)
            continue
        held = ""
        try:
            if kind != "boot":
                return token
            measured = sample()
            # A nightly waiting for its test-VM guest holds every admission (reserve.py) and says so.
            held = measured.get("held_for_nightly", "") if isinstance(measured, dict) else ""
            if boot_admitted(measured) and (inventory or booted_devices)() < 2:
                return token
        except BaseException:
            token.release()
            raise
        token.release()
        print("simulator-budget: " + (f"boot held: {held}" if held else
                                      "boot waits for CPU/memory headroom or an existing booted device"),
              file=sys.stderr, flush=True)
        time.sleep(min(30, max(0, deadline - time.monotonic())))
    raise TimeoutError("simulator %s admission exceeded %gs" % (kind, timeout))


def main(argv):
    if len(argv) >= 4 and argv[0] == "cache" and argv[2] == "--":
        env = {**os.environ, "RICHOS_SIMULATOR_CACHE_HELD": os.path.realpath(argv[1])}
        return worker_tokens.run_command(argv[3:], acquire("cache", cache=argv[1]), env=env, worker=False)
    if len(argv) < 3 or argv[0] not in ("live", "boot") or argv[1] != "--":
        print("usage: simulator_budget.py live|boot -- COMMAND ... OR cache DIRECTORY -- COMMAND ...", file=sys.stderr)
        return 64
    return worker_tokens.run_command(argv[2:], acquire(argv[0]), worker=False)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
