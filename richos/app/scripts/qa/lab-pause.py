#!/usr/bin/env python3
"""Hold the isolated lab Mac silent between two marks of a phone's step log.

    lab-pause.py --lab LAB_CACHE (--log TEST.log | --log-dir DIR) --stop-at REGEX --cont-at REGEX
                 [--cont-delay S] [--times N] [--timeout S]

The phone's list says when (a `mark` step: "MAC GOES AWAY", "PAUSE MAC 1"); this tool makes the
Mac go quiet at that line and come back at the second one, so a phone check can see what a
silent Mac looks like (no heartbeat, a request that hangs) at a moment the list chose. It sends
SIGSTOP and then SIGCONT to ONE process: the lab's own Rust listener, read from the lab's
`mac.json` (`pid`), never chosen by matching a name. It refuses (exit 2) a cache whose data
directory lacks the isolated lab's owner marker, and a pid whose command is not the lab's
listener (`mobile_mac_server::serve`), so it cannot pause anything else on this Mac.

`--log` is a phone-ios.py run's test log (`PHONE_STEP` lines); `--log-dir` waits for the next
`*-test.log` to appear there after this tool starts (start the tool, then the run). `{n}` in
either regex is the try number (1..N), so `--stop-at 'PAUSE MAC {n}' --times 5` follows five
tries in one list. `--cont-delay S` waits S seconds after the continue mark before continuing.

The process this tool stopped is ALWAYS continued: after each try, on a timeout, and when this
tool is stopped (SIGINT, SIGTERM, SIGHUP). Only a process it stopped: SIGCONT goes out only while
this tool holds the listener it checked and stopped, never to whatever owns the pid after a wait
(hunt part 2 v3, R35). Prints one JSON document with each stop and continue on this Mac's
clock (epoch seconds) and the mark lines matched. Exit 0 when every try was held and released;
1 when a mark did not appear before --timeout (the process is continued and that is said);
2 when it cannot answer at all.
"""
import argparse
import json
import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path

OWNER = "richos-mobile-isolated-v1"
LISTENER = "mobile_mac_server::serve"


class CannotAnswer(Exception):
    pass


def lab_pid(cache):
    state_file = Path(cache) / "mac.json"
    try:
        state = json.loads(state_file.read_text())
    except (OSError, ValueError) as error:
        raise CannotAnswer(f"cannot read the lab's {state_file}: {error}")
    data = Path(state.get("data") or Path(cache) / "manual-data")
    try:
        owner = (data / "lab-owner").read_text().strip()
    except OSError:
        owner = ""
    if owner != OWNER:
        raise CannotAnswer(f"{data} has no isolated lab owner marker: refusing to pause anything")
    pid = state.get("pid")
    if not isinstance(pid, int) or pid <= 1:
        raise CannotAnswer(f"{state_file} names no listener pid")
    check_listener(pid, data)
    return pid, data


def check_listener(pid, data):
    """Refuse unless PID is this lab's listener: the listener's command AND this lab's own data
    directory (mac-server.mjs gives each lab its RICHOS_MOBILE_MAC_DIR in the environment; every
    lab runs the same command name, so the name alone names no lab). `ps -E` appends the
    environment, which this user's own processes show."""
    done = subprocess.run(["ps", "-E", "-p", str(pid), "-o", "lstart=,command="], capture_output=True, text=True)
    command = done.stdout
    if done.returncode != 0 or LISTENER not in command:
        raise CannotAnswer(f"pid {pid} is not the lab's listener ({LISTENER}): refusing to pause it")
    # Path() collapses a doubled slash (a TMPDIR that ends in "/"), so the environment's spelling is collapsed too.
    command = re.sub(r"/{2,}", "/", command)
    if not re.search(r"(?:^|\s)RICHOS_MOBILE_MAC_DIR=" + re.escape(re.sub(r"/{2,}", "/", str(data))) + r"(?:\s|$)", command):
        raise CannotAnswer(f"pid {pid} is a listener of another lab, not the one whose data is {data}: refusing to pause it")

    birth = re.match(r"^\s*(\w{3}\s+\w{3}\s+\d+\s+\d{2}:\d{2}:\d{2}\s+\d{4})\s+", command)
    if not birth:
        raise CannotAnswer(f"cannot read the birth identity of pid {pid}")
    return " ".join(birth[1].split())


def newest_log(directory, since, deadline):
    directory = Path(directory)
    while time.time() < deadline:
        logs = [p for p in directory.glob("*-test.log") if p.stat().st_mtime >= since]
        if logs:
            return max(logs, key=lambda p: p.stat().st_mtime)
        time.sleep(0.2)
    return None


def wait_line(path, pattern, start, deadline):
    """The first line at or after byte START that matches, and the byte after it; None on timeout."""
    regex = re.compile(pattern)
    position = start
    while time.time() < deadline:
        try:
            with open(path, "rb") as handle:
                handle.seek(position)
                chunk = handle.read()
        except OSError:
            chunk = b""
        lines = chunk.split(b"\n")
        consumed = 0
        for line in lines[:-1]:
            consumed += len(line) + 1
            text = line.decode("utf-8", "replace")
            if regex.search(text):
                return text.strip(), position + consumed
        position += consumed
        time.sleep(0.1)
    return None, position


def main(argv):
    p = argparse.ArgumentParser(prog="lab-pause.py", description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--lab", required=True)
    p.add_argument("--log")
    p.add_argument("--log-dir")
    p.add_argument("--stop-at", required=True)
    p.add_argument("--cont-at", required=True)
    p.add_argument("--cont-delay", type=float, default=0.0)
    p.add_argument("--times", type=int, default=1)
    p.add_argument("--timeout", type=float, default=300.0)
    a = p.parse_args(argv)
    result = {"tries": []}
    pid = None
    # SIGKILL can end even a stopped listener. Recheck the lab and birth before continuing.
    held = []

    def resume():
        if held:
            birth = held.pop()
            if check_listener(pid, data) != birth:
                raise CannotAnswer(f"pid {pid} changed identity while stopped: refusing to continue it")
            try:
                os.kill(pid, signal.SIGCONT)
            except ProcessLookupError:
                pass
    try:
        if bool(a.log) == bool(a.log_dir):
            raise CannotAnswer("give exactly one of --log or --log-dir")
        if a.times < 1 or a.cont_delay < 0 or a.timeout <= 0:
            raise CannotAnswer("--times must be 1 or more, --cont-delay 0 or more, --timeout above 0")
        pid, data = lab_pid(a.lab)
        result["pid"] = pid
        started = time.time()
        deadline = started + a.timeout
        if a.log_dir:
            log = newest_log(a.log_dir, started, deadline)
            if log is None:
                result["error"] = f"no new *-test.log appeared in {a.log_dir} within {a.timeout:g} s"
                print(json.dumps(result, indent=2))
                return 1
        else:
            log = Path(a.log)
        result["log"] = str(log)

        def release(*_):
            resume()
            result["released"] = time.time()
            print(json.dumps(result, indent=2))
            sys.exit(1)

        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, release)
        position = 0
        for n in range(1, a.times + 1):
            try_record = {"n": n}
            result["tries"].append(try_record)
            line, position = wait_line(log, a.stop_at.replace("{n}", str(n)), position, deadline)
            if line is None:
                result["error"] = f"try {n}: no line matching the stop mark within {a.timeout:g} s"
                print(json.dumps(result, indent=2))
                return 1
            birth = check_listener(pid, data)  # recheck after the wait
            held.append(birth)  # first: a signal arriving between the two lines still continues it
            os.kill(pid, signal.SIGSTOP)
            try_record.update(stopped=round(time.time(), 3), stopMark=line[-200:])
            try:
                line, position = wait_line(log, a.cont_at.replace("{n}", str(n)), position, deadline)
                if line is not None and a.cont_delay:
                    time.sleep(a.cont_delay)
            finally:
                resume()
                try_record["continued"] = round(time.time(), 3)
            if line is None:
                result["error"] = f"try {n}: no line matching the continue mark within {a.timeout:g} s; continued anyway"
                print(json.dumps(result, indent=2))
                return 1
            try_record["contMark"] = line[-200:]
        print(json.dumps(result, indent=2))
        return 0
    except CannotAnswer as error:
        result["error"] = str(error)
        print(json.dumps(result, indent=2))
        return 2
    finally:
        resume()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
