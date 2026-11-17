#!/usr/bin/env python3
"""live-effort-probe.py: does a running Claude Code session take a new effort level, and a new model?

  live-effort-probe.py --cwd EMPTY_DIR [--claude BIN] [--from fable:low] [--to opus:high] [--projects DIR]

Starts ONE `claude` the way the RichOS app starts a lease (richos-core native.rs `child_args`:
--print, stream-json both ways, --verbose, --setting-sources '') with --from's model and effort,
runs one turn, sends the two control requests the app sends when Settings closes on a changed
model or effort (`set_model`, then `apply_flag_settings` with `effortLevel`; native.rs
`NativeCancelHandle::set_model` and `set_effort`), and runs a second turn. It then reads the
session's own transcript (under --projects, Claude Code's `~/.claude/projects` by default) and
prints, per turn, the model that answered and the effort the row records:

  turn    sent            answered_by         effort

Exit 0 when the first turn was answered at --from and the second at --to (the model's name
carries the alias, the row's effort is the level); 1 when either was not; 2 when a control
request was refused or unanswered, or the transcript holds no answer to read. Nothing is shown
on screen and nothing plays. Spends a little quota; run it again after a Claude Code update
(the app's live effort change rests on it).
"""
import argparse
import glob
import json
import os
import subprocess
import sys
import threading
import uuid

ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
ap.add_argument("--cwd", required=True, help="an empty folder the conversation runs in")
ap.add_argument("--claude", default=os.path.expanduser("~/.local/bin/claude"))
ap.add_argument("--from", dest="start", default="fable:low", help="model:effort the session starts with")
ap.add_argument("--to", default="opus:high", help="model:effort sent to the running session")
ap.add_argument("--projects", default=os.path.expanduser("~/.claude/projects"), help="where Claude Code keeps its transcripts")
ap.add_argument("--wait", type=int, default=600, help="seconds to wait for a turn's result")
a = ap.parse_args()
os.makedirs(a.cwd, exist_ok=True)
if not os.access(a.claude, os.X_OK):
    print("no claude to ask: %s" % a.claude)
    sys.exit(2)
start_model, start_effort = a.start.split(":")
to_model, to_effort = a.to.split(":")
session = str(uuid.uuid4())

args = [a.claude, "--print", "--input-format=stream-json", "--output-format=stream-json",
        "--verbose", "--setting-sources", "", "--disallowed-tools", "AskUserQuestion",
        "--permission-mode", "bypassPermissions", "--session-id", session,
        "--model", start_model, "--effort", start_effort]
p = subprocess.Popen(args, cwd=a.cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                     stderr=subprocess.DEVNULL, text=True, bufsize=1)

answers = {}
got = threading.Event()
answered = threading.Event()


def reader():
    for line in p.stdout:
        try:
            o = json.loads(line)
        except ValueError:
            continue
        if o.get("type") == "result":
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
    got.clear()
    send({"type": "user", "uuid": str(uuid.uuid4()), "message": {"role": "user", "content": text}})
    if not got.wait(timeout=a.wait):
        print("no result within %d s" % a.wait)
        p.kill()
        sys.exit(2)


def control(request):
    rid = str(uuid.uuid4())
    answered.clear()
    send({"type": "control_request", "request_id": rid, "request": request})
    while rid not in answers and answered.wait(timeout=60):
        answered.clear()
    r = answers.get(rid)
    if not r or r.get("subtype") != "success":
        print("control request %s was not confirmed: %s" % (request.get("subtype"), json.dumps(r)))
        p.kill()
        sys.exit(2)


try:
    send({"type": "control_request", "request_id": "init", "request": {"subtype": "initialize", "hooks": {}}})
    ask("Reply with the single word one.")
    control({"subtype": "set_model", "model": to_model})
    control({"subtype": "apply_flag_settings", "settings": {"effortLevel": to_effort}})
    ask("Reply with the single word two.")
finally:
    p.stdin.close()
    try:
        p.wait(timeout=20)
    except subprocess.TimeoutExpired:
        p.kill()

rows = []
for path in glob.glob(os.path.join(a.projects, "*", session + ".jsonl")):
    for line in open(path, errors="replace"):
        try:
            row = json.loads(line)
        except ValueError:
            continue
        model = (row.get("message") or {}).get("model")
        if row.get("type") == "assistant" and model and model != "<synthetic>":
            rows.append((model, row.get("effort")))
seen = []
for row in rows:
    if not seen or seen[-1] != row:
        seen.append(row)
print("turn\tsent\tanswered_by\teffort")
want = [(start_model, start_effort), (to_model, to_effort)]
for i, (model, effort) in enumerate(seen):
    sent = "%s:%s" % want[i] if i < len(want) else "?"
    print("%d\t%s\t%s\t%s" % (i + 1, sent, model, effort))
if len(seen) < 2:
    print("verdict: the transcript holds %d distinct answer(s), so there is nothing to compare" % len(seen))
    sys.exit(2)
ok = len(seen) == 2 and all(alias in model and effort == level for (model, effort), (alias, level) in zip(seen, want))
print("verdict: " + ("the running session took the new model and effort" if ok
                     else "the running session did NOT answer at what was sent"))
sys.exit(0 if ok else 1)
