#!/usr/bin/env python3
"""hook-python.test.py: no hook can reach Apple's python3 (or git) install-dialog stub
(scripts/lib/hook-python.sh).

On a Mac without the developer tools /usr/bin/python3 and git are stubs that open "install command
line developer tools" on the user's screen; most regular users have no developer tools. Every case
builds a fake Mac under a temporary root (RICHOS_TOOLCHAIN_ROOT, as toolchain.test.py does): its
stub folder holds one marker-writing stand-in for EVERY stub name (toolchain.SHIMS), first on PATH
before the real /usr/bin. Executing any of them appends its name to a marker file: that marker is
the dialog, and no case may leave one. Nothing here runs a real stub or touches this Mac's state
(HOME, TMPDIR and the project are temporary; osascript, open, curl, gh, launchctl and claude are
quiet stand-ins).

  H1  every hook script sources hook-python.sh as its first command
  H2  the library on four fake Macs: the app's bundled runtime (by path, before the stubs on PATH,
      reached by `python3`, `exec python3`, `env python3`, a child bash script and `git`); a python
      on PATH after the stubs; the Command Line Tools only; nothing but the stub (127, said, no
      stub); the app's lease PATH left as it is; no stub and no python3 on PATH (nothing changes,
      so each hook's own missing-python3 handling still runs)
  H3  the whole hook chain on a fake regular-user Mac with the app's runtime beside the engine:
      every registration in hooks/hooks.json run as Claude Code runs it, with its event's payload
      (the Bash payload names every dispatcher rule's words, so every rule is forked); the desktop
      app's own registrations (engine_profile.rs: app-engine-hook.py on its nine events, its direct
      Bash and Write guards, the dispatcher with the app's rule lists); no marker, and the hooks
      did run the bundled runtime
"""
import concurrent.futures
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import toolchain  # noqa: E402

ENGINE = os.path.dirname(os.path.dirname(HERE))
HOOKS = os.path.join(ENGINE, "scripts", "hooks")
LIB = os.path.join(HERE, "hook-python.sh")
ENGINE_PROFILE = os.path.join(os.path.dirname(ENGINE), "app", "crates", "richos-core", "src", "engine_profile.rs")
SCAFFOLD = (".test.sh", ".mutation.sh", ".selftest.sh")
# shell-evidence.sh was in flight on another branch when this suite landed (2026-10-30); it joins
# H1 and H3 the moment it sources hook-python.sh, and is named on every run until then.
PENDING = {"shell-evidence.sh"}
SOURCE_LINE = re.compile(r'^_hp_dir="\$\{BASH_SOURCE\[0\]%/\*\}"; if \[ -f "\$_hp_dir/\.\./lib/hook-python\.sh" \]; then '
                         r'\. "\$_hp_dir/\.\./lib/hook-python\.sh"; fi')
REAL_PY = os.path.realpath(sys.executable)
REAL_GIT = shutil.which("git")
STUB = """#!/bin/sh
echo "${0##*/}" >> "$FAKE_MARKER"
echo "STUB-PROMPT: ${0##*/} would open the install dialog" >&2
exit 1
"""
QUIET = """#!/bin/sh
echo "${0##*/} $*" >> "$QUIET_LOG"
exit 0
"""
QUIET_NAMES = ("osascript", "open", "say", "afplay", "terminal-notifier", "curl", "gh", "launchctl",
               "claude", "codex", "caffeinate", "pmset")
LOGGED = """#!/bin/sh
echo "$*" >> "{log}"
exec "{real}" "$@"
"""


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write(text)
    os.chmod(path, 0o755)


def hook_scripts():
    return sorted(n for n in os.listdir(HOOKS) if n.endswith(".sh") and not n.endswith(SCAFFOLD))


class FakeMac(unittest.TestCase):
    def setUp(self):
        self.t = tempfile.mkdtemp(prefix="hook-python-test.")
        self.root = os.path.join(self.t, "mac")
        self.marker = os.path.join(self.t, "STUB-PROMPT")
        for name in toolchain.SHIMS:
            write(os.path.join(self.root, "usr/bin", name), STUB)
        self.quiet = os.path.join(self.t, "quiet")
        for name in QUIET_NAMES:
            write(os.path.join(self.quiet, name), QUIET)
        for d in ("home", "tmp", "state"):
            os.makedirs(os.path.join(self.t, d))
        self.path = os.pathsep.join([self.quiet, os.path.join(self.root, "usr/bin"),
                                     "/usr/bin", "/bin", "/usr/sbin", "/sbin"])

    def tearDown(self):
        shutil.rmtree(self.t, ignore_errors=True)

    def env(self, **extra):
        env = {"HOME": os.path.join(self.t, "home"), "TMPDIR": os.path.join(self.t, "tmp"),
               "PATH": self.path, "LANG": "en_US.UTF-8", "USER": os.environ.get("USER", "test"),
               "RICHOS_TOOLCHAIN_ROOT": self.root, "RICHOS_TOOLCHAIN_STATE": os.path.join(self.t, "state"),
               "FAKE_MARKER": self.marker, "QUIET_LOG": os.path.join(self.t, "quiet.log"),
               "PYTHONDONTWRITEBYTECODE": "1"}
        env.update(extra)
        return env

    def stubs_run(self, marker=None):
        try:
            with open(marker or self.marker, encoding="utf-8") as fh:
                return fh.read().split()
        except FileNotFoundError:
            return []

    def runtime(self, engine, log):
        """The app's delivered runtime beside `engine` (runtime.rs: <engine>/runtime/bin)."""
        write(os.path.join(engine, "runtime/bin/python3"), LOGGED.format(log=log, real=REAL_PY))
        if REAL_GIT:
            write(os.path.join(engine, "runtime/bin/git"), LOGGED.format(log=log + ".git", real=REAL_GIT))
        jq = shutil.which("jq")
        if jq:
            os.symlink(jq, os.path.join(engine, "runtime/bin/jq"))


class H1EveryHookSourcesIt(unittest.TestCase):
    def test_h1_every_hook_sources_the_library_first(self):
        missing = []
        for name in hook_scripts():
            if name in PENDING:
                continue
            with open(os.path.join(HOOKS, name), encoding="utf-8") as fh:
                lines = fh.read().splitlines()
            code = [l for l in lines[1:] if l.strip() and not l.lstrip().startswith("#")]
            if not code or not SOURCE_LINE.match(code[0]):
                missing.append(name)
        self.assertEqual(missing, [], "these hooks can still reach the python3 stub by name")
        for name in sorted(PENDING):
            print(f"\nNOTE: {name} does not source hook-python.sh yet (in flight on another branch)", file=sys.stderr)


class H2TheLibrary(FakeMac):
    PROBE = """set -euo pipefail
. "{lib}"
echo "PY=${{RICHOS_HOOK_PYTHON:-}}"
echo "CALLER=$RICHOS_HOOK_CALLER_PATH"
python3 -c 'print("function ok")'
env python3 -c 'print("env ok")'
printf '%s\\n' 'python3 -c "print(\\"child ok\\")"' > "$TMPDIR/child.sh"
bash "$TMPDIR/child.sh"
{git}
exec python3 -c 'print("exec ok")'
"""

    def probe(self, lib, env, git=""):
        script = os.path.join(self.t, "probe.sh")
        write(script, self.PROBE.format(lib=lib, git=git))
        return subprocess.run(["/bin/bash", script], env=env, capture_output=True, text=True, timeout=60)

    def test_h2a_the_bundled_runtime_by_path(self):
        engine = os.path.join(self.t, "engine")
        os.makedirs(os.path.join(engine, "scripts/lib"))
        shutil.copy(LIB, os.path.join(engine, "scripts/lib/hook-python.sh"))
        log = os.path.join(self.t, "runtime.log")
        self.runtime(engine, log)
        env = self.env()
        proc = self.probe(os.path.join(engine, "scripts/lib/hook-python.sh"), env,
                          git='git --version >/dev/null' if REAL_GIT else "")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn(f"PY={engine}/runtime/bin/python3\n", proc.stdout)
        self.assertIn(f"CALLER={self.path}\n", proc.stdout)
        for said in ("function ok", "env ok", "child ok", "exec ok"):
            self.assertIn(said, proc.stdout)
        with open(log, encoding="utf-8") as fh:
            self.assertEqual(len(fh.read().splitlines()), 4, "each form ran the bundled runtime")
        if REAL_GIT:
            self.assertTrue(os.path.exists(log + ".git"), "the runtime's git, not the stub")
        self.assertEqual(self.stubs_run(), [])

    def test_h2f_no_stub_and_no_python_changes_nothing(self):
        # A PATH with neither a python3 nor the stub folder: python3 stays missing exactly as
        # before, so each hook's own missing-python3 handling (most fail closed) still runs.
        script = os.path.join(self.t, "nostub.sh")
        write(script, f'. "{LIB}"\nif command -v python3 >/dev/null; then echo found; else echo missing; fi\n'
                      'echo "PY=${RICHOS_HOOK_PYTHON:-}"\n')
        proc = subprocess.run(["/bin/bash", script], env=self.env(PATH="/bin"), capture_output=True,
                              text=True, timeout=60)
        self.assertIn("missing\nPY=\n", proc.stdout, proc.stderr)
        self.assertEqual(self.stubs_run(), [])

    def test_h2e_the_app_lease_path_is_left_as_it_is(self):
        # The app puts <engine>/runtime/bin first on every lease's PATH (runtime.rs path_with): a
        # hook sourcing the library the way every hook does finds that same folder, by the same
        # spelling, and changes nothing.
        engine = os.path.join(self.t, "engine")
        os.makedirs(os.path.join(engine, "scripts/lib"))
        shutil.copy(LIB, os.path.join(engine, "scripts/lib/hook-python.sh"))
        self.runtime(engine, os.path.join(self.t, "runtime.log"))
        with open(os.path.join(HOOKS, "guard-host-sleep.sh"), encoding="utf-8") as fh:
            line = next(l for l in fh if SOURCE_LINE.match(l))
        hook = os.path.join(engine, "scripts/hooks/probe.sh")
        write(hook, "#!/usr/bin/env bash\n" + line + 'echo "PY=$RICHOS_HOOK_PYTHON"\necho "PATH=$PATH"\n')
        lease = os.path.join(engine, "runtime/bin") + os.pathsep + self.path
        proc = subprocess.run(["/bin/bash", hook], env=self.env(PATH=lease), capture_output=True, text=True, timeout=60)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn(f"PY={engine}/runtime/bin/python3\n", proc.stdout)
        self.assertIn(f"PATH={lease}\n", proc.stdout)
        self.assertEqual(self.stubs_run(), [])

    def test_h2b_a_python_on_path_after_the_stubs(self):
        brew = os.path.join(self.t, "brew/bin")
        log = os.path.join(self.t, "brew.log")
        write(os.path.join(brew, "python3"), LOGGED.format(log=log, real=REAL_PY))
        env = self.env(PATH=self.path + os.pathsep + brew)
        proc = self.probe(LIB, env)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn(f"PY={brew}/python3", proc.stdout)
        with open(log, encoding="utf-8") as fh:
            self.assertEqual(len(fh.read().splitlines()), 4)
        self.assertEqual(self.stubs_run(), [])

    def test_h2c_the_command_line_tools_only(self):
        clt = os.path.join(self.root, "Library/Developer/CommandLineTools/usr/bin")
        log = os.path.join(self.t, "clt.log")
        write(os.path.join(clt, "python3"), LOGGED.format(log=log, real=REAL_PY))
        proc = self.probe(LIB, self.env())
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn(f"PY={clt}/python3", proc.stdout)
        with open(log, encoding="utf-8") as fh:
            self.assertEqual(len(fh.read().splitlines()), 4)
        self.assertEqual(self.stubs_run(), [])

    def test_h2d_no_python_at_all(self):
        script = os.path.join(self.t, "none.sh")
        write(script, f'. "{LIB}"\npython3 -c "print(1)"\necho "rc=$?"\n')
        proc = subprocess.run(["/bin/bash", script], env=self.env(), capture_output=True, text=True, timeout=60)
        self.assertIn("rc=127", proc.stdout)
        self.assertIn("developer-tools stub, which was not run", proc.stderr)
        self.assertEqual(self.stubs_run(), [])


class H3TheWholeHookChain(FakeMac):
    def setUp(self):
        super().setUp()
        self.engine = os.path.join(self.t, "engine")
        shutil.copytree(ENGINE, self.engine, symlinks=True,
                        ignore=shutil.ignore_patterns("__pycache__", ".pytest_cache", "runtime"))
        self.runtime_log = os.path.join(self.t, "runtime.log")
        self.runtime(self.engine, self.runtime_log)
        self.project = os.path.join(self.t, "project")
        os.makedirs(self.project)
        if REAL_GIT:
            subprocess.run([REAL_GIT, "init", "-q", self.project], check=True,
                           env=dict(os.environ, HOME=os.path.join(self.t, "home")))
        self.transcript = os.path.join(self.t, "transcript.jsonl")
        write(self.transcript, json.dumps({"type": "assistant", "message": {"role": "assistant",
              "content": [{"type": "text", "text": "Done."}]}}) + "\n")
        self.scope = os.path.join(self.t, "scope.json")
        write(self.scope, json.dumps({"version": 1, "actions_allowed": True, "binding": {"session_id": "s-1"}}))

    def words(self):
        """Every dispatcher rule's `# dispatch-words:`, so the Bash payload forks every rule."""
        words = set()
        for name in hook_scripts():
            with open(os.path.join(HOOKS, name), encoding="utf-8") as fh:
                for line in fh:
                    if line.startswith("# dispatch-words:"):
                        words.update(line.split(":", 1)[1].split())
                        break
        return " ".join(sorted(words))

    def payload(self, event, tool):
        p = {"session_id": "s-1", "transcript_path": self.transcript, "cwd": self.project,
             "hook_event_name": event, "permission_mode": "bypassPermissions"}
        inputs = {
            "Bash": {"command": "python3 -c 'print(1)' && git status && make && " + self.words()},
            "Write": {"file_path": os.path.join(self.project, "notes.md"), "content": "The color of python3.\n"},
            "Agent": {"subagent_type": "zach", "name": "zach-opus-t1", "description": "test",
                      "prompt": "Run python3 and git in your worktree."},
            "SendMessage": {"to": "main", "message": "done"},
            "AskUserQuestion": {"questions": [{"question": "Which?", "options": [{"label": "A"}]}]},
            "TaskStop": {"task_id": "t-1"},
            "Workflow": {"script": "x"},
            "mcp__playwright__browser_navigate": {"url": "https://example.com"},
        }
        if event in ("PreToolUse", "PostToolUse", "PostToolUseFailure"):
            p["tool_name"] = tool
            p["tool_input"] = inputs.get(tool, {})
            if event != "PreToolUse":
                p["tool_response"] = {"stdout": "", "stderr": "", "exit_code": 0}
        elif event == "UserPromptSubmit":
            p["prompt"] = "Please run python3 and git."
        elif event == "SessionStart":
            p["source"] = "startup"
        elif event in ("Stop", "SubagentStop"):
            p["stop_hook_active"] = False
        elif event == "SubagentStart":
            p["agent_id"] = "a-1"
            p["agent_type"] = "zach"
        return p

    def chain(self):
        """(label, shell command, payload, timeout) for every registration the two hosts make."""
        runs = []
        with open(os.path.join(self.engine, "hooks/hooks.json"), encoding="utf-8") as fh:
            doc = json.load(fh)
        for event, entries in doc["hooks"].items():
            for entry in entries:
                matcher = entry.get("matcher") or ("Bash" if "ToolUse" in event else "")
                tool = matcher.split("|")[0].replace(".*", "playwright__browser_navigate")
                for h in entry["hooks"]:
                    command = h["command"]
                    if any(p in command for p in PENDING):
                        continue
                    runs.append((f"terminal {event}[{matcher}] {command.split('/')[-1]}",
                                 command, self.payload(event, tool), min(h.get("timeout") or 60, 120)))
        # The desktop app's registrations (engine_profile.rs EngineProfile::prepare).
        hooks = os.path.join(self.engine, "scripts/hooks")
        app_hook = f'"$RICHOS_ENGINE_ROOT/runtime/bin/python3" "$RICHOS_ENGINE_ROOT/scripts/app-engine-hook.py"'
        for event in ("SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse",
                      "PostToolUseFailure", "SubagentStart", "SubagentStop", "Stop"):
            for tool in (("Bash", "Agent", "Write") if "ToolUse" in event else ("",)):
                runs.append((f"app {event}[{tool}] app-engine-hook.py", app_hook, self.payload(event, tool), 120))
        if os.path.exists(ENGINE_PROFILE):
            with open(ENGINE_PROFILE, encoding="utf-8") as fh:
                rust = fh.read()
            for chain in ("Bash", "Write"):
                rules = re.findall(r'"([\w.-]+\.sh)"', re.search(
                    r"const APP_%s_RULES[^=]*=\s*\[(.*?)\];" % chain.upper(), rust, re.S).group(1))
                runs.append((f"app dispatcher {chain}", f'/bin/bash "{hooks}/dispatch-pretooluse.sh" {chain} '
                             + " ".join(rules), self.payload("PreToolUse", chain), 60))
            for name in sorted(set(re.findall(r'join\("scripts/hooks/([\w.-]+\.sh)"\)', rust)) - PENDING
                               - {"dispatch-pretooluse.sh"}):
                for tool in ("Bash", "Write"):
                    runs.append((f"app direct {name}[{tool}]", f'/bin/bash "{hooks}/{name}"',
                                 self.payload("PreToolUse", tool), 60))
        else:
            print(f"\nNOTE: {ENGINE_PROFILE} is absent; the app's direct registrations were not run", file=sys.stderr)
        return runs

    def run_one(self, index, label, command, payload, timeout):
        marker = os.path.join(self.t, f"marker.{index}")
        env = self.env(FAKE_MARKER=marker, CLAUDE_PLUGIN_ROOT=self.engine, CLAUDE_PROJECT_DIR=self.project,
                       RICHOS_ENGINE_ROOT=self.engine, RICHOS_ENGINE_DIR=self.engine,
                       RICHOS_ENTITY_ROOT=self.project, RICHOS_APP_SCOPE=self.scope,
                       RICHOS_APP_STATE=os.path.join(self.t, "state", "engine-state"),
                       RICHOS_SESSION_ID="s-1")
        proc = subprocess.Popen(["/bin/sh", "-c", command], cwd=self.project, env=env, text=True,
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                start_new_session=True)
        try:
            out, err = proc.communicate(json.dumps(payload), timeout=timeout)
        except subprocess.TimeoutExpired:
            out, err = "", "TIMED OUT"
        finally:
            # Only the process group this case started: a hook's background child dies with it.
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
            proc.wait()
        return label, self.stubs_run(marker), err

    def test_h3_no_hook_in_either_chain_reaches_a_stub(self):
        runs = self.chain()
        self.assertGreater(len(runs), 60, "the chain was read")
        hits = []
        with concurrent.futures.ThreadPoolExecutor(max_workers=min(8, os.cpu_count() or 2)) as pool:
            futures = [pool.submit(self.run_one, i, *r) for i, r in enumerate(runs)]
            for future in futures:
                label, stubs, _ = future.result()
                if stubs:
                    hits.append(f"{label}: ran the {', '.join(sorted(set(stubs)))} stub")
        self.assertEqual(hits, [], "these hooks would open the install-developer-tools dialog:\n  "
                         + "\n  ".join(hits))
        self.assertTrue(os.path.exists(self.runtime_log), "no hook ran the bundled runtime at all")


if __name__ == "__main__":
    unittest.main(verbosity=2)
