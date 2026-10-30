#!/usr/bin/env python3
"""Regression: Rich may deliver only the standard pause message, on both paths."""
import importlib.util
import json
import os
import shlex
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
ENGINE = HERE.parent.parent
sys.path.insert(0, str(HERE))
import pause_protocol as protocol


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def generate(*args):
    """The pause command, against an EMPTY fixture registry: it freezes before it prints, so it
    must never reach the real one. The fixture agent is not registered there, so the command
    says NOT HELD and exits 3, and still prints the message (the hold itself:
    mega-lander/tests/workspace-pause-hold.test.sh H10)."""
    with tempfile.TemporaryDirectory(prefix="pause-message-registry-") as tmp:
        env = {**os.environ, "CLAUDE_CONFIG_DIR": tmp, "RICHOS_WORKSPACES_DIR": os.path.join(tmp, "workspaces"),
               "RICHOS_AGENT_HOLD_DIR": os.path.join(tmp, "agent-hold"), "RICHOS_SESSION_ID": "fixture-session"}
        result = subprocess.run([sys.executable, str(HERE / "pause_protocol.py"), *args],
                                capture_output=True, text=True, env=env)
    assert result.returncode == 3 and result.stderr.startswith("NOT "), (result.returncode, result.stderr)
    return result


INCIDENT = '''PAUSE: the CEO, verbatim: "tell these 2 to pause their stuff. Not stop, PAUSE. I'm tired of waiting."
Codex needs the Mac's CPU for its tests.
Right now:
- End any heavy run you own, using only PIDs you captured yourself.
- Commit what you have.
- Then hold: end your turn, do nothing further, and wait to be messaged.
Do not hand off and do not mark your task complete. Your context and your workspace stay exactly as they are. When you're woken, re-run what was cut off.
pause-until: the CEO's word to resume (Codex's tests have the CPU)'''


def payload(text=INCIDENT, summary="CEO: PAUSE now (not stop)", **extra):
    return {"hook_event_name": "PreToolUse", "tool_name": "SendMessage", "session_id": "fixture-session",
            "tool_input": {"to": "echo-opus-fixture", "message": text, "summary": summary, **extra}}


# The tool_input Claude Code 2.1.283 really hands a PreToolUse[SendMessage] hook
# for the unchanged generated pause, captured 2026-09-27 by a logging-only hook
# in an isolated headless session (record: richos-hq
# docs/verification/sendmessage-pause-payload-capture-2026-09-27.md). The model
# sent only to, summary and message; the harness backfilled type, recipient and
# content, and content is a 50-column PREVIEW of message, not a copy of it.
# Its text is the generated pause of that day; the message is now WAIT (the CEO,
# 2026-09-28), so the same shape is rebuilt below around today's generated text,
# and the old text itself must now be refused.
CAPTURED_PAUSE_2026_09_27 = {
    "to": "pausefix-capture-nobody",
    "summary": "PAUSE: preserve work and context",
    "message": "PAUSE: preserve this same task, your full context, every workspace and all work in progress.\n"
               "Pause only. Do not stop, terminate, kill, cancel or restart any running work.\n"
               "Do not hand off, mark the task complete, remove a workspace or replace the agent.\n"
               "Hold without starting further work. Resume the same work only when the orchestrator sends RESUME.\n"
               "If you cannot preserve a running operation while pausing, report that limitation; "
               "do not substitute stopping it.\n"
               "richos-pause-control: {\"version\":1,\"reason\":\"manual\",\"reset\":null}\n"
               "pause-until: the user's explicit instruction to resume",
    "type": "message",
    "recipient": "pausefix-capture-nobody",
    "content": "PAUSE: preserve this same task, your full context…",
}
CAPTURED_PAUSE = dict(CAPTURED_PAUSE_2026_09_27, summary=protocol.SUMMARY, message=protocol.render("manual"),
                      content=protocol.render("manual")[:49] + "…")
# The same capture session's RESUME messages (quota_watch.resume_message and
# release_message for a 23:30Z reset). Short messages come back whole.
CAPTURED_RESUMES = [
    {"to": "pausefix-capture-nobody", "summary": "RESUME",
     "message": "RESUME: the five-hour quota window reset at 23:30Z. Continue exactly where you stopped.",
     "type": "message", "recipient": "pausefix-capture-nobody",
     "content": "RESUME: the five-hour quota window reset at 23:30…"},
    {"to": "pausefix-capture-nobody", "summary": "RESUME",
     "message": "RESUME: the five-hour quota window resets at 23:30Z, less than 20 minutes from now, so the quota "
                "hold is released (ruling §87, richos-hq/wiki/ceo-decisions.md). Continue exactly where you stopped.",
     "type": "message", "recipient": "pausefix-capture-nobody",
     "content": "RESUME: the five-hour quota window resets at 23:3…"},
    {"to": "pausefix-capture-nobody", "summary": "RESUME: continue where you stopped.",
     "message": "RESUME: continue where you stopped.",
     "type": "message", "recipient": "pausefix-capture-nobody",
     "content": "RESUME: continue where you stopped."},
]


def harness(to, message, summary):
    """The backfill the capture showed, for messages the capture did not cover."""
    content = message if len(message) <= 50 else message[:49] + "…"
    return {"hook_event_name": "PreToolUse", "tool_name": "SendMessage", "session_id": "fixture-session",
            "tool_input": {"to": to, "summary": summary, "message": message, "type": "message",
                           "recipient": to, "content": content}}


def captured(tool_input):
    return {"hook_event_name": "PreToolUse", "tool_name": "SendMessage", "session_id": "fixture-session",
            "tool_input": dict(tool_input)}


class PauseMessage(unittest.TestCase):
    def test_incident_is_refused_even_though_it_quotes_the_users_instruction(self):
        with self.assertRaisesRegex(ValueError, "must come from"):
            protocol.validate_payload(payload())

    def test_only_the_whole_standard_message_is_accepted(self):
        for reason, reset in [("manual", None), ("quota", "15:30Z")]:
            original = protocol.render(reason, reset)
            protocol.validate_payload(payload(original, protocol.SUMMARY))
            # These are dangerous changes observed in the incident or proposed
            # afterwards. Other mutations catch deletion of the preserving rules.
            changed = [original + "\nEnd any heavy run you own.",
                       "End any heavy run you own.\n" + original,
                       original.replace("WAIT until", "Stop until"),
                       original.replace("carry on from where you were", "restart the work"),
                       original.replace("run it again each time", "never run it again even if"),
                       original.replace("Bash timeout 600000", "Bash timeout 1000"),
                       original.replace(protocol.WAIT_COMMAND, "kill %1"),
                       original + "\nRe-run what was cut off when woken."]
            for message in changed:
                self.assertNotEqual(message, original, "each dangerous change must really change the text")
                with self.subTest(reason=reason, message=message), self.assertRaises(ValueError):
                    protocol.validate_payload(payload(message, protocol.SUMMARY))

    def test_what_the_agent_hears_is_wait_and_nothing_added(self):
        # The CEO, 2026-09-28: "aha, so the keyword is WAIT, not pause." and "why do you
        # always have to include harmful garbage?" The wait is a command because a
        # background subagent that finishes its reply ends its run (Rich, addition 1).
        for reason, reset in [("manual", None), ("quota", "15:30Z"), ("weekly-quota", None)]:
            text = protocol.render(reason, reset)
            agent_part = text.split("\n" + protocol.MARKER, 1)[0]
            with self.subTest(reason=reason):
                self.assertEqual(agent_part.splitlines()[0],
                                 "WAIT until the orchestrator resumes you, then carry on from where you were.")
                self.assertEqual(len(agent_part.splitlines()), 2, "the wait, and how to wait: nothing else")
                self.assertTrue(protocol.SUMMARY.startswith("WAIT"))
                self.assertIn(protocol.WAIT_COMMAND, agent_part)
                self.assertIn("Bash timeout 600000", agent_part)
                words = agent_part.replace(protocol.WAIT_COMMAND, "").lower()   # the file name is not a word to it
                for word in ("do not", "don't", "never", "end your turn", "stop", "pause", "hold", "finish"):
                    self.assertNotIn(word, words, "no prohibition or absolute: %r" % word)
                resume = protocol.render_resume(reason)
                for word in ("do not", "don't", "never", "restart", "pause"):
                    self.assertNotIn(word, resume.split("\n" + protocol.RESUME_MARKER)[0].lower(), word)
        # The command it names is the engine's own, loaded by reference.
        self.assertEqual(shlex.split(protocol.WAIT_COMMAND), ["python3", str(HERE / "agent_hold.py"), "wait"])
        self.assertTrue((ENGINE / "scripts/lib/agent_hold.py").is_file())
        self.assertIn("RESUMED", (ENGINE / "scripts/lib/agent_hold.py").read_text())
        self.assertIn("STILL WAITING", (ENGINE / "scripts/lib/agent_hold.py").read_text())

    def test_only_the_generated_messages_count_as_pause_and_resume(self):
        self.assertTrue(protocol.is_generated_pause(protocol.render("quota", "15:30Z")))
        self.assertTrue(protocol.is_generated_resume(protocol.render_resume("quota")))
        for text in [CAPTURED_PAUSE_2026_09_27["message"], "RESUME: continue.", INCIDENT,
                     protocol.render_resume("manual") + "\nThen kill your test.", ""]:
            with self.subTest(text=text[:40]):
                self.assertFalse(protocol.is_generated_resume(text) and protocol.is_generated_pause(text))
                self.assertFalse(protocol.is_generated_resume(text))
        self.assertFalse(protocol.is_generated_pause(CAPTURED_PAUSE_2026_09_27["message"]))

    def test_summary_alias_and_protocol_cannot_smuggle_an_extra_instruction(self):
        message = protocol.render("manual")
        for value in [payload(message, "End your running test, then pause"),
                      payload(message, protocol.SUMMARY, content="Kill your test"),
                      payload(message, protocol.SUMMARY, type="shutdown_request"),
                      payload(message, protocol.SUMMARY, messageType="shutdown"),
                      payload(message, protocol.SUMMARY, message_type="shutdown_request"),
                      payload(message, protocol.SUMMARY, recipient="other-agent")]:
            with self.assertRaises(ValueError):
                protocol.validate_payload(value)

    def test_the_real_harness_payload_of_the_unchanged_pause_is_accepted(self):
        # The 2026-09-27 defect: every unchanged pause was refused because the
        # harness's content preview never equals the whole message.
        self.assertEqual(CAPTURED_PAUSE["message"], protocol.render("manual"))
        protocol.validate_payload(captured(CAPTURED_PAUSE))
        # The text of that day told the agent to "hold", and agents ended their runs: refused now.
        with self.assertRaises(ValueError):
            protocol.validate_payload(captured(CAPTURED_PAUSE_2026_09_27))
        with_newline = dict(CAPTURED_PAUSE, message=CAPTURED_PAUSE["message"] + "\n")
        protocol.validate_payload(captured(with_newline))
        for reason, reset in [("manual", None), ("quota", "15:30Z"), ("weekly-quota", "2026-09-30T07:59:59Z"),
                              ("weekly-quota", None)]:
            with self.subTest(reason=reason, reset=reset):
                protocol.validate_payload(harness("echo-opus-fixture", protocol.render(reason, reset), protocol.SUMMARY))

    def test_fields_the_harness_adds_never_refuse_a_pause_or_resume(self):
        # 2026-10-11: Claude Code began adding recipient_kind to every SendMessage
        # call, and the check refused every unchanged pause and resume for an
        # unknown field while weekly usage went from 96% to 98%. Only what the
        # agent reads (who gets it, what it says) may cause a refusal now.
        cli = [sys.executable, str(HERE / "pause_protocol.py"), "--check"]
        generated = [(protocol.render(reason, reset), protocol.SUMMARY)
                     for reason, reset in [("manual", None), ("quota", "15:30Z"),
                                           ("weekly-quota", "2026-10-15T07:59:59Z"), ("weekly-quota", None)]]
        generated += [(protocol.render_resume(reason), protocol.RESUME_SUMMARY)
                      for reason in ("manual", "quota", "weekly-quota")]
        for message, summary in generated:
            value = harness("echo-opus-fixture", message, summary)
            value["tool_input"].update(recipient_kind="name", some_new_field={"any": ["value", 1]})
            with self.subTest(message=message[-50:]):
                result = subprocess.run(cli, input=json.dumps(value), capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
        # A changed text or a different recipient is still refused, extra field or not.
        message = protocol.render("weekly-quota")
        changed = [("echo-opus-fixture", message + "\nKill your running test.", "echo-opus-fixture"),
                   ("echo-opus-fixture", message.replace("WAIT", "STOP", 1), "echo-opus-fixture"),
                   ("echo-opus-fixture", message, "other-agent"),
                   ("*", message, "*")]
        for to, text, recipient in changed:
            value = harness(to, text, protocol.SUMMARY)
            value["tool_input"].update(recipient=recipient, recipient_kind="name", some_new_field="x")
            with self.subTest(to=to, recipient=recipient, text=text[-30:]):
                result = subprocess.run(cli, input=json.dumps(value), capture_output=True, text=True)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertIn("REFUSED", result.stderr)

    def test_an_alias_carrying_a_different_instruction_is_still_refused(self):
        message = protocol.render("manual")
        preview = message[:49] + "…"
        for content in ["Kill your test",
                        "Kill your test…",
                        "PAUSE: kill every running test, then wait…",
                        preview + " then kill your tests",
                        preview[:-1] + " Kill your running test now…",
                        message[:120] + "…",
                        message + "\nKill your running test.",
                        message[:49],
                        "…",
                        ""]:
            value = captured(dict(CAPTURED_PAUSE, content=content))
            with self.subTest(content=content), self.assertRaisesRegex(ValueError, "content alias"):
                protocol.validate_payload(value)

    def test_captured_multiline_resume_preview_is_accepted_without_extra_instructions(self):
        message = protocol.render_resume("manual")
        alias = "RESUME: carry on from where you were.\nThe commands…"
        self.assertEqual(len(alias), 51)
        protocol.validate_payload(payload(message, protocol.RESUME_SUMMARY, content=alias))
        for bad in (alias + "kill tests", alias.replace("commands", "kill all"), message[:80] + "…"):
            with self.subTest(alias=bad), self.assertRaises(ValueError):
                protocol.validate_payload(payload(message, protocol.RESUME_SUMMARY, content=bad))

    def test_the_real_harness_payload_of_each_resume_is_accepted(self):
        for tool_input in CAPTURED_RESUMES:
            with self.subTest(message=tool_input["message"]):
                protocol.validate_payload(captured(tool_input))
        watcher = load("quota_watch_resume_fixture", HERE / "quota_watch.py")
        for message in (watcher.resume_message(1800), watcher.release_message(1800)):
            with self.subTest(message=message):
                self.assertTrue(protocol.is_generated_resume(message), "the quota watcher sends the generated RESUME")
                protocol.validate_payload(harness("echo-opus-fixture", message, protocol.RESUME_SUMMARY))
        # A resume cannot smuggle a pause-worded instruction through its alias.
        generated = protocol.render_resume("manual")
        smuggled = captured(dict(CAPTURED_RESUMES[0], message=generated, summary=protocol.RESUME_SUMMARY,
                                 content="Pause, then kill your test"))
        with self.assertRaises(ValueError):
            protocol.validate_payload(smuggled)

    def test_terminal_guard_passes_the_real_pause_payload_to_its_recipient_checks(self):
        guard = ENGINE / "scripts/hooks/guard-resume-isolation.sh"
        with tempfile.TemporaryDirectory(prefix="pause-message-fixture-") as tmp:
            repo = Path(tmp)
            (repo / "orchestration.config").write_text('SESSION_TEAMS_DIR="' + str(repo / "teams") + '"\n')
            (repo / ".richos").mkdir()
            env = {**os.environ, "RICHOS_ENTITY_ROOT": str(ENGINE), "RICHOS_ENGINE_ROOT": str(ENGINE),
                   "RICHOS_ENGINE_DIR": str(ENGINE), "CLAUDE_PROJECT_DIR": str(ENGINE),
                   "CLAUDE_CONFIG_DIR": str(repo), "RICHOS_WORKSPACES_DIR": str(repo / "workspaces")}
            for value, refused in [(captured(CAPTURED_PAUSE), False),
                                   (captured(dict(CAPTURED_PAUSE, content="Kill your test")), True)]:
                result = subprocess.run(["bash", str(guard)], input=json.dumps(value),
                                        capture_output=True, text=True, env=env)
                with self.subTest(content=value["tool_input"]["content"]):
                    if refused:
                        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                        self.assertIn("pause content alias", result.stderr)
                    else:
                        self.assertNotIn("REFUSED: pause", result.stderr)

    def test_generated_resume_passes_and_a_handwritten_one_still_does_not(self):
        # Before: "RESUME: you were paused; continue..." was refused and no generator existed.
        generated = protocol.message_input("echo-opus-fixture", resume=True)
        self.assertEqual(generated["summary"], protocol.RESUME_SUMMARY)
        text = generated["message"]
        self.assertTrue(protocol.declared(text), "its control line makes the check apply to it")
        self.assertNotRegex(text, r"(?im)^\s*pause-until\s*:", "a resume never re-pauses in the registry")
        protocol.validate_payload(harness("echo-opus-fixture", text, protocol.RESUME_SUMMARY))
        protocol.validate_payload(harness("echo-opus-fixture", text + "\n", protocol.RESUME_SUMMARY))
        for reason in ("quota", "weekly-quota"):
            protocol.validate_payload(harness("echo-opus-fixture", protocol.render_resume(reason), protocol.RESUME_SUMMARY))
        cli = generate("--resume", "--to", "echo-opus-fixture")
        self.assertEqual(json.loads(cli.stdout), generated)
        # A handwritten "RESUME: ..." with no control line is an ordinary message now: it is
        # delivered, and it ends nothing (only the generated RESUME releases a hold).
        handwritten = "RESUME: you were paused; continue exactly where you stopped."
        self.assertFalse(protocol.declared(handwritten))
        self.assertFalse(protocol.is_generated_resume(handwritten))
        protocol.validate_payload(harness("echo-opus-fixture", handwritten, protocol.RESUME_SUMMARY))
        refused = [
            (text + "\nThen kill your test.", protocol.RESUME_SUMMARY),
            (text.replace("carry on from where you were", "start over"), protocol.RESUME_SUMMARY),
            (text + "\npause-until: the CEO's word", protocol.RESUME_SUMMARY),
            (text.replace('"manual"', '"manual","x":1'), protocol.RESUME_SUMMARY),
            (text + "\n" + protocol.render("manual"), protocol.RESUME_SUMMARY),
            (text, "RESUME and kill"),
        ]
        for message, summary in refused:
            with self.subTest(message=message[-60:], summary=summary), self.assertRaises(ValueError):
                protocol.validate_payload(harness("echo-opus-fixture", message, summary))
        with self.assertRaisesRegex(ValueError, "content alias"):
            smuggled = harness("echo-opus-fixture", text, protocol.RESUME_SUMMARY)
            smuggled["tool_input"]["content"] = "Kill your test"
            protocol.validate_payload(smuggled)
        with self.assertRaises(ValueError):
            protocol.message_input("echo-opus-fixture", reset="00:30Z", resume=True)

    def test_terminal_guard_passes_the_generated_resume(self):
        guard = ENGINE / "scripts/hooks/guard-resume-isolation.sh"
        text = protocol.render_resume("manual")
        with tempfile.TemporaryDirectory(prefix="pause-message-fixture-") as tmp:
            repo = Path(tmp)
            (repo / "orchestration.config").write_text('SESSION_TEAMS_DIR="' + str(repo / "teams") + '"\n')
            (repo / ".richos").mkdir()
            env = {**os.environ, "RICHOS_ENTITY_ROOT": str(ENGINE), "RICHOS_ENGINE_ROOT": str(ENGINE),
                   "RICHOS_ENGINE_DIR": str(ENGINE), "CLAUDE_PROJECT_DIR": str(ENGINE),
                   "CLAUDE_CONFIG_DIR": str(repo), "RICHOS_WORKSPACES_DIR": str(repo / "workspaces")}
            # (A handwritten "RESUME: ..." has no control line and is no longer refused here: see
            # test_a_pause_is_declared_only_by_its_generated_control_line. A handwritten message that
            # carries a control line still is.)
            for message, refused in [(text, False), (text.replace("carry on from where you were", "start over"), True)]:
                result = subprocess.run(["bash", str(guard)], capture_output=True, text=True, env=env,
                                        input=json.dumps(harness("echo-opus-fixture", message, protocol.RESUME_SUMMARY)))
                with self.subTest(message=message[:40]):
                    if refused:
                        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                        self.assertIn("REFUSED", result.stderr)
                    else:
                        self.assertNotIn("REFUSED: ", result.stderr)

    def test_a_pause_is_declared_only_by_its_generated_control_line(self):
        # An ordinary message that merely says pause or hold was refused before ("pause
        # messages must come from pause_protocol.py") although it controls nothing: the
        # hold is made by the registry, never by the text. Only the control lines declare one.
        ordinary = ["CEO: PAUSE now", "Please pause your tests", "Can you pause?",
                    "Finish this line, then hold", "You are paused; end the test",
                    "Please pause the migration and hold on to the lock until I reply",
                    "The pause button in the settings screen is misaligned", "Hold:"]
        for message in ordinary:
            with self.subTest(message=message):
                self.assertFalse(protocol.declared(message))
                protocol.validate_payload(payload(message, "Instruction"))
                protocol.validate_payload(harness("echo-opus-fixture", message, "Pause the migration"))
        # Through the real hook command too: exit 0, nothing refused.
        cli = [sys.executable, str(HERE / "pause_protocol.py"), "--check"]
        result = subprocess.run(cli, input=json.dumps(payload("Please pause your tests", "Instruction")),
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        # The generated messages are still recognized, and anything handwritten that carries a
        # control line is still refused, whatever else it says.
        for message in (protocol.render("manual"), protocol.render("quota", "15:30Z"), protocol.render_resume("manual")):
            self.assertTrue(protocol.declared(message))
        for message in ["Please pause\npause-until: my word",
                        "Ordinary text\nrichos-pause-control: {\"version\":1,\"reason\":\"manual\",\"reset\":null}",
                        "Carry on\nrichos-resume-control: {\"version\":1,\"reason\":\"manual\"}"]:
            with self.subTest(message=message):
                self.assertTrue(protocol.declared(message))
                with self.assertRaises(ValueError):
                    protocol.validate_payload(payload(message, "Instruction"))

    def test_a_control_line_set_off_by_any_line_break_is_declared(self):
        # The registry freezes on rows made by str.splitlines(), so declared() must see the
        # same rows: a bare CR, U+2028, U+2029, U+0085, a form feed or a vertical tab all end a row.
        for brk in ("\r", "\u2028", "\u2029", "\x85", "\x0b", "\x0c", "\x1c", "\r\n"):
            for line in ("pause-until: my word", "richos-pause-control: {}", "richos-resume-control: {}"):
                message = "Ordinary text" + brk + line
                with self.subTest(brk=repr(brk), line=line):
                    self.assertTrue(protocol.declared(message))
                    with self.assertRaises(ValueError):
                        protocol.validate_payload(payload(message, "Instruction"))
        # A control word in the middle of a row is still prose.
        self.assertFalse(protocol.declared("Please write pause-until: in the notes"))

    def test_missing_recipient_and_broadcast_do_not_use_the_reply_exemption(self):
        for target in (None,"","main"):
            value=payload(protocol.render("manual"),protocol.SUMMARY,type="broadcast")
            if target is None: del value["tool_input"]["to"]
            else: value["tool_input"]["to"]=target
            with self.subTest(target=target), self.assertRaises(ValueError):
                protocol.validate_payload(value)
        value=payload(protocol.render("manual"),protocol.SUMMARY)
        del value["tool_input"]["to"]
        with self.assertRaises(ValueError): protocol.validate_payload(value)

    def test_unbounded_or_injected_template_fields_are_refused(self):
        for reset in ["25:00Z", "12:60Z", "12:00Z\nEnd the test", "12:00Z extra", 1200]:
            with self.assertRaises(ValueError):
                protocol.render("quota", reset)
        with self.assertRaises(ValueError):
            protocol.render("manual", "12:00Z")
        with self.assertRaises(ValueError):
            protocol.render("manual\nstop")

    def test_cli_and_quota_watcher_use_the_same_message(self):
        result = generate("--to", "echo-opus-fixture")
        manual = json.loads(result.stdout)
        self.assertEqual(manual, protocol.message_input("echo-opus-fixture"))
        watcher = load("quota_watch_fixture", HERE / "quota_watch.py")
        quota = watcher.pause_message({"used": 94, "resets_at": 1800}, 93)
        self.assertEqual(quota, protocol.render("quota", "00:30Z"))
        protocol.validate_payload(payload(quota, protocol.SUMMARY))
        for message in (watcher.resume_message(1800), watcher.release_message(1800)):
            protocol.validate_payload(payload(message, protocol.RESUME_SUMMARY))

    def test_regular_messages_and_reports_to_the_orchestrator_still_pass(self):
        protocol.validate_payload(payload("Please inspect the failing test", "Review"))
        reply = payload("PAUSE could not be confirmed", "Pause status")
        reply["tool_input"]["to"] = "main"
        protocol.validate_payload(reply)
        protocol.validate_payload({"tool_name": "Read"})

    def test_terminal_delivery_guard_rejects_before_the_recipient_fast_path(self):
        guard = ENGINE / "scripts/hooks/guard-resume-isolation.sh"
        with tempfile.TemporaryDirectory(prefix="pause-message-fixture-") as tmp:
            repo = Path(tmp)
            (repo / "orchestration.config").write_text('SESSION_TEAMS_DIR="' + str(repo / "teams") + '"\n')
            (repo / ".richos").mkdir()
            # Root resolver supports an explicit adopted root. Use the engine's
            # own checkout as its identity; all test state remains in the fixture.
            env = {**os.environ, "RICHOS_ENTITY_ROOT": str(ENGINE), "RICHOS_ENGINE_ROOT": str(ENGINE),
                   "RICHOS_ENGINE_DIR": str(ENGINE), "CLAUDE_PROJECT_DIR": str(ENGINE),
                   "CLAUDE_CONFIG_DIR": str(repo), "RICHOS_WORKSPACES_DIR": str(repo / "workspaces")}
            result = subprocess.run(["bash", str(guard)], input=json.dumps(payload()),
                                    capture_output=True, text=True, env=env)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("pause messages must come from", result.stderr)

    def test_desktop_delivery_rejects_before_scope_evidence_or_other_side_effects(self):
        hook = load("desktop_pause_fixture", ENGINE / "scripts/app-engine-hook.py")
        with patch.dict(os.environ, {"RICHOS_APP_STATE": "/fixture/state", "RICHOS_ENTITY_ROOT": "/fixture/company"}), \
                patch.object(hook, "scope", side_effect=AssertionError("later hook stage ran")):
            with self.assertRaisesRegex(ValueError, "pause messages must come from"):
                hook.handle(payload())
            # A valid standard message reaches the normal scope validation.
            with self.assertRaisesRegex(AssertionError, "later hook stage ran"):
                hook.handle(payload(protocol.render("manual"), protocol.SUMMARY))

    def test_shared_template_is_the_only_source_in_both_paths(self):
        terminal = (ENGINE / "scripts/hooks/guard-resume-isolation.sh").read_text()
        self.assertLess(terminal.index('pause_protocol.py" --check'), terminal.index('# --- (4a)'))
        desktop = (ENGINE / "scripts/app-engine-hook.py").read_text()
        self.assertIn("pause_protocol.validate_payload(payload)", desktop)
        doctrine = (ENGINE / "CLAUDE.md.template").read_text()
        self.assertNotIn("A SUBAGENT CANNOT BE PAUSED", doctrine)
        self.assertIn("pause_protocol.py --to", doctrine)


if __name__ == "__main__":
    unittest.main(verbosity=2)
