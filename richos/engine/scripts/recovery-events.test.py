#!/usr/bin/env python3
"""Mechanics of the terminal event adapter (Axel/Wes PRD 2026-10-20 section 8, part 3).

Every case runs on the registry suite's sandbox: temporary repositories, HOME
and CLAUDE_CONFIG_DIR redirected, a real session process, real register_spawn
and real recovery records. The lead transcript, the send hook's ledger and
worker transcripts are fixture files in that sandbox. Triage and advisor
scheduling are recorded by fakes unless a case says otherwise. Pierce's verdict
(and the assignment_classification it attests) is supplied at its existing
seam, _pierce_before_registration. Nothing here reads the live registry, a real
session or a real model.
"""
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
spec = importlib.util.spec_from_file_location("workspaces_fixture", HERE.parent / "mega-lander/tests/workspaces.test.py")
W = importlib.util.module_from_spec(spec)
spec.loader.exec_module(W)
ws = W.ws
import recovery as R
import recovery_events as E
import recovery_runner as RR

T0 = datetime.datetime(2026, 10, 20, 8, 0, tzinfo=datetime.timezone.utc)


def at(minutes):
    return (T0 + datetime.timedelta(minutes=minutes)).isoformat().replace("+00:00", "Z")


class Fakes(object):
    """Triage decisions by event id (default routine/continuation) and a
    scheduler that records which job records were scheduled."""

    def __init__(self):
        self.decisions, self.calls, self.scheduled = {}, [], []

    def triage(self, value):
        self.calls.append(value)
        decision, relation = self.decisions.get(value["event"]["id"], ("routine", "continuation"))
        return dict(decision=decision, reason="fixture %s" % decision, references=["event"],
                    relation=relation if value["ask_relation"] else None, input="in-" + value["event"]["id"],
                    model="fixture", tokens=None, cost_usd=0.001, elapsed_seconds=0.1, reused=False, error="")

    def schedule(self, record, engine):
        self.scheduled.append((record["id"], dict(record.get("repeat_signal") or {})))
        return None


class Mechanics(W.Base):
    def setUp(self):
        super().setUp()
        root = Path(self.env.root)
        self.saved_env = {k: os.environ.get(k) for k in ("SECOND_REVIEW_STATE_DIR", "RICHOS_PROJECTS_DIR", "INFLIGHT_TEAMS_DIR",
                                                         "CLAUDE_PROJECT_DIR")}
        os.environ["CLAUDE_PROJECT_DIR"] = self.entity  # as the PreToolUse hook runs registration
        os.environ["SECOND_REVIEW_STATE_DIR"] = str(root / "state")
        os.environ["RICHOS_PROJECTS_DIR"] = str(root / "projects")
        os.environ["INFLIGHT_TEAMS_DIR"] = str(root / "team")
        (root / "team").mkdir()
        (Path(self.entity) / ".claude").mkdir(exist_ok=True)
        (Path(self.entity) / ".claude/recovery.json").write_text(json.dumps(dict(
            schema=1, mode="questions-only", repositories=[self.other])))
        self.lead = root / "lead.jsonl"
        self.ledger = root / "team" / "inflight-notices.jsonl"
        self.lead.write_text(json.dumps(dict(type="user", uuid="u0", timestamp=at(0),
            message=dict(content="Fix the report so a stale pass is never shown as current."))) + "\n")
        self.fakes = Fakes()
        self.minute = 1
        self.classification = {}

    def tearDown(self):
        for k, v in self.saved_env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        super().tearDown()

    # --- the recorded world --------------------------------------------------
    def tick(self):
        self.minute += 1
        return at(self.minute)

    def lead_rows(self, *rows):
        with self.lead.open("a") as out:
            for row in rows:
                out.write(json.dumps(row) + "\n")

    def call(self, tool, call_id, data, error=False, result_text="ok"):
        self.lead_rows(dict(type="assistant", uuid="a-" + call_id, timestamp=self.tick(),
                            message=dict(content=[dict(type="tool_use", id=call_id, name=tool, input=data)])),
                       dict(type="user", uuid="r-" + call_id, timestamp=self.tick(),
                            message=dict(content=[dict(type="tool_result", tool_use_id=call_id, is_error=error, content=result_text)])))

    def dispatch(self, name, correction_of=(), refused=False):
        """A real registration, exactly as PreToolUse[Agent] makes it, with
        Pierce's attested relationship supplied at its seam."""
        cc = self.make_cc(name)
        prompt = "Fix the report.\ncross-repo-worktree: %s\n" % cc
        payload = dict(session_id=self.sid, tool_use_id="tu-" + name, tool_name="Agent", transcript_path=str(self.lead),
                       tool_input=dict(name=name, subagent_type="zach", prompt=prompt, isolation="worktree"))
        relation = dict(kind="implementation", relation="corrective" if correction_of else "new",
                        correction_of=list(correction_of), evidence="fixture attested relationship")
        with patch.object(ws, "_pierce_before_registration", return_value=dict(id="inspection-" + name, verdict="PASS",
                                                                                assignment_classification=relation)):
            ws.register_spawn(payload, self.entity)
        self.call("Agent", "tu-" + name, payload["tool_input"], error=refused,
                  result_text="PreToolUse:Agent hook error: REFUSED" if refused else "Async agent launched successfully.")
        if refused:
            return None
        aid = "a" + name.replace("-", "")[:12] + "0000"
        npath = os.path.join(self.entity, ".claude", "worktrees", "agent-" + aid)
        W.run("git", "-C", self.entity, "worktree", "add", "-q", npath, "-b", "worktree-agent-" + aid)
        ws.record_start(self.sid, aid, npath, "zach")
        ws.bind_agent(self.sid, "tu-" + name, aid, self.entity)
        folder = Path(os.environ["RICHOS_PROJECTS_DIR"]) / "project" / self.sid / "subagents"
        folder.mkdir(parents=True, exist_ok=True)
        (folder / ("agent-%s.jsonl" % aid)).write_text(json.dumps(dict(type="user", uuid="w0", timestamp=self.tick(),
            isSidechain=True, message=dict(content=prompt))) + "\n")
        return aid

    def worker_file(self, aid):
        return Path(os.environ["RICHOS_PROJECTS_DIR"]) / "project" / self.sid / "subagents" / ("agent-%s.jsonl" % aid)

    def send(self, name, call_id, body, ledger=True, received=None, refused=False):
        self.call("SendMessage", call_id, dict(to=name, summary=call_id, message=body), error=refused)
        if ledger and not refused:
            with self.ledger.open("a") as out:
                out.write(json.dumps(dict(event="InflightNotice", session_id=self.sid, to=name,
                    body_sha256=hashlib.sha256(body.encode()).hexdigest(), timestamp=at(self.minute))) + "\n")
        if received:
            self.receive(received, body)

    def receive(self, aid, body):
        with self.worker_file(aid).open("a") as out:
            out.write(json.dumps(dict(type="attachment", uuid="q-" + str(self.minute), timestamp=self.tick(), isSidechain=True,
                                      attachment=dict(type="queued_command", prompt=body))) + "\n")
            out.write(json.dumps(dict(type="assistant", uuid="t-" + str(self.minute), timestamp=self.tick(), isSidechain=True,
                                      message=dict(content=[dict(type="tool_use", id="w-" + str(self.minute), name="Edit", input={})]))) + "\n")

    def drain(self, triage=None, schedule=None):
        return E.drain(str(HERE.parent), self.entity, ws, triage=triage or self.fakes.triage,
                       schedule=schedule or self.fakes.schedule)

    def events(self, name):
        return ws.load_agent(ws.named_key(self.sid, name)).get("recovery_events") or {}

    def rounds(self, name):
        rec = ws.load_agent(ws.named_key(self.sid, name))
        return E.round_sequence(E.chain(ws, rec, "questions-only"))

    def record_for(self, name):
        rec = ws.load_agent(ws.named_key(self.sid, name))
        import recovery_controls as C
        work, _aliases, _acceptance = C.work(ws, dict(rec, recovery_assignment=E.quiet_projection(rec, "questions-only")))
        return R.read(R.key(self.other, work))

    # --- part 3 mechanics ------------------------------------------------------
    def test_refused_preflight_and_denied_agent_call_add_no_round_and_cost_no_triage(self):
        # granola shape: a spawn.sh preflight refusal is a Bash call; nothing registers and no Agent call exists.
        self.call("Bash", "tu-preflight", dict(command="scripts/spawn.sh iris-fable-granola1 --brief b.md"),
                  result_text="exit 1  guard-definition-drift.sh REFUSES this spawn")
        # A PreToolUse[Agent] denial with no registration, then one registered but denied by a later guard.
        self.call("Agent", "tu-unregistered", dict(name="iris-fable-granola1", prompt="mockups"), error=True,
                  result_text="PreToolUse:Agent hook error: REFUSED")
        self.dispatch("denied-late", refused=True)
        self.dispatch("granola-mockups")
        summary = self.drain()
        self.assertEqual([c["event"]["id"] for c in self.fakes.calls], ["tu-granola-mockups"])
        self.assertEqual(sorted(summary["refused"]), ["tu-unregistered"])
        self.assertEqual(self.events("denied-late")["tu-denied-late"]["decision"], "none")
        self.assertEqual(self.rounds("denied-late"), [])
        self.assertEqual([r["round"] for r in self.rounds("granola-mockups")], [1])
        refused = [r for r in R.rows(R.root() / "events.jsonl") if r.get("event") == E.REFUSED_EVENT]
        self.assertEqual([(r["tool_use_id"], r["round_effect"]) for r in refused], [("tu-unregistered", "none")])
        self.drain()
        self.assertEqual(len([r for r in R.rows(R.root() / "events.jsonl") if r.get("event") == E.REFUSED_EVENT]), 1)
        self.assertEqual(len(self.fakes.calls), 1)

    def test_same_worker_round_four_alarms_at_its_witnessed_start_even_when_triage_says_routine(self):
        aid = self.dispatch("voice-fix")
        for n in (1, 2):
            self.fakes.decisions["c%d" % n] = ("routine", "correction")
            self.send("voice-fix", "c%d" % n, "The review requested changes, round %d: fix finding %d." % (n + 1, n), received=aid)
        self.send("voice-fix", "status", "The CPU is free now; commit at the default line.", received=aid)
        self.fakes.decisions["c3"] = ("routine", "correction")
        self.send("voice-fix", "c3", "The third review requested one more change: keep the final recording.")
        self.drain()
        self.assertEqual([r["round"] for r in self.rounds("voice-fix")], [1, 2, 3])
        self.assertEqual(self.events("voice-fix")["c3"]["start_state"], "pending")
        self.assertEqual(self.fakes.scheduled, [])
        self.assertIsNone(self.record_for("voice-fix"))
        # The worker's own transcript shows the third correction arriving and work beginning.
        self.receive(aid, "The third review requested one more change: keep the final recording.")
        calls = len(self.fakes.calls)
        self.drain()
        self.assertEqual(len(self.fakes.calls), calls)  # no event is triaged twice
        self.assertEqual([r["round"] for r in self.rounds("voice-fix")], [1, 2, 3, 4])
        record = self.record_for("voice-fix")
        self.assertEqual((record["repeat_signal"]["kind"], record["repeat_signal"]["round"]), ("round", 4))
        self.assertEqual(record["repeat_signal"]["events"], ["c3"])
        self.assertTrue(all(R.repeat_pending(record, role) for role in ("axel", "wes")))
        self.assertEqual(len(self.fakes.scheduled), 1)
        self.drain()
        self.assertEqual(len(self.fakes.scheduled), 1)  # an unchanged round alarms once

    def test_both_advisors_are_scheduled_through_the_existing_runner(self):
        import recovery_controls as C
        aid = self.dispatch("scope-work")
        self.fakes.decisions["add-1"] = ("inspect", "continuation")
        self.send("scope-work", "add-1", "One addition from the other group's handover: also keep the lost bundles.", received=aid)
        with patch.object(RR, "launch") as launch:
            self.drain(schedule=C.schedule)
        self.assertEqual(sorted(c.args[1] for c in launch.call_args_list), ["axel", "wes"])
        record = launch.call_args_list[0].args[0]
        self.assertEqual(record["mode"], "questions-only")
        self.assertEqual(record["repeat_signal"]["kind"], "event")
        self.assertIn("fixture inspect", record["repeat_signal"]["reason"])

    def test_duplicate_events_and_identical_texts_stay_distinct_and_are_triaged_once(self):
        aid = self.dispatch("same-text")
        body = "Hand over after the current run. Add no new work."
        self.send("same-text", "h1", body, received=aid)
        self.send("same-text", "h2", body, received=aid)
        with self.ledger.open("a") as out:  # a redelivered ledger row
            out.write(self.ledger.read_text().splitlines()[-1] + "\n")
        summary = self.drain()
        self.assertEqual(sorted(e["id"] for e in summary["events"] if e["kind"] == "send"), ["h1", "h2"])
        self.assertEqual({k: v["witness"] for k, v in self.events("same-text").items() if v["kind"] == "send"},
                         {"h1": "ledger", "h2": "ledger"})
        self.assertEqual(self.drain()["events"], [])
        self.assertEqual(len(self.fakes.calls), 3)
        # A send the hook never recorded is still witnessed, and the gap is named.
        self.send("same-text", "h3", "Status: merging main now.", ledger=False)
        summary = self.drain()
        self.assertEqual(self.events("same-text")["h3"]["witness"], "transcript only")
        self.assertTrue(any("not in the send ledger" in g for g in summary["gaps"]))
        # A refused send was never sent: no event.
        self.send("same-text", "h4", "Pause now.", refused=True)
        self.drain()
        self.assertNotIn("h4", self.events("same-text"))

    def test_worker_replacement_keeps_the_round_history_of_its_recorded_predecessor(self):
        first = self.dispatch("first-try")
        for n in (1, 2):
            self.fakes.decisions["fix%d" % n] = ("routine", "correction")
            self.send("first-try", "fix%d" % n, "Review requested changes again: %d." % n, received=first)
        self.drain()
        self.assertEqual([r["round"] for r in self.rounds("first-try")], [1, 2, 3])
        # A new worker with an unrelated name continues the same unresolved outcome.
        self.dispatch("fresh-hands", correction_of=[ws.named_key(self.sid, "first-try")])
        self.drain()
        self.assertEqual([r["round"] for r in self.rounds("fresh-hands")], [1, 2, 3, 4])
        record = self.record_for("fresh-hands")
        self.assertEqual(record["repeat_signal"]["round"], 4)
        self.assertEqual(record["repeat_signal"]["events"], ["tu-fresh-hands"])
        # Review finding 1: a third replacement keeps the same job, and the
        # watcher's questions-only routing looks up that same job.
        self.dispatch("third-hands", correction_of=[ws.named_key(self.sid, "fresh-hands")])
        self.drain()
        import recovery_controls as C
        identities = {C.work(ws, dict(r, recovery_assignment=E.quiet_projection(r, "questions-only")))[0]
                      for r in (ws.load_agent(ws.named_key(self.sid, n)) for n in ("first-try", "fresh-hands", "third-hands"))}
        self.assertEqual(identities, {"teammate:" + ws.named_key(self.sid, "first-try")})
        record = self.record_for("third-hands")
        self.assertEqual(record["id"], self.record_for("fresh-hands")["id"])
        R.repeat_questioned(record["id"], "wes", dict(job=record["id"], acceptance=record["acceptance"],
            frontier=record["evidence_cursor"], questions=[dict(question="What will end this outcome?", evidence="round 4")]), "q")
        third = ws.load_agent(ws.named_key(self.sid, "third-hands"))
        from types import SimpleNamespace
        item = SimpleNamespace(source="native", repo=self.other, ref=third["key"], session=self.sid, state="running",
                               started=1, env={}, claude="", accounts="", tip="a" * 40)
        with patch.object(RR, "launch"):
            RR.tick([item], str(HERE.parent), self.entity, ws)
        self.assertEqual([n["record"] for n in RR.NOTICES], [str(R.root() / (record["id"] + ".json"))])
        # A name ending in a large number with no recorded relation is round 1.
        self.dispatch("unrelated-job9")
        self.drain()
        self.assertEqual([r["round"] for r in self.rounds("unrelated-job9")], [1])

    def test_restart_retries_a_pending_alarm_once_and_keeps_the_current_owner(self):
        aid = self.dispatch("restart-case")
        self.fakes.decisions["grow"] = ("inspect", "continuation")
        self.send("restart-case", "grow", "Also add a second safety layer the request never asked for.", received=aid)
        with patch.object(R, "reconcile", side_effect=R.Unavailable("recovery state is busy")):
            summary = self.drain()
        self.assertTrue(any("kept pending" in g for g in summary["gaps"]))
        self.assertEqual(self.events("restart-case")["grow"]["alarm"], "pending")
        # A new drain process (the restart) adopts the recorded event: no second triage.
        self.drain()
        self.assertEqual(len([c for c in self.fakes.calls if c["event"]["id"] == "grow"]), 1)
        self.assertEqual(self.events("restart-case")["grow"]["alarm"], "raised")
        record = self.record_for("restart-case")
        self.assertEqual(record["owner"], self.sid)
        self.assertEqual(len(self.fakes.scheduled), 1)

    def test_missing_context_is_named_and_an_unwitnessable_start_summons_the_advisors(self):
        aid = self.dispatch("no-context")
        self.lead.write_text("\n".join(l for l in self.lead.read_text().splitlines() if '"u0"' not in l) + "\n")
        self.worker_file(aid).unlink()
        self.fakes.decisions["fix"] = ("routine", "correction")
        self.send("no-context", "fix", "The review asked for one more change.")
        summary = self.drain()
        sent = next(c for c in self.fakes.calls if c["event"]["id"] == "fix")
        self.assertEqual(sent["original_requests"], [])
        self.assertTrue(any("original requests could not be read" in g for g in sent["gaps"]))
        entry = self.events("no-context")["fix"]
        self.assertEqual(entry["start_state"], "unavailable")
        record = self.record_for("no-context")
        self.assertIn("cannot be witnessed", record["repeat_signal"]["reason"])
        self.assertEqual(summary["alarms"][0]["event"], "fix")

    def test_provider_failure_routes_to_one_inspection_and_never_storms(self):
        import recovery_triage as T
        provider = Path(self.env.root) / "failing-claude"
        provider.write_text("#!/bin/sh\necho outage >&2\nexit 1\n")
        provider.chmod(0o755)
        quota = Path(self.env.root) / "quota"
        quota.write_text("#!/bin/sh\necho fine\nexit 0\n")
        quota.chmod(0o755)
        aid = self.dispatch("outage")
        self.send("outage", "during", "Status: still running the proof.", received=aid)
        with patch.dict(os.environ, {"SECOND_REVIEW_CLAUDE": str(provider), "SECOND_REVIEW_QUOTA_CMD": str(quota)}):
            first = self.drain(triage=T.triage)
            again = self.drain(triage=T.triage)
        self.assertEqual({e["id"]: e["decision"] for e in first["events"]}, {"tu-outage": "unavailable", "during": "unavailable"})
        self.assertEqual(again["events"], [])
        self.assertEqual(again["triage_calls"], 0)
        record = self.record_for("outage")
        self.assertEqual(record["repeat_signal"]["events"], ["tu-outage", "during"])
        self.assertIn("triage unavailable", record["repeat_signal"]["reason"])
        self.assertEqual(len(self.fakes.scheduled), 2)  # the first alarm, then one coalesced update, never more
        self.drain(triage=T.triage)
        self.assertEqual(len(self.fakes.scheduled), 2)

    def test_questions_reach_the_owner_and_only_distinct_witnessed_replies_count(self):
        import review_watch as RW
        aid = self.dispatch("asked")
        self.fakes.decisions["extra"] = ("inspect", "continuation")
        self.send("asked", "extra", "Add the cross-group finding too.", received=aid)
        self.drain()
        record = self.record_for("asked")
        answer = dict(job=record["id"], acceptance=record["acceptance"], frontier=record["evidence_cursor"],
                      questions=[dict(question="Which original requirement does the cross-group finding serve?", evidence="extra")])
        record = R.repeat_questioned(record["id"], "wes", answer, "wes-q", [str(self.lead)])
        notice = RR.owner_notice(record)
        self.assertEqual(notice["session"], self.sid)
        self.assertIn("WES QUESTIONS", notice["lines"][0])
        with patch.dict(os.environ, {"REVIEW_WATCH_STATE_DIR": str(Path(self.env.root) / "watch")}):
            Path(RW._p("last-told.json")).parent.mkdir(parents=True, exist_ok=True)
            RW.stall_watch._write_json(RW._p("last-told.json"), {"recovery_refs": {
                notice["key"]: dict(record=notice["record"], session=notice["session"])}})
            RW.deliver([notice["key"]], datetime.datetime.now().timestamp())
        record = R.read(record["id"])
        self.assertIn(R.repeat_question_key(record, "wes"), record["deliveries"])
        # Before any reply: premature control is refused and nothing is held.
        control = dict(job=record["id"], acceptance=record["acceptance"], frontier=record["evidence_cursor"], action="stop",
                       workers={}, reason="an alarm alone", evidence="alarm")
        with self.assertRaises(R.Unavailable):
            R.control(record["id"], control, "premature")
        self.assertEqual(R.landing_gap(self.other, record["work"]), "")
        # The owner's reply: a later send to the worker and prose, each distinct.
        delivered = record["deliveries"][R.repeat_question_key(record, "wes")]["at"]
        later = datetime.datetime.fromtimestamp(delivered + 60, datetime.timezone.utc).isoformat()
        self.lead_rows(dict(type="assistant", uuid="reply", timestamp=later, message=dict(content=[dict(type="text",
            text="Right: that finding belongs to the other group. I withdrew it from this worker.")])))
        replies = RR.repeat_replies(record, "wes")
        self.assertEqual([r["uuid"] for r in replies], ["reply"])
        reply = dict(job=record["id"], acceptance=record["acceptance"], frontier=record["evidence_cursor"], question="wes-q",
                     answer_ids=["reply"], outcome="self_correction", reason="Rich withdrew the addition himself.", questions=[])
        record = R.repeat_response(record["id"], "wes", reply, "wes-assess", replies)
        with self.assertRaises(R.Unavailable):
            R.repeat_response(record["id"], "wes", dict(reply, outcome="probe_again"), "again", replies)  # not a new answer
        self.assertEqual(record["repeat_outcome"], "self_correction")
        self.assertEqual(record["controls"], [])

    def test_a_failed_classification_cannot_refuse_a_questions_only_dispatch_but_ordinary_refusals_hold(self):
        cc = self.make_cc("identity-gap")
        payload = dict(session_id=self.sid, tool_use_id="tu-identity-gap", tool_name="Agent", transcript_path=str(self.lead),
                       tool_input=dict(name="identity-gap", subagent_type="zach", isolation="worktree",
                                       prompt="Fix it.\ncross-repo-worktree: %s\n" % cc))
        with patch.object(ws, "_pierce_before_registration", return_value=dict(id="i", verdict="PASS")), \
                patch.object(ws, "_classified_recovery_assignment", side_effect=ws.SpecError("job identity unavailable")):
            rec = ws.register_spawn(payload, self.entity)
        self.assertEqual(rec["recovery_assignment"]["authority"]["observation_gap"], "job identity unavailable")
        # Review finding 3: the dispatch is permitted and its missing
        # relationship routes to inspection, even when triage says routine.
        self.call("Agent", "tu-identity-gap", payload["tool_input"], result_text="Async agent launched successfully.")
        self.drain()
        entry = self.events("identity-gap")["tu-identity-gap"]
        self.assertEqual((entry["relation"], entry["decision"], entry["alarm"]), ("uncertain", "routine", "raised"))
        self.assertIn("dispatch relationship unavailable", self.record_for("identity-gap")["repeat_signal"]["reason"])
        sent = next(c for c in self.fakes.calls if c["event"]["id"] == "tu-identity-gap")
        self.assertTrue(any("job identity unavailable" in g for g in sent["gaps"]))
        # Ordinary protections still refuse: Pierce findings, and a workspace never registered.
        blocked = dict(payload, tool_use_id="tu-blocked", tool_input=dict(payload["tool_input"], name="blocked-brief"))
        with patch.object(ws, "_pierce_before_registration", side_effect=ws.SpecError("Pierce FINDINGS: unrequested feature")):
            with self.assertRaises(ws.SpecError):
                ws.register_spawn(blocked, self.entity)
        stray = dict(payload, tool_use_id="tu-stray", tool_input=dict(payload["tool_input"], name="stray-path",
                     prompt="Fix it.\ncross-repo-worktree: %s\n" % os.path.join(self.env.root, "never-registered")))
        with patch.object(ws, "_pierce_before_registration", return_value=dict(id="i", verdict="PASS")):
            with self.assertRaises(ws.SpecError):
                ws.register_spawn(stray, self.entity)

    def test_the_watcher_starts_a_drain_only_when_a_recorded_input_changed(self):
        import review_watch as RW
        from types import SimpleNamespace
        self.dispatch("watched")
        started = []
        class Child(object):
            pid = 4242
        def popen(argv, **kwargs):
            started.append(argv)
            return Child()
        with patch.object(E.subprocess, "Popen", side_effect=popen):
            self.assertEqual(E.launch(str(HERE.parent), self.entity, ws), dict(pid=4242))
            R.F.write_json_atomic(str(R.root() / "events.cursor.json"), dict(fingerprint=E.fingerprint(ws, self.entity)))
            self.assertIsNone(E.launch(str(HERE.parent), self.entity, ws))
            self.send("watched", "later", "Status: still testing.")
            self.assertEqual(E.launch(str(HERE.parent), self.entity, ws), dict(pid=4242))
        self.assertIn("--entity", started[0])
        # The terminal watcher calls it after its recovery tick; the app watcher does not.
        world = SimpleNamespace(repos=["other"], repos_error="", ws=ws, registry_items=lambda *a: ([], []),
                                codex_items=lambda *a: ([], []))
        watcher = RW.Watcher(str(HERE.parent), os.path.join(self.entity, "orchestration.config"), world)
        with patch.dict(os.environ, {"REVIEW_WATCH_STATE_DIR": str(Path(self.env.root) / "watch")}), \
                patch.object(watcher, "reconcile", return_value={}), patch.object(RW, "tell", return_value=[]), \
                patch.object(E, "launch", return_value=None) as launch:
            Path(RW._p("x")).parent.mkdir(parents=True, exist_ok=True)
            watcher.look(1000, {})
        launch.assert_called_once()
        self.assertEqual(launch.call_args.args[1], self.entity)

    def test_the_detached_drain_process_runs_end_to_end_and_holds_one_owner(self):
        import subprocess
        provider = Path(self.env.root) / "routine-claude"
        provider.write_text("#!/usr/bin/env python3\nimport json,sys\na=sys.argv\nshape=json.loads(a[a.index('--json-schema')+1])\n"
                            "answer=dict(decision='routine',reason='status only',references=['event'])\n"
                            "answer.update(relation='continuation') if 'relation' in shape['properties'] else None\n"
                            "print(json.dumps(dict(type='result',structured_output=answer,total_cost_usd=0.0005,usage=dict(input_tokens=9,output_tokens=3))))\n")
        provider.chmod(0o755)
        quota = Path(self.env.root) / "quota"
        readings = Path(self.env.root) / "quota-readings"
        quota.write_text("#!/bin/sh\necho read >> %s\necho fine\nexit 0\n" % readings)
        quota.chmod(0o755)
        aid = self.dispatch("detached")
        self.send("detached", "note", "Status: proof run started.", received=aid)
        env = dict(os.environ, SECOND_REVIEW_CLAUDE=str(provider), SECOND_REVIEW_QUOTA_CMD=str(quota))
        argv = [sys.executable, "-B", str(HERE / "lib/recovery_events.py"), "--engine", str(HERE.parent), "--entity", self.entity]
        result = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=120)
        self.assertEqual(result.returncode, 0, result.stderr)
        summary = json.loads(result.stdout.splitlines()[-1])
        self.assertEqual(sorted(e["id"] for e in summary["events"]), ["note", "tu-detached"])
        self.assertEqual(summary["triage_calls"], 2)
        self.assertEqual(readings.read_text().count("read"), 1)  # one quota reading per drain, not per event
        self.assertEqual(summary["alarms"], [])
        self.assertTrue((R.root() / "events.cursor.json").exists())
        # A second drain while one holds the lock exits at once and changes nothing.
        import fcntl
        fd = os.open(R.root() / "events.drain.lock", os.O_RDWR)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            busy = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=60)
        finally:
            os.close(fd)
        self.assertEqual((busy.returncode, busy.stdout), (0, ""))

    def test_observe_and_off_stay_silent(self):
        aid = self.dispatch("quiet")
        self.fakes.decisions["more"] = ("inspect", "continuation")
        self.send("quiet", "more", "Also add this.", received=aid)
        (Path(self.entity) / ".claude/recovery.json").write_text(json.dumps(dict(schema=1, mode="off", repositories=[self.other])))
        self.assertEqual(self.drain()["events"], [])
        (Path(self.entity) / ".claude/recovery.json").write_text(json.dumps(dict(schema=1, mode="observe", repositories=[self.other])))
        self.drain()
        record = self.record_for("quiet")
        self.assertEqual(record["mode"], "observe")
        self.assertIsNone(RR.owner_notice(dict(record, repeat_questions=dict(wes=dict(frontier=record["evidence_cursor"],
            questions=[dict(question="q", evidence="e")], consultation="c")))))


if __name__ == "__main__":
    unittest.main()
