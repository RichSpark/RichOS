#!/usr/bin/env python3
"""Single desktop hook registration; canonical components retain their decisions.

The host supplies all roots and the active scope. There is no terminal-settings
fallback. Provider callbacks are evidence, not proof of assignment completion.
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

ENGINE = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run(command, payload, timeout=20):
    result = subprocess.run(command, input=json.dumps(payload), text=True, capture_output=True, timeout=timeout)
    if result.stdout:
        sys.stdout.write(result.stdout)
    if result.stderr:
        sys.stderr.write(result.stderr)
    if result.returncode:
        raise RuntimeError(f"Required engine control failed or refused (exit {result.returncode})")


def scope():
    path = Path(os.environ["RICHOS_APP_SCOPE"])
    # 256 KiB: the scope carries the user's own words since the second review's slice 4;
    # the same bound as every other reader (richos-core ecs.rs SCOPE_LIMIT).
    if not path.is_absolute() or path.stat().st_size > 256 * 1024:
        raise ValueError("invalid desktop action scope")
    value = json.loads(path.read_text())
    if value.get("version") != 1:
        raise ValueError("unsupported desktop action scope")
    return value


def a_worker_this_lease_already_dispatched(active, payload):
    """Is this tool call a BACKGROUND WORKER's, under a standing grant that survives the
    turn that dispatched it?

    **THE DEFECT THIS ANSWERS, measured on 2026-09-18 and quoted from the worker's own last
    words** (`engine-state/evidence/<session>/callbacks.jsonl`, row 21 of a
    `work_lease_roundtrip` run): *"Unable to complete or report via tools - the host's
    PreToolUse hook is blocking all tool actions this turn, including SubagentHandback
    itself, with: 'RichOS desktop engine: This app turn is stopped or is supplying context.
    New actions are unavailable.'"*

    `actions_allowed` is the TURN's grant, opened and closed by `NativeCognition::prompt`.
    A background worker outlives the turn that dispatched it BY DESIGN — `mega-lander/app.py`
    stamps `run_in_background: true` on every payload, the provider answers
    `{"status":"async_launched"}`, and the turn ends with the worker just getting started.
    So the worker did all of its work in the window where the flag was false and had every
    single call refused, produced nothing, and was then disposed of as landed (an agent that
    produced nothing IS landed, worktree-spec point 7) with its worktree deleted.

    **What authorizes the worker instead is its RECEIPT, and those checks are all still
    ahead of it in `handle`:** `guard-sealed-worktree.sh` refuses a run the platform has
    already ended, `validate_shell_target` refuses a Git command whose target it cannot
    read, and `app.py`'s `worker_context` refuses any `agent_id` that does not join exactly
    one live receipt of this session and this binding, refuses the CEO-scoped `mcp__richos_*`
    tools outright, and fences every write inside the registered target worktree. Nothing is
    waved through here; a different, narrower gate answers for a worker than for the lease.

    **A stop still stops it.** The flag is written true in one place (`bind_work_assignment`'s
    standing §5.4 grant) and cleared by `ecs::revoke` — the CEO's stop, the end of the
    assignment, a crash-recovery sweep — and a stop also kills the process group the worker
    lives in. Absent means false, so an older app, a conversation lease, or a scope written
    by anything else refuses a worker exactly as before.
    """
    return bool(payload.get("agent_id")) and active.get("background_work_allowed") is True


def mid_job_notice(work, active, payload, root, emit):
    """A mid-job second review's request for changes, told ONCE to the running worker it is about,
    at its next tool call (second review, slice 4; plan §2.5; Sage's check, catch 7: the app's
    workers run this hook and not the engine's hook set, so this is where delivery lives). The
    reviews are started by the app's own review-watch, the host's child; the verdicts are in the
    app's ledger (scripts/lib/app_review.py). The notice goes to emit(text), which writes and
    flushes the hook's answer; True when it did. Only a worker's tool call is answered; nothing
    here can refuse a tool call, and a reading that fails says nothing rather than stopping the
    work. An output that fails raises, as any hook output does, and the verdict stays undelivered
    for the next tool call (the second review of 283b4379d, finding 2)."""
    if not payload.get("agent_id"):
        return False
    called = []

    def write(text):
        called.append(True)
        emit(text)

    try:
        review = load("richos_app_review", ENGINE / "scripts/lib/app_review.py")
        # No review has ever been written: nothing to tell, and no receipt is read for it.
        if not os.path.isfile(review.app_paths(root)["ledger"]):
            return False
        spaces = work.worker_spaces(active, payload)
        if not spaces:
            return False
        return review.mid_job_notice(root, spaces, write)
    except Exception as error:  # noqa: BLE001 -- a verdict that cannot be read never blocks the work
        if called:
            raise
        print(f"RichOS desktop engine: a mid-job review could not be read: {error}", file=sys.stderr)
        return False


def held_work_context(work, root, payload):
    """NO SILENT HOLDS in the app (richos-hq docs/plans/2026-10-22-no-silent-holds.md):
    every turn of the app's Rich opens with the hold lines the terminal's workspace gate
    shows at a turn end, as context in his own turn, and his instruction file (app/agents/
    rich.md) has him tell the user. Only the conversation lease: Stu's turns carry none.
    The registry is this conversation's own partition (RICHOS_WORKSPACES_DIR), so every
    hold in it is this conversation's, whichever lease made it."""
    session = str(payload.get("session_id") or "")
    try:
        role = json.loads((root / "evidence" / session / "binding.json").read_text()).get("role")
    except (OSError, ValueError, AttributeError):
        role = None
    if role not in (None, "rich"):
        return
    try:
        lines = work.W.hold_lines("")
    except Exception as error:  # noqa: BLE001 -- an unread list is said, never silence
        lines = [f"HOLDS NOT READ this turn ({type(error).__name__}: {str(error)[:160]})"]
    if lines:
        print(json.dumps({"hookSpecificOutput": {"hookEventName": "UserPromptSubmit",
                                                 "additionalContext": "\n".join(lines)}}))


def handle(payload):
    root = Path(os.environ["RICHOS_APP_STATE"])
    # Recovery, independent reviews and both admission paths share this ledger.
    os.environ["SECOND_REVIEW_STATE_DIR"] = str(root / "second-review")
    coordination = Path(os.environ["RICHOS_ENTITY_ROOT"])
    if not root.is_absolute() or not coordination.is_absolute():
        raise ValueError("desktop roots must be explicit and absolute")
    event = payload.get("hook_event_name")
    presentation = event in ("PreToolUse", "PostToolUse", "PostToolUseFailure") and payload.get("tool_name") == "StructuredOutput" and not payload.get("agent_id")
    if event == "PreToolUse" and payload.get("tool_name") == "SendMessage":
        pause_protocol = load("richos_pause_protocol", ENGINE / "scripts/lib/pause_protocol.py")
        pause_protocol.validate_payload(payload)
    if event == "PreToolUse":
        active = scope()
        if active.get("actions_allowed") is not True and not presentation \
                and not a_worker_this_lease_already_dispatched(active, payload):
            raise ValueError("This app turn is stopped or is supplying context. New actions are unavailable.")
    evidence = load("richos_app_evidence", ENGINE / "scripts/lib/app-evidence.py")
    instruction = None
    if event in ("UserPromptSubmit", "PreToolUse"):
        active = scope()
        if active.get("actions_allowed") is True and active.get("binding", {}).get("session_id") == payload.get("session_id"):
            instruction = active.get("user_instruction")
    # `root` is <data>/engine-state; its parent is the app's data directory, which the output
    # record's command witness never lists (Output side panel PRD §4.1 (c)) -- except the worker
    # worktrees under it, where every back-end worker works (the carve-out, slice S2b).
    payload = evidence.capture(payload, root / "evidence", instruction, app_data=root.parent,
                               worktrees=root / "target-worktrees")
    # Local response formatting performs no work and grants no tool authority.
    if presentation:
        return
    ws = [sys.executable, str(ENGINE / "mega-lander/workspaces.py"), "--entity", str(coordination)]
    work = load("richos_desktop_work", ENGINE / "mega-lander/app.py")
    if event == "UserPromptSubmit":
        held_work_context(work, root, payload)
    if event == "PreToolUse":
        run(["/bin/bash", str(ENGINE / "scripts/hooks/guard-sealed-worktree.sh")], payload)
        work.validate_shell_target(payload)
        context = work.worker_context(active, payload)
        if not payload.get("agent_id"):
            recovery, host = work.W._recovery_adapter()
            notice = recovery.cancellation_notice(host, str(payload.get("session_id") or ""))
            if notice:
                context = context or {"hookSpecificOutput":{"hookEventName":"PreToolUse"}}
                context["hookSpecificOutput"]["additionalContext"] = notice

        def emit(notice):
            # One JSON answer per hook call: the verdict joins the worker's host context.
            answer = context or {"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": ""}}
            out = answer["hookSpecificOutput"]
            out["additionalContext"] = (out.get("additionalContext") + "\n\n" if out.get("additionalContext") else "") + notice
            sys.stdout.write(json.dumps(answer) + "\n")
            sys.stdout.flush()
        if not mid_job_notice(work, active, payload, root, emit) and context and payload.get("tool_name") != "Agent":
            print(json.dumps(context))
    if event == "PreToolUse" and payload.get("tool_name") == "Agent":
        payload["richos_recovery_assignment"] = work.dispatch_intent(active, payload, inspect_only=True)
        # WHICH GUARDS JUDGE A DISPATCH THE APP MAKES — read from
        # spawn-guard-audience.declaration, never typed here.
        #
        # This list was two hand-written paths until 2026-09-18:
        # guard-worktree-isolation.sh and guard-brief-scope.sh. Nothing said why
        # those two, nothing connected them to the identical list in
        # `EngineProfile::prepare`'s spawn-preflight.json, and the second of them
        # is an operator-session guard — it judges a dispatch against a
        # design-round specification recorded in the DEVELOPMENT project, with an
        # escape line only an operator can write. It was silent on a user's own
        # repository only because a user's repository happens to carry no such
        # record. Rich's ruling (CEO §57) is that the app is judged only by
        # guards whose reason protects the user's own work, so it is off this
        # list and the classification is the source of what is on it.
        #
        # guard-sealed-worktree.sh is user-work and is deliberately NOT repeated
        # here: it ran above, for every tool, exactly as its matcherless
        # registration says it should.
        audience = load("richos_spawn_guard_audience", ENGINE / "scripts/lib/spawn-guard-audience.py")
        try:
            run(["/bin/bash", str(ENGINE / "scripts/hooks/guard-pierce.sh")], payload, timeout=315)
        except RuntimeError:
            work.dispatch_intent(active, payload, review_refused=True)
            raise
        for guard in audience.user_work_ids():
            if guard in ("guard-sealed-worktree.sh", "guard-pierce.sh"):
                continue
            run(["/bin/bash", str(ENGINE / "scripts/hooks" / guard)], payload)
        # Only a call that passed every control is a dispatch attempt. In particular,
        # returning findings must leave the prepared receipt available for dismissal.
        work.dispatch_intent(active, payload)
        answer = context or {"hookSpecificOutput": {"hookEventName": "PreToolUse"}}
        answer["hookSpecificOutput"]["updatedInput"] = payload["tool_input"]
        print(json.dumps(answer))
    if event in ("SessionStart", "SubagentStart", "SubagentStop", "PostToolUse", "PostToolUseFailure", "SessionEnd"):
        run(ws + ["hook"], payload)
    if event == "PostToolUse" and payload.get("tool_name") == "Agent":
        run(["/bin/bash", str(ENGINE / "scripts/hooks/detect-nonnative-worktree.sh")], payload)
    if event in ("SubagentStart", "SubagentStop", "PostToolUse", "PostToolUseFailure"):
        work.observe(scope(), payload)
    if event == "Stop":
        # A REPLY THAT HOLDS WORK OPENS A HOLD here too (second review
        # rv-20261022T013056Z-12c77c90-1291, finding 1): the same analyzer as the terminal's
        # deferral notice, writing into this conversation's registry, whose lines open Rich's
        # next turn. Its notice text is the terminal's; nothing of it is printed here.
        held = subprocess.run([sys.executable, str(ENGINE / "scripts/hooks/guard-unasked-deferral.py")],
                              input=json.dumps(payload), text=True, capture_output=True, timeout=20)
        if held.returncode not in (0, 3, 4):
            print(f"RichOS desktop engine: the hold check failed (exit {held.returncode}): "
                  f"{held.stderr.strip()[-300:]}", file=sys.stderr)
        # Context-only priming is host machinery, not a new assignment or promise.
        # The same host grant controls mutation tools and the visible-turn analyzer.
        active = scope()
        if active.get("actions_allowed") is True:
            run([sys.executable, str(ENGINE / "ass-kicker/guard-stated-actions.py")], payload)


if __name__ == "__main__":
    try:
        handle(json.load(sys.stdin))
    except Exception as error:
        print(f"RichOS desktop engine: {error}", file=sys.stderr)
        raise SystemExit(2)
