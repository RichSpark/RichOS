#!/usr/bin/env python3
"""Measure what one Stop does to Claude Code on its own wire: the command, the turn, a worker.

The lead session is started exactly as the app starts a lease (`native.rs::child_args`:
`--print`, stream-json both ways, `--setting-sources ''`), in an empty fixture directory under
`--out`, on the default Claude login (its transcript lands under that fixture path's project
folder; no settings, hooks or plugins of the developer's install load). The lead is asked to
start ONE background worker (the Agent tool, `run_in_background: true`) whose command sleeps
30 s before it answers, and then to run a 60-s command (a script whose child is `sleep 60`) in
the FOREGROUND. Once that foreground call is running, the probe sends one Stop:

  interrupt    `control_request{interrupt}`: what the app's conversation Stop sends a lease
               (`native.rs` NativeCancelHandle::cancel)
  stop_task    `control_request{stop_task, task_id}` for the foreground call's own task: what a
               job's Stop sends (`work_host.rs` stop_own -> `native.rs` stop_background_command)
  none         nothing (control)

Measured on 2.1.314, 2026-10-30: `interrupt` ended the command, the turn
(`error_during_execution`, `aborted_tools`) AND the background worker (`stopped` 8 ms later);
`stop_task` ended only the command (`Exit code 137 [killed]`), the turn went on to `end_turn`,
and the worker finished its sleep and answered.

and then records, from the stream alone, whether the foreground command ended, whether the
turn ended, and whether the worker still finished and delivered its answer. Every frame is
kept in `<out>/frames.jsonl`; the verdicts are in `<out>/summary.json`.

Usage: stop-probe.py --mode interrupt|stop_task|none --out DIR [--model sonnet] [--claude PATH]

Exit 0 with the verdict printed when the Stop was sent and watched; exit 2 with a sentence when
nothing was measured (no claude, the foreground call never started, or it had no task to stop).

The probe owns exactly one process tree, the `claude` it spawns; it ends that tree on exit by
the PID it captured at spawn, never by name.
"""
import argparse, json, os, signal, subprocess, sys, threading, time, uuid
from pathlib import Path

WORKER_ANSWER = "WORKER-STILL-HERE"
WORKER_SLEEP = 30
# Claude Code refuses a bare `sleep N` ("Blocked: standalone sleep"), so the long command is a
# script whose child is `sleep N`: the process the probe looks for in its own tree.
TASK_SCRIPT = 'sleep "$1"\n'
FOREGROUND = "bash ./long-task.sh 60"
WORKER_CMD = f"bash ./long-task.sh {WORKER_SLEEP}"

PROMPT = f"""This is a test of a tool harness. Do exactly these two steps, in order, with no other tool calls:
1. Call the Agent tool ONCE with subagent_type "general-purpose", description "probe worker", run_in_background true, and this prompt: "Run the Bash command `{WORKER_CMD}` in the foreground (do not set run_in_background). When it returns, reply with exactly {WORKER_ANSWER} and nothing else."
2. Immediately after the Agent call returns, call the Bash tool ONCE with command `{FOREGROUND}`, description "probe foreground", timeout 120000, and do NOT set run_in_background.
After step 2 returns, reply with the single word DONE. Do not wait for the worker."""


def lead_args(claude, session_id, model):
    return [claude, "--print", "--input-format=stream-json", "--output-format=stream-json",
            "--include-partial-messages", "--verbose", "--setting-sources", "",
            "--disallowed-tools", "AskUserQuestion", "--session-id", session_id,
            "--dangerously-skip-permissions", "--model", model, "--effort", "low"]


def clean_env():
    env = dict(os.environ)
    # The probe's own host session must not leak into the child it measures.
    for key in list(env):
        if key == "CLAUDECODE" or key.startswith("CLAUDE_CODE_") or key in (
                "CLAUDE_PID", "CLAUDE_EFFORT", "RICHOS_AGENT_OWNER", "RICHOS_AGENT_SESSION"):
            env.pop(key)
    return env


def descendants(root):
    out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,command="], capture_output=True, text=True).stdout
    kids, cmd = {}, {}
    for line in out.splitlines():
        parts = line.split(None, 2)
        if len(parts) < 2:
            continue
        pid, ppid = int(parts[0]), int(parts[1])
        kids.setdefault(ppid, []).append(pid)
        cmd[pid] = parts[2] if len(parts) > 2 else ""
    seen, todo = [], [root]
    while todo:
        p = todo.pop()
        for c in kids.get(p, []):
            seen.append(c)
            todo.append(c)
    return {p: cmd.get(p, "") for p in seen}


def sleeps(root):
    return sorted({c.strip() for c in descendants(root).values() if c.strip().startswith("sleep ")})


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--mode", choices=["interrupt", "stop_task", "none"], required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--model", default="sonnet")
    ap.add_argument("--stop-after", type=float, default=8.0, help="seconds after the foreground call starts")
    ap.add_argument("--observe", type=float, default=WORKER_SLEEP + 60.0)
    ap.add_argument("--start-within", type=float, default=120.0, help="seconds to wait for the foreground call")
    ap.add_argument("--claude", default="claude", help="the claude executable (default: claude on PATH)")
    a = ap.parse_args()
    import shutil
    if not shutil.which(a.claude):
        print(f"stop-probe: {a.claude} is not an executable claude; nothing was measured", file=sys.stderr)
        return 2
    out = Path(a.out); out.mkdir(parents=True, exist_ok=True)
    work = out / "fixture"; work.mkdir(exist_ok=True)
    (work / "long-task.sh").write_text(TASK_SCRIPT)
    session = str(uuid.uuid4())
    t0 = time.time()
    proc = subprocess.Popen(lead_args(a.claude, session, a.model), cwd=work, env=clean_env(), stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=open(out / "stderr.txt", "w"), text=True,
                            start_new_session=True)
    frames_file = open(out / "frames.jsonl", "w")
    lock = threading.Lock()
    st = {"bash_tool": None, "bash_started_at": None, "bash_task": None, "agent_tool": None, "agent_task": None,
          "bash_result": None, "results": [], "notifications": [], "control": [], "worker_answer_seen": False,
          "sleeps": []}

    def send(obj):
        line = json.dumps(obj)
        with lock:
            frames_file.write(json.dumps({"t": round(time.time() - t0, 3), "dir": "out", "frame": obj}) + "\n")
            frames_file.flush()
        proc.stdin.write(line + "\n"); proc.stdin.flush()

    def reader():
        for line in proc.stdout:
            try:
                f = json.loads(line)
            except ValueError:
                continue
            t = round(time.time() - t0, 3)
            with lock:
                frames_file.write(json.dumps({"t": t, "dir": "in", "frame": f}) + "\n"); frames_file.flush()
            ty, sub = f.get("type"), f.get("subtype")
            if ty == "assistant" and not f.get("parent_tool_use_id"):
                for block in f.get("message", {}).get("content", []):
                    if block.get("type") != "tool_use":
                        continue
                    if block.get("name") == "Bash" and FOREGROUND in block.get("input", {}).get("command", ""):
                        st["bash_tool"] = block["id"]; st["bash_started_at"] = t
                    if block.get("name") in ("Agent", "Task"):
                        st["agent_tool"] = block["id"]
            if ty == "user" and not f.get("parent_tool_use_id"):
                for block in f.get("message", {}).get("content", []) or []:
                    if isinstance(block, dict) and block.get("type") == "tool_result" and block.get("tool_use_id") == st["bash_tool"]:
                        st["bash_result"] = {"t": t, "content": str(block.get("content"))[:400], "is_error": block.get("is_error")}
            if ty == "system" and sub == "task_started":
                if f.get("tool_use_id") == st["bash_tool"] and st["bash_tool"]:
                    st["bash_task"] = f.get("task_id"); st["bash_task_at"] = t
                if f.get("tool_use_id") == st["agent_tool"] and st["agent_tool"]:
                    st["agent_task"] = f.get("task_id")
            if ty == "system" and sub == "task_notification":
                st["notifications"].append({"t": t, "task_id": f.get("task_id"), "status": f.get("status"),
                                            "summary": str(f.get("summary"))[:300]})
            if WORKER_ANSWER in line:
                # The worker's own answer: its notification, its text reply, or a later result.
                # Not the Agent call or the worker's prompt, which only quote the word.
                worker_text = ty == "assistant" and f.get("parent_tool_use_id") and any(
                    b.get("type") == "text" and b.get("text", "").strip() == WORKER_ANSWER
                    for b in f.get("message", {}).get("content", []))
                notice = ty == "system" and sub == "task_notification" and str(f.get("summary", "")).strip() == WORKER_ANSWER
                if notice or worker_text:
                    st["worker_answer_seen"] = True
                    st.setdefault("worker_answer_at", t)
            if ty == "result":
                st["results"].append({"t": t, "subtype": sub, "terminal_reason": f.get("terminal_reason"),
                                      "result": str(f.get("result"))[:300]})
            if ty == "control_response":
                st["control"].append({"t": t, "response": f.get("response")})

    threading.Thread(target=reader, daemon=True).start()
    send({"type": "control_request", "request_id": "probe_init", "request": {"subtype": "initialize"}})
    send({"type": "user", "uuid": str(uuid.uuid4()), "message": {"role": "user", "content": [{"type": "text", "text": PROMPT}]}})

    # Wait for the foreground call, then for its own task id (bounded).
    deadline = time.time() + a.start_within
    while time.time() < deadline and not st["bash_started_at"]:
        time.sleep(0.1)
    if not st["bash_started_at"]:
        st["error"] = "the foreground call never started"
    else:
        fire_at = t0 + st["bash_started_at"] + a.stop_after
        while time.time() < fire_at:
            time.sleep(0.05)
        st["sleeps_before_stop"] = sleeps(proc.pid)
        st["stop_sent_at"] = round(time.time() - t0, 3)
        if a.mode == "interrupt":
            send({"type": "control_request", "request_id": "probe_interrupt", "request": {"subtype": "interrupt"}})
        elif a.mode == "stop_task":
            if st["bash_task"]:
                send({"type": "control_request", "request_id": "probe_stop_task",
                      "request": {"subtype": "stop_task", "task_id": st["bash_task"]}})
            else:
                st["error"] = "the foreground call had no task id to stop"
        time.sleep(3)
        st["sleeps_3s_after_stop"] = sleeps(proc.pid)
        end = time.time() + a.observe
        while time.time() < end:
            if any(n["task_id"] == st["agent_task"] and n["status"] != "running" for n in st["notifications"]) \
                    and len(st["results"]) >= 1 and st["bash_result"]:
                break
            time.sleep(0.5)
        time.sleep(min(5.0, a.observe))
    st["sleeps_at_end"] = sleeps(proc.pid)
    # End the one process tree this probe owns, by the PID captured at spawn.
    try:
        proc.stdin.close()
    except OSError:
        pass
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGTERM)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
    worker = [n for n in st["notifications"] if n["task_id"] == st["agent_task"]]
    summary = {
        "claude_version": subprocess.run([a.claude, "--version"], capture_output=True, text=True).stdout.strip(),
        "mode": a.mode, "model": a.model, "session": session, **st,
        "verdict": {
            "foreground_had_task_id": bool(st["bash_task"]),
            "foreground_task_after_call_s": (round(st["bash_task_at"] - st["bash_started_at"], 3)
                                             if st.get("bash_task_at") and st["bash_started_at"] else None),
            "foreground_ended_within_3s_of_stop": "sleep 60" not in st.get("sleeps_3s_after_stop", ["sleep 60"]),
            "worker_sleep_alive_3s_after_stop": f"sleep {WORKER_SLEEP}" in st.get("sleeps_3s_after_stop", []),
            "worker_final_status": worker[-1]["status"] if worker else None,
            "worker_answer_in_stream": st["worker_answer_seen"],
        },
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=1))
    print(json.dumps(summary["verdict"], indent=1))
    if st.get("error"):
        print(f"stop-probe: {st['error']}; this run measured nothing about Stop ({out}/frames.jsonl)", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
