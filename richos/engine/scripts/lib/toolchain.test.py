#!/usr/bin/env python3
"""toolchain.test.py: nothing an agent or a test runs reaches an Apple developer-tools stub
without a usable toolchain behind it (scripts/lib/toolchain.py, guard-toolchain.sh, host_shield).

Every case builds a whole fake Mac under a temporary root (RICHOS_TOOLCHAIN_ROOT): its /usr/bin
stub, and whichever developer directories the case needs. The fake stub models the 2026-10-29
failure conservatively: it can use only DEVELOPER_DIR (a sandboxed or env-scrubbed process that
cannot use the xcode-select link), and with no usable directory it writes a STUB-PROMPT marker and
exits 1. That marker is the install dialog; no case may ever leave one. Nothing here runs a real
/usr/bin stub, installs anything or reads this Mac's toolchain.

  T1  Xcode in /Applications: the guard passes `make`; a sanitized build finds it and builds
  T2  Command Line Tools only: the same
  T3  no developer tools at all (the regular user's Mac): the guard stops `make` before the stub
      with a MISSING TOOL result naming the tool, why and what installing involves, plus the
      RICHOS-MISSING-TOOL line; the harness gives exit 127 and a plain line; no stub exec
  T4  Xcode on another volume, chosen with xcode-select: a sanitized build finds it and builds
  T5  DEVELOPER_DIR set to a non-developer directory while Xcode is installed: the guard refuses
      with the fix (never a missing-tool ask); the harness passes the real directory instead
  T6  asked once: the same missing tool twice gives one ask; once installed it is found and
      recorded; a present tool gives no ask across repeated runs
  T7  not a stub: a git earlier on PATH passes with no tools; `echo make` passes; the stub by its
      /usr/bin path is judged like the bare name
  T8  `xcode-select --install` is refused (it opens the dialog)
  T9  the hook script itself, on a Mac with no tools: exit 2, and its checker is never the
      /usr/bin python3 stub
  T10 host_shield.shield_env, the path every suite runs through: DEVELOPER_DIR passed through
      with a toolchain, stand-ins ahead of /usr/bin without one
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import toolchain  # noqa: E402
import host_shield  # noqa: E402

HOOK = os.path.join(os.path.dirname(HERE), "hooks", "guard-toolchain.sh")
FAKE_STUB = """#!/bin/sh
name="${0##*/}"
d="$DEVELOPER_DIR"
if [ -n "$d" ] && [ -x "$d/usr/bin/$name" ]; then exec "$d/usr/bin/$name" "$@"; fi
echo "$name" >> "$FAKE_MARKER"
echo "STUB-PROMPT: $name would open the install dialog" >&2
exit 1
"""
REAL_TOOL = """#!/bin/sh
echo "built by {label}"
"""


class FakeMac(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="toolchain-test.")
        self.marker = os.path.join(self.root, "STUB-PROMPT")
        self.state = os.path.join(self.root, "state")
        os.makedirs(os.path.join(self.root, "usr/bin"))
        for name in ("make", "git", "python3", "cc"):
            self._write(os.path.join(self.root, "usr/bin", name), FAKE_STUB)
        self.saved = {k: os.environ.get(k) for k in ("RICHOS_TOOLCHAIN_ROOT", "RICHOS_TOOLCHAIN_STATE",
                                                      "DEVELOPER_DIR", "FAKE_MARKER")}
        os.environ["RICHOS_TOOLCHAIN_ROOT"] = self.root
        os.environ["RICHOS_TOOLCHAIN_STATE"] = self.state
        os.environ["FAKE_MARKER"] = self.marker
        os.environ.pop("DEVELOPER_DIR", None)

    def tearDown(self):
        for k, v in self.saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(self.root, ignore_errors=True)

    # --- building the fake Mac -------------------------------------------------------------
    def _write(self, path, text):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text)
        os.chmod(path, 0o755)

    def developer(self, rel, label):
        d = os.path.join(self.root, rel)
        self._write(os.path.join(d, "usr/bin/make"), REAL_TOOL.format(label=label))
        return d

    def xcode_select(self, target):
        link = os.path.join(self.root, "var/db/xcode_select_link")
        os.makedirs(os.path.dirname(link), exist_ok=True)
        os.symlink(target, link)

    def sanitized(self, **extra):
        """An `env -i`-style environment: PATH of system directories only, no DEVELOPER_DIR."""
        env = {"PATH": os.pathsep.join([os.path.join(self.root, "usr/bin"), "/bin"]),
               "FAKE_MARKER": self.marker, "RICHOS_TOOLCHAIN_STATE": self.state}
        env.update(extra)
        return env

    # --- the three ways in -----------------------------------------------------------------
    def guard(self, command, env=None):
        return toolchain.verdict(command, env if env is not None else self.sanitized())

    def build(self, env):
        """`make` by bare name through the harness environment, as a suite would run it."""
        env = toolchain.harness_env(env, os.path.join(self.root, "standins"))
        return subprocess.run(["make"], env=env, capture_output=True, text=True), env

    def asks(self):
        try:
            with open(os.path.join(self.state, "asks.json"), encoding="utf-8") as fh:
                return json.load(fh)["tools"]
        except OSError:
            return {}

    def no_stub(self):
        self.assertFalse(os.path.exists(self.marker), "a stub ran with no toolchain: the install dialog would open")

    # --- cases ---------------------------------------------------------------------------------
    def test_t1_xcode_in_applications_builds(self):
        self.developer("Applications/Xcode.app/Contents/Developer", "xcode")
        self.assertEqual(self.guard("make -C project all")[0], 0)
        proc, env = self.build(self.sanitized())
        self.assertEqual((proc.returncode, proc.stdout.strip()), (0, "built by xcode"), proc.stderr)
        self.assertTrue(env["DEVELOPER_DIR"].endswith("Applications/Xcode.app/Contents/Developer"))
        self.no_stub()

    def test_t2_command_line_tools_only_builds(self):
        self.developer("Library/Developer/CommandLineTools", "clt")
        self.assertEqual(self.guard("cd project && make")[0], 0)
        proc, _env = self.build(self.sanitized())
        self.assertEqual((proc.returncode, proc.stdout.strip()), (0, "built by clt"), proc.stderr)
        self.no_stub()

    def test_t3_no_developer_tools_stops_before_the_stub_and_reports_the_missing_tool(self):
        rc, message = self.guard("make -C project all")
        self.assertEqual(rc, 2)
        for needle in ("MISSING TOOL", "`make`", "no macOS install dialog was opened",
                       "Command Line Tools for Xcode", "asks the user", "Do not run the command again"):
            self.assertIn(needle, message)
        line = [l for l in message.splitlines() if l.startswith("RICHOS-MISSING-TOOL ")][0]
        data = json.loads(line[len("RICHOS-MISSING-TOOL "):])
        self.assertEqual((data["tool"], data["ask"]), ("make", True))
        self.assertIn("install", data)
        proc, env = self.build(self.sanitized())
        self.assertEqual(proc.returncode, 127)
        self.assertIn("no install prompt was opened", proc.stderr)
        self.assertNotIn("DEVELOPER_DIR", env)
        self.no_stub()

    def test_t4_xcode_on_another_volume_builds_from_a_sanitized_environment(self):
        d = self.developer("Volumes/External/Applications/Xcode-Beta.app/Contents/Developer", "external")
        self.xcode_select("/Volumes/External/Applications/Xcode-Beta.app/Contents/Developer")
        self.assertEqual(toolchain.developer_dir(self.sanitized()), (d, "xcode-select"))
        self.assertEqual(self.guard("make")[0], 0)
        proc, env = self.build(self.sanitized())
        self.assertEqual((proc.returncode, proc.stdout.strip()), (0, "built by external"), proc.stderr)
        self.assertEqual(env["DEVELOPER_DIR"], d)
        self.no_stub()

    def test_t5_a_wrong_developer_dir_is_named_not_asked_about(self):
        d = self.developer("Applications/Xcode.app/Contents/Developer", "xcode")
        bad = self.sanitized(DEVELOPER_DIR=os.path.join(self.root, "nowhere"))
        rc, message = self.guard("make", bad)
        self.assertEqual(rc, 2)
        self.assertIn("DEVELOPER_DIR is set", message)
        self.assertIn(d, message)
        self.assertNotIn("RICHOS-MISSING-TOOL", message)
        self.assertEqual(self.asks(), {})
        proc, env = self.build(bad)
        self.assertEqual((proc.returncode, env["DEVELOPER_DIR"]), (0, d), proc.stderr)
        self.no_stub()

    def test_t6_one_ask_per_missing_tool_and_none_for_a_present_tool(self):
        payloads = []
        for _ in range(3):
            rc, message = self.guard("make")
            self.assertEqual(rc, 2)
            line = [l for l in message.splitlines() if l.startswith("RICHOS-MISSING-TOOL ")][0]
            payloads.append(json.loads(line.split(" ", 1)[1])["ask"])
        self.assertEqual(payloads, [True, False, False], "the same missing tool was asked about more than once")
        self.assertIn("ALREADY been asked", self.guard("make")[1])
        self.developer("Library/Developer/CommandLineTools", "clt")  # the user installed it
        for _ in range(3):
            self.assertEqual(self.guard("make")[0], 0)
        self.assertEqual(self.asks()["make"]["state"], "found")
        # A tool present from the start is never asked about.
        self._write(os.path.join(self.root, "Library/Developer/CommandLineTools/usr/bin/git"), REAL_TOOL.format(label="git"))
        for _ in range(5):
            self.assertEqual(self.guard("git status")[0], 0)
        self.assertEqual(self.asks()["git"]["state"], "found")
        self.assertEqual(sum(1 for r in self.asks().values() if r["state"] == "asked"), 0)
        self.no_stub()

    def test_t7_only_a_stub_is_judged(self):
        other = os.path.join(self.root, "opt/bin")
        self._write(os.path.join(other, "git"), REAL_TOOL.format(label="brew git"))
        env = self.sanitized()
        env["PATH"] = other + os.pathsep + env["PATH"]
        self.assertEqual(self.guard("git status", env)[0], 0)
        self.assertEqual(self.guard("echo make", env)[0], 0)
        self.assertEqual(self.guard("grep -n make Makefile", env)[0], 0)
        self.assertEqual(self.guard("echo 'run make later'", env)[0], 0)
        self.assertEqual(self.guard(os.path.join(self.root, "usr/bin/make") + " all", env)[0], 2)
        self.assertEqual(self.guard("FOO=1 nice make all", env)[0], 2)
        self.assertEqual(self.guard("ssh guest make all", env)[0], 0)
        self.no_stub()

    def test_t8_xcode_select_install_is_refused(self):
        self.developer("Applications/Xcode.app/Contents/Developer", "xcode")
        rc, message = self.guard("xcode-select --install")
        self.assertEqual(rc, 2)
        self.assertIn("opens the macOS install dialog", message)
        self.assertEqual(self.guard("xcode-select -p")[0], 0)

    def test_t9_the_hook_on_a_mac_without_tools(self):
        env = self.sanitized()
        env["PATH"] = os.path.dirname(sys.executable) + os.pathsep + env["PATH"]
        env.update({"RICHOS_TOOLCHAIN_ROOT": self.root})
        payload = json.dumps({"tool_name": "Bash", "tool_input": {"command": "make all"}})
        proc = subprocess.run(["/bin/bash", HOOK], input=payload, env=env, capture_output=True, text=True)
        self.assertEqual(proc.returncode, 2, proc.stderr)
        self.assertIn("RICHOS-MISSING-TOOL", proc.stderr)
        # With only the /usr/bin python3 on PATH the hook never runs it: it passes rather than exec a stub.
        env["PATH"] = os.path.join(self.root, "usr/bin") + os.pathsep + "/usr/bin"
        proc = subprocess.run(["/bin/bash", HOOK], input=payload, env=env, capture_output=True, text=True)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        quiet = json.dumps({"tool_name": "Bash", "tool_input": {"command": "ls -la"}})
        self.assertEqual(subprocess.run(["/bin/bash", HOOK], input=quiet, env=env, text=True).returncode, 0)
        self.no_stub()

    def test_t10_every_suite_gets_the_toolchain_through_host_shield(self):
        log = os.path.join(self.root, "shield.log")
        env = host_shield.shield_env(self.sanitized(), log)
        parts = env["PATH"].split(os.pathsep)
        stubs = os.path.join(self.root, "usr/bin")
        self.assertLess(parts.index(log + ".toolchain-standins"), parts.index(stubs))
        self.assertEqual(subprocess.run(["make"], env=env, capture_output=True).returncode, 127)
        d = self.developer("Applications/Xcode.app/Contents/Developer", "xcode")
        env = host_shield.shield_env(self.sanitized(), log)
        self.assertEqual(env["DEVELOPER_DIR"], d)
        proc = subprocess.run(["make"], env=env, capture_output=True, text=True)
        self.assertEqual((proc.returncode, proc.stdout.strip()), (0, "built by xcode"), proc.stderr)
        self.no_stub()


if __name__ == "__main__":
    unittest.main(verbosity=2)
