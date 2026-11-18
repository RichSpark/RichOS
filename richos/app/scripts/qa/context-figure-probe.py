#!/usr/bin/env python3
"""context-figure-probe.py: which figure on Claude Code's stream is a conversation's context size?

  context-figure-probe.py --cwd EMPTY_DIR [--claude BIN] [--model opus] [--words 3000]

Starts ONE `claude` the way the RichOS app starts a lease (richos-core native.rs `child_args`:
--print, stream-json both ways, --include-partial-messages, --verbose, --setting-sources ''),
in EMPTY_DIR, and runs three turns: a first message with a random body, a turn that makes one
tool call (so the turn holds two model requests), and a plain follow-up. After every turn it asks
Claude Code for its own figure (the `get_context_usage` control request, what `/context` shows)
and prints one row:

  turn  last_request  result_usage  claude_code_says  window

  last_request      input + cache read + cache creation of the LAST model request of the turn
                    (the lead's last `message_delta`), what the app's context meter shows
  result_usage      the same three summed off the turn's `result`, which adds up every request
                    of the turn
  claude_code_says  `get_context_usage` -> totalTokens, read right after the turn
  window            `result.modelUsage[model].contextWindow`

Exit 0 when, on every turn, last_request is within 2% of claude_code_says; 1 when it is not;
2 when Claude Code gave no figure to compare with. Nothing is shown on screen and nothing plays.
The app's context meter (richos-core `chat_cache.rs`) rests on this; run it again after a Claude
Code update.
"""
import argparse
import os
from pathlib import Path
import random
import sys
import time
import uuid

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from probe_session import ProbeError, provider

SIDES = ("input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens")


def side(usage):
    return sum(int(usage.get(k) or 0) for k in SIDES)


def ask(probe, text, wait, window=None):
    turn = {"last": None, "requests": 0, "result": None, "window": window}
    for frame in probe.start_turn(text):
        if frame.get("parent_tool_use_id"):
            continue
        if frame.get("type") == "stream_event":
            event = frame.get("event") or {}
            if event.get("type") == "message_start":
                turn["requests"] += 1
            if event.get("type") == "message_delta" and event.get("usage"):
                turn["last"] = side(event["usage"])
        if frame.get("type") == "result":
            turn["result"] = side(frame.get("usage") or {})
            for model in (frame.get("modelUsage") or {}).values():
                turn["window"] = model.get("contextWindow") or turn["window"]
            break
    else:
        raise ProbeError("no result within %d s" % wait)

    answer = probe.control({"subtype": "get_context_usage"})
    if answer is None:
        raise ProbeError("verdict: no figure to compare: Claude Code gave no context size")
    return turn, (answer.get("response") or {}).get("totalTokens")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--cwd", required=True, help="an empty folder the conversation runs in")
    ap.add_argument("--claude", default=os.path.expanduser("~/.local/bin/claude"))
    ap.add_argument("--model", default="opus")
    ap.add_argument("--words", type=int, default=3000, help="words in the first message's body")
    ap.add_argument("--wait", type=int, default=600, help="seconds for each turn and its context request")
    a = ap.parse_args()
    if a.wait <= 0:
        ap.error("--wait must be positive")
    os.makedirs(a.cwd, exist_ok=True)
    if not os.access(a.claude, os.X_OK):
        raise ProbeError("no claude to ask: %s" % a.claude)

    args = [a.claude, "--print", "--input-format=stream-json", "--output-format=stream-json",
            "--include-partial-messages", "--verbose", "--setting-sources", "",
            "--disallowed-tools", "AskUserQuestion", "--permission-mode", "bypassPermissions",
            "--session-id", str(uuid.uuid4()), "--model", a.model, "--effort", "low"]
    rnd = random.Random()
    body = " ".join("w%d" % rnd.randint(1000, 9999) for _ in range(a.words))
    rows = []
    with provider(args, a.cwd, time.monotonic() + a.wait) as probe:
        probe.send({"type": "control_request", "request_id": "init",
                    "request": {"subtype": "initialize", "hooks": {}}}, probe.deadline)
        rows.append(("first", *ask(probe, "Keep this list for later and reply with the single word ok: " + body, a.wait)))
        probe.deadline = time.monotonic() + a.wait
        rows.append(("tool", *ask(probe, "Run the shell command `echo richos-probe` once, then reply with its output only.",
                                  a.wait, rows[-1][1]["window"])))
        probe.deadline = time.monotonic() + a.wait
        rows.append(("plain", *ask(probe, "Reply with the single word ok.", a.wait, rows[-1][1]["window"])))

    print("turn\trequests\tlast_request\tresult_usage\tclaude_code_says\twindow")
    verdict = 0
    for name, t, said in rows:
        print("%s\t%s\t%s\t%s\t%s\t%s" % (name, t["requests"], t["last"], t["result"], said, t["window"]))
        if not said or not t["last"]:
            verdict = max(verdict, 2)
        elif abs(t["last"] - said) > 0.02 * said:
            verdict = max(verdict, 1)
    print("verdict: " + ("the lead's last request is the context size Claude Code reports",
                         "the lead's last request is NOT the context size Claude Code reports",
                         "no figure to compare: Claude Code gave no context size, or a turn made no request")[verdict])
    return verdict

if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ProbeError) as exc:
        print(exc, file=sys.stderr)
        sys.exit(2)
