#!/usr/bin/env python3
"""The one triage command with a fake provider in a private state directory.

These fixtures qualify the contract, parser, coalescing and accounting, never
Haiku's judgment (the frozen replay in private richos-hq measures that).
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / "lib"))
import recovery_triage as T

PROVIDER = r'''#!/usr/bin/env python3
import json, os, sys
a = sys.argv
calls = os.environ["FAKE_CALLS"]
with open(calls, "a") as out:
    out.write(json.dumps(dict(argv=a[1:], stdin=sys.stdin.read())) + "\n")
mode = os.environ.get("FAKE_MODE", "inspect")
if mode == "fail":
    print("provider outage", file=sys.stderr)
    sys.exit(1)
shape = json.loads(a[a.index("--json-schema") + 1])
answer = dict(decision="inspect", reason="The send adds work the request never asked for.", references=["event", "original_requests[0]"])
if "relation" in shape["properties"]:
    answer["relation"] = "continuation"
if mode == "malformed":
    answer = dict(decision="probably fine")
print(json.dumps(dict(type="result", structured_output=answer, total_cost_usd=0.001,
                      usage=dict(input_tokens=100, output_tokens=20), modelUsage={"claude-haiku-test": dict(outputTokens=20)})))
'''


def event_input(**change):
    value = dict(schema=1, event=dict(id="send-1", kind="send", at="2026-10-20T08:22:44Z", worker="worker-c",
                                      outcome="sent", text="SendMessage to worker-c: also cover the other group's finding"),
                 original_requests=["Fix the reuse report."], brief="Fix the reuse report and hand over.",
                 preceding=[], round=dict(current=1, classified_relation=None, references=[]),
                 standing_instructions=[], gaps=[], ask_relation=True)
    value.update(change)
    return value


class Triage(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="recovery-triage-test-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        provider = self.base / "fake-claude"
        provider.write_text(PROVIDER)
        provider.chmod(0o755)
        quota = self.base / "quota"
        quota.write_text("#!/bin/sh\necho 'quota fine'\nexit 0\n")
        quota.chmod(0o755)
        self.calls = self.base / "calls.jsonl"
        self.env = patch.dict(os.environ, {"SECOND_REVIEW_STATE_DIR": str(self.base / "state"),
                                           "SECOND_REVIEW_CLAUDE": str(provider), "SECOND_REVIEW_QUOTA_CMD": str(quota),
                                           "FAKE_CALLS": str(self.calls), "FAKE_MODE": "inspect"})
        self.env.start()
        self.addCleanup(self.env.stop)

    def provider_calls(self):
        return [json.loads(line) for line in self.calls.read_text().splitlines()] if self.calls.exists() else []

    def test_tool_free_haiku_call_records_decision_relation_and_accounting(self):
        result = T.triage(event_input())
        self.assertEqual((result["decision"], result["relation"], result["error"]), ("inspect", "continuation", ""))
        self.assertEqual(result["references"], ["event", "original_requests[0]"])
        self.assertEqual(result["cost_usd"], 0.001)
        self.assertEqual(result["tokens"]["total"], 120)
        self.assertIsNotNone(result["elapsed_seconds"])
        argv = self.provider_calls()[0]["argv"]
        self.assertEqual(argv[argv.index("--model") + 1], "haiku")
        self.assertEqual(argv[argv.index("--tools") + 1], "")
        self.assertEqual(argv[argv.index("--allowedTools") + 1], "")
        self.assertIn("--safe-mode", argv)
        self.assertIn("--no-session-persistence", argv)
        self.assertEqual(argv[argv.index("--system-prompt") + 1], T.PROMPT.read_text())
        self.assertEqual(json.loads(self.provider_calls()[0]["stdin"]), event_input())

    def test_dispatch_and_result_events_never_get_a_second_relation(self):
        result = T.triage(event_input(ask_relation=False, event=dict(event_input()["event"], kind="dispatch", id="agent-1")))
        self.assertIsNone(result["relation"])
        shape = json.loads(self.provider_calls()[0]["argv"][self.provider_calls()[0]["argv"].index("--json-schema") + 1])
        self.assertNotIn("relation", shape["properties"])
        with self.assertRaises(ValueError):
            T.parse(dict(decision="inspect", reason="r", references=[], relation="correction"), False)

    def test_identical_input_is_one_triage_and_a_redelivered_event_costs_nothing(self):
        first = T.triage(event_input())
        again = T.triage(event_input())
        self.assertEqual(len(self.provider_calls()), 1)
        self.assertTrue(again["reused"])
        self.assertEqual(again["input"], first["input"])
        T.triage(event_input(preceding=[dict(at="t", kind="lead-send", text="earlier")]))
        self.assertEqual(len(self.provider_calls()), 2)

    def test_failures_are_unavailable_never_routine_and_an_outage_is_not_retried_per_input(self):
        os.environ["FAKE_MODE"] = "fail"
        failed = T.triage(event_input())
        self.assertEqual(failed["decision"], "unavailable")
        self.assertIn("provider outage", failed["error"])
        self.assertTrue(T.triage(event_input())["reused"])
        self.assertEqual(len(self.provider_calls()), 1)
        os.environ["FAKE_MODE"] = "malformed"
        malformed = T.triage(event_input(event=dict(event_input()["event"], id="send-2")))
        self.assertEqual(malformed["decision"], "unavailable")
        self.assertIn("schema", malformed["error"])
        large = T.triage(event_input(brief="x" * (T.MAX_INPUT + 1)))
        self.assertEqual((large["decision"], len(self.provider_calls())), ("unavailable", 2))
        os.environ["SECOND_REVIEW_CLAUDE"] = str(self.base / "missing")
        self.assertIn("unavailable", T.triage(event_input(gaps=["no provider"]))["error"])
        held = self.base / "held"
        held.write_text("#!/bin/sh\necho 'quota at 95%'\nexit 1\n")
        held.chmod(0o755)
        os.environ["SECOND_REVIEW_CLAUDE"] = str(self.base / "fake-claude")
        os.environ["SECOND_REVIEW_QUOTA_CMD"] = str(held)
        quota = T.triage(event_input(gaps=["quota"]))
        self.assertEqual(quota["decision"], "unavailable")
        self.assertIn("quota at 95%", quota["error"])
        self.assertEqual(len(self.provider_calls()), 2)

    def test_input_contract_refuses_malformed_events(self):
        for change in (dict(schema=2), dict(event=dict(id="x", kind="chat", at="", worker="", outcome="", text="")),
                       dict(preceding=["text"]), dict(standing_instructions=[dict(text="no source")]), dict(ask_relation="yes")):
            with self.subTest(change=change), self.assertRaises(ValueError):
                T.validate(event_input(**change))

    def test_engine_command_contract_for_terminal_and_app(self):
        path = self.base / "event.json"
        path.write_text(json.dumps(event_input()))
        cli = HERE / "recovery.sh"
        result = subprocess.run(["bash", str(cli), "triage", "--input", str(path)], capture_output=True, text=True,
                                stdin=subprocess.DEVNULL)
        self.assertEqual(result.returncode, 0, result.stderr)
        answer = json.loads(result.stdout)
        self.assertEqual(set(answer) >= {"schema", "input", "event", "decision", "reason", "references", "relation",
                                         "model", "tokens", "cost_usd", "elapsed_seconds", "error", "reused"}, True)
        self.assertEqual(answer["event"], "send-1")
        relative = subprocess.run(["bash", str(cli), "triage", "--input", "event.json"], capture_output=True, text=True,
                                  cwd=str(self.base), stdin=subprocess.DEVNULL)
        self.assertEqual(relative.returncode, 2)
        path.write_text("{}")
        invalid = subprocess.run(["bash", str(cli), "triage", "--input", str(path)], capture_output=True, text=True,
                                 stdin=subprocess.DEVNULL)
        self.assertEqual(invalid.returncode, 2)
        self.assertIn("schema 1", invalid.stderr)


if __name__ == "__main__":
    unittest.main()
