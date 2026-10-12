"""Offline output-walk boundaries. No guest, screen or model calls."""
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

sys.dont_write_bytecode = True
SCRIPTS = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("output_walk", SCRIPTS / "testvm/output-walk.py")
output = importlib.util.module_from_spec(spec)
spec.loader.exec_module(output)


class OutputWalkTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory(prefix="output-walk-test-")
        self.addCleanup(tmp.cleanup)
        self.out = Path(tmp.name)
        guard = patch.object(subprocess, "run", side_effect=AssertionError("unexpected guest call"))
        guard.start()
        self.addCleanup(guard.stop)

    def dispatch(self, steps=None, failures=None):
        calls = []
        walk = SimpleNamespace()
        for name in output.STEPS:
            def step(name=name):
                calls.append(name)
                if name in (failures or {}):
                    raise failures[name]
                return {"observed": name}
            setattr(walk, name.replace("-", "_"), step)
        argv = ["output-walk.py", "fixture-vm", "--out", str(self.out), "--expect-sha", "fixture-sha"]
        if steps is not None:
            argv.extend(["--steps", steps])
        with patch.object(sys, "argv", argv), patch.object(output, "OutputWalk", return_value=walk) as owner, \
                contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            result = output.main()
        return result, calls, json.loads((self.out / "report.json").read_text()), owner

    def test_default_runs_witnesses_without_candidate_steps(self):
        result, calls, report, owner = self.dispatch()
        self.assertEqual(result, 0)
        self.assertEqual(calls, ["identity", "first-run", "connect", "tools", "pdf",
                                 "backend-worker", "front-desk-worker"])
        self.assertEqual([row["step"] for row in report["steps"]], calls)
        self.assertTrue(all(row["outcome"] == "PASS" for row in report["steps"]))
        self.assertEqual((report["vm"], report["expect_sha"]), ("fixture-vm", "fixture-sha"))
        self.assertEqual(owner.call_args.args[0].within, 900)

    def test_named_passes_only_for_the_seeded_teammate_shown_by_its_name(self):
        """Slice 2 of the proto-teammate shelf plan: named's verdict, offline."""
        mark = {"name": "mark-sonnet-0123456789ab", "subagent_type": "richos-app-engine:mark", "model": "sonnet"}
        frank = {"name": "frank-opus-ba9876543210", "subagent_type": "richos-app-engine:frank", "model": "opus"}
        landed = [{"workerName": mark["name"], "source": "land"}, {"workerName": mark["name"], "source": "command"}]
        self.assertEqual(output.named_verdict("mark", [mark, frank], landed, ["by Mark"], ["Mark, Ended"]), [])
        # The guest reads the line inside the row button's own name.
        self.assertEqual(output.named_verdict("mark", [mark], landed, ["notes.zip ~/Acme/ by Mark"], []), [])
        # What main does: the generic worker on the fixed sonnet, its agent name on the label.
        generic = {"name": "worker-sonnet-0123456789ab", "subagent_type": "richos-app-engine:worker", "model": "sonnet"}
        failures = output.named_verdict("mark", [generic], [{"workerName": generic["name"], "source": "land"}],
                                        ["by worker-sonnet-0123456789ab"], [])
        self.assertTrue(any("not <teammate>-<model>-<id12>" in f for f in failures), failures)
        self.assertTrue(any("never started" in f for f in failures), failures)
        self.assertTrue(any("does not say 'by Mark'" in f for f in failures), failures)
        self.assertTrue(any("engine's agent name" in f for f in failures), failures)
        # Started on another model than its name, or as another definition.
        wrong = dict(frank, model="sonnet", subagent_type="richos-app-engine:worker")
        failures = output.named_verdict("mark", [mark, wrong], landed, ["by Mark"], [])
        self.assertEqual(len(failures), 2, failures)
        # Another teammate landed the files.
        failures = output.named_verdict("mark", [mark], [{"workerName": frank["name"], "source": "land"}], ["by Mark"], [])
        self.assertTrue(any("not mark's" in f for f in failures), failures)

    def test_consult_passes_only_for_clarks_answer_read_back_with_no_workspace(self):
        """Slice 3 of the proto-teammate shelf plan: consult's verdict, offline."""
        name = "clark-sonnet-0123456789ab"
        receipt = {"name": name, "request": {"role": "consult", "teammate": "clark", "repo": None, "repos": []},
                   "consult_answer": {"answered": True, "message_sha256": "ab" * 32}}
        spawn = {"name": name, "subagent_type": "richos-app-engine:clark", "model": "sonnet",
                 "prompt": "# Your duty in this assignment: consult\n\nWhich was first?"}
        inspects = [json.dumps({"records": [{"consult_answer": {"message_sha256": "ab" * 32}}]})]
        settled = [{"state": "settled", "detail": "answered"}]
        self.assertEqual(output.consult_verdict("clark", [receipt], [spawn], inspects, [], settled), [])
        # What main does: no consult at all, the back end answered itself or prepared nothing.
        self.assertEqual(output.consult_verdict("clark", [], [], [], [], settled),
                         ["no consult receipt was written: the back end prepared no consult"])
        # Each way it can be wrong, named once.
        cases = [
            (dict(receipt, consult_answer={"answered": False}), spawn, inspects, [], settled, "no answer on its receipt"),
            (receipt, spawn, [], [], settled, "never read"),
            (receipt, dict(spawn, prompt="cross-repo-worktree: /x\n\nbrief"), inspects, [], settled, "given a workspace"),
            (receipt, dict(spawn, subagent_type="richos-app-engine:worker"), inspects, [], settled, "was started as"),
            (receipt, spawn, inspects, ["/d/engine-state/target-worktrees/p/" + name], settled, "workspace was created"),
            (receipt, spawn, inspects, [], [{"state": "failed"}], "did not settle"),
            (dict(receipt, request=dict(receipt["request"], repo="/Acme")), spawn, inspects, [], settled, "names a repository"),
        ]
        for r, s, i, w, a, said in cases:
            failures = output.consult_verdict("clark", [r], [s], i, w, a)
            self.assertTrue(len(failures) == 1 and said in failures[0], (said, failures))

    def test_shelf_passes_only_for_an_activated_engineer_in_the_same_assignment(self):
        """Slice 5 of the proto-teammate shelf plan: the end-to-end walk's verdict, offline."""
        shelf = ["mark", "norm", "sage"]
        job = [{"state": "settled", "obligation_id": "obl-1"}]
        mark = "mark-sonnet-0123456789ab"
        receipts = [
            {"name": "dean-sonnet-aaaaaaaaaaaa", "request": {"role": "consult", "teammate": "dean", "obligation_id": "obl-1"}},
            {"name": mark, "request": {"role": "worker", "teammate": "mark", "obligation_id": "obl-1"}},
            {"name": "frank-opus-bbbbbbbbbbbb", "request": {"role": "reviewer", "teammate": "frank", "obligation_id": "obl-1"}},
        ]
        spawns = [{"name": mark, "subagent_type": "richos-app-engine:mark", "model": "sonnet"}]
        lands = [{"workerName": mark, "source": "land"}]
        line = output.SHELF_LINE
        self.assertEqual(output.shelf_verdict(shelf, ["mark"], receipts, spawns, lands, line, [4200], job), [])
        # What the base does: Mark is saved, the run ends, nothing is renewed, nobody does the job.
        failures = output.shelf_verdict(shelf, ["mark"], receipts[:1], [], [], "", [], [dict(job[0], state="failed")])
        for said in ("did not settle", "was prepared as the worker", "nobody reviewed",
                     "does not read back", "never renewed"):
            self.assertTrue(any(said in f for f in failures), (said, failures))
        # Dean may activate a reviewer too: the engineer is the one prepared as the worker.
        self.assertEqual(output.shelf_verdict(shelf, ["mark", "sage"], receipts, spawns, lands, line, [4200], job), [])
        # Each way it can be wrong, named once.
        other = [dict(r, request=dict(r["request"], obligation_id="obl-2")) for r in receipts]
        cases = [
            ([], receipts, spawns, lands, line, [4200], job, "no shelf teammate was activated"),
            (["mark"], receipts[1:], spawns, lands, line, [4200], job, "never consulted Dean"),
            (["mark"], receipts, [dict(spawns[0], subagent_type="richos-app-engine:worker")], lands, line, [4200], job,
             "was started as"),
            (["mark"], receipts, [], lands, line, [4200], job, "never started"),
            (["mark"], receipts, spawns, lands, line, [4200], job + job, "not one assignment"),
            (["mark"], receipts[:2], spawns, lands, line, [4200], job, "nobody reviewed"),
            (["mark"], receipts, spawns, [{"workerName": "frank-opus-bbbbbbbbbbbb"}], line, [4200], job, "not landed"),
            (["mark"], receipts, spawns, lands, line, [], job, "never renewed"),
        ]
        for saved, r, s, l, rb, ms, a, said in cases:
            failures = output.shelf_verdict(shelf, saved, r, s, l, rb, ms, a)
            self.assertTrue(len(failures) == 1 and said in failures[0], (said, failures))
        # Receipts of another assignment are not this job's: Dean, the worker and the reviewer are missing.
        failures = output.shelf_verdict(shelf, ["mark"], other, spawns, lands, line, [4200], job)
        self.assertTrue(any("never consulted Dean" in f for f in failures), failures)
        self.assertTrue(any("nobody reviewed" in f for f in failures), failures)

    def test_explicit_steps_keep_order_and_hyphenated_dispatch(self):
        result, calls, report, _owner = self.dispatch("scroll,job-question,pdf")
        self.assertEqual((result, calls), (0, ["scroll", "job-question", "pdf"]))
        self.assertEqual([row["evidence"] for row in report["steps"]],
                         [{"observed": name} for name in calls])

    def test_unknown_step_refuses_before_constructing_a_walk(self):
        argv = ["output-walk.py", "fixture-vm", "--out", str(self.out), "--expect-sha", "fixture-sha",
                "--steps", "scroll,unknown-step"]
        with patch.object(sys, "argv", argv), patch.object(output, "OutputWalk") as owner, \
                contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as raised:
            output.main()
        self.assertEqual(raised.exception.code, 2)
        owner.assert_not_called()
        self.assertFalse((self.out / "report.json").exists())

    def test_failed_prerequisites_stop_but_independent_witnesses_continue(self):
        for step, error, expected in [
            ("identity", output.StepFailed("wrong source"), ["identity"]),
            ("pdf", RuntimeError("guest refused"), ["identity", "pdf", "backend-worker"]),
            ("pdf", subprocess.TimeoutExpired("fixture", 180), ["identity", "pdf", "backend-worker"]),
        ]:
            with self.subTest(step=step, error=type(error).__name__):
                result, calls, report, _owner = self.dispatch("identity,pdf,backend-worker", {step: error})
                self.assertEqual((result, calls), (1, expected))
                failed = next(row for row in report["steps"] if row["step"] == step)
                self.assertEqual(failed["outcome"], "FAIL")
                self.assertIn(str(error), failed["detail"])

    def walk(self):
        walk = object.__new__(output.OutputWalk)
        walk.vm, walk.out = "fixture-vm", self.out
        walk.a = SimpleNamespace(within=900)
        return walk

    def test_ax_host_wait_outlasts_raised_guest_deadline_and_keeps_larger_caller_budget(self):
        walk = self.walk()
        for guest, caller, expected in [("20", 40, 50), ("150", 40, 180), ("150", 240, 240)]:
            with self.subTest(guest=guest, caller=caller), patch.dict(os.environ, TESTVM_AX_TIMEOUT=guest), \
                    patch.object(output.command_walk.adopt_walk, "command", return_value='{"id":"found"}\n') as run:
                self.assertEqual(walk.ax("find", "--title", "All output", app="fixture-app", timeout=caller),
                                 [{"id": "found"}])
                argv, timeout = run.call_args.args
                self.assertEqual(timeout, expected)
                self.assertEqual(argv, [output.HERE / "ax.sh", "fixture-vm", "find", "--title",
                                        "All output", "--json", "--app", "fixture-app"])

    def test_overlap_refuses_open_panel_pressed_toggle_or_missing_toggle(self):
        for opened, values, fails in [(False, [0, "false"], False), (True, [0, 0], True),
                                       (False, ["1", 0], True), (False, ["true", 0], True),
                                       (False, [0], True)]:
            with self.subTest(opened=opened, values=values), patch.object(output.time, "sleep"):
                walk = self.walk()
                walk.open_panel = walk.to_list = walk.press = Mock()
                walk.present = lambda title: title == "Open the work summary" or opened
                walk.find_all = lambda title, **kw: ([{"title": "Output", "value": v} for v in values]
                                                     if title == "from this thread" else [])
                walk.flip_theme = Mock(return_value=["dark.png", "light.png"])
                if fails:
                    with self.assertRaises(output.StepFailed):
                        walk.overlap()
                else:
                    self.assertFalse(walk.overlap()["output_still_open"])
                evidence = json.loads((self.out / "overlap-observed.json").read_text())
                self.assertEqual(evidence["shots"], ["dark.png", "light.png"])

    def scroll_fixture(self, trace):
        walk = self.walk()
        walk.send = Mock(return_value=("turn-1", 123))
        walk.record = Mock(return_value=[{"path": f"/Acme/scroll-{i:02d}.txt"} for i in range(1, 31)])
        walk.save_record = Mock()
        walk.turn_story = Mock(return_value={})
        walk.close_panel = walk.open_panel = walk.to_list = Mock(
            side_effect=lambda: trace.append(("panel", os.environ["TESTVM_AX_TIMEOUT"])))
        walk.flip_theme = Mock(side_effect=lambda stem: trace.append(("shots", stem)) or ["dark.png", "light.png"])
        walk.app_pid = Mock(return_value=123)
        walk.script = Mock(return_value="0,0,900,100")
        walk.in_panel = lambda nodes: nodes
        walk.divider_x = Mock(return_value=500)
        walk.find_all = lambda title, **kw: trace.append(("read", title)) or [{"title": "row", "y": 120, "h": 20}]
        return walk

    def test_job_question_sets_deadline_before_first_read_and_rejects_false_quit_state(self):
        for quit_card, unavailable, fails in [([], [], False), (["quit card"], [], True),
                                               ([], ["Status unavailable"], True)]:
            with self.subTest(quit_card=quit_card, unavailable=unavailable), patch.dict(os.environ):
                walk = self.walk()
                walk.facts = {"thread": "thread-1"}
                deadlines = []
                walk.close_panel = Mock(side_effect=lambda: deadlines.append(os.environ["TESTVM_AX_TIMEOUT"]))
                walk.present = Mock(return_value=False)
                walk.records = Mock(return_value=[{"thread_id": "thread-1", "obligation_id": "ob-1"}])
                walk.app_pid = Mock(return_value=123)
                walk.conversation_read = Mock(return_value={"question_cards": 1, "quit_card": quit_card,
                                                           "status_unavailable": unavailable})
                walk.flip_theme = Mock(return_value=["dark.png", "light.png"])
                if fails:
                    with self.assertRaisesRegex(output.StepFailed, "cut off by a quit"):
                        walk.job_question()
                else:
                    self.assertEqual(walk.job_question()["question_cards"], 1)
                self.assertEqual(deadlines, ["150"])

    def test_scroll_raises_deadline_and_photographs_before_reading_rows(self):
        trace = []
        with patch.dict(os.environ), patch.object(output.time, "sleep"):
            evidence = self.scroll_fixture(trace).scroll()
        self.assertEqual(trace[:3], [("panel", "150")] * 3)
        self.assertLess(trace.index(("shots", "scroll")), trace.index(("read", "scroll-")))
        self.assertEqual(evidence["recorded"], 30)
        self.assertEqual(evidence["rows_below_window"], 1)

    def test_scroll_without_enough_files_refuses_before_opening_the_panel(self):
        trace = []
        with patch.dict(os.environ), patch.object(output.time, "monotonic", side_effect=[0, 1000]):
            with self.assertRaisesRegex(output.StepFailed, "0 of 30"):
                self.scroll_fixture(trace).scroll()
        self.assertEqual(trace, [])
        self.assertEqual(json.loads((self.out / "scroll-observed.json").read_text())["recorded"], 0)

    def test_scratch_path_is_the_apps_scratch_set(self):
        sys.path.insert(0, str(SCRIPTS.parents[1] / 'engine/scripts/lib'))
        import machine_settings
        # The app's selected named root may be anywhere, including a path with spaces.
        named = Path('/external data/named scratch')
        with patch.object(machine_settings, 'agent_root', return_value=named):
            for p in ['/tmp/x/a.md', '/private/tmp/rv-zip-a1/notes.zip',
                      '/var/folders/ab/cd/T/a.md', str(named / 'echo/a.md'),
                      '/Users/a/proj/.claude/worktrees/agent-1/a.md']:
                self.assertTrue(output.scratch_path(p), p)
            for p in ['/Users/a/Documents/notes.zip', '/Users/a/proj/.claude/agents/notes.txt',
                      '/Users/a/proj/.claude/skills/x/SKILL.md', '/tmpfoo/a.md',
                      '/external data/other scratch/echo/a.md']:
                self.assertFalse(output.scratch_path(p), p)

    def links_fixture(self, opens):
        walk = self.walk()
        walk.divider_x = Mock(return_value=1141)
        # A strip link outside the panel, and the panel's own row for the same file.
        walk.find_all = lambda title, **kw: [{"title": title, "x": 606, "y": 90, "w": 80},
                                             {"title": title + " ~/Acme/ by worker", "x": 1141, "y": 370, "w": 300}]
        walk.ax = Mock(return_value=[])
        walk.wait_for = walk.press_any = walk.press = walk.to_list = walk.shot = Mock()
        walk.script = Mock(return_value="")
        walk.clipboard = Mock(return_value=opens)
        return walk

    @patch.object(output.time, "sleep")
    def test_scratch_links_judge_a_link_by_where_it_opens_not_by_its_name(self, _sleep):
        listed = ["/Users/admin/home/Acme/notes.zip"]
        job = ["/private/tmp/rv1/o/notes.zip"]
        note = Mock()
        walk = self.links_fixture("/Users/admin/home/Acme/notes.zip")
        self.assertEqual(walk.scratch_links(job, listed, note), [])
        walk.ax.assert_called_once_with("click", "--title", "notes.zip", "--role", "AXButton", "--contains", "--nth", "0")
        failed = self.links_fixture("/private/tmp/rv1/o/notes.zip").scratch_links(job, listed, note)
        self.assertEqual(len(failed), 1)
        self.assertIn("opens the scratch copy", failed[0])
        unread = self.links_fixture("seed").scratch_links(job, listed, note)
        self.assertIn("could not read where", unread[0])
        only_scratch = self.links_fixture("").scratch_links(["/private/tmp/rv1/x.zip"], listed, note)
        self.assertIn("only the scratch set holds", only_scratch[0])


if __name__ == "__main__":
    unittest.main()
