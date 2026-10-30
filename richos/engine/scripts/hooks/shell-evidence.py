#!/usr/bin/env python3
"""Preserve ordinary Bash command failures before output filters can hide them.

PreToolUse preserves failure propagation and records subagent ownership. Subagent
CLI commands opt into a six-second foreground grace and native handoff; in the app
(every other entrypoint) an ordinary subagent command runs exactly as the host runs
it, with no added background flag, timeout or wait instruction. Wait calls remain
foreground and explicit background calls remain background. Held calls are denied. No shell parsing, command execution or auto-approval.
Expected failures belong in explicit if/else or || branches. This covers the
calling shell; scripts and explicit failure handling retain their own semantics.
"""
import json
import os
import sys

PREFIX = "set -e -o pipefail\n"
# The rewrite's own bound. hooks.json gives this hook 86400 s so it can hold a held
# agent's wait until its release (agent_hold.gate); the rewrite keeps its old 5 s.
REWRITE_SECONDS = 5


def hold_gate(payload):
    """A held agent's wait call starts only after its release, so no model call is made
    while it waits (agent_hold.gate). Every other call passes at once."""
    try:
        sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "lib"))
        import agent_hold
        agent_hold.gate(payload)
    except Exception:
        pass


def ownership(payload):
    """A subagent's call also records who owns it, so a pause can hold it (scripts/lib/agent_hold.py).

    This hook is the only Bash rewriter, so the capture lives here: two rewriting
    hooks would race for the one updated command. Returns {"command", "input"} or
    None. Any failure leaves the command exactly as it was without capture; the
    pause then cannot hold this call.
    """
    try:
        sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "lib"))
        import agent_hold
        got = agent_hold.rewrite(payload)
        if isinstance(got, dict) and isinstance(got.get("command"), str) and isinstance(got.get("input"), dict):
            return got
    except Exception:
        pass
    return None


def rewrite(payload):
    if not isinstance(payload, dict) or not isinstance(payload.get("tool_name"), str):
        return {"systemMessage": "Shell evidence hook could not read this call (tool identity missing); failure propagation is unverified."}
    if payload["tool_name"] != "Bash":
        return {}
    original = payload.get("tool_input")
    if not isinstance(original, dict) or not isinstance(original.get("command"), str):
        return {"systemMessage": "Shell evidence hook could not read this call (Bash command missing); failure propagation is unverified."}
    command = original["command"]
    if command.startswith(PREFIX) and not payload.get("agent_id"):
        return {}
    owned = ownership(payload)
    if owned and owned.get("deny"):
        return {"hookSpecificOutput": {"hookEventName": "PreToolUse",
                "permissionDecision": "deny", "permissionDecisionReason": owned["deny"]}}
    updated = dict(original, command=PREFIX + command)
    if owned:
        updated.update(owned["input"])
        updated["command"] = PREFIX + owned["command"]
    return {"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "updatedInput": updated,
        "additionalContext": "Shell commands use errexit and pipefail: a failed command or pipeline stops this call. "
        "Handle expected nonzero results explicitly with if/else. Keep JSON stdout separate from stderr. "
        "Read saved logs in a separate call; do not turn a failed check into success with a trailing echo or filter."
        + (" " + owned["context"] if owned and owned.get("context") else "")
    }}


class _Late(Exception):
    pass


def _late(_signum, _frame):
    raise _Late()


if __name__ == "__main__":
    import signal
    try:
        payload = json.load(sys.stdin)
    except (ValueError, OSError):
        payload = None
    if payload is None:
        result = {"systemMessage": "Shell evidence hook could not read this call (invalid payload); failure propagation is unverified."}
    else:
        hold_gate(payload)
        signal.signal(signal.SIGALRM, _late)
        signal.alarm(REWRITE_SECONDS)
        try:
            result = rewrite(payload)
        except _Late:
            # What the harness did at the old 5 s hook timeout: the call runs as typed.
            result = {"systemMessage": "Shell evidence hook ran out of time; failure propagation is unverified."}
        finally:
            signal.alarm(0)
    if result:
        print(json.dumps(result))
