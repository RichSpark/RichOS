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
import json
import os
import random
import subprocess
import sys
import threading
import uuid

ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
ap.add_argument("--cwd", required=True, help="an empty folder the conversation runs in")
ap.add_argument("--claude", default=os.path.expanduser("~/.local/bin/claude"))
ap.add_argument("--model", default="opus")
ap.add_argument("--words", type=int, default=3000, help="words in the first message's body")
ap.add_argument("--wait", type=int, default=600, help="seconds to wait for a turn's result")
a = ap.parse_args()
os.makedirs(a.cwd, exist_ok=True)
if not os.access(a.claude, os.X_OK):
    print("no claude to ask: %s" % a.claude)
    sys.exit(2)

args = [a.claude, "--print", "--input-format=stream-json", "--output-format=stream-json",
        "--include-partial-messages", "--verbose", "--setting-sources", "",
        "--disallowed-tools", "AskUserQuestion", "--permission-mode", "bypassPermissions",
        "--session-id", str(uuid.uuid4()), "--model", a.model, "--effort", "low"]
p = subprocess.Popen(args, cwd=a.cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                     stderr=subprocess.DEVNULL, text=True, bufsize=1)

SIDES = ("input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens")
turn = {"last": None, "requests": 0, "result": None, "window": None}
answers = {}
got = threading.Event()
answered = threading.Event()


def side(usage):
    return sum(int(usage.get(k) or 0) for k in SIDES)


def reader():
    for line in p.stdout:
        try:
            o = json.loads(line)
        except ValueError:
            continue
        if o.get("parent_tool_use_id"):
            continue
        if o.get("type") == "stream_event":
            ev = o.get("event") or {}
            if ev.get("type") == "message_start":
                turn["requests"] += 1
            if ev.get("type") == "message_delta" and ev.get("usage"):
                turn["last"] = side(ev["usage"])
        if o.get("type") == "result":
            turn["result"] = side(o.get("usage") or {})
            for model in (o.get("modelUsage") or {}).values():
                turn["window"] = model.get("contextWindow") or turn["window"]
            got.set()
        if o.get("type") == "control_response":
            r = o.get("response") or {}
            answers[r.get("request_id")] = r
            answered.set()


threading.Thread(target=reader, daemon=True).start()


def send(o):
    p.stdin.write(json.dumps(o) + "\n")
    p.stdin.flush()


def ask(text):
    turn.update(last=None, requests=0, result=None)
    got.clear()
    send({"type": "user", "uuid": str(uuid.uuid4()), "message": {"role": "user", "content": text}})
    if not got.wait(timeout=a.wait):
        print("no result within %d s" % a.wait)
        p.kill()
        sys.exit(2)
    rid = str(uuid.uuid4())
    answered.clear()
    send({"type": "control_request", "request_id": rid, "request": {"subtype": "get_context_usage"}})
    while rid not in answers and answered.wait(timeout=min(a.wait, 60)):
        answered.clear()
    said = ((answers.get(rid) or {}).get("response") or {}).get("totalTokens")
    return dict(turn), said


rnd = random.Random()
body = " ".join("w%d" % rnd.randint(1000, 9999) for _ in range(a.words))
rows = []
try:
    send({"type": "control_request", "request_id": "init", "request": {"subtype": "initialize", "hooks": {}}})
    rows.append(("first", *ask("Keep this list for later and reply with the single word ok: " + body)))
    rows.append(("tool", *ask("Run the shell command `echo richos-probe` once, then reply with its output only.")))
    rows.append(("plain", *ask("Reply with the single word ok.")))
finally:
    p.stdin.close()
    try:
        p.wait(timeout=20)
    except subprocess.TimeoutExpired:
        p.kill()

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
sys.exit(verdict)
