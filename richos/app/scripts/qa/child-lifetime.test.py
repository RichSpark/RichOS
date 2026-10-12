#!/usr/bin/env python3
"""No phone needed: a stand-in idevicesyslog must END after kill -9 of phone-ios.py.

WHAT IS ASSERTED IS THAT IT ENDS, NOT HOW FAST (2026-10-05). This asserted "within 5 s" and failed
once in the merge gate (2026-10-02 16:59): the watchdog that ends the child is a Python process of
its own, and on a loaded Mac its interpreter can take seconds to start, then it polls every 0.5 s.
Reproduced by delaying every Python start by 6 s (a sitecustomize.py that sleeps): the child
ended about 6 s after the kill and the 5 s bound failed; the defect it guards against, a child
that outlives the tool, is still caught. So the bound is only an outer limit, OUTER_LIMIT seconds,
and a child that has exited but is not yet reaped (a zombie; kill(pid, 0) still succeeds) is ended.
"""
import os, signal, subprocess, sys, tempfile, time, unittest
from pathlib import Path

TOOL = Path(__file__).with_name("phone-ios.py")
ROOT = tempfile.gettempdir()
OUTER_LIMIT = 60


def alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    state = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)], capture_output=True, text=True).stdout
    return not state.strip().startswith("Z")


class ChildLifetime(unittest.TestCase):
    def test_child_dies_when_tool_is_sigkilled(self):
        os.makedirs(ROOT, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=ROOT) as d:
            fake = Path(d, "idevicesyslog")
            fake.write_text('#!/bin/sh\ntrap "" PIPE\necho $$ > "%s/pid"\nwhile :; do echo RichOSNative line 2>/dev/null; sleep 0.2; done\n' % d)
            fake.chmod(0o755)
            env = dict(os.environ, PATH=d + ":" + os.environ["PATH"], RICHOS_DEVICE_VERB="rios")  # started the way `rios device` starts it
            tool = subprocess.Popen([sys.executable, str(TOOL), "syslog", "--device", "X", "--seconds", "600",
                                     "--out", d + "/out"], env=env, stdout=subprocess.DEVNULL,
                                    stderr=subprocess.DEVNULL)
            pidfile = Path(d, "pid")
            end = time.time() + 10
            while not pidfile.exists() and time.time() < end:
                time.sleep(0.1)
            # wait for the fact: the tool has both of its children (the relay and its watchdog)
            while time.time() < end:
                kids = subprocess.run(["pgrep", "-P", str(tool.pid)], capture_output=True, text=True).stdout.split()
                if len(kids) >= 2:
                    break
                time.sleep(0.1)
            child = int(pidfile.read_text())
            self.assertTrue(alive(child))
            tool.send_signal(signal.SIGKILL)
            tool.wait()
            killed = time.time()
            end = killed + OUTER_LIMIT
            while alive(child) and time.time() < end:
                time.sleep(0.1)
            gone = not alive(child)
            if not gone:
                os.kill(child, signal.SIGKILL)  # the pid this test read from its own stand-in
            self.assertTrue(gone, "the child outlived the tool by %d s" % OUTER_LIMIT)
            print("child ended %.1f s after the tool was killed" % (time.time() - killed), file=sys.stderr)


if __name__ == "__main__":
    unittest.main()
