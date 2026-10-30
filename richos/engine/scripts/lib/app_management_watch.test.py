#!/usr/bin/env python3
"""app_management_watch.test.py: the read-only watcher that records the evidence behind the next
macOS App Management notice naming RichOS (scripts/lib/app_management_watch.py).

  R1  a kernel `deny(1) file-write-...` line is recorded with its process, pid and full path,
      from a System Policy line and from a Sandbox line
  R2  tccd's App Management request is recorded whole: its AUTHREQ_CTX, and the AUTHREQ_ATTRIBUTION
      and AUTHREQ_RESULT with the same msgID, whichever order they arrive in, with binary paths
  R3  everything else is left out: another TCC service's request, a kernel read denial, another
      process, a line that is not JSON
  R4  the predicate keeps device writes in logd (the noise) and asks for exactly the three kinds
  R5  the real child: started on a fifo, it records a line written to it; and while nothing
      arrives it uses no CPU (it blocks; there is no polling loop)
  R6  it ends by itself when the host process it was given exits (the app crashed)
  R7  the ledger is bounded: past the cap it moves to ledger.1.jsonl
  R8  `log` accepts the predicate (read-only, two seconds; skipped where /usr/bin/log is absent)
"""
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.dont_write_bytecode = True
import app_management_watch as amw  # noqa: E402

SCRIPT = os.path.join(HERE, "app_management_watch.py")


def kernel(message, ts="2026-10-28 12:16:53.100000+0000"):
    return {"timestamp": ts, "processImagePath": "/kernel", "processID": 0, "eventMessage": message,
            "subsystem": "com.apple.sandbox.reporting", "category": "violation"}


def tccd(message, ts="2026-10-28 12:16:53.200000+0000"):
    return {"timestamp": ts, "processImagePath": "/System/Library/PrivateFrameworks/TCC.framework/Support/tccd",
            "processID": 162, "eventMessage": message}


CTX = ("AUTHREQ_CTX: msgID=651.4471, function=TCCAccessRequest, service=kTCCServiceSystemPolicyAppBundles, "
       "preflight=yes, query=1, client_dict=(null), daemon_dict=0x0")
ATTR = ("AUTHREQ_ATTRIBUTION: msgID=651.4471, attribution={responsible={TCCDProcess: identifier=com.richos.app, "
        "pid=811, auid=501, euid=501, responsible_path=/Applications/RichOS.app/Contents/MacOS/richos-tauri, "
        "binary_path=/Applications/RichOS.app/Contents/MacOS/richos-tauri}, accessing={TCCDProcess: "
        "identifier=python3, pid=9120, auid=501, euid=501, binary_path=/opt/fixture/bin/python3.14}, },")
RESULT = "AUTHREQ_RESULT: msgID=651.4471, authValue=0, authReason=5, authVersion=1, error=(null),"
OTHER_CTX = "AUTHREQ_CTX: msgID=300.12, function=TCCAccessRequest, service=kTCCServiceScreenCapture, preflight=yes"
OTHER_ATTR = ("AUTHREQ_ATTRIBUTION: msgID=300.12, attribution={accessing={TCCDProcess: identifier=x, pid=1, "
              "binary_path=/usr/sbin/screencapture}, },")
LEASE = ("System Policy: python3.14(9120) deny(1) file-write-create "
         "/Users/fixture/Library/Application Support/com.richos.app/engine-state/provider-leases/.tmp-lease-1.json")


class Ledger(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="amw-test-")
        self.ledger = os.path.join(self.root, "app-management", "ledger.jsonl")
        self.rec = amw.Recorder(self.ledger)

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def lines(self):
        if not os.path.exists(self.ledger):
            return []
        with open(self.ledger, encoding="utf-8") as fh:
            return [json.loads(l) for l in fh if l.strip()]

    def test_r1_kernel_write_denials_are_recorded_with_full_paths(self):
        self.rec.event(kernel(LEASE))
        self.rec.event(kernel("Sandbox: node(4410) deny(1) file-write-unlink /Applications/Fixture.app/Contents/Info.plist"))
        got = self.lines()
        self.assertEqual([r["kind"] for r in got], ["kernel-deny", "kernel-deny"])
        self.assertEqual(got[0]["process"], "kernel")
        self.assertEqual(got[0]["at"], "2026-10-28 12:16:53.100000+0000")
        self.assertEqual(got[0]["message"], LEASE)
        self.assertEqual(got[0]["paths"], ["/Users/fixture/Library/Application Support/com.richos.app/"
                                           "engine-state/provider-leases/.tmp-lease-1.json"], "the full path, spaces kept")
        self.assertEqual(got[1]["paths"], ["/Applications/Fixture.app/Contents/Info.plist"])

    def test_r2_the_app_management_request_is_recorded_whole_in_either_order(self):
        for message in (CTX, ATTR, RESULT):
            self.rec.event(tccd(message))
        got = self.lines()
        self.assertEqual([r["kind"] for r in got], ["tcc-context", "tcc-attribution", "tcc-result"])
        self.assertTrue(all(r["msgID"] == "651.4471" for r in got))
        self.assertIn("/Applications/RichOS.app/Contents/MacOS/richos-tauri", got[1]["paths"])
        self.assertIn("/opt/fixture/bin/python3.14", got[1]["paths"])
        # The attribution first, its context after: still paired, nothing lost.
        os.remove(self.ledger)
        rec = amw.Recorder(self.ledger)
        rec.event(tccd(ATTR.replace("651.4471", "651.5000")))
        self.assertEqual(self.lines(), [], "an attribution was recorded before its service was known")
        rec.event(tccd(CTX.replace("651.4471", "651.5000")))
        self.assertEqual([r["kind"] for r in self.lines()], ["tcc-context", "tcc-attribution"])

    def test_r3_everything_else_is_left_out(self):
        self.rec.event(tccd(OTHER_CTX))
        self.rec.event(tccd(OTHER_ATTR))
        self.rec.event(kernel("System Policy: du(1) deny(1) file-read-data /Users/fixture/Library/Containers/x"))
        self.rec.event({"processImagePath": "/usr/libexec/other", "eventMessage": "deny(1) file-write-data /x"})
        self.assertEqual(self.rec.line("Filtering the log data using ..."), [])
        self.assertEqual(self.rec.line("{not json"), [])
        self.assertEqual(self.lines(), [])

    def test_r4_the_predicate_asks_for_the_three_kinds_and_leaves_device_writes_in_logd(self):
        p = amw.PREDICATE
        for needle in ('process == "kernel"', '"deny("', '"file-write"', 'NOT eventMessage CONTAINS " /dev/"',
                       'process == "tccd"', '"AUTHREQ_CTX"', '"AUTHREQ_ATTRIBUTION"', '"AUTHREQ_RESULT"'):
            self.assertIn(needle, p)
        cmd = amw.stream_command()
        self.assertEqual(cmd[:2], ["/usr/bin/log", "stream"])
        self.assertIn("ndjson", cmd)
        self.assertNotIn("debug", " ".join(cmd), "debug level would wake it for every debug line")

    def test_r7_the_ledger_is_bounded(self):
        os.makedirs(os.path.dirname(self.ledger), exist_ok=True)
        with open(self.ledger, "w") as fh:
            fh.write("x" * (amw.MAX_LEDGER_BYTES + 1))
        self.rec.event(kernel(LEASE))
        self.assertTrue(os.path.exists(self.ledger[:-len(".jsonl")] + ".1.jsonl"))
        self.assertEqual(len(self.lines()), 1)


def cpu_seconds(pid):
    out = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
    if not out:
        return None
    parts = out.replace("-", ":").split(":")
    total = 0.0
    for part in parts:
        total = total * 60 + float(part)
    return total


class Child(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="amw-child-")
        self.fifo = os.path.join(self.root, "events")
        os.mkfifo(self.fifo)
        self.state = os.path.join(self.root, "engine-state")
        self.ledger = os.path.join(self.state, "app-management", "ledger.jsonl")
        self.procs = []

    def tearDown(self):
        for p in self.procs:
            if p.poll() is None:
                p.kill()
            p.wait()
        shutil.rmtree(self.root, ignore_errors=True)

    def spawn(self, argv, **kw):
        env = {k: v for k, v in os.environ.items() if not k.startswith("RICHOS_")}
        env.update(kw.pop("env", {}))
        p = subprocess.Popen(argv, env=env, stdin=subprocess.DEVNULL, **kw)
        self.procs.append(p)
        return p

    def wait_for(self, predicate, bound=30.0):
        began = time.monotonic()
        while not predicate() and time.monotonic() - began < bound:
            time.sleep(0.05)
        return predicate()

    def test_r5_the_child_records_a_line_and_uses_no_cpu_while_idle(self):
        watcher = self.spawn([sys.executable, "-B", SCRIPT, "--state", self.state, "--input", self.fifo])
        writer = open(self.fifo, "w")   # the watcher's open of the fifo completes now
        try:
            time.sleep(1.0)               # past its start-up
            before = cpu_seconds(watcher.pid)
            time.sleep(3.0)               # nothing arrives
            after = cpu_seconds(watcher.pid)
            self.assertIsNotNone(before)
            self.assertLessEqual(after - before, 0.02,
                                 "the idle watcher used %.2f s of CPU in 3 s; it must block, not poll" % (after - before))
            writer.write(json.dumps(kernel(LEASE)) + "\n")
            writer.flush()
            self.assertTrue(self.wait_for(lambda: os.path.exists(self.ledger) and os.path.getsize(self.ledger) > 0),
                            "the line never reached the ledger")
            with open(self.ledger) as fh:
                self.assertEqual(json.loads(fh.readline())["kind"], "kernel-deny")
        finally:
            writer.close()
        self.assertEqual(watcher.wait(timeout=30), 0)

    def test_r6_it_ends_when_its_host_exits(self):
        host = self.spawn(["/bin/sleep", "600"])
        watcher = self.spawn([sys.executable, "-B", SCRIPT, "--state", self.state, "--input", self.fifo],
                             env={"RICHOS_WATCH_HOST": str(host.pid)})
        writer = open(self.fifo, "w")
        try:
            time.sleep(0.5)
            self.assertIsNone(watcher.poll(), "the watcher ended while its host was alive")
            host.kill()
            host.wait()
            self.assertTrue(self.wait_for(lambda: watcher.poll() is not None, 30),
                            "the watcher outlived its host")
        finally:
            writer.close()

    def test_r8_log_accepts_the_predicate(self):
        if not os.path.exists("/usr/bin/log"):
            self.skipTest("no /usr/bin/log here")
        out = subprocess.run([sys.executable, "-B", SCRIPT, "--check-predicate"], capture_output=True, text=True,
                             timeout=120)
        self.assertEqual(out.returncode, 0, out.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
