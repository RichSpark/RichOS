#!/usr/bin/env python3
"""Opt-in live probe of engine delivery through the app's native Claude protocol.

Uses a disposable Git workspace and synthetic plugin. Requires an authenticated
Claude installation. Never loads user/project settings or writes engine state.
Provider responses are kept out of stdout except for the bounded result summary.
"""
import argparse
import json
import os
from pathlib import Path
import queue
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import uuid


def run(claude, timeout):
    with tempfile.TemporaryDirectory(prefix="richos runtime ") as temporary:
        root = Path(temporary).resolve()
        workspace = root / "workspace"
        workspace.mkdir()
        engine = Path(__file__).resolve().parents[2] / "engine"
        (workspace / ".claude/agents").mkdir(parents=True)
        (workspace / ".claude/agents/canary.md").write_text("---\nname: canary\n---\n")
        target = root / "target project"
        target.mkdir()
        for repo in (workspace, target):
            subprocess.run(["git", "init", "-q", "-b", "main", str(repo)], check=True)
            subprocess.run(["git", "-C", str(repo), "config", "core.hooksPath", str(root / "nohooks")], check=True)
            subprocess.run(["git", "-C", str(repo), "-c", "user.name=Runtime Probe", "-c", "user.email=probe@example.invalid",
                            "-c", "commit.gpgSign=false", "commit", "-q", "--allow-empty", "-m", "Synthetic fixture"], check=True)
        worker_tree = root / "target worktrees" / "canary-sonnet-probe"
        plugin = root / "plugin"
        (plugin / ".claude-plugin").mkdir(parents=True)
        (plugin / "agents").mkdir()
        (plugin / "hooks").mkdir()
        (plugin / ".claude-plugin/plugin.json").write_text(json.dumps({
            "name": "runtime-probe", "version": "1.0.0",
            "agents": ["./agents/canary.md"],
        }))
        (plugin / "agents/canary.md").write_text(
            "---\nname: canary\ndescription: Runs the synthetic runtime canary.\n"
            "tools: Bash, Read, Write\n---\n"
            "Work only in the directory specified in the task. Follow the task exactly.\n"
        )
        hook = plugin / "hook.py"
        hook.write_text('''import importlib.util, json, os, subprocess, sys
from pathlib import Path
p=json.load(sys.stdin)
root=Path(os.environ["RICHOS_PROBE_ROOT"])
engine=Path(os.environ["RICHOS_PROBE_ENGINE"])
spec=importlib.util.spec_from_file_location("app_evidence", engine/"scripts/lib/app-evidence.py")
adapter=importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)
p=adapter.capture(p, root/"evidence")
with (root/"hooks.jsonl").open("a") as f:
    f.write(json.dumps(p)+"\\n")
if p.get("hook_event_name")=="PreToolUse" and "HOOK_REFUSE_CANARY" in json.dumps(p.get("tool_input", {})):
    print("Synthetic refusal was enforced. Continue with the allowed canary.", file=sys.stderr)
    sys.exit(2)
event=p.get("hook_event_name")
ws=[sys.executable,str(engine/"mega-lander/workspaces.py"),"--entity",str(root/"workspace")]
if event=="PreToolUse" and p.get("tool_name")=="Agent":
    result=subprocess.run(ws+["register-spawn"],input=json.dumps(p),text=True,capture_output=True)
    if result.returncode:
        sys.stderr.write(result.stderr)
        sys.exit(2)
if event in ("SessionStart","SubagentStart","SubagentStop","PostToolUse","SessionEnd"):
    result=subprocess.run(ws+["hook"],input=json.dumps(p),text=True,capture_output=True)
    if result.returncode:
        sys.stderr.write(result.stderr)
        sys.exit(2)
if event=="Stop":
    result=subprocess.run([sys.executable,str(engine/"ass-kicker/ass-kicker.py")],
                          input=json.dumps(p),text=True,capture_output=True)
    with (root/"guard-results.jsonl").open("a") as f:
        f.write(json.dumps({"exit":result.returncode})+"\\n")
    sys.stdout.write(result.stdout)
    sys.stderr.write(result.stderr)
    sys.exit(result.returncode)
''')
        import shlex
        command = shlex.join([sys.executable, str(hook)])
        events = ["SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure", "SubagentStart", "SubagentStop", "Stop"]
        (plugin / "hooks/hooks.json").write_text(json.dumps({"hooks": {
            event: [{"hooks": [{"type": "command", "command": command, "timeout": 20}]}]
            for event in events
        }}))
        doctrine = root / "doctrine.md"
        doctrine.write_text("This is a disposable runtime test. Work only inside the supplied workspace.\n")
        session = str(uuid.uuid4())
        args = [claude, "--print", "--input-format=stream-json", "--output-format=stream-json",
                "--include-partial-messages", "--verbose", "--setting-sources", "",
                "--no-session-persistence", "--session-id", session,
                "--permission-prompt-tool", "stdio", "--append-system-prompt-file", str(doctrine),
                "--plugin-dir", str(plugin), "--strict-mcp-config", "--mcp-config", '{"mcpServers":{}}']
        env = os.environ.copy()
        env.pop("CLAUDECODE", None)
        env["RICHOS_PROBE_ROOT"] = str(root)
        env["RICHOS_PROBE_ENGINE"] = str(engine)
        env["RICHOS_SA_ENTITY_ROOT"] = str(workspace)
        env["RICHOS_SA_TEAMS_DIR"] = str(root / "teams")
        env["RICHOS_WORKSPACES_DIR"] = str(root / "workspaces-state")
        env["RICHOS_SESSION_ID"] = session
        env["RICHOS_SESSION_PID"] = str(os.getpid())
        for repo in (workspace, target):
            subprocess.run([sys.executable, str(engine / "mega-lander/workspaces.py"), "--session", session,
                            "integration", "--repo", str(repo), "--branch", "main", "--why", "Disposable runtime probe"],
                           env=env, capture_output=True, text=True, check=True)
        proc = subprocess.Popen(args, cwd=workspace, env=env, text=True, start_new_session=True,
                                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        inbox = queue.Queue()
        stderr = []

        def read():
            for line in proc.stdout:
                try:
                    inbox.put(json.loads(line))
                except ValueError:
                    pass
            inbox.put(None)

        def read_errors():
            for line in proc.stderr:
                stderr.append(line[-500:])

        threading.Thread(target=read, daemon=True).start()
        threading.Thread(target=read_errors, daemon=True).start()

        def send(msg):
            proc.stdin.write(json.dumps(msg) + "\n")
            proc.stdin.flush()

        send({"type": "control_request", "request_id": "init",
              "request": {"subtype": "initialize", "hooks": {}}})
        initialized = False
        loaded = False
        result = None
        phase = 0
        permission_denied = False
        permission_allowed = False
        deadline = time.monotonic() + timeout
        try:
            while time.monotonic() < deadline:
                if result is not None:
                    observed = [json.loads(line) for line in (root / "hooks.jsonl").read_text().splitlines()]
                    started = {h["agent_id"] for h in observed if h.get("hook_event_name") == "SubagentStart"}
                    ended = {h["agent_id"] for h in observed if h.get("hook_event_name") == "SubagentStop"}
                    if started and started <= ended:
                        break
                try:
                    msg = inbox.get(timeout=min(1, max(.01, deadline-time.monotonic())))
                except queue.Empty:
                    continue
                if msg is None:
                    break
                kind = msg.get("type")
                if kind == "control_response" and not initialized:
                    response = msg.get("response", {})
                    if response.get("request_id") != "init" or response.get("subtype") != "success":
                        raise RuntimeError("Native initialization rejected")
                    initialized = True
                    prepared = subprocess.run([sys.executable, str(engine / "scripts/prepare-agent-spawn.py")],
                        input=json.dumps({"name": "canary-sonnet-probe", "subagent_type": "runtime-probe:canary",
                            "model": "sonnet", "isolation": "worktree", "description": "Verify isolated target write",
                            "run_in_background": False,
                            "prompt": f"cross-repo-worktree: {worker_tree}\nUse Write to create {worker_tree / 'worker-result.txt'} "
                                      "with exactly CANARY_OK. Read it back and report completion. Change no other files."}),
                        capture_output=True, text=True, check=True)
                    prompt = (
                        f"Runtime test in {workspace}. First use Bash to run `printf HOOK_REFUSE_CANARY`. "
                        "The test hook must refuse that operation; do not retry or bypass it. "
                        f"Next use Write to create {root / 'NATIVE_REFUSE_CANARY.txt'} containing denied; the native permission callback must deny it. "
                        "Do not retry or bypass either refusal. Then dispatch this exact Agent payload: " + prepared.stdout +
                        " Wait for the agent result. Do not create the file yourself. "
                        "Finally report the observed refusals and worker result."
                    )
                    send({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text":
                        "Test the Stop control. Without calling any tool, answer exactly: Canary builds it tomorrow. "
                        "If a hook refuses that sentence, acknowledge that the work has not been dispatched."}]}})
                elif kind == "control_request":
                    request = msg.get("request", {})
                    if request.get("subtype") == "can_use_tool":
                        if "NATIVE_REFUSE_CANARY" in json.dumps(request.get("input", {})):
                            permission_denied = True
                            decision = {"behavior": "deny", "message": "The disposable probe denied this action. Do not retry."}
                        else:
                            permission_allowed = True
                            decision = {"behavior": "allow", "updatedInput": request.get("input", {})}
                        send({"type": "control_response", "response": {"subtype": "success",
                              "request_id": msg["request_id"], "response": decision}})
                    else:
                        send({"type": "control_response", "response": {"subtype": "error",
                              "request_id": msg["request_id"], "error": "Unsupported probe control"}})
                elif kind == "system" and msg.get("subtype") == "init":
                    loaded = any(p.get("name") == "runtime-probe" for p in msg.get("plugins", []))
                elif kind == "result":
                    if phase == 0:
                        phase = 1
                        subprocess.run(["bash", str(engine / "mega-lander/create-teammate-worktree.sh"), str(target),
                                        "canary-sonnet-probe", "--dir", str(worker_tree), "--session", session],
                                       env=env, capture_output=True, text=True, check=True)
                        send({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": prompt}]}})
                    else:
                        result = msg
        finally:
            proc.stdin.close()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGTERM)
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(proc.pid, signal.SIGKILL)
                    proc.wait()
        hooks_file = root / "hooks.jsonl"
        hooks = [json.loads(line) for line in hooks_file.read_text().splitlines()] if hooks_file.exists() else []
        transcripts = [Path(h["transcript_path"]) for h in hooks if h.get("transcript_path")]
        receipts = root / "guard-results.jsonl"
        guard_results = [json.loads(line) for line in receipts.read_text().splitlines()] if receipts.exists() else []
        worker = worker_tree / "worker-result.txt"
        denied = any(h.get("hook_event_name") == "PreToolUse" and
                     h.get("tool_name") == "Bash" and
                     "HOOK_REFUSE_CANARY" in h.get("tool_input", {}).get("command", "") for h in hooks)
        denied_executed = any(h.get("hook_event_name") == "PostToolUse" and
                             h.get("tool_name") == "Bash" and
                             "HOOK_REFUSE_CANARY" in h.get("tool_input", {}).get("command", "") for h in hooks)
        summary = {"initialized": initialized, "plugin_loaded": loaded,
                   "hook_events": sorted(set(h.get("hook_event_name", "") for h in hooks)),
                   "refused": denied and not denied_executed,
                   "canonical_stop_refused": any(r["exit"] == 2 for r in guard_results),
                   "canonical_stop_settled": bool(guard_results) and guard_results[-1]["exit"] == 0,
                   "native_permission_denied": permission_denied and not (root / "NATIVE_REFUSE_CANARY.txt").exists(),
                   "native_permission_allowed": permission_allowed,
                   "target_main_untouched": not (target / "worker-result.txt").exists(),
                   "native_workspace_observed": any(h.get("agent_id") and "/.claude/worktrees/" in h.get("cwd", "") for h in hooks),
                   "worker_ran": any(h.get("hook_event_name") == "SubagentStart" for h in hooks),
                   "artifact_correct": worker.exists() and worker.read_text().strip() == "CANARY_OK",
                   "artifact_exists": worker.exists(),
                   "worker_tool_names": sorted({h.get("tool_name", "") for h in hooks if h.get("agent_id") and h.get("tool_name")}),
                   "transcript_available": any(p.is_file() and p.stat().st_size for p in transcripts),
                   "result_received": result is not None,
                   "result_error": result.get("is_error") if result else None,
                   "process_exit": proc.returncode,
                   "hook_shapes": {event: sorted(set(k for h in hooks if h.get("hook_event_name") == event for k in h))
                                   for event in sorted(set(h.get("hook_event_name", "") for h in hooks))}}
        print(json.dumps(summary, indent=2))
        return 0 if all(summary[k] for k in ["initialized", "plugin_loaded", "refused", "worker_ran",
                                             "artifact_correct", "result_received", "canonical_stop_refused", "canonical_stop_settled", "transcript_available", "native_permission_denied", "native_permission_allowed",
                                             "target_main_untouched", "native_workspace_observed"]) and not summary["result_error"] else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--claude", default=shutil.which("claude"))
    parser.add_argument("--timeout", type=int, default=180)
    opts = parser.parse_args()
    if not opts.claude:
        parser.error("An authenticated Claude executable is required")
    try:
        raise SystemExit(run(opts.claude, opts.timeout))
    except subprocess.CalledProcessError as error:
        print(error.stderr or "A disposable fixture command failed", file=sys.stderr)
        raise SystemExit(1)
