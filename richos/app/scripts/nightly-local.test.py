#!/usr/bin/env python3
"""Local release entry point: explicit trigger, isolation and private credentials."""
import contextlib
import fcntl
import importlib.util
import io
import json
import os
import shlex
import shutil
import signal
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import uuid
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location("local_nightly", Path(__file__).with_name("nightly-local.py"))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class LocalTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def file(self, value, mode=0o600):
        path = self.root / "notary.env"
        path.write_text(value)
        path.chmod(mode)
        return path

    def test_notary_file_is_parsed_without_shell_execution(self):
        key = self.root / "private key.p8"
        key.write_text("fixture")
        key.chmod(0o600)
        path = self.file(f'export RICHOS_NOTARY_KEY="{key}"\nRICHOS_NOTARY_KEY_ID=id\nRICHOS_NOTARY_ISSUER=issuer\n')
        result = m.notary_environment(path)
        self.assertEqual(result["RICHOS_NOTARY_KEY"], str(key))
        self.assertEqual(result["RICHOS_NOTARY_ISSUER"], "issuer")

    def test_notary_profile_is_supported(self):
        self.assertEqual(m.notary_environment(self.file('RICHOS_NOTARY_PROFILE="nightly profile"\n')),
                         {"RICHOS_NOTARY_PROFILE": "nightly profile"})

    def test_unsafe_credentials_refuse(self):
        cases = ['RICHOS_NOTARY_PROFILE="$(touch /tmp/never-run)"',
                 'RICHOS_NOTARY_PROFILE="`id`"', 'OTHER=value',
                 'RICHOS_NOTARY_KEY_ID=partial', 'RICHOS_NOTARY_PROFILE=value; echo unsafe']
        for value in cases:
            with self.subTest(value=value), self.assertRaises(ValueError):
                m.notary_environment(self.file(value))
        with self.assertRaises(ValueError):
            m.notary_environment(self.file('RICHOS_NOTARY_PROFILE=value', 0o644))

    # The credential set as nightly-local.py would assemble it, with values that are
    # obviously fixtures. Names are what these tests assert on; values never leave here.
    CREDENTIALS = {"RICHOS_NOTARY_KEY": "/fixture/AuthKey_FIXTURE000.p8",
                   "RICHOS_NOTARY_KEY_ID": "FIXTURE000", "RICHOS_NOTARY_ISSUER": "fixture-issuer",
                   "RICHOS_NOTARY_PROFILE": "fixture-profile", "RICHOS_NOTARIZE": "1",
                   "RICHOS_SIGNING_IDENTITY": "Developer ID Application: Fixture (FIXTURE000)",
                   "TAURI_SIGNING_PRIVATE_KEY_PATH": "/fixture/richos-updater.key",
                   "TAURI_SIGNING_PRIVATE_KEY_PASSWORD": "fixture-password",
                   "APPLE_ID": "fixture@example.invalid", "APPLE_PASSWORD": "fixt-fixt-fixt-fixt",
                   "APPLE_TEAM_ID": "FIXTURETM1"}

    def test_exported_signing_variables_leave_the_gate_environment(self):
        # The operator's own shell is a source of these, not only notary.env, and the
        # gates must not inherit them from either.
        env = dict(self.CREDENTIALS, PATH="/usr/bin", RICHOS_NAMED_PERSONS_FILE="/fixture/list",
                   RICHOS_NIGHTLY_RUN_ID="20260917T000000Z-fixture", RICHOS_RUNTIME_DIR="/fixture/runtime")
        credentials = m.split_credentials(env)
        self.assertEqual(credentials, self.CREDENTIALS)
        self.assertEqual(sorted(env), ["PATH", "RICHOS_NAMED_PERSONS_FILE",
                                       "RICHOS_NIGHTLY_RUN_ID", "RICHOS_RUNTIME_DIR"])

    def test_gates_receive_no_signing_credential_and_still_get_the_privacy_list(self):
        # The 2026-09-16 nightly published nothing because package-app.test.sh's
        # "no credentials" cases ran with the operator's complete API key. This asserts
        # the environment of every gate subprocess, not the suite's verdict.
        base = {"PATH": "/usr/bin", "RICHOS_NAMED_PERSONS_FILE": "/fixture/list"}
        r = m.Runner(self.root, self.root / "state", dict(base), io.StringIO(), self.CREDENTIALS)
        seen = []

        def record(args, **kwargs):
            seen.append(kwargs["env"])
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=record), contextlib.redirect_stdout(io.StringIO()):
            r.gates()
        # Eight gates (workspace-mutants runs two commands: the fourteen-point pass and, since hunt
        # part 4 finding 19, the other workspace passes), the UI suite's own worktree
        # (rev-parse, worktree add), its cleanup.
        self.assertEqual(len(seen), 12)
        for env in seen:
            self.assertEqual([name for name in env if m.is_credential(name)], [])
            self.assertEqual(env["RICHOS_NAMED_PERSONS_FILE"], "/fixture/list")
        # ...and the same runner still hands the whole set to a step that signs.
        with patch.object(m, "owned_run", side_effect=record):
            r.command("codesign", credentials=True)
        self.assertEqual({name: seen[-1][name] for name in self.CREDENTIALS}, self.CREDENTIALS)

    def test_one_declared_host_gap_reaches_the_runner_and_nothing_else(self):
        # The nightly build of candidate .10 failed on front-door.test.sh, a suite that
        # drives the shipped window and has no window on a build host. run-tests.sh
        # tolerates its "this host cannot answer" ONLY under a caller declaration naming
        # it WITH a reason. This asserts the declaration is on the runner's step and on
        # NOTHING else, and that it is ONE well-formed line -- run-tests.sh:117-133
        # refuses a bare name, and a second suite smuggled in here would be tolerated
        # without anyone reading it.
        base = {"PATH": "/usr/bin", "RICHOS_NAMED_PERSONS_FILE": "/fixture/list",
                "RUN_TESTS_DECLARED_GAPS": "smuggled.test.sh: from the operator's shell"}
        r = m.Runner(self.root, self.root / "state", dict(base), io.StringIO(), self.CREDENTIALS)
        seen = []

        def record(args, **kwargs):
            seen.append((args, kwargs["env"]))
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=record), contextlib.redirect_stdout(io.StringIO()):
            r.gates()
        declared = [(args, env["RUN_TESTS_DECLARED_GAPS"]) for args, env in seen
                    if env.get("RUN_TESTS_DECLARED_GAPS") != base["RUN_TESTS_DECLARED_GAPS"]]
        self.assertEqual(len(declared), 1)
        args, value = declared[0]
        # The declaration reaches the SUITE RUNNER and nothing else. Matched anywhere in
        # argv rather than at its end: the runner grew `--results-out` (and may grow
        # `--no-host-screen`), and an assertion about an argument's POSITION would fail on
        # a change that has nothing to do with what this case is about.
        self.assertTrue([a for a in args if str(a).endswith("run-tests.sh")], args)
        lines = [line for line in value.splitlines() if line.strip()]
        self.assertEqual(len(lines), 1, lines)
        suite, _, reason = lines[0].partition(":")
        self.assertEqual(suite, "front-door.test.sh")
        self.assertGreater(len(reason.strip()), 40, reason)
        # Every other gate keeps whatever the environment held: this value is stated about
        # one step, and the pop in local_environment() is what keeps a shell out of it.
        for other_args, env in seen:
            if other_args is not args:
                self.assertEqual(env["RUN_TESTS_DECLARED_GAPS"], base["RUN_TESTS_DECLARED_GAPS"])

    def test_an_exported_commit_identity_leaves_the_release_environment(self):
        # These override every level of git config, so a stray export in the operator's
        # shell would author the release as someone else while `git config user.email`
        # still reported the configured address.
        env = dict.fromkeys(m.IDENTITY_OVERRIDES, "someone-else@example.invalid")
        env.update(PATH="/usr/bin", RICHOS_NAMED_PERSONS_FILE="/fixture/list")
        removed = m.strip_identity_overrides(env)
        self.assertEqual(sorted(removed), sorted(m.IDENTITY_OVERRIDES))
        self.assertEqual(sorted(env), ["PATH", "RICHOS_NAMED_PERSONS_FILE"])
        for name in ("GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL",
                     "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL"):
            self.assertIn(name, m.IDENTITY_OVERRIDES)

    def test_a_variable_nobody_thought_about_reaches_no_gate(self):
        """The allowlist's whole claim, tested the only way that proves it: by surprise.

        Every other case here names a variable somebody already knew was dangerous, so
        every one of them would still pass against the deny-list this replaced -- which
        is exactly how the deny-list looked right on 2026-09-18 and broke the build on
        2026-09-19. The variable planted below is deliberately one NO list anywhere in
        this repository mentions. A deny-list passes it through; an allowlist cannot,
        and it cannot for a reason that does not depend on anyone having foreseen it.
        """
        stray = "RICHOS_TEST_STRAY_" + uuid.uuid4().hex
        with patch.dict(os.environ, {stray: "from the operator's shell", "HOME": os.environ["HOME"]}):
            env, credentials = m.local_environment()
        self.assertNotIn(stray, env)
        self.assertNotIn(stray, credentials)
        # Not vacuous: the planting worked, and an allowlisted name from the SAME
        # os.environ did come through. Without this the case would pass just as well
        # against a local_environment() that returned an empty dict.
        self.assertIn("HOME", env)
        # And the property in general, not one specimen of it: nothing in the returned
        # environment came from the shell except by being named in the allowlist.
        #
        # BOTH SIDES SUBTRACT GATE_SET_BY_BUILD, and the first version of this case
        # subtracted it from only the left -- which passed in a terminal and failed inside
        # a build, the exact shape of defect this file's allowlist exists to end. The
        # cause is that RICHOS_NAMED_PERSONS_FILE is in BOTH tuples: the operator may set
        # it, and the build sets it regardless. So in a terminal it is absent from
        # os.environ and the asymmetry is invisible; inside a gate it is present and the
        # two sides disagree about a name that never came from a shell at all. A name
        # this script sets is not evidence about what the shell got through, whichever
        # tuple also lists it.
        from_shell = sorted(set(env) - set(m.GATE_SET_BY_BUILD))
        self.assertEqual(from_shell,
                         sorted(n for n in m.GATE_PASSTHROUGH
                                if n in os.environ and n not in m.GATE_SET_BY_BUILD))

    def test_every_gate_runs_whole_whatever_checkout_or_shell_it_starts_from(self):
        # A runner started from a teammate workspace narrows to the branch's cases by default
        # (engine/scripts/lib/workspace_scope.py, 2026-10-01). A nightly never does: its gates
        # and the checks proof-run.py runs under its conditions are told RICHOS_TEST_SCOPE=full,
        # and a `narrow` exported in the operator's shell does not reach them.
        with patch.dict(os.environ, {"RICHOS_TEST_SCOPE": "narrow", "HOME": os.environ["HOME"]}):
            env, _credentials = m.local_environment()
            conditions, _ = m.gate_conditions("gates/script-suites")
        self.assertEqual(env["RICHOS_TEST_SCOPE"], "full")
        self.assertEqual(conditions["RICHOS_TEST_SCOPE"], "full")
        self.assertIn("RICHOS_TEST_SCOPE", m.GATE_SET_BY_BUILD)
        self.assertNotIn("RICHOS_TEST_SCOPE", m.GATE_PASSTHROUGH)

    def test_gates_and_author_checks_share_a_root_independent_of_caller_tmpdir(self):
        # Nightly attempt 2 (2026-10-01): cargo-cache-env.test.sh failed with "path must be
        # shorter than SUN_LEN" under the build's long TMPDIR after passing where TMPDIR was
        # short. One folder for every launcher, regardless of its shell's TMPDIR.
        platform_root = subprocess.run(["/usr/bin/getconf", "DARWIN_USER_TEMP_DIR"], capture_output=True,
                                       text=True, check=True).stdout.strip()
        expected = "/Volumes/E1TB/tmp/richos-gates/" if Path("/Volumes/E1TB").is_mount() else platform_root
        for caller in ("/tmp/", str(self.root) + "/", None):
            with self.subTest(caller=caller):
                planted = {"HOME": os.environ["HOME"]}
                if caller:
                    planted["TMPDIR"] = caller
                with patch.dict(os.environ, planted):
                    if caller is None:
                        os.environ.pop("TMPDIR", None)
                    env, _credentials = m.local_environment()
                self.assertEqual(env["TMPDIR"], expected)
                conditions, _ = m.gate_conditions(m.UI_SUITE_GATE, planted)
                self.assertEqual(conditions["TMPDIR"], expected)
        with patch.object(Path, "is_mount", return_value=False):
            self.assertEqual(m.gate_tmpdir({"TMPDIR": str(self.root)}), platform_root)
        self.assertNotIn("TMPDIR", m.GATE_PASSTHROUGH)
        self.assertIn("TMPDIR", m.GATE_SET_BY_BUILD)

    def test_the_conditions_proof_run_reads_are_exactly_what_the_gates_hand_their_suites(self):
        # gate_conditions() is what proof-run.py hands a check of a suite a nightly gate runs.
        # Its claim is "what the gate hands its command, minus what the shell gives both
        # anyway". Measured the only way that holds whatever is edited later: by running the
        # real gates() with a recording owned_run, so a value written inline at a call site
        # (not through script_suites_environment) turns this red.
        base = m.gate_environment(os.environ, m.CONDITIONS_RUN_ID, strict=False)
        r = m.Runner(self.root, self.root / "state", dict(base), io.StringIO(), self.CREDENTIALS,
                     simulated_phones=m.CONDITIONS_SIMULATED_PHONES)
        seen = {}

        def record(args, **kwargs):
            args = [str(a) for a in args]
            if any(a.endswith("run-tests.sh") for a in args):
                seen["gates/script-suites"] = kwargs["env"]
            elif args[:2] == ["node", "run.js"]:
                seen[m.UI_SUITE_GATE] = kwargs["env"]
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=record), contextlib.redirect_stdout(io.StringIO()):
            r.gates()
        self.assertEqual(sorted(seen), sorted(m.CONDITIONS_GATES))
        for gate in m.CONDITIONS_GATES:
            with self.subTest(gate=gate):
                conditions, missing = m.gate_conditions(gate)
                handed = {name: value for name, value in seen[gate].items()
                          if name not in m.CONDITIONS_NOT_REPRODUCED
                          and (name not in m.GATE_PASSTHROUGH or name in m.GATE_SET_BY_BUILD)}
                self.assertEqual(conditions, handed)
                # Only the privacy list can be unreproducible (a machine without it), and
                # then it is named, never silently dropped.
                self.assertEqual(sorted(set(missing) - {"RICHOS_NAMED_PERSONS_FILE"}), [])
                self.assertEqual("RICHOS_NAMED_PERSONS_FILE" in missing,
                                 "RICHOS_NAMED_PERSONS_FILE" not in conditions)
        # The variable nightly attempt 2 tripped on reaches the checks, with the gate's value.
        conditions, _ = m.gate_conditions("gates/script-suites")
        self.assertEqual(conditions["RICHOS_IOS_POOL_WAIT"], str(m.GATE_BUDGETS["gates/script-suites"]))
        self.assertEqual(conditions["TMPDIR"], m.gate_tmpdir())
        # Every name declared as not reproduced is one the build really sets, so the list
        # cannot quietly excuse a name that no longer exists.
        declared = set(m.GATE_SET_BY_BUILD) | set(m.GATE_SET_PER_STEP)
        self.assertEqual(sorted(set(m.CONDITIONS_NOT_REPRODUCED) - declared), [])
        with self.assertRaises(ValueError):
            m.gate_conditions(m.WORKSPACE_MUTANTS_GATE)

    def test_the_allowlist_is_the_only_door_and_e1_derives_its_list_from_it(self):
        """`gate-environment` is what run-tests.test.sh case E1 reads instead of copying.

        E1 re-runs the whole script suite under the environment a build hands it. Its
        list used to be hand-written, so it went stale in silence the next time anyone
        added a variable. This asserts the printed contract is complete and well-formed:
        if a name is added to either tuple and this is the only copy, E1 picks it up for
        free; if the printing ever stops covering a tuple, this fails rather than E1
        quietly testing less than it claims to.
        """
        printed = subprocess.run(
            [sys.executable, str(Path(__file__).with_name("nightly-local.py")), "gate-environment"],
            capture_output=True, text=True, check=True).stdout
        rows = [line.split("\t") for line in printed.splitlines() if line]
        self.assertTrue(all(len(row) == 2 for row in rows), rows)
        by_kind = {}
        for kind, name in rows:
            by_kind.setdefault(kind, []).append(name)
        self.assertEqual(by_kind.get("passthrough"), list(m.GATE_PASSTHROUGH))
        self.assertEqual(by_kind.get("set"), list(m.GATE_SET_BY_BUILD))
        self.assertEqual(by_kind.get("per-step"), list(m.GATE_SET_PER_STEP))
        # No credential name may ever be printed here: E1 exports every name this prints.
        self.assertEqual([name for _, name in rows if m.is_credential(name)], [])

    def identity_runner(self, configured=True):
        state = self.root / "state"
        source = state / "source"
        source.mkdir(parents=True)
        env = {**os.environ, "GIT_CONFIG_GLOBAL": os.devnull,
               "GIT_CONFIG_SYSTEM": os.devnull, "GIT_CONFIG_NOSYSTEM": "1"}
        m.strip_identity_overrides(env)
        subprocess.run(["git", "init", "-q", str(source)], check=True, env=env)
        if configured:
            for name, value in (("user.name", "Fixture Operator"),
                                ("user.email", "fixture@example.invalid")):
                subprocess.run(["git", "config", name, value], cwd=source, check=True, env=env)
        return m.Runner(self.root, state, env, io.StringIO())

    def test_release_is_authored_with_the_checkouts_own_identity(self):
        self.assertEqual(self.identity_runner().identity(),
                         "Fixture Operator <fixture@example.invalid>")

    def test_unset_identity_is_refused_before_any_other_preflight_check(self):
        # The 2026-09-17 attempt found its wrong identity at the tag push, after every
        # gate had passed. This is the same class of defect found in milliseconds.
        r = self.identity_runner(configured=False)
        r.command = Mock()
        with self.assertRaisesRegex(ValueError, "no configured git identity"), \
                contextlib.redirect_stdout(io.StringIO()):
            r.preflight()
        r.command.assert_not_called()

    def test_preflight_asks_the_publisher_whether_branches_are_still_banned(self):
        """The check that used to REQUIRE a hole in the branch rule now forbids one.

        Until 2026-09-17 this preflight called
        `gh api repos/<repo>/rules/branches/nightly-channel` and refused to run if any
        `creation`/`update` rule applied -- "configure its narrow exception first". A
        publisher that demands an exemption is how `refs/heads/nightly-channel` came to
        be added to the `main-only` ruleset on 2026-09-16 and how the public repository
        page came to read "2 Branches" the next morning. The channel is a release tag
        now, so no exemption is wanted and the absence of one is the precondition.
        """
        r = self.identity_runner()
        r.command = Mock(return_value="true")
        with contextlib.redirect_stdout(io.StringIO()), contextlib.suppress(Exception):
            r.preflight()
        commands = [call.args for call in r.command.call_args_list]
        checks = [c for c in commands if "check-rules" in c]
        self.assertEqual(len(checks), 1, commands)
        self.assertEqual(Path(checks[0][1]).name, "nightly.py")
        # The old shape must not come back under any spelling.
        for command in commands:
            joined = " ".join(str(part) for part in command)
            self.assertNotIn("rules/branches", joined)
            self.assertNotIn("nightly-channel", joined)

    def test_lock_rejects_concurrent_manual_commands_and_releases_after_failure(self):
        state = self.root / "state"
        with self.assertRaises(RuntimeError):
            with m.exclusive(state):
                with self.assertRaisesRegex(ValueError, "another local"):
                    with m.exclusive(state):
                        self.fail("second publisher acquired lock")
                raise RuntimeError("failed build")
        with m.exclusive(state):
            pass

    def runner(self, build=True):
        r = m.Runner(self.root, self.root / "state", {}, io.StringIO())
        r.checkout = Mock(return_value="source-sha")
        r.plan = Mock(return_value=(self.root / "plan.json", {
            "build": build, "reason": "already published", "tag": "v1.2.0-nightly.20260916.1"}))
        for name in ("preflight", "runtime", "gates", "command"):
            setattr(r, name, Mock())
        r.installed_dependencies_digest = Mock(return_value=self.DEPENDENCIES)
        return r

    def test_check_never_builds_or_publishes(self):
        r = self.runner()
        r.perform("check")
        r.preflight.assert_called_once()
        r.gates.assert_not_called()
        r.runtime.assert_not_called()
        r.command.assert_not_called()

    def test_a_changed_runtime_recipe_builds_its_own_runtime(self):
        # Frank's media-tools review M3: the cache used to be <state>/runtime whatever the
        # recipe said, so the first nightly after a recipe change found the old runtime and
        # verify-runtime.py refused it. Keyed to the recipe, a changed recipe names a new
        # directory, which runtime() builds; the old one is left unused.
        r = m.Runner(self.root, self.root / "state", {}, io.StringIO())
        recipe = r.source / m.SCRIPTS / "runtime-sources.json"
        recipe.parent.mkdir(parents=True)
        recipe.write_text('{"recipe": 1}')
        old = r.runtime_path()
        old.mkdir(parents=True)
        (r.state / "runtime").mkdir()  # the pre-keying cache: never used again
        r.command = Mock()
        r.runtime(None)
        self.assertEqual([Path(c.args[1]).name for c in r.command.call_args_list], ["verify-runtime.py"])
        self.assertEqual(r.command.call_args.args[2], old)
        recipe.write_text('{"recipe": 2}')
        new = r.runtime_path()
        self.assertNotEqual(new, old)
        self.assertEqual(new.parent, r.state)
        self.assertRegex(new.name, r"^runtime-[0-9a-f]{12}$")
        r.command = Mock()
        r.runtime(None)
        self.assertEqual([(Path(c.args[1]).name, c.args[2]) for c in r.command.call_args_list],
                         [("build-runtimes.py", new), ("verify-runtime.py", new)])
        self.assertEqual(r.env["RICHOS_RUNTIME_DIR"], str(new))
        # An operator's --runtime-dir is used as given.
        self.assertEqual(r.runtime_path(self.root / "mine"), self.root / "mine")

    def test_runtime_hands_gui_boot_the_speech_models_only_when_both_are_there(self):
        # The video tools are a setup essential (media-tools plan, slice 3), so gui-boot's
        # healthy machine needs a pinned speech model; runtime() hands it the one this Mac has
        # (lib/runtime_cache.py) and sets nothing when the file is absent, so gui-boot refuses
        # by name rather than booting a machine missing something.
        sys.path.insert(0, str(Path(__file__).resolve().with_name("lib")))
        import runtime_cache
        r = m.Runner(self.root, self.root / "state", {}, io.StringIO())
        r.command = Mock()
        (self.root / "mine").mkdir()
        small, large = self.root / "ggml-small.en.bin", self.root / "ggml-large-v3-turbo-q5_0.bin"
        with patch.object(runtime_cache, "GUI_SPEECH_MODELS", (small, large)):
            r.runtime(self.root / "mine")
            self.assertNotIn("RICHOS_GUI_SPEECH_MODELS", r.env)
            small.write_bytes(b"weights")
            r.runtime(self.root / "mine")
            self.assertNotIn("RICHOS_GUI_SPEECH_MODELS", r.env, "one model is not both")
            large.write_bytes(b"weights")
            r.runtime(self.root / "mine")
            self.assertEqual(r.env["RICHOS_GUI_SPEECH_MODELS"], f"{small}:{large}")
        self.assertIn("RICHOS_GUI_SPEECH_MODELS", m.GATE_SET_BY_BUILD)

    def test_command_timeout_does_not_expose_signing_password(self):
        r = m.Runner(self.root, self.root, {}, io.StringIO())
        args = ["cargo", "tauri", "signer", "sign", "-p", "secret-password"]
        with patch.object(m, "owned_run", side_effect=subprocess.TimeoutExpired(args, 30)):
            with self.assertRaisesRegex(RuntimeError, "cargo timed out") as raised:
                r.command(*args, timeout=30)
        self.assertNotIn("secret-password", str(raised.exception))

    def test_capture_cannot_wait_forever_on_a_descendants_inherited_pipe(self):
        # The leader exits, but its child keeps the stdout pipe open. A timeout
        # must stop that child too, otherwise communicate() cannot finish.
        # Normal-exit cleanup now closes it before a timeout is necessary.
        script = self.root / "pipe.py"
        script.write_text("""import subprocess, sys
from pathlib import Path
p = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
Path(sys.argv[1]).write_text(str(p.pid))
""")
        pid_file = self.root / "pipe-child"
        with (self.root / "pipe.log").open("w") as log:
            # Its own worker budget: the wall bound below is about the pipe, not the queue.
            r = m.Runner(self.root, self.root, self.private_budget()[0], log)
            start = time.monotonic()
            r.command(sys.executable, script, pid_file, cwd=self.root,
                      capture=True, timeout=5)
            # The child sleeps 60 s; returning well before that proves the pipe was not waited on.
            # A tight bound here failed on a busy Mac (5.37 s), so it has room for a loaded host.
            # load-bound: a hang catcher only; the verdict is the child's pid being gone below
            self.assertLess(time.monotonic() - start, 45)
        self.assert_pid_gone(int(pid_file.read_text()))

    def private_budget(self):
        """An environment whose machine worker budget is this test's own, every token free,
        and the tool that makes it. Nothing inherited from a runner around this test."""
        tool = Path(m.__file__).resolve().parents[2] / "engine/scripts/lib/worker_tokens.py"
        workers = self.root / "workers"
        # The size worker_tokens.py's machine_directory() insists on, so its init agrees.
        subprocess.run([sys.executable, str(tool), "init", str(workers),
                        str(max(1, int((os.cpu_count() or 4) * 0.8)))], check=True)
        env = dict(os.environ, RICHOS_MACHINE_WORKERS=str(workers))
        for key in ("RICHOS_WORKER_TOKENS", "RICHOS_WORKER_TOKENS_TOOL", "RICHOS_WORKER_SLOT_HELD",
                    "RICHOS_WORKER_BORROW_LOCK", "RICHOS_WORKER_TOKENS_RESERVED"):
            env.pop(key, None)
        return env, workers

    def test_a_deadline_counts_execution_never_the_wait_for_a_worker_token(self):
        # Run 20260929T003824Z-01545196: the UI gate's 30 s `git status` cleanup "timed out"
        # while the mutation pool held the machine's worker tokens, because the deadline ran
        # from the spawn. Here every token is held until the command has been seen queued for
        # longer than its whole budget; it must then run, pass, and say how long it queued.
        env, workers = self.private_budget()
        budget = 5
        held = []
        for token in sorted(workers.glob("token-*")):
            fd = os.open(token, os.O_RDWR)
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            held.append(fd)

        def release_once_queued_past_the_budget():
            try:
                guard = time.monotonic() + 120            # a hang guard only
                while not list(workers.glob("wait-*")) and time.monotonic() < guard:
                    time.sleep(0.05)
                time.sleep(budget + 1)                     # queued for longer than the budget
            finally:
                for fd in held:
                    os.close(fd)

        releaser = threading.Thread(target=release_once_queued_past_the_budget)
        releaser.start()
        try:
            with (self.root / "run.log").open("w") as log:
                r = m.Runner(self.root, self.root, env, log)
                with r.phase("gates/fixture"):
                    r.command("/usr/bin/true", cwd=self.root, timeout=budget)
        finally:
            releaser.join()
        admission, execution = r.clocks["gates/fixture"]
        self.assertGreaterEqual(admission, budget + 1, "the queueing is reported as admission")
        text = (self.root / "run.log").read_text()
        self.assertIn("gates/fixture: admission", text)
        self.assertIn("a deadline counts execution only", text)

    def test_the_gate_line_says_admission_and_execution_and_a_refusal_says_nothing_ran(self):
        r = m.Runner(self.root, self.root / "state", {"PATH": "/usr/bin"}, io.StringIO(),
                     gates_at_once="all")

        def run(args, **kwargs):
            result = subprocess.CompletedProcess(args, 0, "", "")
            result.admitted, result.admission_seconds, result.execution_seconds = True, 2.0, 3.0
            return result

        def gate(name):
            def body():
                with r.phase(name):
                    r.command("true", timeout=10)
            return (name, body)

        # Two gates, so they run side by side and each gets a verdict line.
        with patch.object(m, "owned_run", side_effect=run), \
                contextlib.redirect_stdout(io.StringIO()) as out:
            r.run_gates([gate("gates/fixture"), gate("gates/other")])
        line = next(l for l in out.getvalue().splitlines() if "PASSED gates/fixture" in l)
        self.assertRegex(line, r"PASSED gates/fixture in [\d.]+s \(admission 2\.0s, execution 3\.0s\)$")
        self.assertTrue(m.Runner.GATE_VERDICT.match(line), "--gates-passed-in still reads the line")

        def refused(args, **kwargs):
            result = subprocess.CompletedProcess(args, 75, "", "")
            result.admitted, result.admission_seconds, result.execution_seconds = False, 1800.0, 0.0
            return result

        with patch.object(m, "owned_run", side_effect=refused), \
                self.assertRaisesRegex(RuntimeError, "never admitted: waited 1800s .* nothing ran"):
            m.Runner(self.root, self.root, {}, io.StringIO()).command("true", timeout=10)

    def test_every_gate_has_a_named_deadline(self):
        for (phase, budget), at_once in [(item, n) for item in m.GATE_BUDGETS.items()
                                         for n in (1, "all")]:
            with self.subTest(phase=phase, gates_at_once=at_once):
                r = m.Runner(self.root, self.root, {}, io.StringIO(), gates_at_once=at_once)
                r.restore_source_tree = Mock()
                def run(args, **kwargs):
                    if r.active_phase == phase:
                        self.assertEqual(kwargs["timeout"], budget)
                        raise subprocess.TimeoutExpired(args, budget)
                    return subprocess.CompletedProcess(args, 0, "", "")
                with patch.object(m, "owned_run", side_effect=run), \
                        contextlib.redirect_stdout(io.StringIO()), \
                        self.assertRaisesRegex(RuntimeError, f"{phase} timed out after {budget}s"):
                    r.gates()

    def assert_executed_gate_deadlines(self, checks_done_at_land=None, gates_at_once=1):
        """Discover phases by executing gates(), independently of the budget table."""
        r = m.Runner(self.root, self.root, {}, io.StringIO(), gates_at_once=gates_at_once)
        r.restore_source_tree = Mock()
        original_phase = r.phase
        phases, commands = set(), set()

        @contextlib.contextmanager
        def registered_phase(name):
            if name.startswith("gates/"):
                self.assertIn(name, m.GATE_BUDGETS, f"{name} has no registered deadline")
                phases.add(name)
            with original_phase(name):
                yield

        def run(args, **kwargs):
            name = r.active_phase
            if name and name.startswith("gates/"):
                self.assertEqual(kwargs.get("timeout"), m.GATE_BUDGETS[name],
                                 f"{name} did not pass its registered deadline")
                commands.add(name)
            return subprocess.CompletedProcess(args, 0, "", "")

        r.phase = registered_phase
        with patch.object(m, "owned_run", side_effect=run), \
                contextlib.redirect_stdout(io.StringIO()):
            r.gates(checks_done_at_land)
        self.assertTrue(phases)
        self.assertEqual(commands, phases)

    def test_executed_gates_have_registered_and_wired_deadlines(self):
        for at_once in (1, "all"):
            with self.subTest(gates_at_once=at_once):
                self.assert_executed_gate_deadlines(gates_at_once=at_once)
                self.assert_executed_gate_deadlines("land-proof-fixture", gates_at_once=at_once)

    def test_gate_coverage_detects_a_missing_budget_entry(self):
        budgets = dict(m.GATE_BUDGETS)
        del budgets["gates/lint-tauri"]
        with patch.object(m, "GATE_BUDGETS", budgets), \
                self.assertRaisesRegex(AssertionError, "gates/lint-tauri has no registered deadline"):
            self.assert_executed_gate_deadlines()

    def test_gate_coverage_detects_an_unwired_timeout(self):
        command = m.Runner.command

        def without_lint_timeout(runner, *args, **kwargs):
            if runner.active_phase == "gates/lint-tauri":
                kwargs.pop("timeout", None)
            return command(runner, *args, **kwargs)

        with patch.object(m.Runner, "command", without_lint_timeout), \
                self.assertRaisesRegex(AssertionError, "gates/lint-tauri did not pass its registered deadline"):
            self.assert_executed_gate_deadlines()

    def test_cleanup_commands_have_deadlines(self):
        r = m.Runner(self.root, self.root, {}, io.StringIO())
        r.command = Mock(side_effect=[" M fixture", None])
        with contextlib.redirect_stdout(io.StringIO()):
            r.restore_source_tree("fixture")
        self.assertEqual(len(r.command.call_args_list), 2)
        for call in r.command.call_args_list:
            self.assertEqual(call.kwargs["timeout"], m.CLEANUP_TIMEOUT)

    def test_ui_does_not_restore_files_when_group_cleanup_failed(self):
        r = m.Runner(self.root, self.root, {}, io.StringIO())
        r.command = Mock(side_effect=m.CommandCleanupError("owned group still running"))
        r.restore_source_tree = Mock()
        with contextlib.redirect_stdout(io.StringIO()), \
                self.assertRaisesRegex(RuntimeError, "owned group still running"):
            r.ui_suite()
        r.restore_source_tree.assert_not_called()

    def test_a_red_ui_suite_names_its_failed_check_and_the_reason(self):
        # Run 20260928T233221Z-56bcde43 refused the build with "home.js (exit 1, 1 failed
        # check(s))" and the run log had lost the lines that said which. The receipt carries
        # the failed check's name and message; the gate's own FAILED line must repeat them.
        receipts = self.root / "receipts"
        receipts.mkdir()
        (receipts / "home.receipt.json").write_text(json.dumps({
            "suite": "home.js", "exit": 1, "records": [{
                "suite": "home.js", "label": "The home screen", "checks": 38, "failed": 1,
                "failures": [{"check": "12  the settle deadline holds",
                              "message": "settled after 2731 ms of 1500\nexpected 1\nactual   0"}]}]}))
        (receipts / "splash.receipt.json").write_text(json.dumps({
            "suite": "splash.js", "exit": 0, "records": [{"checks": 4, "failed": 0}]}))
        red = m.Runner.red_ui_suites(receipts)
        self.assertEqual(len(red), 1, red)
        self.assertIn("home.js (exit 1, 1 failed check(s))", red[0])
        self.assertIn("12  the settle deadline holds: settled after 2731 ms of 1500", red[0])
        self.assertNotIn("expected 1", red[0], "only the message's first line belongs in the verdict")

    def ui_gate_with_a_flaky_suite(self, failures):
        """Run the real gate over the real run.js and two fake suites: `steady.js` passes and
        `flaky.js` fails its first `failures` runs. Returns (runner, log text, error or None,
        how many times each suite ran)."""
        tree = self.root / "ui-tree"
        tests = tree / m.UI_TESTS
        (tests / "lib").mkdir(parents=True)
        ui = Path(__file__).resolve().parents[1] / "ui/tests"
        for name in ("run.js", "lib/ui-sources.js", "lib/blocking-stdio.js"):
            shutil.copyfile(ui / name, tests / name)
        counts = self.root / "counts"
        counts.mkdir()
        suite = """const fs = require('fs');
const run = {check() {}};
run.check();
const counter = COUNTER;
const n = fs.existsSync(counter) ? Number(fs.readFileSync(counter, 'utf8')) : 0;
fs.writeFileSync(counter, String(n + 1));
const failed = n < FAILS ? 1 : 0;
fs.appendFileSync(process.env.RICHOS_UI_TESTS_LEDGER, JSON.stringify({suite: NAME, label: 'fixture',
  checks: 1, failed, failures: failed ? [{check: '10b  the shutter window holds',
  message: 'moved from 238ms to 353ms'}] : []}) + '\\n');
process.exit(failed);
"""
        for name, fails in (("steady.js", 0), ("flaky.js", failures)):
            (tests / name).write_text(suite.replace("COUNTER", json.dumps(str(counts / name)))
                                      .replace("FAILS", str(fails)).replace("NAME", json.dumps(name)))
        scratch = self.root / "tmp"
        scratch.mkdir()
        log = io.StringIO()
        r = m.Runner(self.root, self.root / "state",
                     {"PATH": os.environ["PATH"], "GITHUB_SHA": "a" * 40, "TMPDIR": str(scratch)}, log)
        r.ui_checkout = lambda: tree
        r.restore_source_tree = Mock()

        def direct(args, *, timeout=None, groups=None, cleanup=False, release_build=False, **kwargs):
            out = kwargs.pop("stdout")
            kwargs.pop("stderr")
            done = subprocess.run(args, timeout=timeout, stdout=subprocess.PIPE,
                                  stderr=subprocess.STDOUT, **kwargs)
            if out is subprocess.PIPE:
                return subprocess.CompletedProcess(args, done.returncode, done.stdout, "")
            log.write(done.stdout)
            return subprocess.CompletedProcess(args, done.returncode, None, None)

        error = None
        with patch.object(m, "owned_run", side_effect=direct), patch.object(m, "UI_SHARDS", 2), \
                contextlib.redirect_stdout(io.StringIO()):
            try:
                r.ui_suite()
            except RuntimeError as raised:
                error = raised
        ran = {name: int((counts / name).read_text()) for name in ("steady.js", "flaky.js")}
        return r, log.getvalue(), error, ran

    def test_a_ui_suite_that_fails_in_the_sharded_run_and_passes_alone_lets_the_build_go_on(self):
        # Run 20261010T025551Z-f0359d9b: splash.js 10b failed under the build's own load ("the
        # shutter's window moved from 238ms to 353ms") and passed alone on the same tree, and the
        # whole build was thrown away. The gate now gives a red suite ONE attempt alone, after
        # the shards finish, as run-tests.sh does for a script suite, and quotes both results.
        r, log, error, ran = self.ui_gate_with_a_flaky_suite(failures=1)
        self.assertIsNone(error, log)
        self.assertEqual(ran, {"steady.js": 1, "flaky.js": 2}, "only the red suite runs again, once")
        retry = log[log.index("retrying it ONCE, alone"):]
        self.assertIn("flaky.js PASSED on the retry, alone", retry)
        self.assertIn("First attempt: flaky.js (exit 1, 1 failed check(s)) "
                      "[10b  the shutter window holds: moved from 238ms to 353ms]", retry)
        self.assertIn("ui-suite: 2/2 discovered suite(s) ran, all green", retry,
                      "the coverage job over every receipt is still the verdict")
        r.restore_source_tree.assert_called()

    def test_a_ui_suite_that_fails_again_alone_refuses_the_build_quoting_both_attempts(self):
        r, log, error, ran = self.ui_gate_with_a_flaky_suite(failures=2)
        self.assertIsNotNone(error, log)
        self.assertEqual(ran, {"steady.js": 1, "flaky.js": 2}, "one solo attempt, never more")
        message = str(error)
        self.assertIn("FAILED AGAIN alone", message)
        quoted = "flaky.js (exit 1, 1 failed check(s)) [10b  the shutter window holds: moved from 238ms to 353ms]"
        self.assertIn("first attempt: " + quoted, message)
        self.assertIn("solo attempt: " + quoted, message)
        self.assertIn("FAILED AGAIN on the retry, alone", log)

    def process_fixture(self):
        script = self.root / "tree.py"
        script.write_text("""import os, signal, subprocess, sys, time
from pathlib import Path
root = Path(sys.argv[1])
level = int(sys.argv[2])
signal.signal(signal.SIGTERM, signal.SIG_IGN)
(root / ('pid' + str(level))).write_text(str(os.getpid()))
if level < 2:
    subprocess.Popen([sys.executable, __file__, str(root), str(level + 1)])
while True: time.sleep(.02)
""")
        return [sys.executable, str(script), str(self.root), "0"]

    def assert_pid_gone(self, pid):
        # GONE MEANS GONE OR A ZOMBIE, READ ONCE (audit R13, 2026-09-29). This used to poll
        # `os.kill(pid, 0)` for 3 s, and that call still finds a zombie: a group member
        # whose parent died waits for launchd to reap it, which on a loaded Mac takes
        # longer than any number here. Every caller asks after cleanup has returned, and
        # m.finish_group returns only once the group is gone, so the kernel's state is
        # read once, with no wait: a survivor is still a running process at that moment.
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return
        state = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)],
                               capture_output=True, text=True).stdout.strip()
        if state and "Z" not in state:
            self.fail(f"owned process {pid} survived cleanup (state {state})")

    def test_an_exited_process_nobody_has_reaped_reads_gone(self):
        # A zombie is an exited process: cleanup stopped it, and only the reaping is left,
        # which launchd does when it gets round to it (audit R13, 2026-09-29).
        child = os.fork()
        if child == 0:
            os._exit(0)
        try:
            while "Z" not in subprocess.run(["ps", "-o", "stat=", "-p", str(child)],
                                            capture_output=True, text=True).stdout:
                time.sleep(.02)
            os.kill(child, 0)  # Positive evidence: the kernel still lists it.
            self.assert_pid_gone(child)
        finally:
            os.waitpid(child, 0)

    def test_parent_only_kill_leaves_descendants_but_group_cleanup_does_not(self):
        p = subprocess.Popen(self.process_fixture(), start_new_session=True)
        try:
            # The tree is three Python starts deep: wait for the fact, or for the tree to die,
            # never for a number of seconds (a loaded Mac starts Python in more than 5).
            while not (self.root / "pid2").exists() and p.poll() is None:
                time.sleep(.02)
            ids = [int((self.root / f"pid{i}").read_text()) for i in range(3)]
            p.kill()
            p.wait()  # SIGKILL is certain; only the kernel's time is left
            for pid in ids[1:]:
                os.kill(pid, 0)  # Positive evidence: parent-only kill left both alive.
            m.finish_group(p)
            for pid in ids:
                self.assert_pid_gone(pid)
        finally:
            m.finish_group(p)

    def test_real_timeout_escalates_and_preserves_unrelated_process(self):
        sentinel = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"],
                                    start_new_session=True)
        try:
            with (self.root / "log").open("w") as log:
                # Its own worker budget: the wall bound below is about cleanup, not the queue.
                r = m.Runner(self.root, self.root, self.private_budget()[0], log)
                start = time.monotonic()
                with self.assertRaisesRegex(RuntimeError, "timed out after 1s"):
                    r.command(*self.process_fixture(), cwd=self.root, timeout=1)
                # The supervisor gives EXIT traps eight seconds to clean up.
                # Bound this by the caller's complete cleanup allowance rather
                # than the former, shorter grace period.
                # load-bound: cleanup hang guard under the private token budget; pid removal is the verdict.
                self.assertLess(time.monotonic() - start,
                                1 + m.TERM_GRACE + 2 * m.KILL_GRACE + 2)
            for i in range(3):
                self.assert_pid_gone(int((self.root / f"pid{i}").read_text()))
            self.assertIsNone(sentinel.poll())
        finally:
            sentinel.kill()
            sentinel.wait()  # SIGKILL is certain; only the kernel's time is left

    def test_only_release_reaches_publisher(self):
        r = self.runner()
        r.perform("release")
        r.runtime.assert_called_once()
        r.gates.assert_called_once()
        args = r.command.call_args.args
        self.assertEqual(args[2], "run")
        # The publisher is the step that signs, notarizes and signs the updater manifest.
        self.assertIs(r.command.call_args.kwargs["credentials"], True)
        self.assertEqual(r.plan.call_count, 2)

    def test_only_the_build_step_is_registered_with_the_cpu_guard_as_a_release_build(self):
        # Run 20260928T190111Z-40a16163: every gate passed, then the CPU guard's 10-second
        # rule stopped rustc in the build step. The build step's supervisor carries the
        # release-build role (cpu_guard.BUILD_WINDOW); no gate's does.
        for command in ("release", "build"):
            with self.subTest(command=command):
                r = self.runner()
                r.env = {"RICHOS_NIGHTLY_RUN_ID": "fixture-run-id"}
                if command == "build":
                    r.command = Mock(side_effect=lambda *a, **k: self.write_candidate(
                        Path(a[a.index("--out") + 1])))
                with contextlib.redirect_stdout(io.StringIO()):
                    r.perform(command)
                self.assertIs(r.command.call_args.kwargs.get("release_build"), True)
        seen = []
        r = m.Runner(self.root, self.root, {}, io.StringIO())
        with patch.object(m, "owned_run", side_effect=lambda args, **kwargs: seen.append(kwargs) or
                          subprocess.CompletedProcess(args, 0, "", "")):
            r.command("cargo", "test")
            r.command("cargo", "build", release_build=True)
        self.assertNotIn("release_build", seen[0])
        self.assertIs(seen[1]["release_build"], True)
        launched = []
        def popen(argv, **kwargs):
            launched.append(argv)
            raise OSError("fixture stops before anything starts")
        with patch.object(m.subprocess, "Popen", side_effect=popen):
            for flag in (True, False):
                with self.assertRaises(OSError):
                    m.owned_run(["cargo", "build"], release_build=flag)
        role = launched[0].index("--guard-role")
        self.assertEqual(launched[0][role:role + 2], ["--guard-role", "release-build"])
        self.assertLess(role, launched[0].index("--"))
        self.assertNotIn("--guard-role", launched[1])

    # ── --gates-passed-in: resume after the gates (run 20260928T190111Z-40a16163) ─────────
    RESUMED = "20260928T190111Z-40a16163"
    SOURCE = "9fbf4332b47093be6539dac4b898f2e754eaa78b"

    DEPENDENCIES = "d" * 64

    def recorded_run(self, verdicts=None, at_once=2, grade="release", reached_recheck=True,
                     inside=(), suites_run=None, suites_commit=None, dependencies=DEPENDENCIES):
        """A run log shaped like the coordinator writes it, and its script-suites record."""
        verdicts = verdicts or {}
        lines = [f"Gates at once: {at_once} (--gates-at-once {at_once}, chosen by fixture)",
                 "=== phase fetch begins ===", "=== phase fetch ends: 1.0s ===", f"Source: {self.SOURCE}"]
        for gate in m.GATE_NAMES:
            verdict = verdicts.get(gate, "PASSED")
            if verdict == "SKIPPED":
                lines += [f"SKIPPED {gate}: the land already ran it"] + ([f"  SKIPPED {gate}"] if at_once != 1 else [])
                continue
            lines += [f"=== phase {gate} begins ===", "gate output",
                      *(inside if gate == "gates/script-suites" else ()),
                      f"=== phase {gate} ends: 1.0s ==="]
            if verdict is None:
                lines.pop()   # the section never ended: this gate recorded nothing
            elif at_once != 1:
                lines.append(f"  {verdict} {gate} {'in' if verdict == 'PASSED' else 'after'} 1.0s")
        if dependencies:
            lines.append(f"Installed dependencies: {dependencies}")
        if reached_recheck:
            lines += ["=== phase plan-recheck begins ===", "=== phase plan-recheck ends: 5.0s ===",
                      "Building and publishing v1.2.0-nightly.20260928.28..." if grade == "release"
                      else "Building v1.2.0-nightly.20260928.28..."]
        logs = self.root / "state" / "logs"
        logs.mkdir(parents=True, exist_ok=True)
        (logs / f"{self.RESUMED}.log").write_text(
            "".join(f"[2026-09-28T19:01:11.429Z] {line}\n" for line in lines))
        (self.root / "state" / m.SUITE_RESULTS).write_text(json.dumps(
            {"run_id": suites_run or self.RESUMED, "commit": suites_commit or self.SOURCE, "suites": []}))

    def test_a_run_whose_gates_all_passed_is_built_without_running_any_gate(self):
        self.recorded_run()
        r = self.runner()
        r.checkout = Mock(return_value=self.SOURCE)
        (self.root / "plan.json").write_text("{}")
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("release", gates_passed_in=self.RESUMED)
        r.gates.assert_not_called()
        r.checkout.assert_called_once_with(self.SOURCE)       # that run's commit, not main's tip
        ancestry, build = r.command.call_args_list
        self.assertEqual(ancestry.args[:4], ("git", "merge-base", "--is-ancestor", self.SOURCE))
        self.assertEqual(build.args[2], "run")
        self.assertIs(build.kwargs["release_build"], True)
        plan = json.loads((self.root / "plan.json").read_text())
        self.assertEqual(plan["gates_passed_in"], {"run_id": self.RESUMED, "source_commit": self.SOURCE})
        self.assertEqual(plan["checks_skipped"], sorted(m.GATE_NAMES))
        self.assertEqual(plan["script_suites"]["run_id"], self.RESUMED)

    def test_a_pass_is_reused_only_where_the_installed_dependencies_are_the_ones_it_ran_against(self):
        # Codex hunt part 2, finding 11, second reuse path: a saved pass certifies the
        # dependencies it ran against (node_modules, browsers, toolchain), not only the commit.
        cases = {
            "the dependencies changed": (self.DEPENDENCIES, "e" * 64, "differ"),
            "the run recorded none": (None, self.DEPENDENCIES, "recorded no identity"),
            "they cannot be read now": (self.DEPENDENCIES, None, "cannot be read"),
        }
        for name, (recorded, now, why) in cases.items():
            with self.subTest(name):
                self.recorded_run(dependencies=recorded)
                r = self.runner()
                r.installed_dependencies_digest = Mock(return_value=now)
                r.checkout = Mock(return_value="main-tip")
                (self.root / "plan.json").write_text("{}")
                out = io.StringIO()
                with contextlib.redirect_stdout(out):
                    r.perform("release", gates_passed_in=self.RESUMED)
                r.gates.assert_called_once()                   # the gates run
                r.checkout.assert_called_once_with(None)       # on main's tip, not the old commit
                self.assertIn("Gates run", out.getvalue())
                self.assertIn(why, out.getvalue())
        # Where they match, the pass is reused and says the dependencies are the same.
        self.recorded_run()
        r = self.runner()
        r.checkout = Mock(return_value=self.SOURCE)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            r.perform("release", gates_passed_in=self.RESUMED)
        r.gates.assert_not_called()
        self.assertIn("installed dependencies are the ones they ran against", out.getvalue())

    def test_a_run_that_runs_the_gates_records_the_dependencies_they_ran_against(self):
        r = self.runner()
        r.log = io.StringIO()
        r.gates.return_value = None
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("release")
        self.assertIn(f"Installed dependencies: {self.DEPENDENCIES}", r.log.getvalue())

    def test_a_run_whose_gates_did_not_all_pass_is_refused_before_anything_runs(self):
        cases = {
            "a gate failed": dict(verdicts={"gates/lint-tauri": "FAILED"}, reached_recheck=False),
            "a gate was stopped": dict(verdicts={"gates/ui-suite": "STOPPED"}, reached_recheck=False),
            "a gate was skipped": dict(verdicts={m.LAND_PROVEN_GATE: "SKIPPED"}),
            "a gate never recorded a pass": dict(verdicts={"gates/privacy-sweep": None}),
            "passes forged into a run that failed": dict(reached_recheck=False),
            "a build run reused by release": dict(grade="build"),
            "the suite record is another run's": dict(suites_run="20260928T180049Z-bed5009b"),
            "the suite record is another commit's": dict(suites_commit="0" * 40),
        }
        for name, shape in cases.items():
            with self.subTest(name):
                self.recorded_run(**shape)
                r = self.runner()
                with self.assertRaises(ValueError), contextlib.redirect_stdout(io.StringIO()):
                    r.perform("release", gates_passed_in=self.RESUMED)
                r.checkout.assert_not_called()
                r.gates.assert_not_called()
                r.command.assert_not_called()

    def test_only_the_coordinators_own_lines_count_and_one_at_a_time_runs_count_too(self):
        # A suite printing a verdict-shaped line inside its own section is not a verdict.
        self.recorded_run(inside=("  FAILED gates/core-tests after 0.0s: a fixture",
                                  "Source: " + "1" * 40))
        r = self.runner()
        self.assertEqual(r.accept_passed_gates(self.RESUMED, "release")["source_commit"], self.SOURCE)
        self.recorded_run(at_once=1)
        self.assertEqual(r.accept_passed_gates(self.RESUMED, "release")["source_commit"], self.SOURCE)
        self.recorded_run(grade="build")
        self.assertEqual(r.accept_passed_gates(self.RESUMED, "build")["source_commit"], self.SOURCE)
        for bad in ("", "../x", "20260928T190111Z-40a1616"):
            with self.assertRaisesRegex(ValueError, "run id"):
                r.accept_passed_gates(bad, "release")

    def test_a_resume_is_refused_for_any_other_command_or_beside_other_gate_options(self):
        r = self.runner()
        self.recorded_run()
        for kwargs in (dict(checks_done_at_land=self.SOURCE), dict(no_host_screen=True)):
            with self.subTest(**kwargs), self.assertRaisesRegex(ValueError, "alone"):
                r.perform("build", gates_passed_in=self.RESUMED, **kwargs)
        with self.assertRaisesRegex(ValueError, "alone"):
            r.perform("stable", gates_passed_in=self.RESUMED, from_nightly="v1")
        script = Path(m.__file__)
        for argv in (["publish", "--run", "x", "--gates-passed-in", self.RESUMED],
                     ["release", "--gates-passed-in", self.RESUMED, "--gates-at-once", "2"]):
            result = subprocess.run([sys.executable, str(script), *argv], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertIn("--gates-passed-in", result.stderr)

    def test_unchanged_source_does_not_build(self):
        r = self.runner(False)
        r.perform("release")
        r.gates.assert_not_called()
        r.command.assert_not_called()

    CANDIDATE_INFO = {"tag": "v1.2.0-nightly.20260916.1", "version": "1.2.0-nightly.20260916.1",
                      "candidate_ref": "refs/candidates/1",
                      "source_commit": "deadbeefcafe0123456789", "run_id": "fixture-run-id",
                      "run_attempt": "1"}

    def write_candidate(self, out):
        out.mkdir(parents=True, exist_ok=True)
        (out / "candidate.json").write_text(json.dumps({"info": self.CANDIDATE_INFO, "files": {}}))

    def test_build_stops_before_publishing_and_records_the_run(self):
        r = self.runner()
        r.env = {"RICHOS_NIGHTLY_RUN_ID": "fixture-run-id"}
        def fake_command(*args, **kwargs):
            self.assertEqual(args[2], "build")
            self.assertIs(kwargs.get("credentials"), True)
            out = Path(args[args.index("--out") + 1])
            self.write_candidate(out)
        r.command = Mock(side_effect=fake_command)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            r.perform("build")
        r.command.assert_called_once()  # only "build" -- never "run" or "finish"
        pointer = self.root / "state" / "runs" / "fixture-run-id.json"
        self.assertTrue(pointer.exists())
        recorded_out = Path(json.loads(pointer.read_text())["out"])
        self.assertTrue((recorded_out / "candidate.json").exists())
        self.assertIn("publish --run fixture-run-id", buf.getvalue())
        self.assertIn("RICHOS_ACTIVATION=regular", buf.getvalue())
        self.assertIn("the update channel has not moved", buf.getvalue())

    # ---- 2026-09-29: the build boots its own candidate in the test VM ----------------------
    def screenless_candidate(self, out):
        info = {**self.CANDIDATE_INFO, "no_host_screen": True, "script_suites": {"suites": [
            {"name": "gui-boot.test.sh", "state": "not-run", "reason": "no screen"}]}}
        out.mkdir(parents=True, exist_ok=True)
        (out / "candidate.json").write_text(json.dumps({"info": info, "files": {}}))

    def build_that_takes_its_vm_proof(self, proof_result):
        r = self.runner()
        r.env = {"RICHOS_NIGHTLY_RUN_ID": "fixture-run-id"}
        r.gui_host, r.vm_settings = "richos-test-1", {"TESTVM_ROOT": "/Volumes/E1TB/testvm"}
        calls = []

        def fake_command(*args, **kwargs):
            calls.append((args, kwargs))
            if args[2] == "build":
                self.screenless_candidate(Path(args[args.index("--out") + 1]))
            elif str(args[1]).endswith("gui-proof-in-vm.sh"):
                proof = Path(args[args.index("--out") + 1])
                proof.parent.mkdir(parents=True, exist_ok=True)
                proof.write_text(f"richos-gui-proof 1\nsuite=shipped-bundle-boot\n"
                                 f"commit={self.CANDIDATE_INFO['source_commit']}\nwhere=vm:richos-test-1\n"
                                 f"result={proof_result}\nat=2026-09-29T12:00:00Z\n--- output ---\n")
                if proof_result != "pass":
                    raise RuntimeError("bash failed (exit 1); see the run log")
        r.command = Mock(side_effect=fake_command)
        return r, calls

    def test_a_screenless_build_boots_its_candidate_in_the_vm_and_publish_uses_that_proof(self):
        r, calls = self.build_that_takes_its_vm_proof("pass")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            r.perform("build", no_host_screen=True)
        proof_calls = [(a, k) for a, k in calls if str(a[1]).endswith("gui-proof-in-vm.sh")]
        self.assertEqual(len(proof_calls), 1, calls)
        args, kwargs = proof_calls[0]
        self.assertEqual(args[args.index("--run") + 1], "fixture-run-id")
        self.assertEqual(args[args.index("--vm") + 1], "richos-test-1")
        self.assertEqual(kwargs["env_extra"]["RICHOS_NIGHTLY_STATE"], str(self.root / "state"))
        self.assertEqual(kwargs["env_extra"]["TESTVM_ROOT"], "/Volumes/E1TB/testvm")
        self.assertIn("Screen suites: never on this Mac's display", buf.getvalue())
        self.assertIn("VM boot proof PASSED", buf.getvalue())
        # And publish takes it with nobody passing --gui-proof.
        (self.root / "state" / "source").mkdir(parents=True, exist_ok=True)
        publisher = self.runner()
        publisher.command = Mock()
        with contextlib.redirect_stdout(io.StringIO()):
            publisher.perform("publish", run_id="fixture-run-id")
        self.assertEqual(publisher.command.call_args.args[2], "finish")

    def test_a_vm_proof_that_fails_fails_the_build_and_still_shows_the_candidate(self):
        r, _ = self.build_that_takes_its_vm_proof("fail:no-window")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), \
                self.assertRaisesRegex(RuntimeError, "THE CANDIDATE IS BUILT, AND ITS BOOT WAS NOT PROVEN"):
            r.perform("build", no_host_screen=True)
        self.assertIn("Candidate build", buf.getvalue())
        # The failed proof is kept and publish refuses it: a failed boot is not evidence.
        (self.root / "state" / "source").mkdir(parents=True, exist_ok=True)
        publisher = self.runner()
        publisher.command = Mock()
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(ValueError, "not a pass"):
            publisher.perform("publish", run_id="fixture-run-id")
        publisher.command.assert_not_called()

    def test_publish_calls_finish_without_signing_credentials(self):
        r = self.runner()
        (self.root / "state" / "source").mkdir(parents=True)
        out = self.root / "state" / "releases" / self.CANDIDATE_INFO["tag"]
        self.write_candidate(out)
        r.record_run(self.CANDIDATE_INFO["run_id"], out)
        r.command = Mock()
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"])
        r.checkout.assert_not_called()
        r.plan.assert_not_called()
        args, kwargs = r.command.call_args
        self.assertEqual(args[2], "finish")
        self.assertEqual(Path(args[args.index("--out") + 1]), out)
        self.assertNotIn("credentials", kwargs)

    def moved_worktree(self):
        """A real repository whose dedicated worktree built a release commit, then moved.

        The release commit exists only on the remote's candidate ref, the way a fresh clone
        of this repository sees it; the worktree is then checked out on main, which is what a
        `check` or another `build` does to it (hunt part 2, section 16).
        """
        git = ["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
               "-c", "core.hooksPath=/dev/null"]
        def run(*args, cwd):
            return subprocess.run([*git, *args], cwd=cwd, check=True, capture_output=True,
                                  text=True).stdout.strip()
        remote, repo, other = self.root / "remote.git", self.root / "repo", self.root / "other"
        run("init", "-q", "--bare", str(remote), cwd=self.root)
        run("init", "-q", "-b", "main", str(repo), cwd=self.root)
        (repo / "VERSION").write_text("1.2.0\n")
        run("add", ".", cwd=repo)
        run("commit", "-qm", "source", cwd=repo)
        run("remote", "add", "origin", str(remote), cwd=repo)
        run("push", "-q", "origin", "main", cwd=repo)
        run("clone", "-q", str(remote), str(other), cwd=self.root)
        (other / "nightly-build.json").write_text("{}\n")
        run("add", ".", cwd=other)
        run("commit", "-qm", "Build v1.2.0-nightly.20260916.1", cwd=other)
        build_commit = run("rev-parse", "HEAD", cwd=other)
        run("push", "-q", "origin", f"{build_commit}:refs/candidates/1", cwd=other)
        source = self.root / "state" / "source"
        run("worktree", "add", "-q", "--detach", str(source), "main", cwd=repo)
        info = {**self.CANDIDATE_INFO, "build_commit": build_commit}
        r = self.runner()
        r.repo = repo
        at_finish = []
        def command(*args, cwd=None, capture=False, **kwargs):
            if len(args) > 2 and args[2] == "finish":
                at_finish.append(run("rev-parse", "HEAD", cwd=source))
                return None
            result = subprocess.run([str(a) for a in args], cwd=cwd or r.source,
                                    capture_output=True, text=True)
            if result.returncode:
                raise RuntimeError(f"{Path(str(args[0])).name} failed (exit {result.returncode})")
            return result.stdout.strip() if capture else None
        r.command = Mock(side_effect=command)
        out = self.root / "state" / "releases" / info["tag"]
        out.mkdir(parents=True)
        (out / "candidate.json").write_text(json.dumps({"info": info, "files": {}}))
        r.record_run(info["run_id"], out)
        return r, source, build_commit, at_finish, run

    def test_publish_puts_the_build_commit_back_instead_of_asking_for_a_rebuild(self):
        r, source, build_commit, at_finish, run = self.moved_worktree()
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"])
        # finish ran, from the commit the candidate was built from; nothing was rebuilt.
        self.assertEqual(at_finish, [build_commit])
        self.assertFalse([c for c in r.command.call_args_list
                          if len(c.args) > 2 and c.args[2] in ("build", "run")])
        self.assertIn(build_commit[:12], r.log.getvalue())

    def test_publish_never_discards_changes_in_the_dedicated_worktree(self):
        r, source, build_commit, at_finish, run = self.moved_worktree()
        (source / "VERSION").write_text("someone's edit\n")
        with contextlib.redirect_stdout(io.StringIO()), \
                self.assertRaisesRegex(ValueError, "nightly worktree has changes"):
            r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"])
        self.assertEqual(at_finish, [])
        self.assertEqual((source / "VERSION").read_text(), "someone's edit\n")

    def test_candidate_prints_without_touching_anything(self):
        r = self.runner()
        out = self.root / "state" / "releases" / self.CANDIDATE_INFO["tag"]
        self.write_candidate(out)
        r.record_run(self.CANDIDATE_INFO["run_id"], out)
        r.command = Mock()
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            r.perform("candidate", run_id=self.CANDIDATE_INFO["run_id"])
        r.command.assert_not_called()
        r.checkout.assert_not_called()
        r.plan.assert_not_called()
        self.assertIn(str(out), buf.getvalue())
        self.assertIn("Candidate build 1:", buf.getvalue())
        self.assertIn("no public version tag or release-list entry", buf.getvalue())
        self.assertIn("RICHOS_ACTIVATION=regular", buf.getvalue())
        self.assertIn(f"publish --run {self.CANDIDATE_INFO['run_id']}", buf.getvalue())

    def test_publish_and_candidate_require_a_recorded_run(self):
        r = self.runner()
        for command in ("publish", "candidate"):
            with self.subTest(command=command):
                with self.assertRaisesRegex(ValueError, "no recorded build"):
                    r.perform(command, run_id="never-built")
        r.command.assert_not_called()

    def test_publish_and_candidate_require_a_run_id(self):
        r = self.runner()
        for command in ("publish", "candidate"):
            with self.subTest(command=command):
                with self.assertRaisesRegex(ValueError, "valid --run"):
                    r.perform(command, run_id=None)

    def test_failed_gates_do_not_publish(self):
        for method in ("preflight", "runtime", "gates"):
            with self.subTest(method=method):
                r = self.runner()
                getattr(r, method).side_effect = RuntimeError("failed")
                with self.assertRaises(RuntimeError):
                    r.perform("release")
                r.command.assert_not_called()

    def test_fresh_plan_can_skip_after_checks(self):
        r = self.runner()
        r.plan.side_effect = [r.plan.return_value,
                             (self.root / "plan.json", {"build": False, "reason": "already published"})]
        r.perform("release")
        r.command.assert_not_called()

    def test_the_ui_suite_writes_nowhere_another_gates_leak_canary_watches(self):
        # Audit R5: workspace-mutants runs ci-shard.sh from `source`, whose leak canary fails
        # its unit for any new or changed `git status` line there, while the UI suite rewrites
        # committed screenshots in its own checkout. So its checkout must not be `source`.
        r, seen = self.gate_commands()
        calls = []

        def record(args, **kwargs):
            calls.append(([str(a) for a in args], Path(kwargs["cwd"])))
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=record), \
                contextlib.redirect_stdout(io.StringIO()):
            r.ui_suite()
        ui = [cwd for argv, cwd in calls if "run.js" in " ".join(argv)]
        self.assertEqual(len(ui), 1, calls)
        self.assertNotEqual(ui[0], r.source / m.UI_TESTS)
        self.assertFalse(ui[0].is_relative_to(r.source), f"the UI suite ran inside {r.source}: {ui[0]}")
        # ...and what it changed is looked for, and put back, THERE.
        status = [cwd for argv, cwd in calls if argv[:3] == ["git", "status", "--porcelain"]]
        self.assertEqual(status, [ui[0].parents[len(m.UI_TESTS.parts) - 1]], calls)

    def test_the_ui_worktree_is_this_commit_and_a_leftover_change_never_survives(self):
        repo = self.root / "repo"
        repo.mkdir()

        def git(*args, cwd=repo):
            return subprocess.check_output(["git", *args], cwd=cwd, text=True,
                                           stderr=subprocess.DEVNULL).strip()
        git("init", "-b", "main")
        git("config", "user.name", "Fixture")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "core.hooksPath", "/dev/null")
        (repo / "shot.png").write_text("committed picture")
        git("add", ".")
        git("commit", "-m", "one")
        first = git("rev-parse", "HEAD")
        state = self.root / "state"
        state.mkdir()
        with (self.root / "commands.log").open("w") as log:
            r = m.Runner(repo, state, self.private_budget()[0], log)
            git("worktree", "add", "--detach", str(r.source), first)
            tree = r.ui_checkout()
            self.assertNotEqual(tree.resolve(), r.source.resolve())
            self.assertEqual(git("rev-parse", "HEAD", cwd=tree), first)
            # A run that died before its restore left a rewritten shot behind.
            (tree / "shot.png").write_text("rewritten by a shard")
            (repo / "shot.png").write_text("committed picture, two")
            git("commit", "-am", "two")
            second = git("rev-parse", "HEAD")
            git("checkout", "--detach", second, cwd=r.source)
            self.assertEqual(r.ui_checkout(), tree)
            self.assertEqual(git("rev-parse", "HEAD", cwd=tree), second)
            self.assertEqual((tree / "shot.png").read_text(), "committed picture, two")
            self.assertEqual(git("status", "--porcelain", cwd=r.source), "")
            # Something else at that path is refused, never adopted or overwritten.
            other = self.root / "other"
            other.mkdir()
            git("init", "-b", "main", cwd=other)
            with patch.object(m.Runner, "UI_CHECKOUT", "../other"), \
                    self.assertRaisesRegex(ValueError, "not the UI suite's worktree"):
                r.ui_checkout()

    def test_checkout_uses_remote_main_without_touching_developer_edits(self):
        repo, remote = self.root / "repo", self.root / "remote.git"
        repo.mkdir()
        def git(*args, cwd=repo):
            return subprocess.check_output(["git", *args], cwd=cwd, text=True, stderr=subprocess.DEVNULL).strip()
        git("init", "--bare", str(remote))
        git("init", "-b", "main")
        git("config", "user.name", "Fixture")
        git("config", "user.email", "fixture@example.invalid")
        git("config", "core.hooksPath", "/dev/null")
        (repo / "source").write_text("committed")
        git("add", ".")
        git("commit", "-m", "source")
        sha = git("rev-parse", "HEAD")
        git("remote", "add", "origin", str(remote))
        git("push", "origin", "main")
        (repo / "source").write_text("uncommitted developer work")
        state = self.root / "state"
        state.mkdir()
        with (self.root / "commands.log").open("w") as log:
            r = m.Runner(repo, state, os.environ.copy(), log)
            self.assertEqual(r.checkout(), sha)
            self.assertEqual((r.source / "source").read_text(), "committed")
            self.assertEqual((repo / "source").read_text(), "uncommitted developer work")
            self.assertEqual(r.checkout(), sha)
            (r.source / "unexpected").write_text("do not delete")
            with self.assertRaisesRegex(ValueError, "has changes"):
                r.checkout()
            self.assertTrue((r.source / "unexpected").exists())

    # ---- where the time goes, and the one gate a land is allowed to have already run ----

    def gate_commands(self, checks_done_at_land=None):
        """Every gate subprocess a build would launch, as plain argv lists."""
        r = m.Runner(self.root, self.root / "state",
                     {"PATH": "/usr/bin", "RICHOS_NAMED_PERSONS_FILE": "/fixture/list"},
                     io.StringIO())
        seen = []

        def record(args, **kwargs):
            seen.append([str(a) for a in args])
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=record), \
                contextlib.redirect_stdout(io.StringIO()):
            r.gates(checks_done_at_land)
        return r, seen

    def test_default_runs_every_gate_and_names_each_one_in_the_timings(self):
        r, seen = self.gate_commands()
        self.assertEqual(len(seen), 12)
        self.assertEqual(r.skipped, {})
        # ORDER IS PART OF THE ASSERTION, not incidental. The release smoke is first
        # because it is the cheapest refusal in the build (0.2 s against ~950 s), and the
        # defect class it catches -- a release-only step that rotted since the last
        # release -- is otherwise found by the release that needed it.
        self.assertEqual([name for name, _, _ in r.timings],
                         ["gates/release-smoke", "gates/core-tests", "gates/updater-tests",
                          "gates/script-suites", "gates/lint-tauri", m.WORKSPACE_MUTANTS_GATE,
                          m.UI_SUITE_GATE, "gates/privacy-sweep", "gates-wall-clock"])
        # The workspace-spec mutation pass runs HERE, before every nightly, and on no land
        # (CEO, 2026-09-23, "Only before nightlies"): the unit through ci-shard.sh, with the
        # opt-in stated at this call site.
        mut = [argv for argv in seen if "fourteen.txt" in " ".join(argv)]
        self.assertEqual(len(mut), 1)
        self.assertIn("proof-run.py", " ".join(mut[0]))
        self.assertEqual(Path(mut[0][mut[0].index("--commands") + 1]).read_text().strip(),
                         "cd richos/engine && bash scripts/ci-shard.sh --only-units " +
                         "mega-lander/tests/workspace-spec-fourteen.test.sh")
        lint = [argv for argv in seen if any(a.endswith('/lint.sh') for a in argv)]
        self.assertEqual(lint, [["bash", str(r.source / m.SCRIPTS / "lint.sh"), "--all",
                                 "--suite-results", str(r.state / m.SUITE_RESULTS)]])
        smoke = [argv for argv in seen if "release-smoke" in " ".join(argv)]
        self.assertEqual(len(smoke), 1, seen)
        self.assertTrue([a for a in smoke[0] if a.endswith("nightly.py")], smoke)
        # THE UI SUITE IS ONE SUBPROCESS, not a fan-out written beside `command()`. If this
        # ever counts more than one `run.js`, the shards have been hoisted back into this
        # file and every assertion in this class about a gate's argv and environment has
        # quietly stopped covering four of the five gates.
        ui = [argv for argv in seen if "run.js" in " ".join(argv)]
        self.assertEqual(len(ui), 1, seen)
        self.assertIn(f"--shards={m.UI_SHARDS}", ui[0])
        # THE SUITE WRITES TO ITS OWN CHECKOUT (lib/harness.js:publishShot rewrites a
        # committed screenshot whose picture changed), and `checkout()` refuses a dirty
        # worktree on the NEXT build. So the gate asks git what moved. Losing this call
        # would not fail anything today; it would refuse to start tomorrow.
        self.assertTrue([argv for argv in seen if argv[:3] == ["git", "status", "--porcelain"]],
                        seen)

    def test_the_flag_alone_drops_nothing_a_land_did_not_prove(self):
        """`--checks-done-at-land` may drop what a land ran and NOTHING else.

        It does not run the updater crate, the fourteen script suites or the privacy sweep,
        so those stay on the candidate path no matter what the flag says. A flag that grew
        to cover them would be trading a gate for time nobody measured.

        THE SHA ALONE PROVES NO SCOPE (hunt part 2, finding 24). The core tests used to go
        on the sha alone, "because the land ALWAYS runs them". It does not: proof-for.sh
        selects only the integration target or the `--lib module::` tests a change touches,
        so a matching sha says which tree was checked, never how much of richos-core was.
        Both skips now need evidence of the whole gate on this tree (the land's receipt for
        the core tests, the coverage proof for the UI suite); with neither on this machine,
        both gates run, which is the safe direction for a missing file to fail in.
        """
        r, seen = self.gate_commands("a" * 40)
        joined = [" ".join(argv) for argv in seen]
        self.assertTrue([c for c in joined if "-p richos-core" in c], joined)
        self.assertNotIn(m.LAND_PROVEN_GATE, r.skipped)
        self.assertTrue([c for c in joined if "richos-user-update" in c], joined)
        self.assertTrue([c for c in joined if "run-tests.sh" in c], joined)
        self.assertTrue([c for c in joined if "named-persons.sh" in c], joined)
        # The release smoke is NEVER dropped by this flag. A land does not run it, and
        # its input is the release path itself rather than a tree whose sha was proved.
        self.assertTrue([c for c in joined if "release-smoke" in c], joined)
        # NO PROOF ON THIS MACHINE: every gate runs, and none is recorded as skipped.
        self.assertEqual(len(seen), 12)
        # The workspace-spec mutation pass is never dropped by this flag: a land does not run it
        # (CEO, 2026-09-23, "Only before nightlies"), so there is nothing a land proved.
        self.assertTrue([c for c in joined if "fourteen.txt" in c], joined)
        self.assertTrue([c for c in joined if "run.js" in c], joined)
        self.assertNotIn(m.UI_SUITE_GATE, r.skipped)

    def landed_repository(self, commands, not_run=()):
        """A real repository whose HEAD tree has autocheck's land receipt, and HEAD's sha.

        The receipt is written exactly where and how autocheck.py's land_check writes one
        (`<git-common-dir>/richos-autocheck/land/<tree>`, JSON with `commands` and `not_run`),
        so this case reads the file the land really leaves, not a shape invented for it."""
        repo = self.root / "landed"
        repo.mkdir()
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}

        def git(*args):
            return subprocess.run(
                ["git", "-C", str(repo), "-c", "user.name=fixture",
                 "-c", "user.email=fixture@example.invalid", "-c", "core.hooksPath=/dev/null",
                 "-c", "commit.gpgsign=false", *args],
                check=True, env=env, capture_output=True, text=True).stdout.strip()

        git("init", "-q")
        (repo / "tree.txt").write_text("the landed tree\n")
        git("add", "-A")
        git("commit", "-q", "-m", "fixture land")
        sha, tree = git("rev-parse", "HEAD"), git("rev-parse", "HEAD^{tree}")
        receipt = repo / ".git" / "richos-autocheck" / "land" / tree
        receipt.parent.mkdir(parents=True)
        receipt.write_text(json.dumps({
            "tree": tree, "what": "merge into main", "commands": list(commands),
            "not_run": [{"check": c, "why": "no-screen", "suites": []} for c in not_run],
            "seconds": 1.0, "at": "2026-09-30T00:00:00Z"}) + "\n")
        return repo, sha

    def core_gate(self, repo, sha):
        """(runner, whether `cargo test -p richos-core` was launched) for gates(sha)."""
        r = m.Runner(repo, self.root / "state",
                     {"PATH": "/usr/bin", "RICHOS_NAMED_PERSONS_FILE": "/fixture/list"},
                     io.StringIO())
        seen = []

        def record(args, **kwargs):
            seen.append(" ".join(str(a) for a in args))
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=record), \
                contextlib.redirect_stdout(io.StringIO()):
            r.gates(sha)
        return r, any("-p richos-core" in c for c in seen)

    def test_the_core_tests_are_skipped_only_on_a_land_receipt_that_ran_the_whole_crate(self):
        """A land's receipt for this tree has to show `cargo test -p richos-core` unfiltered.

        proof-for.sh writes that command only when the whole crate is selected (its manifest
        changed); for an ordinary change it writes `--test <target>` or `--lib <module>::`,
        and those passing say nothing about the rest of the crate (hunt part 2, finding 24).
        """
        whole = "cd richos/app && cargo test -p richos-core"  # proof-for.sh's line, verbatim
        cases = (
            ("only the module the change touched", [whole + " --lib session::"], (), False),
            ("only an integration target", [whole + " --test resume"], (), False),
            ("the whole crate, listed as not run", [whole], [whole], False),
            ("the whole crate, run and passed", [whole + " --lib session::", whole], (), True),
        )
        for label, commands, not_run, skipped in cases:
            with self.subTest(label):
                for leftover in (self.root / "landed", self.root / "state"):
                    shutil.rmtree(leftover, ignore_errors=True)
                repo, sha = self.landed_repository(commands, not_run)
                r, ran = self.core_gate(repo, sha)
                self.assertEqual(ran, not skipped)
                self.assertEqual(m.LAND_PROVEN_GATE in r.skipped, skipped)
                if skipped:
                    self.assertIn(sha, r.skipped[m.LAND_PROVEN_GATE])
                    self.assertIn("land receipt", r.skipped[m.LAND_PROVEN_GATE])

    def test_a_ui_coverage_proof_for_this_sha_drops_the_ui_suite_and_a_wrong_one_refuses(self):
        """The proof is read the way every other proof in this file is: by its commit.

        A file named after the right sha whose CONTENTS name a different tree is the most
        useful lie available here -- it would buy 378 s by certifying a run over something
        else -- so both are checked, and disagreement raises rather than silently running
        the suite.
        """
        sha = "a" * 40
        r = m.Runner(self.root, self.root / "state",
                     {"PATH": "/usr/bin", "RICHOS_NAMED_PERSONS_FILE": "/fixture/list"},
                     io.StringIO())
        proof_path = r.ui_proof_path(sha)
        proof_path.parent.mkdir(parents=True, exist_ok=True)

        # No file at all: nothing is accepted and nothing is raised.
        self.assertIsNone(r.accept_ui_proof(sha))

        proof_path.write_text(json.dumps(
            {"proof": "ui-suite-coverage", "commit": sha, "suites": 55, "ran": 55, "checks": 863,
             "at": "2026-09-20T00:00:00Z"}))
        self.assertEqual(r.accept_ui_proof(sha)["ran"], 55)

        seen = []

        def record(args, **kwargs):
            seen.append([str(a) for a in args])
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=record), \
                contextlib.redirect_stdout(io.StringIO()):
            r.gates(sha)
        self.assertFalse([c for c in seen if "run.js" in " ".join(c)], seen)
        self.assertIn(m.UI_SUITE_GATE, r.skipped)

        # A proof whose contents name a different tree proves nothing about this one.
        proof_path.write_text(json.dumps(
            {"proof": "ui-suite-coverage", "commit": "b" * 40, "ran": 55, "checks": 863}))
        with self.assertRaises(ValueError):
            r.accept_ui_proof(sha)

        # And a file that is not a coverage proof at all is not one.
        proof_path.write_text(json.dumps({"proof": "gui-boot", "commit": sha}))
        self.assertIsNone(r.accept_ui_proof(sha))

    def test_a_ui_coverage_proof_that_records_no_coverage_never_skips_the_ui_gate(self):
        """Hunt part 2, finding 17: the reader enforces the shape run.js writes on a PASS.

        `{proof, commit}` with no counts, or a one-character commit, used to skip the whole
        UI gate and announce "None suite(s), None checks". Each of these names this build's
        sha (or a prefix of it) and claims coverage it does not show, so each is refused, and
        the gate never skips on it.
        """
        sha = "b" * 40
        r = m.Runner(self.root, self.root / "state",
                     {"PATH": "/usr/bin", "RICHOS_NAMED_PERSONS_FILE": "/fixture/list"},
                     io.StringIO())
        proof_path = r.ui_proof_path(sha)
        proof_path.parent.mkdir(parents=True, exist_ok=True)
        full = {"proof": "ui-suite-coverage", "commit": sha, "suites": 55, "ran": 55,
                "checks": 863, "at": "2026-09-30T00:00:00Z"}
        empty = [
            {"proof": "ui-suite-coverage", "commit": sha},
            {"proof": "ui-suite-coverage", "commit": "b", "suites": 55, "ran": 55, "checks": 863},
            {"proof": "ui-suite-coverage", "commit": sha[:12], "suites": 55, "ran": 55, "checks": 863},
            {"proof": "ui-suite-coverage", "commit": "b", "suites": 0, "ran": 0, "checks": 0},
            {**full, "ran": 0},
            {**full, "checks": 0},
            {**full, "suites": 0},
            {**full, "ran": 56},
            {**full, "ran": True},
            {**full, "checks": "863"},
            {k: v for k, v in full.items() if k != "suites"},
        ]
        for body in empty:
            with self.subTest(proof=body):
                proof_path.write_text(json.dumps(body))
                with self.assertRaises(ValueError):
                    r.accept_ui_proof(sha)
                seen = []

                def record(args, **kwargs):
                    seen.append([str(a) for a in args])
                    return subprocess.CompletedProcess(args, 0, "", "")

                with patch.object(m, "owned_run", side_effect=record), \
                        contextlib.redirect_stdout(io.StringIO()), self.assertRaises(ValueError):
                    r.ui_suite(sha)
                self.assertNotIn(m.UI_SUITE_GATE, r.skipped)

        # The writer's real shape, with every suite run, is still accepted.
        proof_path.write_text(json.dumps(full))
        self.assertEqual(r.accept_ui_proof(sha)["checks"], 863)

    def test_a_ui_coverage_proof_that_admits_unrun_suites_never_skips_the_ui_gate(self):
        """Part 2 recheck, R17: a receipt that says suites were not run does not stand for them.

        run.js writes a PASS proof when a suite was skipped under --allow-skip or excused by
        a quarantine without any evidence, and it says so honestly: `ran` below `suites`. The
        gate this proof replaces runs EVERY discovered suite, so skipping it on a proof that
        ran one of 55 certified 54 suites nobody ran. Each partial proof below is refused and
        the gate is not skipped; the full proof beside them is still accepted.
        """
        sha = "c" * 40
        r = m.Runner(self.root, self.root / "state",
                     {"PATH": "/usr/bin", "RICHOS_NAMED_PERSONS_FILE": "/fixture/list"},
                     io.StringIO())
        proof_path = r.ui_proof_path(sha)
        proof_path.parent.mkdir(parents=True, exist_ok=True)
        full = {"proof": "ui-suite-coverage", "commit": sha, "suites": 55, "ran": 55,
                "checks": 863, "at": "2026-09-30T00:00:00Z", "quarantined": [],
                "quarantined_red": []}
        for body in ({**full, "ran": 1, "checks": 1}, {**full, "ran": 54}):
            with self.subTest(ran=body["ran"]):
                proof_path.write_text(json.dumps(body))
                with self.assertRaisesRegex(ValueError, r"%d of %d" % (body["ran"], body["suites"])):
                    r.accept_ui_proof(sha)
                seen = []

                def record(args, **kwargs):
                    seen.append([str(a) for a in args])
                    return subprocess.CompletedProcess(args, 0, "", "")

                with patch.object(m, "owned_run", side_effect=record), \
                        contextlib.redirect_stdout(io.StringIO()), self.assertRaises(ValueError):
                    r.ui_suite(sha)
                self.assertNotIn(m.UI_SUITE_GATE, r.skipped)

        proof_path.write_text(json.dumps(full))
        self.assertEqual(r.accept_ui_proof(sha)["ran"], 55)

    def test_a_sha_that_is_not_this_runs_source_is_refused(self):
        """The whole safety of the flag is this comparison.

        A flag that took its argument on trust would let yesterday's land wave a different
        tree's tests through today's candidate, which is worse than not having the flag:
        the candidate would carry a provenance record saying a gate was covered when the
        gate covered something else.
        """
        r = m.Runner(self.root, self.root / "state", {}, io.StringIO())
        source = "4f2d57c9f9519409427b86c7502b9a7588216bf1"
        self.assertEqual(r.accept_land_proof(source, source), source)
        self.assertEqual(r.accept_land_proof("4f2d57c", source), source)
        for wrong in ("deadbeefcafe0123456789012345678901234567", "4f2d57d", "4f2d5",
                      "", None, "not-hex", "4F2D57C9"):
            with self.subTest(sha=wrong), self.assertRaises(ValueError):
                r.accept_land_proof(wrong, source)

    def test_a_taken_skip_is_written_into_the_candidates_provenance(self):
        """A skip recorded only in a log lives on this Mac. This one travels.

        nightly.py copies the plan verbatim into `build-info.json` and into the commit it
        builds, so putting the record in the plan is what makes a candidate unable to
        claim a gate it did not run.
        """
        r = self.runner()
        r.env = {"RICHOS_NIGHTLY_RUN_ID": "fixture-run-id"}
        plan_path = self.root / "plan.json"
        r.plan = Mock(return_value=(plan_path, {
            "build": True, "tag": "v1.2.0-nightly.20260916.1",
            "source_commit": "4f2d57c9f9519409427b86c7502b9a7588216bf1"}))
        r.checkout = Mock(return_value="4f2d57c9f9519409427b86c7502b9a7588216bf1")
        r.command = Mock(side_effect=lambda *a, **k: self.write_candidate(
            Path(a[a.index("--out") + 1])))
        # `checks_skipped` is now read off the runner's OWN record of its skips rather than
        # being a literal, so the stand-in for gates() has to record one. That is the point
        # of the change: the plan cannot claim a skip that did not happen, or miss one that
        # did, the next time a gate becomes skippable.
        r.gates.side_effect = lambda *a, **k: r.skip(m.LAND_PROVEN_GATE, "fixture skip")
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("build", checks_done_at_land="4f2d57c9")
        recorded = json.loads(plan_path.read_text())
        self.assertEqual(recorded["checks_done_at_land"],
                         "4f2d57c9f9519409427b86c7502b9a7588216bf1")
        self.assertEqual(recorded["checks_skipped"], [m.LAND_PROVEN_GATE])
        # The land proof reaches gates() as the FIRST positional, which is what this case
        # is about. The keywords beside it (`--no-host-screen`, skip-when-unchanged) are a
        # different question with its own cases; asserting the exact call signature here
        # would make every future gate option fail a case about the land proof.
        self.assertEqual(r.gates.call_count, 1)
        self.assertEqual(r.gates.call_args.args[0],
                         "4f2d57c9f9519409427b86c7502b9a7588216bf1")

    def test_a_wrong_sha_refuses_before_any_gate_runs(self):
        r = self.runner()
        r.checkout = Mock(return_value="4f2d57c9f9519409427b86c7502b9a7588216bf1")
        with contextlib.redirect_stdout(io.StringIO()), \
                self.assertRaisesRegex(ValueError, "not the source this run fetched"):
            r.perform("build", checks_done_at_land="deadbeef")
        r.gates.assert_not_called()
        r.preflight.assert_not_called()
        r.command.assert_not_called()

    def test_every_logged_line_carries_the_clock_and_a_child_needs_no_change(self):
        """The log is stamped by handing children a pipe, not by asking them to cooperate.

        nightly.py, make-release.sh and package-app.sh are read from the FETCHED tree, not
        from this checkout, so an instrument that needed their cooperation would not
        measure anything until it had been landed and fetched. This one measures the tree
        as it is.
        """
        path = self.root / "run.log"
        with m.TimestampedLog(path) as log:
            subprocess.run(["/bin/echo", "a child said this"], stdout=log, check=True)
            log.write("and this script said this\n")
        lines = path.read_text().splitlines()
        self.assertEqual(len(lines), 2, lines)
        for line, text in zip(lines, ("a child said this", "and this script said this")):
            self.assertRegex(line, r"^\[\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z\] ")
            self.assertTrue(line.endswith(text), line)

    def test_a_process_that_outlives_its_command_cannot_crash_or_hang_the_log(self):
        """Run 20260928T143332Z-1f0e1b3a: a gate's log was closed while a process its command
        started still held the pipe. close() closed a buffered reader under the pump thread,
        waited on that process, and Python 3.14 raised in the pump. Now the pump is told to
        stop, the log says it stopped, and nothing raises."""
        path = self.root / "run.log"
        errors = []
        previous = threading.excepthook
        threading.excepthook = lambda args: errors.append(repr(args.exc_value))
        self.addCleanup(setattr, threading, "excepthook", previous)
        log = m.TimestampedLog(path)
        log.CLOSE_WAIT_SECONDS = 1
        straggler = subprocess.Popen(["sh", "-c", "echo before; exec sleep 30"], stdout=log)
        self.addCleanup(straggler.wait)
        self.addCleanup(straggler.kill)
        began = time.monotonic()
        log.close()
        # load-bound: a hang catcher only; the straggler sleeps 30 s, so 25 s proves close() did not wait for it
        self.assertLess(time.monotonic() - began, 25, "close() waited on a process it does not own")
        self.assertEqual(errors, [])
        lines = path.read_text().splitlines()
        self.assertTrue(lines[0].endswith(" before"), lines)
        self.assertIn("still holds this log's pipe", lines[-1])

    def test_milestones_match_in_order_and_only_while_the_build_step_is_open(self):
        """In order first; armed inside `build` as the second condition.

        Order is what does the work -- the first marker gates every later one. Arming is
        the belt: on the real log of run 20260919T180454Z-ac11d13e not one of the eleven
        patterns matches any of the 2,727 lines before the build step, so today it changes
        nothing. It is kept because `make-engine-asset.test.sh` and `make-release.test.sh`
        drive the very scripts these markers come from, and the day one of them echoes a
        real banner instead of a shim's, an unarmed matcher puts the engine boundary in
        the middle of the suites and reports a compile that took nine minutes.

        This test asserts both: the decoy ahead of the build step is ignored, and a marker
        arriving after the step closes is ignored too.
        """
        path = self.root / "run.log"
        decoy = "https://github.com/WebDevBooster/richos/releases/tag/v-from-a-test-fixture"
        with m.TimestampedLog(path, m.BUILD_MILESTONES) as log:
            log.write(decoy + "\n")
            log.arm(True)
            log.write("building the engine asset for v1.2.0-nightly.20260919.9\n")
            log.write("=== --check: building a second time, in a DIFFERENT environment\n")
            log.arm(False)
            log.write("=== every member accounted for, against git ===\n")
        self.assertEqual([name for name, _ in log.seen],
                         ["build/engine-asset", "build/engine-asset-recheck"])

    def test_milestones_are_not_gated_on_a_release_url_the_build_never_prints(self):
        """R49: the build uploads into an existing release and prints no release URL, so a
        marker waiting for one stopped the collector before `verify-download` and after."""
        path = self.root / "run.log"
        with m.TimestampedLog(path, m.BUILD_MILESTONES) as log:
            log.arm(True)
            log.write("building the engine asset for v1.2.0-nightly.20260919.9\n")
            log.write("=== --check: building a second time, in a DIFFERENT environment\n")
            log.write("=== every member accounted for, against git ===\n")
            log.write("fetching the PUBLISHED asset \u2014 the bytes a customer's first run will get:\n")
        self.assertEqual([name for name, _ in log.seen],
                         ["build/engine-asset", "build/engine-asset-recheck",
                          "build/engine-member-audit", "build/engine-verify-download"])

    def test_an_unseen_milestone_is_reported_rather_than_folded_into_its_neighbor(self):
        """A boundary nobody saw must not silently inflate the segment beside it."""
        r = m.Runner(self.root, self.root / "state",
                     {"RICHOS_NIGHTLY_RUN_ID": "fixture-run-id"}, io.StringIO())
        start = time.time()
        r.timings = [("fetch", start, start + 3), ("build", start + 3, start + 63)]
        r.log.seen = [("build/engine-asset", start + 13)]
        rows = dict(r.segments())
        self.assertEqual(round(rows["fetch"]), 3)
        self.assertEqual(round(rows["build/tag-and-prepare"]), 10)
        self.assertEqual(round(rows["build/engine-asset"]), 50)
        self.assertIsNone(rows["build/app-compile"])
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            r.summary()
        self.assertIn("not observed", buf.getvalue())

    # =====================================================================================
    # `--no-host-screen`, and the evidence `publish` demands in exchange for it
    # =====================================================================================
    #
    # The CEO, 2026-09-19: *"So, every engineer will keep opening the app making me unable
    # to do anything here or WHAT???"*. `gui-boot.test.sh` boots the real app on the real
    # screen for ~162 s of every build, and `--no-host-screen` holds it back.
    #
    # THAT IS A TRADE, NOT A FREE WIN, and every case below is one way the second half of
    # the trade could quietly not happen — which would leave the mode as nothing but a way
    # of skipping a gate on the road to a published release.

    def gui_candidate(self, gui_state=None, no_host_screen=None, build_commit=None):
        """A staged candidate whose build-info records `gui_state` for gui-boot."""
        info = dict(self.CANDIDATE_INFO)
        if build_commit:
            info["build_commit"] = build_commit
        if no_host_screen is not None:
            info["no_host_screen"] = no_host_screen
        if gui_state is not None:
            info["script_suites"] = {"jobs": 3, "suites": [
                {"name": "make-release.test.sh", "state": "passed"},
                {"name": "gui-boot.test.sh", "state": gui_state,
                 "reason": "no screen this run may use"}]}
        r = self.runner()
        (self.root / "state" / "source").mkdir(parents=True, exist_ok=True)
        out = self.root / "state" / "releases" / info["tag"]
        out.mkdir(parents=True, exist_ok=True)
        (out / "candidate.json").write_text(json.dumps({"info": info, "files": {}}))
        r.record_run(info["run_id"], out)
        r.command = Mock()
        return r, info

    def proof(self, name="gui.proof", suite="gui-boot.test.sh", result="pass",
              commit="deadbeefcafe0123456789", where="vm:richos-test-1"):
        path = self.root / name
        path.write_text(f"richos-gui-proof 1\nsuite={suite}\ncommit={commit}\n"
                        f"where={where}\nresult={result}\nat=2026-09-19T20:00:00Z\n"
                        "--- output ---\n  PASS  B1 the app booted\n")
        return path

    def test_a_candidate_whose_gui_boot_did_not_run_is_refused_without_a_proof(self):
        """THE CASE THE WHOLE MODE TURNS ON.

        A screenless build is only honest if the boot it did not watch has to be watched
        before anything becomes installable. Without this refusal, `--no-host-screen` is a
        way to publish an app nobody ever saw start.
        """
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
        with self.assertRaises(ValueError) as caught:
            with contextlib.redirect_stdout(io.StringIO()):
                r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"])
        self.assertIn("gui-boot", str(caught.exception))
        self.assertIn("gui-proof-in-vm.sh", str(caught.exception))
        # AND NOTHING WAS PUBLISHED. A refusal that still called `finish` would be a
        # warning wearing an exception's clothes.
        r.command.assert_not_called()

    def test_a_screenless_build_that_lost_its_report_is_refused_too(self):
        """Silence from a build that was TOLD not to use the screen is not a pass.

        The report could go missing for an innocent reason; what cannot happen is that the
        innocent reason and "the suite never ran" become indistinguishable. `no_host_screen`
        is recorded separately from the report for exactly this case.
        """
        r, _ = self.gui_candidate(gui_state=None, no_host_screen=True)
        with self.assertRaises(ValueError):
            with contextlib.redirect_stdout(io.StringIO()):
                r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"])
        r.command.assert_not_called()

    def test_a_candidate_from_before_the_field_still_publishes(self):
        """...and the mirror image, which the first version of this got WRONG.

        "Silence is never read as a pass" sounds right and would have made every candidate
        already staged on this Mac unpublishable, because none of them carries a field that
        did not exist when it was built. Those builds ran gui-boot; it was not skippable
        then. This case is what stops that rule being rediscovered.
        """
        r, _ = self.gui_candidate(gui_state=None, no_host_screen=None)
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"])
        self.assertEqual(r.command.call_args.args[2], "finish")

    def test_a_proof_against_this_candidate_lets_it_publish(self):
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"],
                      gui_proof=str(self.proof()))
        self.assertEqual(r.command.call_args.args[2], "finish")

    def test_a_proof_taken_against_another_tree_is_refused(self):
        """The sha is the whole point, exactly as it is for --checks-done-at-land.

        A proof that is not required to name THIS commit is a proof that can be taken once
        and reused forever, which is worse than no proof: it reads as diligence.
        """
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
        with self.assertRaises(ValueError) as caught:
            with contextlib.redirect_stdout(io.StringIO()):
                r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"],
                          gui_proof=str(self.proof(commit="0123456789abcdef")))
        self.assertIn("different tree", str(caught.exception))
        r.command.assert_not_called()

    def test_a_proof_naming_a_too_short_or_non_hex_commit_is_refused(self):
        """N03: one shared hex character is not this candidate's identity. The proof's commit
        must be 7-40 hex digits (as --checks-done-at-land's is) before a prefix match counts."""
        for bad in ("d", "deadbe", "DEADBEEF", "deadbeefcafe0123456789zz"):
            r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
            with self.subTest(commit=bad):
                with self.assertRaises(ValueError) as caught:
                    with contextlib.redirect_stdout(io.StringIO()):
                        r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"],
                                  gui_proof=str(self.proof(commit=bad)))
                self.assertIn("different tree", str(caught.exception))
                r.command.assert_not_called()
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"],
                      gui_proof=str(self.proof(commit="deadbee")))
        self.assertEqual(r.command.call_args.args[2], "finish")

    def test_a_proof_naming_the_build_commit_is_accepted(self):
        """The bundle is compiled from `build_commit` -- the version-bump commit whose
        parent is `source_commit`. A proof taken against the thing that was actually built
        must not be refused for naming it."""
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True,
                                  build_commit="abc123def456")
        # Putting the worktree back on the build commit has its own tests
        # (test_publish_puts_the_build_commit_back_...); this one is about the proof, and
        # the mocked `command` cannot answer the git questions that restore asks.
        r.restore_build_commit = Mock()
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"],
                      gui_proof=str(self.proof(commit="abc123def456")))
        self.assertEqual(r.command.call_args.args[2], "finish")

    def test_a_proof_that_records_a_failure_is_refused(self):
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
        with self.assertRaises(ValueError) as caught:
            with contextlib.redirect_stdout(io.StringIO()):
                r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"],
                          gui_proof=str(self.proof(result="fail:1")))
        self.assertIn("not a pass", str(caught.exception))
        r.command.assert_not_called()

    def test_a_proof_for_a_different_suite_is_refused(self):
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
        with self.assertRaises(ValueError):
            with contextlib.redirect_stdout(io.StringIO()):
                r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"],
                          gui_proof=str(self.proof(suite="front-door.test.sh")))

    def test_a_shipped_bundle_boot_in_a_guest_is_accepted_evidence(self):
        """`gui-proof-in-vm.sh` boots the SIGNED, NOTARIZED artifact this release will
        publish, on a clean guest with no developer environment -- fewer assertions than
        gui-boot.test.sh, and a truer artifact. It is accepted, under its OWN name: a proof
        that borrowed the suite's name would be the most useful lie in the release chain."""
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"],
                      gui_proof=str(self.proof(suite="shipped-bundle-boot")))
        self.assertEqual(r.command.call_args.args[2], "finish")

    def test_a_file_that_is_not_a_proof_is_refused_by_name(self):
        """A malformed proof is refused loudly rather than read past: this file is the one
        thing standing between an unexercised boot and a published release."""
        path = self.root / "not-a-proof"
        path.write_text("looks official enough\ncommit=deadbeefcafe0123456789\nresult=pass\n")
        r, _ = self.gui_candidate(gui_state="not-run", no_host_screen=True)
        with self.assertRaises(ValueError) as caught:
            with contextlib.redirect_stdout(io.StringIO()):
                r.perform("publish", run_id=self.CANDIDATE_INFO["run_id"], gui_proof=str(path))
        self.assertIn("richos-gui-proof 1", str(caught.exception))

    def test_the_flag_reaches_the_suite_runner_and_the_record_travels(self):
        """`--no-host-screen` has to arrive at `run-tests.sh` AND be written into the plan.

        Either half alone is a defect: a flag that does not reach the runner opens a window
        while claiming not to, and a run that does not record what it held back leaves
        `publish` unable to tell a screenless build from an old one.
        """
        # A real Runner: self.runner() mocks `gates` itself, which is the method under test.
        r = m.Runner(self.root, self.root / "state", {}, io.StringIO())
        r.state.mkdir(parents=True, exist_ok=True)
        (r.state / m.SUITE_RESULTS).write_text(json.dumps(
            {"jobs": 3, "suites": [{"name": "gui-boot.test.sh", "state": "not-run"}]}))
        r.command = Mock(return_value="")
        with contextlib.redirect_stdout(io.StringIO()):
            report = r.gates(no_host_screen=True, skip_unchanged=True)
        argvs = [" ".join(str(a) for a in call.args) for call in r.command.call_args_list]
        runner_calls = [c for c in argvs if "run-tests.sh" in c]
        self.assertEqual(len(runner_calls), 1, argvs)
        self.assertIn("--no-host-screen", runner_calls[0])
        self.assertEqual(report["suites"][0]["state"], "not-run")
        # ...and skip-when-unchanged is stated at the call site, never inherited.
        env = [c.kwargs["env_extra"] for c in r.command.call_args_list
               if "env_extra" in c.kwargs][0]
        self.assertEqual(env["RUN_TESTS_SKIP_UNCHANGED"], "1")
        # The phone apps are not this build (CEO, 2026-09-26), so the middle iPhone size is no
        # longer asked for here: it belongs to the iPhone app's release check.
        self.assertNotIn("RICHOS_NATIVE_IOS_APP_A8", env)
        self.assertIn("--for desktop", runner_calls[0])
        # The simulator suites queue for the one prepared-simulator lease for as long as this
        # gate may run, never the CLI's 300 s default that failed them side by side.
        self.assertEqual(env["RICHOS_IOS_POOL_WAIT"], str(m.GATE_BUDGETS["gates/script-suites"]))
        # ...and so does the workspace-spec mutation pass, at its own gate.
        mut = [c for c in r.command.call_args_list
               if "fourteen.txt" in " ".join(str(a) for a in c.args)]
        self.assertEqual(len(mut), 1, argvs)
        self.assertEqual(mut[0].kwargs.get("env_extra"), {"RICHOS_FOURTEEN_MUTANTS": "1",
                                                          "RICHOS_NIGHTLY_RUN_ID": m.CONDITIONS_RUN_ID})
        self.assertEqual(mut[0].kwargs.get("timeout"), m.GATE_BUDGETS[m.WORKSPACE_MUTANTS_GATE])
        # ...and, at the same gate, the other workspace suites' mutation passes, which no land
        # runs any more (hunt part 4 finding 19).
        others = [c for c in r.command.call_args_list
                  if "workspace.txt" in " ".join(str(a) for a in c.args)]
        self.assertEqual(len(others), 1, argvs)
        selection = Path(others[0].args[others[0].args.index("--commands") + 1])
        units = selection.read_text().strip().split("--only-units ", 1)[1].split(",")
        self.assertEqual(sorted(units), sorted(["mega-lander/tests/workspaces.test.sh",
                                                "mega-lander/tests/create-teammate-worktree.test.sh",
                                                "mega-lander/tests/workspace-probes.test.sh",
                                                "mega-lander/tests/app.test.sh"]))
        self.assertEqual(others[0].kwargs.get("env_extra"), {"RICHOS_MUTATION_PASSES": "1",
                                                             "RICHOS_NIGHTLY_RUN_ID": m.CONDITIONS_RUN_ID})

    def test_release_never_skips_a_suite_over_unchanged_inputs(self):
        """A proof file on this host may excuse a suite for a CANDIDATE. It may never
        excuse one for the command that makes a build installable in one motion."""
        r = self.runner()
        r.gates = Mock(return_value=None)
        r.plan = Mock(return_value=(self.root / "plan.json",
                                    {"build": True, **self.CANDIDATE_INFO}))
        (self.root / "plan.json").write_text("{}")
        r.checkout = Mock(return_value="deadbeefcafe0123456789")
        r.preflight = Mock()
        r.runtime = Mock()
        r.command = Mock()
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("release")
        self.assertIs(r.gates.call_args.kwargs["skip_unchanged"], False)


class GatesAtOnceTests(unittest.TestCase):
    """The gates run side by side when the operator says so, and only as far as is true.

    THE CEO, 2026-09-25: "why the fuck do I fucking have to wait for ONE FUCKING HOUR WHEN THE
    WHOLE FUCKING MAC IS FREE???" -- then "how do I know that you won't fuck this up next
    time?". So: independent gates overlap, a gate that reads another's output waits for it,
    one failure refuses the build and stops the rest, the phone count reaches the suites,
    and a build that does not name both numbers does not start. Each is proven red by
    nightly-local.mutation.py.
    """

    # Every gate that reads nothing another gate writes. GATE_AFTER holds the other one.
    INDEPENDENT = ("gates/release-smoke", "gates/core-tests", "gates/updater-tests",
                   "gates/script-suites", "gates/workspace-mutants", "gates/ui-suite",
                   "gates/privacy-sweep")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def runner(self, gates_at_once, simulated_phones=1, log=None):
        return m.Runner(self.root, self.root / "state",
                        {"PATH": "/usr/bin", "RICHOS_NAMED_PERSONS_FILE": "/fixture/list"},
                        log if log is not None else io.StringIO(),
                        gates_at_once=gates_at_once, simulated_phones=simulated_phones)

    def run_gates_recording(self, r, delay=0.0, barrier=None):
        """gates() with every command stood in for; returns the (event, phase, argv, env) list."""
        events, lock, firsts = [], threading.Lock(), set()

        def run(args, **kwargs):
            phase = r.active_phase
            with lock:
                events.append(("start", phase, [str(a) for a in args], kwargs.get("env")))
                first = phase not in firsts
                firsts.add(phase)
            if barrier is not None and first and phase in self.INDEPENDENT:
                barrier.wait()   # Raises BrokenBarrierError unless all seven are running at once.
            time.sleep(delay)
            with lock:
                events.append(("end", phase, [str(a) for a in args], kwargs.get("env")))
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=run), \
                contextlib.redirect_stdout(io.StringIO()):
            r.gates()
        return events

    def test_independent_gates_run_at_the_same_time(self):
        # Six commands wait for each other at one barrier: only six gates running at once
        # can pass it. One at a time, the first waits alone and the barrier breaks.
        r = self.runner("all")
        events = self.run_gates_recording(r, barrier=threading.Barrier(len(self.INDEPENDENT),
                                                                       timeout=10))
        self.assertEqual({p for e, p, _, _ in events if e == "start"},
                         set(self.INDEPENDENT) | {"gates/lint-tauri"})
        # Each gate still gets its own timing row, and the whole set one more.
        names = [name for name, _, _ in r.timings]
        self.assertEqual(sorted(names[:-1]), sorted(m.GATE_BUDGETS))
        self.assertEqual(names[-1], "gates-wall-clock")

    def test_a_gate_waits_only_for_the_gate_whose_output_it_reads(self):
        r = self.runner("all")
        events = self.run_gates_recording(r, delay=0.2)
        order = [(e, p) for e, p, _, _ in events]

        def last(event, phase):
            return max(i for i, row in enumerate(order) if row == (event, phase))

        def first(event, phase):
            return min(i for i, row in enumerate(order) if row == (event, phase))

        for gate, needs in m.GATE_AFTER.items():
            for need in needs:
                with self.subTest(gate=gate, needs=need):
                    self.assertGreater(first("start", gate), last("end", need), order)
        # And nothing ELSE waited: every independent gate started before the first one ended.
        first_end = min(i for i, (e, _) in enumerate(order) if e == "end")
        started_early = {p for i, (e, p) in enumerate(order) if e == "start" and i < first_end}
        self.assertEqual(started_early, set(self.INDEPENDENT), order)

    def test_the_number_of_gates_at_once_is_honored(self):
        for at_once in (1, 2, 3):
            with self.subTest(gates_at_once=at_once):
                r = self.runner(at_once)
                events = self.run_gates_recording(r, delay=0.05)
                live = peak = 0
                gates_live = {}
                for e, p, _, _ in events:
                    if e == "start":
                        gates_live[p] = gates_live.get(p, 0) + 1
                    else:
                        gates_live[p] -= 1
                    live = sum(1 for v in gates_live.values() if v)
                    peak = max(peak, live)
                self.assertEqual(peak, at_once, events)
                if at_once == 1:
                    # One at a time is the old order, exactly.
                    self.assertEqual([n for n, _, _ in r.timings][:-1],
                                     ["gates/release-smoke", "gates/core-tests",
                                      "gates/updater-tests", "gates/script-suites",
                                      "gates/lint-tauri", m.WORKSPACE_MUTANTS_GATE,
                                      m.UI_SUITE_GATE, "gates/privacy-sweep"])

    def test_the_desktop_build_runs_only_the_desktop_apps_suites(self):
        """CEO, 2026-09-26: "the native mobile apps are 2 COMPLETELY INDEPENDENT DIFFERENT
        APPS ... So, WHY THE FUCK ARE THEY PART OF THE SAME FUCKING BUILD???"

        The script-suites gate asks run-tests.sh for the desktop build and nothing wider, and
        asks for no phone-only case. Which suites that is, is run-tests.sh's to decide from
        phone-app-suites.tsv; run-tests.test.sh case B7 holds the real list to it.
        """
        r = self.runner("all")
        events = self.run_gates_recording(r)
        suites = [(argv, env) for e, p, argv, env in events
                  if e == "start" and any(a.endswith("run-tests.sh") for a in argv)]
        self.assertEqual(len(suites), 1)
        argv, env = suites[0]
        self.assertIn("--for", argv)
        self.assertEqual(argv[argv.index("--for") + 1], "desktop")
        self.assertNotIn("--only", argv)
        self.assertNotIn("RICHOS_NATIVE_IOS_APP_A8", env or {})

    def test_the_simulated_phone_count_reaches_the_suites_and_nothing_else_sets_it(self):
        for phones in (1, 2, 3):
            with self.subTest(simulated_phones=phones):
                r = self.runner("all", simulated_phones=phones)
                events = self.run_gates_recording(r)
                suites = [env for e, p, argv, env in events
                          if e == "start" and any(a.endswith("run-tests.sh") for a in argv)]
                self.assertEqual(len(suites), 1)
                self.assertEqual(suites[0][m.SIMULATED_PHONES_ENV], str(phones))
                # Stated at that one call site only: no other gate is handed it.
                others = [p for e, p, argv, env in events if e == "start" and env
                          and m.SIMULATED_PHONES_ENV in env
                          and not any(a.endswith("run-tests.sh") for a in argv)]
                self.assertEqual(others, [])
        # A Runner nobody configured keeps the old single simulator.
        r = m.Runner(self.root, self.root / "state", {"PATH": "/usr/bin"}, io.StringIO())
        self.assertEqual(r.simulated_phones, 1)

    def test_one_failing_gate_refuses_the_build_and_stops_the_rest(self):
        """Real processes, real owned groups: the slow gate is stopped, not waited out."""
        workers = self.root / "workers"
        env = dict(os.environ, RICHOS_MACHINE_WORKERS=str(workers))
        pid_file = self.root / "slow.pid"
        slow = ("import os, sys, time; open(sys.argv[1], 'w').write(str(os.getpid())); "
                "time.sleep(30)")
        with m.TimestampedLog(self.root / "run.log") as log:
            r = m.Runner(self.root, self.root / "state", env, log, gates_at_once="all")

            def slow_gate():
                with r.phase("gates/slow"):
                    r.command(sys.executable, "-c", slow, pid_file, cwd=self.root, timeout=120)

            def failing_gate():
                with r.phase("gates/fails"):
                    r.command(sys.executable, "-c",
                              "import time; print('the failing gate says this'); "
                              "time.sleep(1); raise SystemExit(3)",
                              cwd=self.root, timeout=120)

            def never():
                raise AssertionError("a gate after a failed one must never start")

            start = time.monotonic()
            with patch.dict(m.GATE_AFTER, {"gates/after-fails": ("gates/fails",)}), \
                    contextlib.redirect_stdout(io.StringIO()), \
                    self.assertRaisesRegex(RuntimeError, r"failed \(exit 3\)"):
                r.run_gates([("gates/slow", slow_gate), ("gates/fails", failing_gate),
                             ("gates/after-fails", never)])
            took = time.monotonic() - start
        # Bounded by the stop and the owned cleanup, never by the slow gate's own 30 s.
        self.assertLess(took, 25)
        pid = int(pid_file.read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(pid, 0)
        text = (self.root / "run.log").read_text()
        self.assertIn("FAILED gates/fails", text)
        self.assertIn("STOPPED gates/slow", text)
        self.assertIn("never started: gates/after-fails", text)
        # Each gate's own log is ONE section of the run log, in its own time stamps.
        lines = text.splitlines()
        begin = next(i for i, l in enumerate(lines) if "=== phase gates/fails begins ===" in l)
        end = next(i for i, l in enumerate(lines) if "=== phase gates/fails ends" in l)
        self.assertTrue(any("the failing gate says this" in l for l in lines[begin:end]), lines)
        self.assertFalse(any("gates/slow" in l for l in lines[begin:end]), lines)
        # And the per-gate files it copied are gone.
        self.assertEqual(list(self.root.glob("run.log.gates*")), [])

    def test_one_at_a_time_a_failure_still_stops_before_the_next_gate(self):
        r = self.runner(1)
        seen = []

        def run(args, **kwargs):
            seen.append(r.active_phase)
            if r.active_phase == "gates/updater-tests":
                return subprocess.CompletedProcess(args, 1, "", "")
            return subprocess.CompletedProcess(args, 0, "", "")

        with patch.object(m, "owned_run", side_effect=run), \
                contextlib.redirect_stdout(io.StringIO()), \
                self.assertRaisesRegex(RuntimeError, "cargo failed"):
            r.gates()
        self.assertEqual(seen, ["gates/release-smoke", "gates/core-tests", "gates/updater-tests"])

    # What a shell always has and that is no setting of this build's.
    BASE_ENVIRON = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": "/Users/fixture", "LANG": "en_US.UTF-8"}

    def main_refusal(self, *argv, environ=None):
        """Run main() in this process (so a mutation of it is what runs); (exit code, stderr).

        Anything past the argument checks is replaced by a tripwire, so a main() that failed
        to refuse can never go on to fetch, sign or run a gate from inside a test. `environ`,
        when given, is the WHOLE environment main() sees.
        """
        err = io.StringIO()
        tripwire = AssertionError("main() went past its argument checks")
        scoped = (patch.dict(os.environ, {**self.BASE_ENVIRON, **environ}, clear=True)
                  if environ is not None else contextlib.nullcontext())
        with scoped, patch.object(sys, "argv", ["nightly-local.py", *argv,
                                                "--state-dir", str(self.root / "state")]), \
                patch.object(m, "local_environment", side_effect=tripwire), \
                patch.object(m, "exclusive", side_effect=tripwire), \
                patch.object(m, "Runner", side_effect=tripwire), \
                contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()), \
                self.assertRaises(SystemExit) as stop:
            m.main()
        return stop.exception.code, err.getvalue()

    def test_a_build_that_does_not_name_the_gates_at_once_does_not_start(self):
        stable = ["--from-nightly", "v1.2.0-nightly.20260916.1"]
        for command, extra in (("build", []), ("release", []), ("stable", stable)):
            for given in ([], ["--simulated-phones", "2"]):
                with self.subTest(command=command, given=given):
                    code, err = self.main_refusal(command, *extra, *given)
                    self.assertEqual(code, 2)
                    self.assertIn("refuses to start without --gates-at-once", err)
                    # Refused before anything: no state, no lock, no log.
                    self.assertFalse((self.root / "state").exists())
        # A number that is not one is refused, never read as a default.
        for flag, bad in (("--gates-at-once", "0"), ("--gates-at-once", "every"),
                          ("--simulated-phones", "0"), ("--simulated-phones", "all")):
            with self.subTest(flag=flag, value=bad):
                other = "--simulated-phones" if flag == "--gates-at-once" else "--gates-at-once"
                code, err = self.main_refusal("build", flag, bad, other, "1")
                self.assertEqual(code, 2)
                self.assertIn(flag, err)
        # And a command that runs no gate is not handed them.
        code, err = self.main_refusal("check", "--gates-at-once", "all")
        self.assertEqual(code, 2)
        self.assertIn("mean nothing to check", err)

    def test_a_desktop_build_is_not_made_to_name_a_simulated_phone_count(self):
        """R48 (hunt part 2 v3): no gate of the desktop build leases a simulated iPhone, so a build,
        release or stable that names only --gates-at-once starts; the log says no count was named
        and the script suites are told one at a time, literally."""
        for command in ("build", "release"):
            with self.subTest(command=command):
                kwargs, _ = self.main_with(command, "--gates-at-once", "2")
                self.assertIsNone(kwargs["simulated_phones"])
        kwargs, _ = self.main_with("build", *self.NUMBERS)
        self.assertEqual(kwargs["simulated_phones"], 1)
        log = io.StringIO()
        r = m.Runner(self.root, self.root / "state", {}, log, gates_at_once="all",
                     simulated_phones=None, chosen_by="fixture-user on the command line")
        r.record_settings()
        self.assertIn("Simulated phones at once: not named", log.getvalue())
        events = self.run_gates_recording(self.runner("all", simulated_phones=None))
        suites = [env for e, p, argv, env in events
                  if e == "start" and any(a.endswith("run-tests.sh") for a in argv)]
        self.assertEqual([env[m.SIMULATED_PHONES_ENV] for env in suites], ["1"])

    def test_the_chosen_numbers_and_who_chose_them_are_the_logs_first_lines(self):
        log = io.StringIO()
        r = m.Runner(self.root, self.root / "state", {}, log, gates_at_once="all",
                     simulated_phones=2, chosen_by="fixture-user on the command line")
        r.checkout = Mock(return_value="source-sha")
        r.plan = Mock(return_value=(self.root / "plan.json",
                                    {"build": False, "reason": "already published"}))
        with contextlib.redirect_stdout(io.StringIO()):
            r.perform("release")
        first, second = log.getvalue().splitlines()[:2]
        self.assertEqual(first, "Gates at once: all (--gates-at-once all, chosen by "
                                "fixture-user on the command line)")
        self.assertEqual(second, "Simulated phones at once: 2 (--simulated-phones 2, chosen by "
                                 "fixture-user on the command line)")
        # main() names the account and the command line, not a placeholder.
        self.assertIn(" on the command line", m.chosen_by())

    # ---- 2026-09-29: a build on a quiet Mac, the first time ---------------------------------
    # Attempt 1 (20260929T100554Z-3155ee52) ran gui-boot on this Mac's asleep display; attempt
    # 2 (20260929T101720Z-bee22332) was given RICHOS_GUI_HOST=richos-test-1, which the gates'
    # allowlisted environment dropped without a word.

    def main_with(self, *argv, environ=None):
        """Run main() under exactly BASE_ENVIRON + `environ`, up to the Runner and no further:
        (the Runner's keyword arguments, perform()'s positional arguments)."""
        runner = Mock()
        state = self.root / "state"
        state.mkdir(exist_ok=True)
        with patch.dict(os.environ, {**self.BASE_ENVIRON, **(environ or {})}, clear=True), \
                patch.object(sys, "argv", ["nightly-local.py", *argv, "--state-dir", str(state)]), \
                patch.object(m.platform, "system", return_value="Darwin"), \
                patch.object(m.platform, "machine", return_value="arm64"), \
                patch.object(m, "local_environment", return_value=({"RICHOS_NIGHTLY_RUN_ID": "run-x"}, {})), \
                patch.object(m, "exclusive", return_value=contextlib.nullcontext()), \
                patch.object(m, "TimestampedLog", return_value=contextlib.nullcontext(io.StringIO())), \
                patch.object(m, "Runner", runner), contextlib.redirect_stdout(io.StringIO()):
            m.main()
        runner.return_value.perform.assert_called_once()
        return runner.call_args.kwargs, runner.return_value.perform.call_args.args

    NUMBERS = ("--gates-at-once", "2", "--simulated-phones", "1")
    NO_HOST_SCREEN_ARG = 5  # perform(command, force, runtime, run, checks_done_at_land, no_host_screen, ...)

    def test_a_build_never_uses_this_macs_screen_unless_told_to(self):
        _, args = self.main_with("build", *self.NUMBERS)
        self.assertIs(args[self.NO_HOST_SCREEN_ARG], True)
        _, args = self.main_with("build", *self.NUMBERS, "--host-screen")
        self.assertIs(args[self.NO_HOST_SCREEN_ARG], False)
        # A build reusing a recorded run's gates runs no screen suite, so none is held back.
        _, args = self.main_with("build", "--gates-passed-in", "20260929T101720Z-bee22332")
        self.assertIs(args[self.NO_HOST_SCREEN_ARG], False)
        code, err = self.main_refusal("release", *self.NUMBERS, "--host-screen")
        self.assertEqual(code, 2)
        self.assertIn("--host-screen is for `build` alone", err)

    def test_the_guest_and_vm_settings_the_shell_gives_reach_the_vm_boot_proof(self):
        kwargs, _ = self.main_with("build", *self.NUMBERS, environ={
            "RICHOS_GUI_HOST": "richos-test-1", "TESTVM_ROOT": "/Volumes/E1TB/testvm",
            # Not settings of this build's, and never refused: the agent harness's identity,
            # and a credential read from the shell on purpose for the signing steps.
            "RICHOS_AGENT_OWNER": "zach", "RICHOS_NOTARY_PROFILE": "fixture-profile"})
        self.assertEqual(kwargs["gui_host"], "richos-test-1")
        self.assertEqual(kwargs["vm_settings"], {"TESTVM_ROOT": "/Volumes/E1TB/testvm"})

    def stand_in_gui_proof(self, result, exit_code, write_proof=True):
        """A gui-proof-in-vm.sh that writes the proof file it was given `--out` and exits."""
        script = self.root / "state" / "source" / "richos" / "app" / "scripts" / "gui-proof-in-vm.sh"
        script.parent.mkdir(parents=True, exist_ok=True)
        body = ""
        if write_proof:
            body = ('while [ $# -gt 0 ]; do [ "$1" = --out ] && OUT="$2"; shift; done\n'
                    'mkdir -p "$(dirname "$OUT")"\n'
                    'printf "richos-gui-proof 1\\nsuite=shipped-bundle-boot\\nwhere=vm:richos-test-7\\n'
                    f'result={result}\\nwindows=1\\n" > "$OUT"\n')
        script.write_text(f"#!/bin/bash\n{body}exit {exit_code}\n")
        r = m.Runner(self.root, self.root / "state", {"PATH": "/usr/bin:/bin"}, io.StringIO())

        def run_it(*args, **_):
            # Runner.command's contract without the worker machinery: a nonzero exit raises.
            code = subprocess.run([str(a) for a in args], stdin=subprocess.DEVNULL).returncode
            if code:
                raise RuntimeError(f"{Path(str(args[0])).name} failed (exit {code}); see the run log")
        r.command = run_it
        return r

    def boot_proof(self, runner):
        with contextlib.redirect_stdout(io.StringIO()):
            return runner.vm_boot_proof("run-1")

    def test_a_passing_proof_over_a_failed_guest_stop_says_the_boot_was_proven(self):
        """Since a4bbd0ba gui-proof-in-vm.sh exits 1 when the proof passed but the guest would not
        stop. The boot was proven; the leftover guest is what is wrong, and the sentence names it."""
        error = self.boot_proof(self.stand_in_gui_proof("pass", 1))
        self.assertIn("ITS BOOT WAS PROVEN", error)
        self.assertNotIn("WAS NOT PROVEN", error)
        self.assertIn("COULD NOT BE STOPPED", error)
        self.assertIn("richos-test-7", error)
        self.assertIn("testvm/stop.sh richos-test-7", error)
        self.assertIn("still accepts the proof", error)

    def test_a_failed_or_missing_proof_is_still_not_proven(self):
        for result, code, write in (("fail:no-window", 1, True), ("pass", 2, False)):
            with self.subTest(result=result, written=write):
                error = self.boot_proof(self.stand_in_gui_proof(result, code, write_proof=write))
                self.assertIn("ITS BOOT WAS NOT PROVEN", error)
        self.assertIsNone(self.boot_proof(self.stand_in_gui_proof("pass", 0)))

    def test_a_passing_proof_left_by_an_earlier_attempt_does_not_prove_this_one(self):
        runner = self.stand_in_gui_proof("pass", 1, write_proof=False)
        old = runner.gui_proof_path("run-1")
        old.parent.mkdir(parents=True, exist_ok=True)
        old.write_text("richos-gui-proof 1\nwhere=vm:old\nresult=pass\n")
        os.utime(old, (1_000_000_000, 1_000_000_000))
        self.assertIn("ITS BOOT WAS NOT PROVEN", self.boot_proof(runner))

    def test_a_setting_no_step_would_receive_refuses_before_anything_starts(self):
        for command, extra, name in (("build", self.NUMBERS, "RICHOS_MUTANT_JOBS"),
                                     ("build", self.NUMBERS, "RUN_TESTS_NO_HOST_SCREEN"),
                                     # release runs every gate on this Mac; a guest name
                                     # would be ignored, so it is refused, not dropped.
                                     ("release", self.NUMBERS, "RICHOS_GUI_HOST")):
            with self.subTest(command=command, name=name):
                code, err = self.main_refusal(command, *extra, environ={name: "1"})
                self.assertEqual(code, 2)
                self.assertIn(f"your shell sets {name}, and no step of `{command}` would receive it", err)
                self.assertIn("Nothing has started", err)
                self.assertFalse((self.root / "state").exists())


class WalkRecipeTests(unittest.TestCase):
    """The printed walk recipe starts the candidate on ONE scratch home (2026-09-24).

    Under HOME alone, Foundation's NSHomeDirectory() still named the real home, so a
    walked candidate put WebKit's store and the URL cache in the real ~/Library, where the
    daily driver (same bundle identifier) keeps them. These run the recipe's own lines with
    lib/home-probe.sh standing in for the app: no build, no window."""

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()

    def test_the_recipe_names_one_canonical_noindex_home_and_a_clean_environment(self):
        # The un-resolved temporary folder is the /var symlink the product refuses as a home.
        scratch, lines = m.walk_recipe(Path("/x/RichOS.zip"), "run1", temp_root=tempfile.gettempdir())
        home = scratch / "home.noindex"
        self.assertEqual(scratch, scratch.resolve())
        self.assertTrue(scratch.name.endswith(".noindex"))
        text = "\n".join(lines)
        self.assertIn(f"HOME={shlex.quote(str(home))} CFFIXED_USER_HOME={shlex.quote(str(home))}", text)
        self.assertIn("/usr/bin/env -i ", text)
        self.assertIn("PATH=/usr/bin:/bin:/usr/sbin:/sbin", text)

    @unittest.skipUnless(sys.platform == "darwin", "Foundation's NSHomeDirectory()")
    def test_the_recipe_as_printed_starts_the_app_on_its_scratch_home(self):
        app = self.root / "src" / "RichOS.app" / "Contents" / "MacOS"
        app.mkdir(parents=True)
        probe = app / "richos-tauri"
        probe.write_bytes((Path(__file__).parent / "lib" / "home-probe.sh").read_bytes())
        probe.chmod(0o755)
        bundle = self.root / "RichOS-probe.zip"
        subprocess.run(["ditto", "-c", "-k", "--keepParent", str(self.root / "src" / "RichOS.app"), str(bundle)],
                       check=True)
        scratch, lines = m.walk_recipe(bundle, "probe", temp_root=self.root)
        home = scratch / "home.noindex"
        env = {"USER": os.environ.get("USER", "probe"), "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
               "HOME": os.environ.get("HOME", "/"), "WALK_RECIPE_CANARY": "from-the-shell"}
        result = subprocess.run(["bash", "-c", "\n".join(lines)], env=env, capture_output=True, text=True,
                                timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("[richos] boot complete", result.stdout)
        seen = dict(line.split("=", 1) for line in (home / ".home-probe" / "env").read_text().splitlines()
                    if "=" in line)
        for bash_own in ("PWD", "SHLVL", "_"):
            seen.pop(bash_own, None)
        self.assertEqual(sorted(seen), ["CFFIXED_USER_HOME", "HOME", "PATH", "RICHOS_ACTIVATION", "TMPDIR", "USER"])
        self.assertEqual((seen["HOME"], seen["CFFIXED_USER_HOME"]), (str(home), str(home)))
        self.assertEqual(seen["TMPDIR"], str(home / "tmp") + "/")
        nshome = (home / ".home-probe" / "nshome").read_text().strip()
        self.assertEqual(Path(nshome).resolve(), home)



class GateReconciliationTests(unittest.TestCase):
    setUp = LocalTests.setUp
    def store(self, run="20261007T170000Z-11111111", identity="same"):
        library = str(Path(m.__file__).parent / "lib")
        if library not in sys.path:
            sys.path.insert(0, library)
        from nightly_gate_evidence import GateEvidence
        return GateEvidence(self.root / "records", run, lambda: identity, lambda _: None)

    def test_pass_survives_another_gates_failure_and_names_its_actual_run(self):
        first = self.store()
        first.record("gates/core-tests", "same", "passed", "525 passed")
        first.record("gates/updater-tests", "same", "failed", "55 passed; 1 failed")
        retry = self.store("20261007T170100Z-22222222")
        self.assertIsNotNone(retry.reusable("gates/core-tests", "same"))
        self.assertIsNone(retry.reusable("gates/updater-tests", "same"))
        retry.reuse("gates/core-tests", "same")
        self.assertEqual(retry.selected["gates/core-tests"]["status"], "reused")
        self.assertEqual(retry.selected["gates/core-tests"]["run_id"], first.run_id)
        self.assertEqual(len(list((self.root / "records").glob("*/*.json"))), 2)

    def test_changed_inputs_never_reuse_a_pass(self):
        store = self.store()
        store.record("gates/core-tests", "same", "passed", "green")
        self.assertIsNone(store.reusable("gates/core-tests", "different"))

    def test_a_new_failure_does_not_fall_back_to_older_green(self):
        first = self.store()
        first.record("gates/core-tests", "same", "passed", "green")
        second = self.store("20261007T170100Z-22222222")
        second.record("gates/core-tests", "same", "failed", "red")
        self.assertIsNone(second.reusable("gates/core-tests", "same"))
        self.assertTrue(second.unresolved("gates/core-tests", "same"))

    def test_damaged_record_log_or_suite_evidence_cannot_certify_a_pass(self):
        for suffix in (".log", ".json", ".sha256"):
            with self.subTest(suffix=suffix):
                store = self.store("20261007T170100Z-" + uuid.uuid4().hex[:8])
                store.record("gates/core-tests", "same", "passed", "green")
                path = Path(store.selected["gates/core-tests"]["record"])
                path.with_suffix(suffix).write_text("damaged")
                self.assertIsNone(store.reusable("gates/core-tests", "same"))

    def test_a_record_rotated_during_discovery_is_a_cache_miss(self):
        store = self.store()
        store.record("gates/core-tests", "same", "passed", "green")
        self.assertIsNotNone(store.reusable("gates/core-tests", "same"))
        path = Path(store.selected["gates/core-tests"]["record"])
        def rotate(_):
            path.unlink()
            return [path]
        with patch.object(Path, "glob", side_effect=rotate):
            self.assertIsNone(store.reusable("gates/core-tests", "same"))

    def test_cancelled_skipped_and_input_changed_gates_are_never_passes(self):
        for state in ("stopped", "skipped", "inputs-changed", "unqualified"):
            with self.subTest(state=state):
                store = self.store("20261007T170100Z-" + uuid.uuid4().hex[:8])
                store.record("gates/core-tests", "same", state, state)
                self.assertIsNone(store.reusable("gates/core-tests", "same"))

    def test_successful_body_with_changing_inputs_is_recorded_without_reuse(self):
        store = self.store(identity="after")
        with self.assertRaisesRegex(RuntimeError, "inputs-changed"):
            store.wrap("gates/core-tests", lambda: None, "before", lambda: "green", lambda: None)()
        self.assertEqual(store.selected["gates/core-tests"]["status"], "inputs-changed")
        self.assertIsNone(store.reusable("gates/core-tests", "before"))

    def test_exception_preserves_failure_evidence_and_propagates(self):
        store = self.store()
        def red():
            raise RuntimeError("real failure")
        with self.assertRaisesRegex(RuntimeError, "real failure"):
            store.wrap("gates/core-tests", red, "same", lambda: "red", lambda: None)()
        self.assertEqual(store.selected["gates/core-tests"]["status"], "failed")

    def test_deadline_refusal_and_cancellation_keep_their_actual_states(self):
        for error, state, stopped in ((m.GateTimedOut("deadline"), "timed-out", False),
                                      (m.GateNotAdmitted("queue"), "refused", False),
                                      (RuntimeError("child stopped"), "stopped", True)):
            with self.subTest(state=state):
                store = self.store("20261007T170100Z-" + uuid.uuid4().hex[:8])
                def fail(error=error):
                    raise error
                with self.assertRaises(RuntimeError):
                    store.wrap("gates/core-tests", fail, "same", lambda: str(error),
                               lambda: None, stopping=lambda: stopped)()
                self.assertEqual(store.selected["gates/core-tests"]["status"], state)
                self.assertIsNone(store.reusable("gates/core-tests", "same"))

    def test_identical_run_cannot_overwrite_completed_evidence(self):
        store = self.store()
        store.record("gates/core-tests", "same", "failed", "red")
        with self.assertRaisesRegex(ValueError, "already exists"):
            store.record("gates/core-tests", "same", "passed", "green")
        self.assertIsNone(store.reusable("gates/core-tests", "same"))

    def test_real_identity_binds_dirty_source_untracked_source_external_files_and_settings(self):
        library = str(Path(m.__file__).parent / "lib")
        if library not in sys.path:
            sys.path.insert(0, library)
        import proof_evidence
        source = self.root / "state/source"
        source.mkdir(parents=True)
        config = self.root / "fixture.gitconfig"
        config.write_text('[user]\nname = Verification Fixture\nemail = fixture@example.invalid\n')
        fixture_env = {**os.environ, "HOME": str(self.root), "GIT_CONFIG_GLOBAL": str(config),
                       "GIT_CONFIG_NOSYSTEM": "1"}
        subprocess.run(["git", "init", "-q", str(source)], env=fixture_env, check=True)
        text = source / "input.txt"
        text.write_text("original")
        subprocess.run(["git", "-C", str(source), "add", "."], env=fixture_env, check=True)
        subprocess.run(["git", "-C", str(source), "commit", "-qm", "fixture"],
                       env=fixture_env, check=True)
        private = self.root / "private-list"
        private.write_text("fixture roster")
        models = [self.root / "model-a", self.root / "model-b"]
        for model in models:
            model.write_text("model")
        env = {"PATH": os.environ["PATH"], "HOME": str(self.root / "home"),
               "TMPDIR": str(self.root), "RICHOS_NAMED_PERSONS_FILE": str(private),
               "RICHOS_GUI_SPEECH_MODELS": os.pathsep.join(map(str, models)),
               "RICHOS_NIGHTLY_RUN_ID": "20261007T170000Z-11111111"}
        r = m.Runner(source, self.root / "state", env, io.StringIO(), gates_at_once="all")
        with patch.object(proof_evidence, "installed_dependencies", return_value={"fixture": "dependencies"}):
            initial = r.gate_input_identity(None, True)
            env["RICHOS_NIGHTLY_RUN_ID"] = "20261007T170100Z-22222222"
            self.assertEqual(r.gate_input_identity(None, True), initial)
            text.write_text("dirty")
            self.assertNotEqual(r.gate_input_identity(None, True), initial)
            text.write_text("original")
            self.assertEqual(r.gate_input_identity(None, True), initial)
            extra = source / "untracked.txt"
            extra.write_text("fixture")
            self.assertNotEqual(r.gate_input_identity(None, True), initial)
            extra.unlink()
            private.write_text("another roster")
            self.assertNotEqual(r.gate_input_identity(None, True), initial)
            private.write_text("fixture roster")
            models[1].write_text("changed model")
            self.assertNotEqual(r.gate_input_identity(None, True), initial)
            models[1].write_text("model")
            self.assertNotEqual(r.gate_input_identity(None, False), initial)
            r.gates_at_once = 1
            self.assertNotEqual(r.gate_input_identity(None, True), initial)
            r.gates_at_once = "all"
            with patch.object(proof_evidence, "installed_dependencies", return_value={"fixture": "changed"}):
                self.assertNotEqual(r.gate_input_identity(None, True), initial)

    def test_release_keeps_every_gate_fresh(self):
        r = self.retry_runner("20261007T170100Z-22222222")
        r.reconcile_gates = Mock(side_effect=AssertionError("release reused evidence"))
        r.run_gates = Mock()
        with contextlib.redirect_stdout(io.StringIO()):
            r.gates(skip_unchanged=False)
        self.assertEqual(tuple(name for name, _ in r.run_gates.call_args.args[0]), m.GATE_NAMES)
        r.reconcile_gates.assert_not_called()

    def retry_runner(self, run):
        r = m.Runner(self.root, self.root / "state", {"RICHOS_NIGHTLY_RUN_ID": run},
                     io.StringIO(), gates_at_once=1)
        r.gate_input_identity = Mock(return_value={"source_commit": "a" * 40, "inputs_sha256": "b" * 64})
        return r

    def test_runner_retries_only_unresolved_gates_and_keeps_prerequisites(self):
        seen = []
        first = self.retry_runner("20261007T170000Z-11111111")
        gates = [("gates/core-tests", lambda: seen.append("core")),
                 ("gates/updater-tests", lambda: (_ for _ in ()).throw(RuntimeError("red")))]
        with contextlib.redirect_stdout(io.StringIO()), self.assertRaisesRegex(RuntimeError, "red"):
            first.run_gates(first.reconcile_gates(gates, None, False))
        second = self.retry_runner("20261007T170100Z-22222222")
        gates = [("gates/core-tests", lambda: seen.append("core-again")),
                 ("gates/updater-tests", lambda: seen.append("updater-retry")),
                 ("gates/release-smoke", lambda: seen.append("smoke"))]
        with contextlib.redirect_stdout(io.StringIO()):
            second.run_gates(second.reconcile_gates(gates, None, False))
        self.assertEqual(seen, ["core", "updater-retry", "smoke"])
        self.assertEqual(second.gate_evidence.selected["gates/core-tests"]["status"], "reused")

    def test_a_reused_script_gate_restores_its_frozen_report(self):
        first = self.retry_runner("20261007T170000Z-11111111")
        report = {"commit": "a" * 40, "run_id": first.env["RICHOS_NIGHTLY_RUN_ID"], "suites": []}
        first.state.mkdir(parents=True)
        def scripts():
            (first.state / m.SUITE_RESULTS).write_text(json.dumps(report))
        with contextlib.redirect_stdout(io.StringIO()):
            first.run_gates(first.reconcile_gates([("gates/script-suites", scripts)], None, True))
        (first.state / m.SUITE_RESULTS).write_text('{"another": "run"}')
        second = self.retry_runner("20261007T170100Z-22222222")
        with contextlib.redirect_stdout(io.StringIO()):
            second.run_gates(second.reconcile_gates([
                ("gates/script-suites", lambda: self.fail("script gate repeated")),
                ("gates/lint-tauri", lambda: None)], None, True))
        self.assertEqual(json.loads((first.state / m.SUITE_RESULTS).read_text()), report)
        self.assertEqual(second.gate_evidence.selected["gates/script-suites"]["run_id"], report["run_id"])

    def test_ui_reuse_reconciles_frozen_receipts_instead_of_running_suites(self):
        first = self.retry_runner("20261007T170000Z-11111111")
        receipts = first.state / "ui-receipts"
        receipts.mkdir(parents=True)
        record = {"commit": "a" * 40, "suite": "fixture.js", "exit": 0, "records": []}
        def ui():
            (receipts / "fixture.receipt.json").write_text(json.dumps(record))
        with contextlib.redirect_stdout(io.StringIO()):
            first.run_gates(first.reconcile_gates([(m.UI_SUITE_GATE, ui)], None, True))
        second = self.retry_runner("20261007T170100Z-22222222")
        second.command = Mock()
        with contextlib.redirect_stdout(io.StringIO()):
            pending = second.reconcile_gates([(m.UI_SUITE_GATE, lambda: self.fail("UI repeated"))], None, True)
        self.assertEqual(pending, [])
        args = second.command.call_args.args
        self.assertEqual(args[:2], ("node", "run.js"))
        self.assertTrue(args[2].startswith("--coverage="))
        snapshot = Path(args[2].split("=", 1)[1])
        self.assertEqual(json.loads((snapshot / "fixture.receipt.json").read_text()), record)
        self.assertEqual(second.gate_evidence.selected[m.UI_SUITE_GATE]["status"], "reused")
        third = self.retry_runner("20261007T170200Z-33333333")
        third.command = Mock(side_effect=RuntimeError("missing coverage"))
        with contextlib.redirect_stdout(io.StringIO()):
            pending = third.reconcile_gates([(m.UI_SUITE_GATE, ui)], None, True)
        self.assertEqual([name for name, _ in pending], [m.UI_SUITE_GATE])
        self.assertNotIn(m.UI_SUITE_GATE, third.reused_gates)

    def test_sequential_retry_still_runs_the_prerequisite_before_its_failed_dependent(self):
        r = self.retry_runner("20261007T170000Z-11111111")
        seen = []
        r.run_gates([("gates/lint-tauri", lambda: seen.append("lint")),
                     ("gates/script-suites", lambda: seen.append("scripts"))])
        self.assertEqual(seen, ["scripts", "lint"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
