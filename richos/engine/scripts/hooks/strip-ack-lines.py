#!/usr/bin/env python3
"""Keep acknowledgment lines away from the teammate that is spawned.

The spawn guards (PreToolUse[Agent]) read acknowledgment lines such as
`owned-state-ack: <reason>` from the Agent call's prompt. They tell the guard
something and the teammate nothing about its job. The host runs every matching
PreToolUse hook in parallel and hands each the ORIGINAL tool input, so this
transformer can return an updatedInput with those lines removed while every guard
still reads them from the original. The teammate boots on the stripped prompt.

Lines the teammate needs stay: `cross-repo-worktree:` names its workspace.
Only whole lines that begin with one of the prefixes below are removed (the same
anchoring the guards use), so a sentence that merely mentions one is untouched.
"""
import json
import re
import sys

# Every prefix a PreToolUse[Agent] guard requires or accepts as an acknowledgment.
# Source guard in comments (scripts/hooks/<name>.sh unless noted).
ACK_PREFIXES = (
    "hand-roll-ack",          # guard-worktree-isolation
    "model-downgrade-ack",    # guard-worktree-isolation
    "main-checkout-run",      # guard-worktree-isolation, verify-agent-prompt, guard-owned-state
    "definition-drift-ack",   # guard-definition-drift
    "conceal-ack",            # verify-agent-prompt
    "data-contract-bypass",   # verify-agent-prompt, guard-stale-staging
    "no-inflight-ack",        # verify-agent-prompt
    "ceo-todos-deferred",     # guard-model-ceiling, guard-owned-state
    "model-ceiling-ack",      # guard-model-ceiling
    "stale-staging-ack",      # guard-stale-staging
    "owned-state-ack",        # guard-owned-state
    "already-done-ack",       # guard-brief-scope
    "public-record-ack",      # guard-public-record-repo
    "reference",              # guard-reference-ledger
    "full-suite-ack",         # femcboost scripts/hooks/guard-brief-verification-scope.sh
)
ACK_LINE = re.compile(r"^[ \t]*(?:%s):" % "|".join(re.escape(p) for p in ACK_PREFIXES), re.I)


def strip(prompt):
    """The prompt without acknowledgment lines, and without the double gap they leave."""
    kept = [line for line in prompt.split("\n") if not ACK_LINE.match(line)]
    text = "\n".join(kept)
    if text != prompt:
        text = re.sub(r"\n{3,}", "\n\n", text)
    return text


def main():
    try:
        payload = json.load(sys.stdin)
    except (ValueError, OSError):
        return 0
    tool_input = payload.get("tool_input") if isinstance(payload, dict) else None
    if not isinstance(tool_input, dict) or not isinstance(tool_input.get("prompt"), str):
        return 0
    stripped = strip(tool_input["prompt"])
    if stripped == tool_input["prompt"]:
        return 0
    print(json.dumps({"hookSpecificOutput": {
        "hookEventName": "PreToolUse",
        "updatedInput": dict(tool_input, prompt=stripped),
    }}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
