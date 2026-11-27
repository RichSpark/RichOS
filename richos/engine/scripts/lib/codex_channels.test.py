#!/usr/bin/env python3
"""Channel persistence/isolation checks on owned fixture paths only."""
import concurrent.futures
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import selectors
import shlex
import sys
import tempfile
import unittest
from unittest import mock
import codex_watch as watcher

spec = importlib.util.spec_from_file_location("codex_channels", Path(__file__).with_name("codex_channels.py"))
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)


class Channels(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="codex-channels-")
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.old = os.environ.get("CODEX_COORDINATION_ROOT")
        os.environ["CODEX_COORDINATION_ROOT"] = str(self.root / "coord with space")
        self.addCleanup(self.restore)
        codex_home = mock.patch.dict(os.environ, {"CODEX_HOME": str(self.root / "codex-home")})
        codex_home.start()
        self.addCleanup(codex_home.stop)
        self.a = dict(version=1, state_root=str(self.root / "state"), entity_id="company-a", thread_id="chat-a")
        self.b = dict(self.a, entity_id="company-b", thread_id="chat-b")

    def restore(self):
        if self.old is None:
            os.environ.pop("CODEX_COORDINATION_ROOT", None)
        else:
            os.environ["CODEX_COORDINATION_ROOT"] = self.old

    def connect(self, scope=None, locator=None):
        return c.connect("Exact user task", "fixture transcript", locator, scope)["locator"]

    def bind_queued_pair(self, locator, scope):
        path = Path(locator) / "connection.json"
        c.save(path, dict(c.load(path), binding={key: scope[key] for key in ("entity_id", "thread_id")}))

    def move_pair(self, locator, entity):
        old = Path(locator)
        _, endpoint = c.connection(old)
        new = c.endpoint(entity, endpoint["thread_id"], old.name)
        c.mkdir(new.parent)
        old.rename(new)
        c.save(new / "connection.json", dict(endpoint, entity_id=entity, previous_entity_id=endpoint["entity_id"]))
        c.save(c.root() / "legacy-company-move.json",
               {"version": 1, "from": endpoint["entity_id"], "to": entity,
                "thread_id": endpoint["thread_id"], "complete": True})
        return new

    def test_independent_work_never_inherits_the_old_automatic_company(self):
        c.save(c.root() / "automatic.json", self.a)
        with contextlib.chdir(self.root):
            path = Path(self.connect())
        self.assertEqual(path.parent.parent.name, "_automatic")
        record = c.load(path / "connection.json")
        self.assertEqual(record["project_dir"], str(self.root.resolve()))
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))

    def test_cutover_never_associates_with_the_old_automatic_company(self):
        folder = c.root() / "rich-codex"
        folder.mkdir(parents=True)
        (folder / "to-rich.md").write_text("Legacy task\n")
        c.save(c.root() / "automatic.json", self.a)
        with mock.patch.object(watcher, "channels", return_value=[str(folder / "to-rich.md")]):
            watcher.prepare_app_connections(self.a["state_root"])
        self.assertIsNone(c.legacy_owner(folder))

    def test_old_locator_reopens_the_moved_pair_in_its_registered_scope(self):
        old = Path(self.connect(self.a))
        self.move_pair(old, self.b["entity_id"])
        owner = dict(self.b, thread_id=self.a["thread_id"])
        c.send(old, "new-reply", "Through the original locator", scope=owner)
        self.assertEqual(c.read(old)["records"][-1]["message_id"], "new-reply")
        with self.assertRaisesRegex(ValueError, "belongs to another"):
            c.read(old, scope=self.a)

    def test_pairing_default_and_three_independent_reply_paths(self):
        c.save(c.root() / "automatic.json", self.a)
        x, y = self.connect(), self.connect()
        self.bind_queued_pair(x, self.a)
        self.bind_queued_pair(y, self.a)
        z = self.connect(self.b)
        for p in (x, y, z):
            c.send(p, "first", "independent input")
        for p, scope in ((x, self.a), (y, self.a), (z, self.b)):
            c.send(p, "reply", p, "first", scope)
            self.assertEqual(c.read(p)["records"][-1]["text"], p)
        with self.assertRaises(ValueError):
            c.send(z, "wrong", "bad", scope=self.a)
        c.designate(self.b, [])
        self.assertEqual(c.connection(x)[1]["thread_id"], "chat-a")
        sibling = self.connect(locator=x)
        self.assertNotEqual(sibling, x)
        self.assertEqual(c.connection(sibling)[1]["thread_id"], "chat-a")

    def test_fresh_connection_saves_before_app_has_a_company(self):
        first = self.connect()
        c.send(first, "hello", "First automatic message")
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))
        self.assertEqual(list(c.automatic_requests()), [dict(type="automatic", locator=first)])
        self.bind_queued_pair(first, self.a)
        second = self.connect()
        self.bind_queued_pair(second, self.b)
        self.assertNotEqual(first, second)
        self.assertEqual(c.connection(first)[1]["thread_id"], "chat-a")
        self.assertEqual(c.connection(second)[1]["thread_id"], "chat-b")
        events, errors = c.scan(self.a["state_root"])
        self.assertFalse(errors)
        self.assertEqual({event["locator"]: event["connection"]["thread_id"] for event in events},
                         {first: "chat-a", second: "chat-b"})
        c.send(first, "ack", "First only", scope=self.a)
        self.assertEqual(c.read(second)["records"], [])

    def test_terminal_replies_only_reach_the_owning_session_and_pair(self):
        first = c.connect("First task", "User transcript", terminal_session="rich-session-a")["locator"]
        second = c.connect("Second task", "User transcript", terminal_session="rich-session-b")["locator"]
        c.send(first, "reply-a", "Result A")
        c.send(second, "reply-b", "Result B")
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))
        self.assertEqual(c.read(first)["records"][0]["text"], "First task")
        with mock.patch.dict(os.environ, {"CODEX_WATCH_STATE_DIR": str(self.root / "watch"),
                                         "CODEX_WATCH_CHANNELS": str(self.root / "old" / "to-rich.md")}):
            output = io.StringIO()
            state = watcher.session_dir("rich-session-a")
            watcher.tick(state, out=output, sid="rich-session-a")
            self.assertIn("Result A", output.getvalue())
            self.assertNotIn("Result B", output.getvalue())
            self.assertIn(first, output.getvalue())
            self.assertEqual(watcher.tick(state, out=io.StringIO(), sid="rich-session-a"), 0)

    def quota_fixture(self, used=100, resets=1, fail=False, fail_once=False, uncertain=False,
                      reset_details=None, midrun=None, concurrent_runs=1, preflight_failed=False):
        binary = self.root / "codex"
        ledger = self.root / "cli-events"
        binary.write_text(f"#!{sys.executable}\n" + r"""
import json, sys, time
from pathlib import Path
from threading import Event
ledger = Path(sys.argv[0]).with_name("cli-events")
def record(value):
    with ledger.open("a") as out:
        out.write(json.dumps(value) + "\n")
def calls():
    return [json.loads(line) for line in ledger.read_text().splitlines()] if ledger.exists() else []
""" + f"""
used, resets, fail, fail_once, uncertain = {used}, {resets}, {fail}, {fail_once}, {uncertain}
details, midrun, concurrent_runs = {reset_details!r}, {midrun!r}, {concurrent_runs}
preflight_failed = {preflight_failed}
""" + """
if sys.argv[1] == "app-server":
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request:
            continue
        method = request["method"]
        result = {}
        if method == "account/read":
            result = {"account": {"type": "chatgpt"}, "userId": "fixture-user"}
        if method == "config/read":
            result = {"config": {"model_provider": "openai"}}
        if method == "account/rateLimits/read":
            events = calls()
            if preflight_failed and not events:
                sys.exit(1)
            redeemed = any(x[0] == "reset" for x in events)
            runs = sum(x[0] == "exec" for x in events)
            exhausted = (fail_once and runs > 0) or (midrun and runs >= concurrent_runs)
            percent = 0 if redeemed else 100 if exhausted else used
            credit_rows = [] if uncertain and any(x[0] == "unconfirmed" for x in events) else details
            result = {
                "accountId": "fixture-account", "ordinaryUsageAllowed": percent < 100,
                "rateLimits": {"limitId": "codex", "primary": {"usedPercent": percent,
                    "windowDurationMins": 10080, "resetsAt": 4200000000},
                    "credits": {"hasCredits": True, "unlimited": False, "balance": "12.345"}},
                "rateLimitResetCredits": {"availableCount": resets - int(redeemed), "credits": credit_rows},
            }
        if method == "account/rateLimitResetCredit/consume":
            key = request["params"]["idempotencyKey"]
            if uncertain and not any(x[0] == "unconfirmed" for x in calls()):
                record(["unconfirmed", key, request["params"].get("creditId")])
                sys.exit(1)
            record(["reset", key, request["params"].get("creditId")])
            result = {"outcome": "reset"}
        print(json.dumps({"id": request["id"], "result": result}), flush=True)
else:
    previous_exec = any(x[0] == "exec" for x in calls())
    record(["exec", sys.argv[1:], sys.stdin.read()])
    print(json.dumps({"type": "thread.started", "thread_id": "quota-session"}), flush=True)
    if midrun == "event":
        print(json.dumps({"method": "account/rateLimits/updated", "params": {"rateLimits": {
            "limitId": "codex", "primary": {"usedPercent": 100, "resetsAt": 4200000000}}}}), flush=True)
    if midrun:
        # Simulate a successful request that uses credits if no reset arrives.
        # The deadline only bounds the fixture when the dispatcher never redeems.
        deadline = time.monotonic() + 4
        ready = Event()
        while not any(row[0] == "reset" for row in calls()):
            if time.monotonic() >= deadline:
                record(["credits"])
                break
            ready.wait(0.01)
    if fail or fail_once and not previous_exec:
        print(json.dumps({"type": "turn.failed", "error": {"message": "You’ve hit your usage limit."}}))
        sys.exit(1)
""")
        binary.chmod(0o700)
        patch = mock.patch.object(c, "codex_binary", return_value=str(binary))
        patch.start()
        self.addCleanup(patch.stop)
        return lambda: [json.loads(line) for line in ledger.read_text().splitlines()]

    def test_codex_read_only_quota_reports_windows_resets_and_credit_balance(self):
        self.quota_fixture()
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            self.assertEqual(c.main(["quota"]), 0)
        reading = json.loads(output.getvalue())
        self.assertEqual(reading["limits"][0]["windows"][0]["used_percent"], 100)
        self.assertEqual(reading["available_resets"], 1)
        self.assertEqual(reading["limits"][0]["credits"]["balance"], "12.345")
        self.assertNotIn("fixture-account", output.getvalue())
        self.assertFalse((self.root / "cli-events").exists())

    def test_exhausted_codex_redeems_one_reset_before_running(self):
        calls = self.quota_fixture()
        first = self.connect(self.a)
        second = c.connect("Independent", "Fixture", terminal_session="rich-terminal")["locator"]
        with concurrent.futures.ThreadPoolExecutor(2) as pool:
            self.assertEqual(list(pool.map(c.dispatch, (first, second))), [0, 0])
        self.assertEqual([row[0] for row in calls()], ["reset", "exec", "exec"])
        messages = [row["text"] for pair in (first, second) for row in c.read(pair, "to-rich")["records"]]
        self.assertEqual(sum("used a banked reset" in text for text in messages), 1)

    def test_codex_credits_are_left_to_cli_and_failed_work_is_reported_and_recoverable(self):
        calls = self.quota_fixture(resets=0, fail=True)
        pair = self.connect(self.a)
        self.assertEqual(c.dispatch(pair), 1)
        self.assertEqual(c.dispatch(pair, resume=True), 1)
        self.assertEqual([row[0] for row in calls()], ["exec", "exec"])
        self.assertIn("quota-session", calls()[1][1])
        self.assertIn("Exact user task", calls()[1][2])
        self.assertEqual(c.load(Path(pair) / "dispatch.json")["through"], 0)
        messages = [row["text"] for row in c.read(pair, "to-rich")["records"]]
        self.assertTrue(any("quota" in text and "exit 1" in text and "resume" in text for text in messages))
        health = c.load(c.root() / "quota/dispatch-health.json")
        self.assertEqual(health["exit_code"], 1)
        self.assertTrue(health["quota_failed"])
        self.assertNotIn("12.345", json.dumps(health))
        self.assertNotIn("Exact user task", json.dumps(health))

    def test_codex_reset_uses_effective_chatgpt_account_with_shell_api_keys(self):
        calls = self.quota_fixture()
        pair = self.connect(self.a)
        with mock.patch.dict(os.environ, {"OPENAI_API_KEY": "fixture", "CODEX_API_KEY": "fixture",
                                         "OPENAI_BASE_URL": "https://example.invalid"}):
            self.assertEqual(c.dispatch(pair), 0)
        self.assertEqual([row[0] for row in calls()], ["reset", "exec"])

    def test_manual_resume_warns_started_chat_to_check_prior_progress(self):
        calls = self.quota_fixture(resets=0, fail=True)
        pair = self.connect(self.a)
        self.assertEqual(c.dispatch(pair), 1)
        self.assertEqual(c.dispatch(pair, resume=True), 1)
        self.assertIn("quota-session", calls()[1][1])
        self.assertIn("Check prior progress before repeating any effects.", calls()[1][2])

    def test_failed_followup_replays_even_after_codex_acknowledged_reading_it(self):
        pair = self.connect(self.a)
        prompts = []
        followup = "Retry this correction after the limit"
        late = "Deliver this later correction too"

        def run(binary, path, marker, state, prompt, watch):
            prompts.append(prompt)
            if len(prompts) == 1:
                state["session_id"] = "fixture-session"
                c.save(marker, state)
            if len(prompts) == 2:
                c.send(pair, "late", late, scope=self.a)
                env = {"RICHOS_CODEX_DISPATCH_LOCATOR": pair,
                       "RICHOS_CODEX_DISPATCH_ATTEMPT": state.get("attempt_id", "")}
                with mock.patch.dict(os.environ, env):
                    c.acknowledge_dispatch_read(pair, 0, c.read(pair)["through"])
                self.assertTrue((Path(pair) / "dispatch-read.json").is_file())
                return 1, True
            return 0, False

        with mock.patch.object(c, "codex_binary", return_value="fixture"), \
             mock.patch.object(c, "check_codex_quota", return_value=(None, "unavailable")), \
             mock.patch.object(c, "run_codex", run):
            self.assertEqual(c.dispatch(pair), 0)
            c.send(pair, "follow-up", followup, scope=self.a)
            self.assertEqual(c.dispatch(pair, resume=True), 1)
            self.assertEqual(c.dispatch(pair, resume=True), 0)
            self.assertEqual(c.dispatch(pair, resume=True), 0)

        self.assertEqual(len(prompts), 3)
        self.assertIn(followup, prompts[2])
        self.assertIn(late, prompts[2])
        self.assertNotIn("Exact user task", prompts[2])

    def test_codex_continues_under_credit_billing_without_banked_resets(self):
        calls = self.quota_fixture(resets=0)
        pair = self.connect(self.a)
        self.assertEqual(c.dispatch(pair), 0)
        self.assertEqual([row[0] for row in calls()], ["exec"])
        self.assertIn("existing credits for eligible work", c.read(pair, "to-rich")["records"][0]["text"])

    def test_live_codex_rate_limit_event_redeems_before_billing_credits(self):
        calls = self.quota_fixture(used=99, midrun="event")
        pair = self.connect(self.a)
        with mock.patch.object(c, "QUOTA_POLL_SECONDS", 60, create=True):
            self.assertEqual(c.dispatch(pair), 0)
        self.assertEqual([row[0] for row in calls()], ["exec", "reset"])
        self.assertIn("used a banked reset", c.read(pair, "to-rich")["records"][0]["text"])
        health = c.load(c.root() / "quota/dispatch-health.json")
        self.assertEqual((health["in_flight_source"], health["in_flight_reset"]), ("event", "reset"))

    def test_live_codex_poll_redeems_once_across_two_successful_runs(self):
        calls = self.quota_fixture(used=99, resets=2, midrun="poll", concurrent_runs=2)
        first = self.connect(self.a)
        second = c.connect("Independent", "Fixture", terminal_session="rich-terminal")["locator"]
        with mock.patch.object(c, "QUOTA_POLL_SECONDS", 0.05, create=True), \
             concurrent.futures.ThreadPoolExecutor(2) as pool:
            self.assertEqual(list(pool.map(c.dispatch, (first, second))), [0, 0])
        self.assertEqual([row[0] for row in calls()], ["exec", "exec", "reset"])
        self.assertIsNone(calls()[-1][2])

    def test_live_codex_poll_recovers_from_failed_preflight_read(self):
        calls = self.quota_fixture(used=99, midrun="poll", preflight_failed=True)
        pair = self.connect(self.a)
        with mock.patch.object(c, "QUOTA_POLL_SECONDS", 0.05):
            self.assertEqual(c.dispatch(pair), 0)
        self.assertEqual([row[0] for row in calls()], ["exec", "reset"])
        health = c.load(c.root() / "quota/dispatch-health.json")
        self.assertEqual((health["in_flight_source"], health["in_flight_reset"]), ("poll", "reset"))

    def test_codex_redeems_the_soonest_expiring_available_reset(self):
        details = [dict(id=credit_id, status=status, expiresAt=expiry, resetType="codexRateLimits")
                   for credit_id, status, expiry in (("later", "available", 4100000000),
                                                     ("no-expiry", "available", None),
                                                     ("expired", "available", 1),
                                                     ("redeemed", "redeemed", 3800000000),
                                                     ("soonest", "available", 4000000000))]
        calls = self.quota_fixture(resets=3, reset_details=details)
        self.assertEqual(c.dispatch(self.connect(self.a)), 0)
        self.assertEqual(calls()[0][2], "soonest")

    def test_codex_reuses_reset_key_after_an_uncertain_response(self):
        details = [dict(id="initial", status="available", expiresAt=4000000000, resetType="codexRateLimits")]
        calls = self.quota_fixture(uncertain=True, reset_details=details)
        first, second = self.connect(self.a), self.connect(self.b)
        self.assertEqual(c.dispatch(first), 0)
        self.assertEqual(c.dispatch(second), 0)
        rows = calls()
        self.assertEqual([row[0] for row in rows], ["unconfirmed", "exec", "reset", "exec"])
        self.assertEqual(rows[0][1], rows[2][1])
        self.assertEqual(rows[0][2], "initial")
        self.assertEqual(rows[0][2], rows[2][2])
        self.assertIn("unconfirmed", c.read(first, "to-rich")["records"][0]["text"])

    def test_codex_exhaustion_during_run_resumes_same_chat_after_reset(self):
        calls = self.quota_fixture(used=99, fail_once=True)
        pair = self.connect(self.a)
        self.assertEqual(c.dispatch(pair), 0)
        self.assertEqual([row[0] for row in calls()], ["exec", "reset", "exec"])
        self.assertIn("quota-session", calls()[2][1])
        self.assertGreater(c.load(Path(pair) / "dispatch.json")["through"], 0)

    def test_dispatch_starts_one_chat_with_its_own_original_task(self):
        first = self.connect(self.a)
        second = c.connect("Private second task", "Second transcript", scope=self.a)["locator"]
        binary = self.root / "codex"
        transcript = self.root / "codex-transcript"
        event = json.dumps({"type": "thread.started", "thread_id": "codex-fixture"})
        binary.write_text("#!/bin/sh\n[ \"$1\" != app-server ] || exit 1\nprintf '%s\\n' " + shlex.quote(event) + "\ncat >> " +
                          shlex.quote(str(transcript)) + "\n")
        binary.chmod(0o700)
        with mock.patch.dict(os.environ, {"PATH": str(self.root) + os.pathsep + os.environ["PATH"]}):
            request = c.read(first)["instructions"]
            self.assertIn(first, request)
            self.assertEqual(c.dispatch(first), 0)
            with self.assertRaisesRegex(ValueError, "already dispatched"):
                c.dispatch(first)
        prompt = transcript.read_text()
        self.assertEqual(prompt.count("Exact user task"), 1)
        self.assertIn(first, prompt)
        self.assertNotIn(second, prompt)
        self.assertNotIn("Private second task", prompt)

    def test_followup_resumes_same_id_and_reconciles_after_restart(self):
        p = self.connect(self.a)
        binary = self.root / "codex"
        calls = self.root / "calls"
        prompt = self.root / "prompt"
        binary.write_text(
            "#!/bin/sh\n[ \"$1\" != app-server ] || exit 1\nprintf '%s\\n' \"$*\" >> " + shlex.quote(str(calls)) +
            "\nprintf '%s\\n' '{\"type\":\"thread.started\",\"thread_id\":\"first-session\"}'" +
            "\ncat >> " + shlex.quote(str(prompt)) + "\n")
        binary.chmod(0o700)
        env = dict(os.environ, PATH=str(self.root) + os.pathsep + os.environ["PATH"])
        cli = [sys.executable, "-B", c.__file__]
        subprocess.run(cli + ["dispatch", "--locator", p], env=env, check=True, capture_output=True)
        reply = c.send(p, "follow-up", "Apply only this correction", scope=self.a)
        self.assertIn("resume", reply["resume_command"])
        for _ in range(2):
            subprocess.run(cli + ["resume", "--locator", p], env=env, check=True, capture_output=True)
        rows = calls.read_text().splitlines()
        self.assertEqual(len(rows), 2)
        self.assertNotIn(" resume ", rows[0])
        self.assertIn(" resume first-session ", rows[1])
        for row in rows:
            args = shlex.split(row)
            self.assertEqual(args[args.index("--sandbox") + 1], "danger-full-access")
            self.assertNotIn("--add-dir", args)
        self.assertEqual(prompt.read_text().count("Apply only this correction"), 1)

    def test_terminal_dispatch_uses_full_access_on_start_and_resume(self):
        pair = c.connect("Terminal task", "Fixture", terminal_session="rich-terminal")["locator"]
        binary = self.root / "codex"
        calls = self.root / "terminal-calls"
        binary.write_text(
            "#!/bin/sh\n[ \"$1\" != app-server ] || exit 1\nprintf '%s\\n' \"$*\" >> " + shlex.quote(str(calls)) +
            "\nprintf '%s\\n' '{\"type\":\"thread.started\",\"thread_id\":\"terminal-chat\"}'"
            "\ncat >/dev/null\n"
        )
        binary.chmod(0o700)
        with mock.patch.object(c, "codex_binary", return_value=str(binary)):
            self.assertEqual(c.dispatch(pair), 0)
            c.send(pair, "next", "Continue", scope=dict(entity_id=c.TERMINAL, thread_id="rich-terminal"))
            self.assertEqual(c.dispatch(pair, resume=True), 0)
        start, resume = (shlex.split(line) for line in calls.read_text().splitlines())
        for args in (start, resume):
            self.assertEqual(args[args.index("--sandbox") + 1], "danger-full-access")
            self.assertNotIn("--add-dir", args)
        self.assertIn("terminal-chat", resume)

    def test_terminal_restart_hears_and_reviews_its_project_pairs(self):
        import review_watch
        config = self.root / "project/orchestration.config"
        config.parent.mkdir()
        config.touch()
        owner = c.terminal_scope(config.parent, "first-session")
        pair = c.connect("Task", "Transcript", terminal_session=owner)["locator"]
        c.send(pair, "outlived-terminal", "READY TO LAND: codex/fixture")
        with mock.patch.dict(os.environ, {"CODEX_WATCH_STATE_DIR": str(self.root / "watch"),
                                         "CODEX_WATCH_CHANNELS": ""}):
            output = io.StringIO()
            watcher.tick(watcher.session_dir("new-session"), config=str(config), sid="new-session", out=output)
            self.assertIn("READY TO LAND: codex/fixture", output.getvalue())
            self.assertEqual(watcher.tick(watcher.session_dir("later-session"), config=str(config),
                                          sid="later-session", out=io.StringIO()), 0)
        world = object.__new__(review_watch.World)
        world.config = str(config)
        with mock.patch.dict(review_watch.MONITOR, {"session": "new-session"}), \
             mock.patch.object(world, "channel_items", side_effect=lambda _, __, rows, reviewed: (list(rows), [])):
            items, _ = world.codex_items(10, {})
        self.assertEqual([str(path) for path, _ in items], [pair])

    def test_terminal_review_reads_company_ready_with_original_context(self):
        import review_watch
        config = self.root / "project" / "orchestration.config"
        config.parent.mkdir()
        config.write_text('SECOND_REVIEW_REPOS="richos"\n')
        repo = self.root / "richos"
        repo.mkdir()
        git = ["git", "-C", str(repo), "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid"]
        subprocess.run(git + ["init", "-q", "-b", "main"], check=True)
        tree = subprocess.check_output(git + ["mktree"], input="", text=True).strip()
        base = subprocess.check_output(git + ["commit-tree", tree, "-m", "Base"], text=True).strip()
        subprocess.run(git + ["update-ref", "refs/heads/main", base], check=True)
        tip = subprocess.check_output(git + ["commit-tree", tree, "-p", base, "-m", "Tip"], text=True).strip()
        pair = self.connect(self.b)
        self.assertEqual(Path(pair).parent.parent.name, "company-b")
        c.send(pair, "other-company-ready", "READY TO LAND: codex/other-company\nRepository: other")
        review_watch.channel_reply(pair, "other-company-ready", "review-held", "The app held this READY.")
        c.send(pair, "company-review", f"READY TO LAND: codex/company-ready\nRepository: richos\nCommit: {tip}")

        world = review_watch.World(str(Path(c.__file__).parents[2]), str(config))
        world.ws = mock.Mock(known_repos=mock.Mock(return_value=[str(repo)]))
        state = {}
        with mock.patch.dict(os.environ, {
            "REVIEW_WATCH_STATE_DIR": str(self.root / "review"),
            "REVIEW_WATCH_TO_RICH": str(self.root / "unused-to-rich.md"),
            "REVIEW_WATCH_TO_CODEX": str(self.root / "unused-to-codex.md"),
        }):
            for session in ("original", "replacement", "after-move"):
                if session == "after-move":
                    pair = str(self.move_pair(pair, self.a["entity_id"]))
                with mock.patch.dict(review_watch.MONITOR, {"session": session}):
                    items, problems = world.codex_items(10, state)
                self.assertEqual(problems, [])
                self.assertEqual([item.tip for item in items], [tip])
                self.assertEqual(items[0].channel, {"locator": pair, "message_id": "company-review"})
                self.assertIn("Exact user task", Path(items[0].words[0]).read_text())
                self.assertEqual(len(state["channel_pending"]), 1)
                holds = [row for row in c.read(pair, "to-rich")["records"]
                         if row.get("reply_to") == "other-company-ready"]
                self.assertEqual(len(holds), 1)
                self.assertEqual(holds[0]["text"], "The app held this READY.")

    def test_company_ready_has_one_review_owner_across_hosts(self):
        import review_watch
        pair = self.connect(self.a)
        first_id = "company-ready"
        c.send(pair, first_id, "READY TO LAND: codex/company-ready")
        repo = str(self.root / "richos")
        item = review_watch.Item("branch:" + repo + ":codex/company-ready", "codex", repo, "1" * 40,
                                 "0" * 40, "ended", branch="codex/company-ready", source="codex")
        item.channel = {"locator": pair, "message_id": first_id}
        watch = object.__new__(review_watch.Watcher)
        terminal = {"REVIEW_WATCH_STATE_DIR": str(self.root / "terminal-review")}
        app = {"REVIEW_WATCH_STATE_DIR": str(self.root / "app-review")}

        with mock.patch.dict(os.environ, terminal):
            lock, _ = watch.take_lock(item, "handover", 10, 1)
            Path(lock).unlink()  # The terminal ends its first attempt and later starts again.
        self.move_pair(pair, self.b["entity_id"])
        with mock.patch.dict(os.environ, app):
            self.assertIsNone(watch.take_lock(item, "handover", 20, 1))
        with mock.patch.dict(os.environ, terminal):
            self.assertIsNotNone(watch.take_lock(item, "handover", 30, 2))

        c.send(pair, "previously-reviewed", "READY TO LAND: codex/company-ready")
        review_watch.channel_reply(pair, "previously-reviewed", "previous-verdict",
                                   review_watch.CHANNEL_VERDICT_PREFIX + "A completed review")
        item.tip = "2" * 40
        item.channel["message_id"] = "previously-reviewed"
        with mock.patch.dict(os.environ, app):
            self.assertIsNone(watch.take_lock(item, "handover", 40, 1))

    def test_terminal_records_delivery_if_a_company_pair_moves_during_output(self):
        pair = self.connect(self.a)
        c.send(pair, "ready-during-move", "READY TO LAND: codex/move-during-output")
        config = str(self.root / "project" / "orchestration.config")
        output = io.StringIO()
        with mock.patch.dict(os.environ, {"CODEX_WATCH_STATE_DIR": str(self.root / "watch"),
                                         "CODEX_WATCH_CHANNELS": str(self.root / "unused-to-rich.md")}):
            with mock.patch.object(output, "flush", side_effect=lambda: self.move_pair(pair, self.b["entity_id"])):
                self.assertEqual(watcher.tick(watcher.session_dir("original"), config=config,
                                              sid="original", out=output), 1)
            self.assertIn("READY TO LAND: codex/move-during-output", output.getvalue())
            self.assertEqual(watcher.tick(watcher.session_dir("replacement"), config=config,
                                          sid="replacement", out=io.StringIO()), 0)

    def test_terminal_shell_uses_lead_project_from_work_folder(self):
        scripts = Path(c.__file__).resolve().parents[1]
        project = self.root / "lead project"
        work = self.root / "work checkout"
        other = self.root / "other project"
        for directory in (project, work, other):
            directory.mkdir()
        for adopted in (project, other):
            (adopted / "orchestration.config").touch()
        sessions = self.root / "sessions"
        sessions.mkdir()
        platform_session = sessions / (str(os.getpid()) + ".json")
        env = dict(os.environ, RICHOS_SESSION_PID=str(os.getpid()),
                   RICHOS_SESSIONS_DIR=str(sessions), RICHOS_WORKSPACES_DIR=str(self.root / "registry"),
                   CODEX_WATCH_STATE_DIR=str(self.root / "watch"), CODEX_WATCH_CHANNELS="")
        for key in ("RICHOS_ENTITY_ROOT", "CLAUDE_PROJECT_DIR", "RICHOS_SESSION_ID", "RICHOS_APP_STATE"):
            env.pop(key, None)

        def seat(sid, cwd):
            platform_session.write_text(json.dumps(dict(sessionId=sid, cwd=str(cwd))))

        def channel(cwd, *args, check=True):
            return subprocess.run(["bash", str(scripts / "codex-channel.sh"), *args],
                                  cwd=cwd, env=env, check=check, text=True, capture_output=True)

        seat("original", project)
        local = json.loads(channel(project, "connect", "--terminal", "--text", "Local task",
                                   "--provenance", "Transcript").stdout)["locator"]
        pair = json.loads(channel(work, "connect", "--terminal", "--text", "Out-of-folder task",
                                  "--provenance", "Transcript").stdout)["locator"]
        self.assertEqual(Path(pair).parent, Path(local).parent)
        channel(other, "send", "--terminal", "--locator", local, "--id", "follow-up", "--text", "Proceed")
        c.send(pair, "result", "Dedicated reply from the task.")
        company = self.connect(self.a)
        ready = "READY TO LAND: codex/company-ready"
        c.send(company, "company-ready", ready)
        second_company = self.connect(self.b)
        c.send(second_company, "company-reply", "Another company reply")
        for session, expected in (("replacement", 1), ("restarted", 0), ("after-move", 0)):
            if session == "after-move":
                self.move_pair(company, self.b["entity_id"])
            seat(session, project)
            result = subprocess.run(["bash", str(scripts / "codex-watch.sh"), "--tick"],
                                    cwd=project, env=env, check=True, text=True, capture_output=True)
            self.assertEqual(result.stdout.count("Dedicated reply from the task."), expected)
            self.assertEqual(result.stdout.count(ready), expected, result.stdout)
            self.assertEqual(result.stdout.count("Another company reply"), expected)
        seat("unrelated", other)
        refused = channel(work, "send", "--terminal", "--locator", local, "--id", "wrong", "--text", "No",
                          check=False)
        self.assertEqual(refused.returncode, 1)
        self.assertIn("Connection belongs to another company or conversation", refused.stderr)

    def test_resume_skips_followup_already_read_by_running_chat(self):
        pair = self.connect(self.a)
        before = "Read by the initial Codex turn"
        after = "Sent after that read"
        prompts = []

        def run(binary, path, marker, state, prompt, watch):
            prompts.append(prompt)
            if len(prompts) == 1:
                state["session_id"] = "initial-chat"
                c.save(marker, state)
                c.send(pair, "already-read", before, scope=self.a)
                child = dict(os.environ, RICHOS_CODEX_DISPATCH_LOCATOR=pair,
                             RICHOS_CODEX_DISPATCH_ATTEMPT=state.get("attempt_id", ""))
                cli = [sys.executable, "-B", c.__file__, "read", "--locator", pair]
                rows = json.loads(subprocess.check_output(cli, env=child))["records"]
                self.assertEqual(rows[-1]["text"], before)
                c.send(pair, "unread", after, scope=self.a)
                self.assertEqual(json.loads(subprocess.check_output(cli))["records"][-1]["text"], after)
            return 0, False

        with mock.patch.object(c, "codex_binary", return_value="codex"), \
             mock.patch.object(c, "check_codex_quota", return_value=(None, "unavailable")), \
             mock.patch.object(c, "run_codex", run):
            self.assertEqual(c.dispatch(pair), 0)
            self.assertEqual(c.dispatch(pair, resume=True), 0)
            self.assertEqual(c.dispatch(pair, resume=True), 0)
        self.assertEqual(len(prompts), 2)
        self.assertNotIn(before, prompts[1])
        self.assertIn(after, prompts[1])

    def test_automatic_association_keeps_legacy_replies_live(self):
        old = c.root() / "rich-codex"
        old.mkdir(parents=True)
        raw = old / "to-rich.md"
        raw.touch()
        state = Path(self.a["state_root"])
        c.save(c.root() / "legacy-target.json", self.a)
        with mock.patch.dict(os.environ, {"CODEX_WATCH_CHANNELS": str(raw),
                                         "CODEX_WATCH_STATE_DIR": str(self.root / "terminal-watch")}):
            terminal = io.StringIO()
            watcher.tick(watcher.session_dir("terminal"), out=terminal)
            self.assertFalse(watcher.prepare_app_connections(str(state)))
            with mock.patch.dict(os.environ, {"CODEX_WATCH_STATE_DIR": str(state / "codex-watch/watch")}):
                app = io.StringIO()
                sd = watcher.session_dir("app-host")
                watcher.tick(sd, out=app, json_lines=True)
                raw.write_text("## Later Codex reply\nThis still reaches Rich.\n")
                watcher.tick(sd, out=app, json_lines=True)
                watcher.tick(sd, out=app, json_lines=True)
                self.assertIn("This still reaches Rich.", json.loads(app.getvalue())["text"])
            watcher.tick(watcher.session_dir("terminal"), out=terminal)
            watcher.tick(watcher.session_dir("terminal"), out=terminal)
            self.assertEqual(terminal.getvalue().count("This still reaches Rich."), 1)

    def test_codex_startup_guide_keeps_existing_user_instructions(self):
        home = Path(os.environ["CODEX_HOME"])
        home.mkdir()
        instructions = home / "AGENTS.md"
        instructions.write_text("Existing personal instructions.\n")
        c.install_codex_instructions(home)
        c.install_codex_instructions(home)
        contents = instructions.read_text()
        self.assertTrue(contents.startswith("Existing personal instructions.\n"))
        self.assertEqual(contents.count("<!-- RichOS Codex coordination -->"), 1)
        guide = c.root() / "codex-instructions.md"
        self.assertIn(str(guide), contents)
        self.assertIn(str(Path(c.__file__).resolve()), guide.read_text())
        self.assertIn("connect --text", guide.read_text())

    def test_lost_response_retry_and_intentional_identical_messages(self):
        p = self.connect(self.a)
        c.send(p, "operation-1", "status?")
        # A fresh helper process recovers the durable ID from the sent journal.
        out = subprocess.check_output([sys.executable, "-B", c.__file__, "read", "--locator", p,
                                       "--direction", "to-rich"], text=True)
        prior = json.loads(out)["records"][-1]
        self.assertTrue(c.send(p, prior["message_id"], prior["text"])["already_saved"])
        c.send(p, "operation-2", "status?")
        self.assertEqual(len(c.read(p, "to-rich")["records"]), 2)
        with self.assertRaises(ValueError):
            c.send(p, "operation-1", "different payload")

    def test_partial_tail_never_emitted_and_writer_recovers_it(self):
        p = self.connect(self.a)
        source = Path(p) / "to-rich.jsonl"
        with source.open("ab") as f:
            f.write(b'{"version":1,"text":"partial\n')
        events, errors = c.scan(self.a["state_root"])
        self.assertEqual(events, [])
        self.assertTrue(errors)  # Complete but invalid newline: held, never skipped.
        source.write_bytes(b'{"version":1')
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))
        c.send(p, "complete", "whole request")
        self.assertEqual(c.read(p, "to-rich")["records"][0]["text"], "whole request")

    def test_scanner_does_not_acknowledge_and_one_bad_channel_does_not_block(self):
        x, y = self.connect(self.a), self.connect(self.b)
        c.send(x, "one", "good")
        c.send(y, "two", "also good")
        bad = Path(x) / "to-rich.jsonl"
        bad.write_bytes(b'{invalid}\n')
        events, errors = c.scan(self.a["state_root"])
        self.assertTrue(errors)
        self.assertEqual([e["record"]["message_id"] for e in events], ["two"])
        self.assertEqual(c.scan(self.a["state_root"])[0], events)
        e = events[0]
        c.save(Path(self.a["state_root"]) / "codex-watch/accepted" / (e["connection"]["connection_id"] + ".json"),
               dict(source_identity=e["source_identity"], through=e["through"]))
        self.assertEqual(c.scan(self.a["state_root"])[0], [])

    def test_concurrent_process_writers_preserve_every_frame(self):
        p = self.connect(self.a)
        def send(i):
            subprocess.run([sys.executable, "-B", c.__file__, "send", "--locator", p,
                            "--id", "msg-%d" % i, "--text", "writer %d" % i], check=True,
                           stdout=subprocess.DEVNULL)
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            list(pool.map(send, range(12)))
        self.assertEqual({r["message_id"] for r in c.read(p, "to-rich")["records"]},
                         {"msg-%d" % i for i in range(12)})

    def test_legacy_owner_and_handoff_are_frozen(self):
        old = c.root() / "rich-codex"
        c.mkdir(old)
        (old / "to-rich.md").write_text("old source\n")
        c.designate(self.a, [old])
        p, record = c.legacy_owner(old)
        handoff = (old / "to-codex.md").read_bytes()
        c.designate(self.b, [old])
        self.assertEqual(c.connection(p)[1], record)
        self.assertEqual((old / "to-codex.md").read_bytes(), handoff)
        self.assertEqual(c.connection(self.connect(locator=p))[1]["thread_id"], "chat-a")
        self.assertIn(b"EVERY unanswered request", handoff)

    def test_later_legacy_association_cannot_move_the_live_spool(self):
        first, later = c.root() / "old-a", c.root() / "old-b"
        for folder in (first, later):
            c.mkdir(folder)
            (folder / "to-rich.md").write_text("preserved raw input")
        c.designate(self.a, [first])
        c.designate(self.b, [later])
        spool = Path(self.a["state_root"]) / "codex-watch/pending.jsonl"
        c.mkdir(spool.parent)
        spool.write_text(json.dumps("Saved old observation") + "\n")
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))
        spool.with_name("live.jsonl").write_text(json.dumps("New observation") + "\n")
        events, errors = c.scan(self.a["state_root"])
        self.assertFalse(errors)
        self.assertEqual(len(events), 1)
        self.assertEqual(events[0]["record"]["kind"], "legacy_live")
        self.assertEqual(events[0]["connection"]["thread_id"], self.a["thread_id"])

    def test_missing_legacy_endpoint_holds_without_reactivating_markdown(self):
        import codex_watch
        old = c.root() / "old"
        c.mkdir(old)
        (old / "to-rich.md").write_text("retained old input\n")
        c.designate(self.a, [old])
        path, record = c.legacy_owner(old)
        metadata = (path / "connection.json").read_bytes()
        (path / "connection.json").unlink()
        self.assertTrue(c.legacy_associated(old), "synced handoff preserves association when metadata is unavailable")
        with self.assertRaises(ValueError):
            c.designate(self.b, [old])
        with mock.patch.dict(os.environ, {"CODEX_WATCH_CHANNELS": str(old / "to-rich.md"),
                                         "CODEX_WATCH_STATE_DIR": str(self.root / "watch")}):
            self.assertEqual(codex_watch.tick(str(self.root / "watch/session")), 0)
        (path / "connection.json").write_bytes(metadata)
        self.assertEqual(c.connection(path)[1], record)

    def test_short_writes_finish_or_fail_without_false_saved(self):
        p = self.connect(self.a)
        real_write = c.os.write
        with mock.patch.object(c.os, "write", side_effect=lambda fd, data: real_write(fd, data[:7])):
            self.assertTrue(c.send(p, "short", "complete across short writes")["saved"])
        calls = 0
        def failed(fd, data):
            nonlocal calls
            calls += 1
            if calls == 1:
                return real_write(fd, data[:9])
            raise OSError(28, "fixture disk full after partial write")
        with mock.patch.object(c.os, "write", side_effect=failed):
            with self.assertRaises(OSError):
                c.send(p, "partial", "retry original")
        self.assertEqual([r["message_id"] for r in c.read(p, "to-rich")["records"]], ["short"])
        c.send(p, "partial", "retry original")
        self.assertEqual([r["message_id"] for r in c.read(p, "to-rich")["records"]], ["short", "partial"])

    def test_busy_writer_and_nonregular_paths_hold_only_their_connection(self):
        x, y = self.connect(self.a), self.connect(self.b)
        c.send(x, "held", "owner held")
        c.send(y, "healthy", "other owner progresses")
        path = Path(x) / "to-rich.jsonl"
        with c.locked(path):
            events, errors = c.scan(self.a["state_root"])
            self.assertEqual([e["record"]["message_id"] for e in events], ["healthy"])
            self.assertTrue(any("writer is busy" in e for e in errors))
        saved = path.read_bytes()
        path.unlink()
        path.symlink_to(Path(y) / "to-rich.jsonl")
        with self.assertRaises(OSError):
            c.send(x, "wrong", "must never reach the other owner")
        events, errors = c.scan(self.a["state_root"])
        self.assertEqual([e["record"]["message_id"] for e in events], ["healthy"])
        self.assertTrue(errors)
        path.unlink()
        os.mkfifo(path)
        events, errors = c.scan(self.a["state_root"])
        self.assertEqual([e["record"]["message_id"] for e in events], ["healthy"])
        self.assertTrue(errors)
        path.unlink()
        path.write_bytes(saved)
        self.assertEqual(len(c.scan(self.a["state_root"])[0]), 2)

    def test_handoff_is_self_contained_and_missing_journals_are_not_recreated(self):
        p = self.connect(self.a)
        command = c.handoff(p).partition("Helper: ")[2].partition(". Use send/read")[0]
        env = dict(os.environ)
        env.pop("CODEX_COORDINATION_ROOT", None)
        out = subprocess.check_output(shlex.split(command) + ["read", "--locator", p], env=env, text=True)
        self.assertEqual(json.loads(out)["records"][0]["text"], "Exact user task")
        self.assertIn("READY TO LAND:", json.loads(out)["instructions"])
        source = Path(p) / "to-rich.jsonl"
        original = source.with_suffix(".preserved")
        source.rename(original)
        with self.assertRaises(FileNotFoundError):
            c.send(p, "held", "restore this endpoint only")
        self.assertFalse(source.exists())
        original.rename(source)
        c.send(p, "held", "restore this endpoint only")
        self.assertEqual(c.read(p, "to-rich")["records"][0]["message_id"], "held")

    def test_send_failure_does_not_claim_saved_and_requires_id(self):
        p = self.connect(self.a)
        result = subprocess.run([sys.executable, "-B", c.__file__, "send", "--locator", p,
                                 "--text", "missing id"], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(c.read(p, "to-rich")["records"], [])
        with self.assertRaises(ValueError):
            c.send(p, "oversize", "x" * (c.TEXT_LIMIT + 1))

    def test_large_stdin_message_is_readable_and_bad_positions_replay(self):
        p = self.connect(self.a)
        text = "large original words " + "x" * (c.TEXT_LIMIT - 21)
        sent = subprocess.run([sys.executable, "-B", c.__file__, "send", "--request"],
            input=json.dumps(dict(locator=p, message_id="large", text=text)), capture_output=True, text=True)
        self.assertEqual(sent.returncode, 0, sent.stderr)
        self.assertEqual(c.read(p, "to-rich")["records"][0]["text"], text)
        source = Path(p) / "to-rich.jsonl"
        pos = Path(self.a["state_root"]) / "codex-watch/accepted" / (Path(p).name + ".json")
        for through in (-1, "bad", True, 7, 10**9):
            c.save(pos, dict(source_identity=c.identity(source), through=through))
            self.assertEqual(c.scan(self.a["state_root"])[0][0]["record"]["message_id"], "large")
        pos.write_text("broken advisory state")
        self.assertEqual(c.scan(self.a["state_root"])[0][0]["record"]["message_id"], "large")

    def test_sync_and_denied_write_failures_never_return_saved(self):
        p = self.connect(self.a)
        with mock.patch.object(c.os, "fsync", side_effect=OSError(28, "fixture disk full")):
            with self.assertRaises(OSError):
                c.send(p, "disk-full", "retained retry")
        # A complete but uncertain append is reconciled by its recorded ID.
        self.assertTrue(c.send(p, "disk-full", "retained retry")["saved"])
        with mock.patch.object(c.os, "open", side_effect=PermissionError("fixture denied write")):
            with self.assertRaises(PermissionError):
                c.send(p, "denied", "must not claim success")
        self.assertEqual([r["message_id"] for r in c.read(p, "to-rich")["records"]], ["disk-full"])

    def test_producer_killed_at_real_append_sync_boundaries_recovers_same_id(self):
        p = self.connect(self.a)
        for boundary in ("before-write", "before-sync", "after-sync"):
            # Only this subprocess is killed. The monkeypatch intercepts an actual
            # lock/write/fsync boundary, not a sleep or a fabricated receipt.
            program = """
import contextlib, os, signal, sys
sys.path.insert(0, sys.argv[1])
import codex_channels as c
boundary, locator, ident = sys.argv[2:]
kill = lambda: os.kill(os.getpid(), signal.SIGKILL)
if boundary == 'before-write':
    original = c.locked
    @contextlib.contextmanager
    def locked(path, **kwargs):
        with original(path, **kwargs) as fd:
            kill()
            yield fd
    c.locked = locked
else:
    original = c.os.fsync
    def sync(fd):
        if boundary == 'before-sync': kill()
        original(fd)
        kill()
    c.os.fsync = sync
c.send(locator, ident, 'recover original request')
"""
            child = subprocess.run([sys.executable, "-B", "-c", program, str(Path(c.__file__).parent), boundary, p, boundary],
                                   capture_output=True, text=True)
            self.assertEqual(child.returncode, -9)
            self.assertEqual(child.stdout, "")
            recovered = c.read(p, "to-rich")["records"]
            if boundary != "before-write":
                self.assertIn(boundary, [r["message_id"] for r in recovered])
            c.send(p, boundary, "recover original request")
            c.send(p, boundary, "recover original request")
            self.assertEqual(sum(r["message_id"] == boundary for r in c.read(p, "to-rich")["records"]), 1)

    def test_partial_migration_handoff_repairs_and_raw_tail_never_imports(self):
        old = c.root() / "rich-codex"
        c.mkdir(old)
        (old / "to-rich.md").write_text("unframed original\n")
        (old / "to-codex.md").write_text("\n## RICHOS CHANNEL MIGRATION ")
        c.designate(self.a, [old])
        p, _ = c.legacy_owner(old)
        self.assertIn("EVERY unanswered request", (old / "to-codex.md").read_text())
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))
        with (old / "to-rich.md").open("a") as f:
            f.write("a second newline fragment\n")
        self.assertEqual(c.scan(self.a["state_root"]), ([], []))

    def test_upgrade_skips_legacy_history_but_delivers_new_appends(self):
        old = self.root / "legacy"
        old.mkdir()
        raw = old / "to-rich.md"
        raw.write_text("## Old entry\nOld archived message\n" * 250)
        outgoing = old / "to-codex.md"
        outgoing.write_text("Old Rich handoff, preserved in place.\n")
        state = Path(self.a["state_root"])
        watch = state / "codex-watch/watch"
        legacy_offset = {str(raw): dict(offset=0, inode=raw.stat().st_ino, seen=raw.stat().st_size)}
        c.save(watch / "app-host/offsets.json", legacy_offset)
        c.save(watch / "last-told.json", legacy_offset)
        spool = state / "codex-watch/pending.jsonl"
        entry = json.dumps("Saved before cutover") + "\n"
        spool.write_text(entry * 250)
        environment = dict(RICHOS_CODEX_CHANNEL_STATE=str(state),
                           CODEX_WATCH_STATE_DIR=str(watch),
                           CODEX_WATCH_CHANNELS=str(raw))
        def monitor_output():
            alive = iter([True, True, True, False])
            output = io.StringIO()
            with mock.patch.dict(os.environ, environment), \
                 mock.patch.object(watcher.host_child, "start", return_value=lambda: next(alive)), \
                 mock.patch.object(watcher.stall_watch, "_nap"), \
                 mock.patch.object(watcher, "tick", wraps=watcher.tick) as tick, \
                 contextlib.redirect_stdout(output):
                self.assertEqual(watcher.run_host_loop(), 0)
                self.assertEqual(tick.call_count, 2, "the real host loop must have polled")
            return output.getvalue()
        self.assertEqual(monitor_output(), "", "history alone should not request a destination")
        self.assertEqual(c.load(watch / "app-host/live-offsets.json")[str(raw)]["offset"], raw.stat().st_size)
        health = state / "codex-watch/health.json"
        c.associate_legacy(self.a, [old])
        _, endpoint = c.legacy_owner(old)
        receipt = state / "codex-watch/accepted" / (endpoint["connection_id"] + ".json")
        c.save(receipt, dict(through=len(entry), source_identity=c.identity(spool)))
        self.assertEqual(monitor_output(), "", "a partially consumed archive should stay quiet")
        self.assertTrue(outgoing.read_text().startswith("Old Rich handoff, preserved in place.\n"))
        self.assertEqual(spool.read_text(), entry * 250)
        self.assertEqual(c.load(watch / "app-host/offsets.json"), legacy_offset)
        self.assertEqual(outgoing.read_text().count("## RICHOS CHANNEL MIGRATION"), 1)
        self.assertEqual(set(c.load(health)), {"at_ms", "waiting", "held"})
        self.assertFalse(c.load(health)["waiting"])
        c.save(health.with_name("ownership-health.json"), dict(at_ms=12, outcome="moved", private="fixture"))
        with raw.open("a") as f:
            f.write("## New entry\nNew Codex message\n")
        emitted = [json.loads(line) for line in monitor_output().splitlines()]
        self.assertEqual([row["type"] for row in emitted], ["legacy_append"])
        self.assertEqual(c.load(health)["ownership_repair"], dict(at_ms=12, outcome="moved"))
        self.assertIn("New Codex message", emitted[0]["text"])
        self.assertNotIn("Old archived message", emitted[0]["text"])
        # The native watcher stores the live append before the next scan.
        (state / c.LIVE_LEGACY_SPOOL).write_text(json.dumps(emitted[0]["text"]) + "\n")
        events, errors = c.scan(state)
        self.assertFalse(errors)
        self.assertEqual([row["record"]["kind"] for row in events], ["legacy_live"])
        self.assertEqual(events[0]["record"]["text"], emitted[0]["text"])
        self.assertEqual(events[0]["connection"]["thread_id"], "chat-a")
        c.save(receipt.with_suffix(".live.json"),
               dict(through=events[0]["through"], source_identity=events[0]["source_identity"]))
        self.assertEqual(c.scan(state), ([], []))
        self.assertEqual(c.load(receipt)["through"], len(entry))

    def test_one_managed_receiver_is_fenced_and_process_death_releases_ownership(self):
        p = self.connect(self.a)
        c.designate(self.a, [])
        c.send(p, "receiver-test", "owning app only")
        watcher = Path(c.__file__).with_name("codex_watch.py")
        children = []
        def start(name):
            state = self.root / name
            env = dict(os.environ, RICHOS_CODEX_CHANNEL_STATE=str(state),
                       CODEX_WATCH_STATE_DIR=str(state / "codex-watch/watch"),
                       CODEX_WATCH_CHANNELS=str(self.root / "unused-legacy" / "to-rich.md"),
                       RICHOS_WATCH_HOST=str(os.getpid()))
            proc = subprocess.Popen([sys.executable, "-B", str(watcher), "--monitor", "--host-json"],
                                    env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            children.append(proc)
            return proc
        def receive(proc):
            with selectors.DefaultSelector() as selected:
                selected.register(proc.stdout, selectors.EVENT_READ)
                self.assertTrue(selected.select(30), "hang guard: no first scan from owned receiver")
                return json.loads(proc.stdout.readline())
        try:
            first = start("first-state")
            self.assertEqual(receive(first)["record"]["message_id"], "receiver-test")
            second = start("second-state")
            out, err = second.communicate(timeout=30)
            self.assertEqual(second.returncode, 0, err)
            self.assertEqual(out, b"", "second managed receiver must not emit app-owned input")
            first.kill()
            first.communicate(timeout=30)
            third = start("second-state")
            self.assertEqual(receive(third)["record"]["message_id"], "receiver-test")
        finally:
            for proc in children:
                if proc.poll() is None:
                    proc.kill()
                proc.communicate(timeout=30)


if __name__ == "__main__":
    unittest.main()
