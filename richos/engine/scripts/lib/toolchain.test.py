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
  T15 off macOS the harness changes nothing: no stand-ins, the real /usr/bin tools stay found
  T16 a shell heredoc after a here-string (<<<), a comment or quoted text naming `<<EOF` is still
      read as commands; one inside "$( ... )" too
  T17 a missing tool with no permission: the ask (with the yes and no commands) and no install
  T18 the user's yes opens the installer once and returns (so the user is told to press Install);
      the next run waits for it and the step runs; a repeated yes or install installs nothing
  T19 a general permission stored in the user's orchestration.config (off by default, other
      lines kept): no ask, one install; switched off again, it asks
  T20 a second request for the same tool: after a no it neither asks nor installs; two requests
      at once and waits that run out before the download ends open the installer once
The fake installer at the fake Mac's /usr/bin/xcode-select counts its runs; nothing real installs.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import unittest.mock

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

    def test_t11_the_dispatcher_runs_the_rule_for_every_stub(self):
        # Review 2026-10-29: `# dispatch-words:` named nine tools, so the dispatcher skipped this
        # rule for `pip3 install` and the stub opened the installer. The dispatcher looks for each
        # word as a substring of the payload, so every stub name must CONTAIN one of the words.
        with open(HOOK, encoding="utf-8") as fh:
            words = next(l for l in fh if l.startswith("# dispatch-words:")).split(":", 1)[1].split()
        uncovered = sorted(s for s in toolchain.SHIMS | {"xcode-select"} if not any(w in s for w in words))
        self.assertEqual(uncovered, [], "the dispatcher would skip guard-toolchain for these stubs")
        # The dispatcher's own skip code (the PYWORDS program in dispatch-pretooluse.sh), run as it
        # runs there: it prints the modules it would NOT fork for this payload.
        dispatcher = os.path.join(os.path.dirname(HOOK), "dispatch-pretooluse.sh")
        with open(dispatcher, encoding="utf-8") as fh:
            text = fh.read()
        program = text.split("<<'PYWORDS'", 1)[1].split("\n", 1)[1].split("\nPYWORDS\n", 1)[0]
        for command in ("pip3 install requests", "lldb ./a.out", "otool -L x", "xcodebuild -list"):
            payload = json.dumps({"tool_name": "Bash", "tool_input": {"command": command}})
            skipped = subprocess.run([sys.executable, "-c", program, os.path.dirname(HOOK), "guard-toolchain.sh"],
                                     input=payload, capture_output=True, text=True).stdout.split()
            self.assertNotIn("guard-toolchain.sh", skipped, "the dispatcher skips guard-toolchain for `%s`" % command)
        # And the hook, run once per stub on a Mac without developer tools, stops every one of them:
        # its own cheap prefilter names them all. The caller's PATH has the stubs BEFORE the python
        # the hook runs on; the hook must resolve with that PATH, not with its python first (the
        # test python's folder may hold a real pip3 that would hide the stub).
        env = self.sanitized()
        env["PATH"] = os.pathsep.join([os.path.join(self.root, "usr/bin"), os.path.dirname(sys.executable), "/bin"])
        env.update({"RICHOS_TOOLCHAIN_ROOT": self.root})
        missed = []
        for name in sorted(toolchain.SHIMS):
            self._write(os.path.join(self.root, "usr/bin", name), FAKE_STUB)
            for command in ("%s --version" % name, name):
                payload = json.dumps({"tool_name": "Bash", "tool_input": {"command": command}})
                proc = subprocess.run(["/bin/bash", HOOK], input=payload, env=env, capture_output=True, text=True)
                if proc.returncode != 2 or ("`%s`" % name) not in proc.stderr:
                    missed.append("%s (rc %d)" % (command, proc.returncode))
        self.assertEqual(missed, [], "the hook let these stubs through")
        self.no_stub()

    def test_t12_the_command_own_environment_picks_the_executable_and_the_toolchain(self):
        # Review 2026-10-29: with another git first on the hook's PATH, `env -i PATH=/usr/bin:/bin
        # git status` passed although the shell would run Apple's stub.
        other = os.path.join(self.root, "opt/bin")
        self._write(os.path.join(other, "git"), REAL_TOOL.format(label="bundled git"))
        stubs = os.path.join(self.root, "usr/bin")
        env = self.sanitized()
        env["PATH"] = other + os.pathsep + env["PATH"]
        self.assertEqual(self.guard("git status", env)[0], 0)
        for command in ("env -i PATH=%s:/bin git status" % stubs,
                        "PATH=%s git status" % stubs,
                        "env -i git status",
                        "nohup env -i git status",
                        "export PATH=%s:/bin; git status" % stubs,
                        "PATH=%s:/bin; git log" % stubs,
                        "env -P %s git status" % stubs,
                        "bash -c 'env -i git status'"):
            rc, message = self.guard(command, env)
            self.assertEqual(rc, 2, "%s passed: %s" % (command, message))
            self.assertIn("`git`", message)
        self.assertEqual(self.guard("env PATH=%s:%s git status" % (other, stubs), env)[0], 0)
        # DEVELOPER_DIR, too: the command's own value decides, set or unset.
        d = self.developer("Applications/Xcode.app/Contents/Developer", "xcode")
        rc, message = self.guard("DEVELOPER_DIR=%s make" % os.path.join(self.root, "nowhere"))
        self.assertEqual(rc, 2)
        self.assertIn("DEVELOPER_DIR is set", message)
        bad = self.sanitized(DEVELOPER_DIR=os.path.join(self.root, "nowhere"))
        self.assertEqual(self.guard("env -u DEVELOPER_DIR make", bad)[0], 0)
        self.assertEqual(self.guard("DEVELOPER_DIR=%s make" % d, bad)[0], 0)
        self.no_stub()

    def test_t13_a_heredoc_fed_to_a_shell_is_read_as_commands(self):
        # Review 2026-10-30: `bash <<'EOF'` running make passed on a Mac without developer tools.
        for command in ("bash <<'EOF'\nmake -C project all\nEOF",
                        "nohup sh <<EOF\necho start\nmake\nEOF",
                        "zsh -s <<'EOF'\ncd project && make\nEOF"):
            rc, message = self.guard(command)
            self.assertEqual(rc, 2, "%r passed: %s" % (command, message))
            self.assertIn("`make`", message)
        # Text, not commands: a heredoc written to a file, one sent to the guest, and one a shell
        # runs with -c (its script is the -c argument, the heredoc is only its input). A shell
        # heredoc inside a file's or the guest's heredoc is text too (review 2026-10-30).
        for command in ("cat > notes.sh <<'EOF'\nmake\nEOF",
                        "ssh vm bash <<'EOF'\nmake\nEOF",
                        "bash -c 'cat' <<'EOF'\nmake\nEOF",
                        "cat > notes.sh <<'SCRIPT'\nbash <<'BUILD'\n/usr/bin/make\nBUILD\nSCRIPT",
                        "ssh vm bash <<'REMOTE'\nbash <<'BUILD'\n/usr/bin/make\nBUILD\nREMOTE"):
            self.assertEqual(self.guard(command)[0], 0, command)
        # With developer tools the same heredoc runs.
        self.developer("Library/Developer/CommandLineTools", "clt")
        self.assertEqual(self.guard("bash <<'EOF'\nmake -C project all\nEOF")[0], 0)
        self.no_stub()

    def test_t14_xcrun_runs_wherever_a_developer_directory_is_usable(self):
        # Review 2026-10-30: xcrun is /usr/bin's own resolver; Xcode's Developer/usr/bin has no
        # copy of it, so every `xcrun` command was refused on a Mac with Xcode.
        self._write(os.path.join(self.root, "usr/bin", "xcrun"), FAKE_STUB)
        rc, message = self.guard("xcrun --show-sdk-path")
        self.assertEqual(rc, 2, "no developer tools: xcrun would open the install dialog")
        self.assertIn("`xcrun`", message)
        for rel, label in (("Applications/Xcode.app/Contents/Developer", "xcode"),
                           ("Library/Developer/CommandLineTools", "clt")):
            d = self.developer(rel, label)
            self.assertFalse(os.path.exists(os.path.join(d, "usr/bin/xcrun")))
            for command in ("xcrun --show-sdk-path", "xcrun simctl list devices", "xcrun clang --version"):
                self.assertEqual(self.guard(command, self.sanitized(DEVELOPER_DIR=d))[0], 0, "%s with %s" % (command, label))
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

    def test_t15_the_stand_ins_are_for_macos_only(self):
        # Review rv-20261030T130236Z-aaf88b85-fcb0: the stubs are Apple's. Anywhere else /usr/bin/git
        # and make are the real tools, and a stand-in ahead of them would hide them.
        env = self.sanitized()
        standins = os.path.join(self.root, "standins")
        with unittest.mock.patch.object(toolchain.sys, "platform", "linux"):
            self.assertEqual(toolchain.harness_env(env, standins), env)
        self.assertFalse(os.path.exists(standins), "no stand-in was written off macOS")
        with unittest.mock.patch.object(toolchain.sys, "platform", "darwin"):
            self.assertIn(standins, toolchain.harness_env(env, standins)["PATH"].split(os.pathsep))
        self.no_stub()

    def test_t16_a_heredoc_after_a_here_string_a_comment_or_quoted_text_is_still_read(self):
        # Review rv-20261030T162409Z-41c917da-c71e finding 1: `<<<`, a comment and quoted text were
        # read as heredoc heads, whose "bodies" swallowed the real heredoc after them.
        for command in ("cat <<< hello\nbash <<'EOF'\nmake\nEOF",
                        "ls  # writes <<EOF somewhere\nbash <<'EOF'\nmake\nEOF",
                        "echo \"use <<EOF here\"\nbash <<'X'\nmake\nX",
                        "echo 'use <<EOF here'\nbash <<X\nmake\nX",
                        # review rv-20261030T221210Z-f238edab-a661: ANSI quoting and arithmetic
                        "echo $'it\\'s'\nbash <<'EOF'\nmake\nEOF",
                        "x=$((1 << SHIFT))\nbash <<EOF\nmake\nEOF",
                        "out=\"$(bash <<EOF\nmake\nEOF\n)\""):
            rc, message = self.guard(command)
            self.assertEqual(rc, 2, "%r passed: %s" % (command, message))
            self.assertIn("`make`", message)
        self.no_stub()


    # --- the ask, the answer and the install (CEO 2026-10-29) -------------------------------
    def installer(self, delay=0.0, installs=True):
        """A fake Apple installer at the fake Mac's /usr/bin/xcode-select: each run appends one
        line to INSTALLS and, like the real one after the user presses Install, puts the Command
        Line Tools (make and git) in place, `delay` seconds later."""
        self.installs = os.path.join(self.root, "INSTALLS")
        clt = os.path.join(self.root, "Library/Developer/CommandLineTools/usr/bin")
        body = 'mkdir -p "%s" && for t in make git; do printf "#!/bin/sh\\necho built\\n" > "%s/$t"; chmod 755 "%s/$t"; done' % (clt, clt, clt)
        self._write(os.path.join(self.root, "usr/bin/xcode-select"),
                    "#!/bin/sh\necho \"$*\" >> \"%s\"\n%s\n" % (self.installs, (
                        "(sleep %s; %s) >/dev/null 2>&1 &" % (delay, body) if delay else body) if installs else ":"))

    def install_count(self):
        try:
            with open(self.installs, encoding="utf-8") as fh:
                return len(fh.read().splitlines())
        except OSError:
            return 0

    def payload(self, message):
        line = [l for l in message.splitlines() if l.startswith("RICHOS-MISSING-TOOL ")][0]
        return json.loads(line.split(" ", 1)[1])

    def user_env(self):
        folder = os.path.join(self.root, "myrichos")
        os.makedirs(folder, exist_ok=True)
        return self.sanitized(RICHOS_USER_FOLDER=folder)

    def run_install(self, env, name, said=None, wait=10):
        import io
        out = io.StringIO()
        if said:
            rc = toolchain.answer(env, name, said, out=out, wait=wait, poll=0.05)
        else:
            rc = toolchain.install(env, name, wait=wait, poll=0.05, out=out)
        return rc, out.getvalue()

    def test_t17_a_missing_tool_without_permission_is_asked_about_and_never_installed(self):
        self.installer()
        env = self.user_env()
        rc, message = self.guard("make all", env)
        self.assertEqual(rc, 2)
        data = self.payload(message)
        self.assertEqual((data["tool"], data["ask"], data["permitted"]), ("make", True, False))
        self.assertIn("asks the user", message)
        self.assertIn(data["yes_command"], message)
        self.assertIn(data["no_command"], message)
        self.assertFalse(toolchain.general_permission(env), "the general permission is off by default")
        # The commands survive a shell where the engine is fetched to: ~/Library/Application Support.
        import shlex
        spaced = ("/Users/x/Library/Application Support/RichOS/engine/runtime/bin/python3",
                  "/Users/x/Library/Application Support/RichOS/engine/scripts/lib/toolchain.py")
        with unittest.mock.patch.object(toolchain.sys, "executable", spaced[0]), \
                unittest.mock.patch.object(toolchain, "__file__", spaced[1]):
            for key, tail in toolchain._commands("make").items():
                self.assertEqual(shlex.split(tail)[:2], list(spaced), key)
        rc, said = self.run_install(env, "make")
        self.assertEqual(rc, 3, said)
        self.assertIn("has not given permission", said)
        self.assertEqual(self.install_count(), 0, "installed without the user's yes")
        self.no_stub()

    def test_t18_a_yes_installs_once_and_the_work_continues(self):
        self.installer()
        env = self.user_env()
        self.assertEqual(self.guard("make all", env)[0], 2)
        rc, said = self.run_install(env, "make", said="yes")
        self.assertEqual(rc, 4, "the yes opens the installer and returns, so the user can be told: " + said)
        self.assertIn("presses Install", said)
        rc, said = self.run_install(env, "make")
        self.assertEqual(rc, 0, said)
        self.assertIn("INSTALLED `make`", said)
        self.assertEqual(self.install_count(), 1)
        self.assertEqual(self.guard("make all", env)[0], 0, "the work that needed it runs now")
        # Asked again, installed again: never. The tool is checked first and found.
        for again in ("yes", None):
            rc, said = self.run_install(env, "make", said=again)
            self.assertEqual(rc, 0, said)
            self.assertIn("ALREADY-INSTALLED", said)
        self.assertEqual(self.install_count(), 1, "the same tool was installed twice")
        # git came with the same install: no ask, no install.
        self.assertEqual(self.guard("git status", env)[0], 0)
        self.assertEqual(self.install_count(), 1)
        self.no_stub()

    def test_t19_a_stored_general_permission_installs_without_asking(self):
        self.installer()
        env = self.user_env()
        settings = os.path.join(env["RICHOS_USER_FOLDER"], "orchestration.config")
        with open(settings, "w", encoding="utf-8") as fh:
            fh.write('# the user\'s own values\nMODEL_CEILING="opus"\n')
        self.assertEqual(toolchain.set_general_permission(env, True), settings)
        with open(settings, encoding="utf-8") as fh:
            text = fh.read()
        self.assertIn('MODEL_CEILING="opus"', text, "the user's other values are kept")
        self.assertIn('INSTALL_MISSING_TOOLS="yes"', text)
        self.assertTrue(toolchain.general_permission(env))
        rc, message = self.guard("make all", env)
        self.assertEqual(rc, 2, "the stub still never runs")
        data = self.payload(message)
        self.assertEqual((data["ask"], data["permitted"]), (False, True))
        self.assertNotIn("The lead asks the user", message)
        self.assertNotIn("Stop this step", message)
        self.assertIn(data["install_command"], message)
        self.assertNotIn("make", self.asks(), "nobody was asked")
        self.assertEqual(self.run_install(env, "make")[0], 4)
        rc, said = self.run_install(env, "make")
        self.assertEqual(rc, 0, said)
        self.assertIn("Tell the user once", said)
        self.assertEqual(self.install_count(), 1)
        self.assertEqual(self.guard("make all", env)[0], 0)
        # The way out: switched off, it asks again for the next missing tool.
        toolchain.set_general_permission(env, False)
        with open(settings, encoding="utf-8") as fh:
            self.assertEqual(fh.read().count("INSTALL_MISSING_TOOLS="), 1)
        self.assertFalse(toolchain.general_permission(env))
        self.no_stub()

    def test_t20_a_second_request_for_the_same_tool_neither_asks_nor_installs(self):
        # After a no: never asked again, never installed, even with a direct install request.
        self.installer()
        env = self.user_env()
        self.assertTrue(self.payload(self.guard("make", env)[1])["ask"])
        rc, said = self.run_install(env, "make", said="no")
        self.assertEqual(rc, 0, said)
        for _ in range(2):
            rc, message = self.guard("make", env)
            data = self.payload(message)
            self.assertEqual((rc, data["ask"], data["declined"]), (2, False, True))
            self.assertIn("said NO", message)
        self.assertEqual(self.run_install(env, "make")[0], 3)
        self.assertEqual(self.install_count(), 0)
        # While one install runs, a second request for the same tool (or another tool of the same
        # package) waits for it: one installer run.
        # Two requests at once, and a wait that runs out before the download ends: one installer.
        self.installer(delay=1.5)
        toolchain.set_general_permission(env, True)
        results = []
        threads = [threading.Thread(target=lambda: results.append(self.run_install(env, "git", wait=0.2)))
                   for _ in range(2)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(10)
        self.assertEqual(sorted(r[0] for r in results), [4, 4], results)
        self.assertTrue(any("STILL-INSTALLING" in r[1] for r in results), results)
        rc, message = self.guard("git status", env)
        self.assertEqual((rc, self.payload(message)["ask"]), (2, False))
        self.assertIn("being installed right now", message)
        rc, said = self.run_install(env, "git", wait=0.2)
        self.assertEqual(rc, 4, "a wait that ran out never opens the installer again: " + said)
        rc, said = self.run_install(env, "git", wait=10)
        self.assertEqual(rc, 0, said)
        self.assertEqual(self.install_count(), 1, "the installer started twice: %r" % results)
        self.no_stub()


if __name__ == "__main__":
    unittest.main(verbosity=2)
