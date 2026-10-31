#!/usr/bin/env python3
"""Single desktop hook registration; canonical components retain their decisions.

The host supplies all roots and the active scope. There is no terminal-settings
fallback. Provider callbacks are evidence, not proof of assignment completion.
"""
import concurrent.futures
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shlex
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


def registered_timeout(guard, floor=20):
    """The budget the engine's own registration (hooks/hooks.json, the terminal's) gives this
    Agent guard, never less than `floor`. guard-worktree-isolation.sh waits up to 30 s for the
    registry lock and runs Pierce inside register-spawn, so a typed 20 s killed real dispatches
    (follow-up "app work tools refused dispatches for unexplained reasons", 2026-10-28)."""
    try:
        groups = json.loads((ENGINE / "hooks/hooks.json").read_text())["hooks"]["PreToolUse"]
        found = [entry["timeout"] for group in groups if group.get("matcher") == "Agent"
                 for entry in group["hooks"] if entry["command"].endswith("/" + guard)]
    except (OSError, ValueError, KeyError, TypeError):
        found = []
    return max([floor, *found])


# THE TURN-END CHECKS THE APP LEAVES OUT, each with the terminal-only thing it needs. Every other
# Stop check the engine's registration (hooks/hooks.json, the terminal's) runs is run at Rich's and
# Stu's turn end too (the CEO's §154; Hank's audit finding 8, richos-hq docs/research/
# 2026-10-31-app-terminal-differences.md). A name here that hooks.json no longer registers fails
# test-app-engine-hook.py, so this list cannot go stale quietly.
TERMINAL_ONLY_TURN_END = {
    "notice-hook-staleness.sh": (
        "It compares the hooks registered now with the snapshot the terminal's SessionStart hook "
        "(snapshot-enforcing-hooks.sh) takes, and its remedy is restarting the terminal session. An app "
        "lease's hooks are rendered fresh from the engine the app ships, at every lease start, so there is "
        "no snapshot to compare and nothing a restart would arm."),
}


def registered(event):
    """[(file name, argv, timeout)] for every command the engine's own registration (hooks/hooks.json,
    the terminal's) runs on `event`, in its order, with the terminal's own arguments and budget."""
    audience = load("richos_spawn_guard_audience", ENGINE / "scripts/lib/spawn-guard-audience.py")
    out = []
    for group in json.loads((ENGINE / "hooks/hooks.json").read_text())["hooks"].get(event, []):
        for entry in group["hooks"]:
            argv = shlex.split(entry["command"].replace("${CLAUDE_PLUGIN_ROOT}", str(ENGINE)))
            if argv and argv[0] == "bash":
                argv[0] = "/bin/bash"
            out.append((audience.guard_file_name(entry["command"]), argv, entry.get("timeout", 60)))
    return out


def turn_end_checks(payload, coordination):
    """Run the terminal's Stop checks at an app lead's turn end, together, as the terminal runs them,
    each within its registered budget. Returns (contexts for the lead, messages for the person,
    refusals). A check's answer is read the way Claude Code reads it: exit 2 refuses with its stderr,
    a JSON `decision: block` refuses with its reason, `additionalContext` and `systemMessage` are
    kept; any other exit is a non-blocking error, logged on stderr."""
    checks = [check for check in registered("Stop") if check[0] not in TERMINAL_ONLY_TURN_END]
    env = dict(os.environ, RICHOS_ENTITY_ROOT=str(coordination), CLAUDE_PLUGIN_ROOT=str(ENGINE))
    # The lease's team folders are the app's (engine_profile.rs gives the stated-actions check
    # RICHOS_SA_TEAMS_DIR); the claim check reads the same folders under its own name, and without
    # them it looks in Claude Code's ~/.claude/teams, which an app lease never writes.
    if os.environ.get("RICHOS_SA_TEAMS_DIR"):
        env.setdefault("RICHOS_CLAIMS_TEAMS_DIR", os.environ["RICHOS_SA_TEAMS_DIR"])
    body = json.dumps(payload)

    def one(check):
        name, argv, timeout = check
        try:
            done = subprocess.run(argv, input=body, text=True, capture_output=True, timeout=timeout, env=env)
            return name, done.returncode, done.stdout, done.stderr
        except subprocess.TimeoutExpired:
            return name, None, "", "%s did not finish inside its registered %s s\n" % (name, timeout)
        except OSError as error:
            return name, None, "", "%s could not start: %s\n" % (name, error)

    contexts, messages, refusals = [], [], []
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, len(checks))) as pool:
        results = list(pool.map(one, checks))
    for name, code, out, err in results:
        for line in out.splitlines():
            try:
                answer = json.loads(line)
            except ValueError:
                answer = None
            if not isinstance(answer, dict):
                if line.strip():
                    sys.stderr.write(line + "\n")
                continue
            context = (answer.get("hookSpecificOutput") or {}).get("additionalContext")
            if context:
                contexts.append(context)
            if answer.get("systemMessage"):
                messages.append(answer["systemMessage"])
            if answer.get("decision") == "block":
                refusals.append(answer.get("reason") or "%s refused the turn end" % name)
        if code == 2:
            refusals.append(err.strip() or "%s refused the turn end" % name)
        elif err.strip():
            sys.stderr.write(err if err.endswith("\n") else err + "\n")
    return contexts, messages, refusals


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


def closure_of(active):
    """Which work a closed scope is, who closed its grant and when (scripts/lib/scope_closure.py),
    so a refusal names its actual cause and scope rather than one sentence for every cause
    (follow-up "app work tools refused dispatches for unexplained reasons", 2026-10-28)."""
    try:
        return load("richos_scope_closure", ENGINE / "scripts/lib/scope_closure.py").describe(active)
    except Exception as error:  # noqa: BLE001 -- a cause that cannot be read is said, never hidden
        return "Its cause could not be read (%s: %s)." % (type(error).__name__, str(error)[:200])


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
    """THE FALLBACK FOR DUE WORK in the app (orchestration reliability R5, richos-hq docs/
    prds/2026-10-23-orchestration-reliability.md): while no healthy stall watcher evaluates
    this conversation, Rich's turn opens with the due work (workspaces.fallback_notice, the
    terminal Stop gate's own reading), once per change; with a healthy watcher, nothing.
    The old every-turn hold lines are gone (R6). Only the conversation lease: Stu's turns
    carry none. The registry is this conversation's own partition (RICHOS_WORKSPACES_DIR)."""
    session = str(payload.get("session_id") or "")
    try:
        role = json.loads((root / "evidence" / session / "binding.json").read_text()).get("role")
    except (OSError, ValueError, AttributeError):
        role = None
    if role not in (None, "rich"):
        return
    try:
        text = work.W.fallback_notice(session)
    except Exception as error:  # noqa: BLE001 -- an unread list is said, never silence
        text = f"DUE WORK NOT READ this turn ({type(error).__name__}: {str(error)[:160]})"
    if text:
        print(json.dumps({"hookSpecificOutput": {"hookEventName": "UserPromptSubmit",
                                                 "additionalContext": text}}))


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
            raise ValueError("This app turn is stopped or is supplying context. New actions are unavailable. " + closure_of(active))
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
        # It is every guard the terminal's registration (hooks/hooks.json) runs on
        # a dispatch except the few that need a terminal-only thing (the CEO's
        # §154; Hank's audit finding 7). Each guard reads the user's own
        # configuration through the coordination folder and stands down where
        # its rule is not declared, as in a terminal that never adopted it. Its
        # refusal reaches the lead in its own words, through `run` below.
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
            run(["/bin/bash", str(ENGINE / "scripts/hooks" / guard)], payload, timeout=registered_timeout(guard))
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
        # THE TERMINAL'S TURN-END CHECKS, ALL OF THEM BUT THOSE IN TERMINAL_ONLY_TURN_END, as one
        # answer: Claude Code reads a hook's stdout only as one JSON object, and a bare alert line
        # reached nobody. This replaced the app's own short list (the deferral notice, ass-kicker.py
        # called directly and its own due-work fallback, which guard-workspace-gate.sh gives).
        # Context-only priming is host machinery, not a turn of the lead's: the same host grant that
        # controls mutation tools decides whether the turn is judged.
        if scope().get("actions_allowed") is not True:
            return
        contexts, messages, refusals = turn_end_checks(payload, coordination)
        if refusals:
            # Exit 2 refuses the turn end and hands stderr to the lead, as each refusing check does in
            # the terminal; the notices of the same turn end go with it rather than being dropped.
            sys.stderr.write("\n\n".join(refusals + contexts) + "\n")
            raise RuntimeError("Required engine control failed or refused (exit 2)")
        answer = {}
        if messages:
            answer["systemMessage"] = "\n\n".join(messages)
        if contexts:
            answer["hookSpecificOutput"] = {"hookEventName": "Stop", "additionalContext": "\n\n".join(contexts)}
        if answer:
            print(json.dumps(answer))


DECISION_RANK = {"allow": 1, "ask": 2, "defer": 3, "deny": 4}


def one_envelope(text):
    """Everything a PreToolUse call printed, as ONE hook answer, or None when it printed nothing.

    Claude Code acts on a hook's stdout only when it is one JSON object: measured 2026-10-21 on
    2.1.306, an Agent call whose hook printed a context object and then the answer kept its
    original prompt, and the same answer printed alone changed it. The canonical guards this
    hook runs forward their own output, so an Agent dispatch printed several objects and the
    `updatedInput` below (the receipt's name, the repository's own instruction file) never
    reached the teammate. Here every context is kept, the strictest decision wins, and the last
    `updatedInput` (this hook's own, written after every guard) is the one passed on. A line that
    is not a JSON object goes to stderr, where it is kept for whoever reads the log."""
    envelope, contexts, reasons, decision, updated = {}, [], [], None, None
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except ValueError:
            obj = None
        if not isinstance(obj, dict):
            print(line, file=sys.stderr)
            continue
        specific = obj.get("hookSpecificOutput") or {}
        envelope.update({k: v for k, v in obj.items() if k != "hookSpecificOutput"})
        if specific.get("additionalContext"):
            contexts.append(specific["additionalContext"])
        if specific.get("permissionDecisionReason"):
            reasons.append(specific["permissionDecisionReason"])
        if DECISION_RANK.get(specific.get("permissionDecision"), 0) > DECISION_RANK.get(decision, 0):
            decision = specific["permissionDecision"]
        if "updatedInput" in specific:
            updated = specific["updatedInput"]
    if not envelope and not contexts and not reasons and decision is None and updated is None:
        return None
    out = {"hookEventName": "PreToolUse"}
    if decision is not None:
        out["permissionDecision"] = decision
    if reasons:
        out["permissionDecisionReason"] = "\n\n".join(reasons)
    if updated is not None:
        out["updatedInput"] = updated
    if contexts:
        out["additionalContext"] = "\n\n".join(contexts)
    envelope["hookSpecificOutput"] = out
    return envelope


def runtime_before_stubs(environ=os.environ):
    """The bundled runtime's folder before Apple's stub folder on PATH, as scripts/lib/hook-python.sh
    does for the bash hooks: on a Mac without developer tools /usr/bin/git and python3 open the
    install dialog, and the git this process and the hooks it starts run is found by name."""
    runtime = ENGINE / "runtime" / "bin"
    if not (runtime / "python3").is_file():
        return
    stubs = {"/usr/bin", environ.get("RICHOS_TOOLCHAIN_ROOT", "").rstrip("/") + "/usr/bin"}
    for entry in environ.get("PATH", "").split(os.pathsep):
        if entry == str(runtime):
            return
        if entry in stubs:
            break
    environ["PATH"] = str(runtime) + os.pathsep + environ.get("PATH", "")


def main(stdin=sys.stdin, stdout=sys.stdout):
    runtime_before_stubs()
    payload = json.load(stdin)
    if payload.get("hook_event_name") == "PreToolUse" and payload.get("tool_name") == "Agent":
        captured = io.StringIO()
        with contextlib.redirect_stdout(captured):
            handle(payload)
        envelope = one_envelope(captured.getvalue())
        if envelope is not None:
            stdout.write(json.dumps(envelope) + "\n")
            stdout.flush()
    else:
        handle(payload)


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"RichOS desktop engine: {error}", file=sys.stderr)
        raise SystemExit(2)
