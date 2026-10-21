#!/usr/bin/env python3
"""lease-retention-probe.py — does a lease-shaped `claude` start delete old transcripts?

  lease-retention-probe.py --claude /path/to/claude [--scratch DIR]

WHY. An app lease on the user's first Claude account shares `~/.claude` with their terminal, but
starts with `--setting-sources ''`, so their own `cleanupPeriodDays` is not read. If Claude Code's
transcript cleanup ran there on its default of 30 days, every lease start would delete terminal
transcripts the user keeps longer (richos-hq docs/plans/2026-10-21-switching-to-the-app.md, row K).
Claude Code updates itself, so the answer belongs to a version, and this re-measures it.

HOW. Three starts of the given binary, each against its own FIXTURE home (HOME points there and
CLAUDE_CONFIG_DIR is unset, the shape of a first-account lease) holding transcripts aged 10, 60
and 150 days, with a fake API key so no account is spent and nothing reaches the real ~/.claude:
  lease            the lease's flags (`--setting-sources ''`, the app's `--settings`), the user's
                   own settings.json saying 120 days
  control-default  default setting sources, no setting: Claude Code's own default applies
  control-user120  `--setting-sources user`, the user's settings.json saying 120 days
Each start also gets a `--plugin-dir` whose `skills/` holds one real folder and one LINK to a
folder, and reports which of the two `system/init.skills` lists (the premise of
engine_profile.rs `link_skills`).

EXIT. 0: every start reached system/init, the lease kept all three and control-default deleted its
60-day transcript (so a sweep was visible to this probe). 1: the lease deleted something. 2: a start
never reached system/init, or no sweep was seen in control-default either, so the probe cannot tell. The JSON for each start goes to stdout.
"""
import argparse
import json
import os
import select
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

DAY = 86400
AGES = {"age010": 10, "age060": 60, "age150": 150}
LEASE_SETTINGS = json.dumps({"autoMemoryEnabled": True,
                             "claudeMdExcludes": ["**/CLAUDE.md", "**/CLAUDE.local.md", "**/.claude/rules/**"]})
CASES = {
    "lease": ("", {"cleanupPeriodDays": 120}),
    "control-default": (None, None),
    "control-user120": ("user", {"cleanupPeriodDays": 120}),
}


def fixture(root, user_settings, linked_skill):
    shutil.rmtree(root, ignore_errors=True)
    home = os.path.join(root, "home")
    project = os.path.join(home, ".claude", "projects", "-fixture-old-project")
    os.makedirs(project)
    now = time.time()
    for name, days in AGES.items():
        path = os.path.join(project, f"{uuid.uuid4()}-{name}.jsonl")
        stamp = time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime(now - days * DAY))
        with open(path, "w") as f:
            f.write(json.dumps({"type": "user", "timestamp": stamp, "sessionId": "fixture",
                                "message": {"role": "user", "content": "fixture"}}) + "\n")
        os.utime(path, (now - days * DAY, now - days * DAY))
    if user_settings is not None:
        with open(os.path.join(home, ".claude", "settings.json"), "w") as f:
            json.dump(user_settings, f)
    plugin = os.path.join(root, "plugin")
    os.makedirs(os.path.join(plugin, ".claude-plugin"))
    with open(os.path.join(plugin, ".claude-plugin", "plugin.json"), "w") as f:
        json.dump({"name": "skillprobe", "version": "1.0.0"}, f)
    copied = os.path.join(plugin, "skills", "probe-copied")      # a real folder in the plugin
    target = os.path.join(root, "skills-src", linked_skill)      # a folder outside it, linked in
    for folder in (copied, target):
        os.makedirs(folder)
        with open(os.path.join(folder, "SKILL.md"), "w") as f:
            f.write(f"---\nname: {os.path.basename(folder)}\ndescription: A probe skill.\n---\n\nProbe.\n")
    os.symlink(target, os.path.join(plugin, "skills", linked_skill))
    work = os.path.join(root, "work")
    os.makedirs(work)
    os.makedirs(os.path.join(root, "tmp"))
    return home, project, plugin, work


def start(claude, root, sources, user_settings):
    home, project, plugin, work = fixture(root, user_settings, "probe-linked")
    args = [claude, "--print", "--input-format=stream-json", "--output-format=stream-json",
            "--include-partial-messages", "--verbose"]
    if sources is not None:
        args += ["--setting-sources", sources]
    args += ["--disallowed-tools", "AskUserQuestion", "--session-id", str(uuid.uuid4()),
             "--permission-prompt-tool", "stdio", "--plugin-dir", plugin, "--permission-mode", "bypassPermissions",
             "--settings", LEASE_SETTINGS, "--debug-file", os.path.join(root, "debug.log")]
    env = {"HOME": home, "PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "ANTHROPIC_API_KEY": "sk-ant-fixture-not-a-key",
           "TMPDIR": os.path.join(root, "tmp"), "LANG": "en_US.UTF-8"}
    with open(os.path.join(root, "stderr.log"), "w") as stderr:
        proc = subprocess.Popen(args, cwd=work, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr)
    for frame in ({"type": "control_request", "request_id": "req_init", "request": {"subtype": "initialize"}},
                  {"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": "Say ok."}]}}):
        proc.stdin.write((json.dumps(frame) + "\n").encode())
        proc.stdin.flush()
    init, buf, deadline = None, b"", time.time() + 25
    while time.time() < deadline:
        ready, _, _ = select.select([proc.stdout], [], [], 0.5)
        if not ready:
            continue
        chunk = os.read(proc.stdout.fileno(), 65536)
        if not chunk:
            break
        buf += chunk
        while b"\n" in buf:
            line, buf = buf.split(b"\n", 1)
            try:
                frame = json.loads(line)
            except ValueError:
                continue
            if frame.get("type") == "system" and frame.get("subtype") == "init":
                init = frame
            if frame.get("type") == "result":
                deadline = min(deadline, time.time() + 8)  # the sweep runs in the background
    try:
        proc.stdin.close()
    except BrokenPipeError:
        pass
    try:
        proc.wait(timeout=15)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
    kept = sorted(n.rsplit("-", 1)[-1].removesuffix(".jsonl") for n in os.listdir(project))
    debug = ""
    if os.path.exists(os.path.join(root, "debug.log")):
        debug = open(os.path.join(root, "debug.log"), errors="replace").read()
    return {"setting_sources": sources, "user_settings": user_settings, "started": init is not None,
            "exit": proc.returncode, "kept": kept,
            "deleted": sorted(set(AGES) - set(kept)),
            "init_skills": sorted(s for s in (init or {}).get("skills", []) if s.startswith("skillprobe:")),
            "retention_log": [l for l in debug.splitlines()
                              if "retention cleanup" in l or "retention sweep" in l][:4]}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--claude", required=True, help="the claude binary to measure")
    p.add_argument("--scratch", help="where the fixture homes go (default: a new temporary folder, removed after)")
    a = p.parse_args()
    if not os.access(a.claude, os.X_OK):
        p.error(f"{a.claude} is not an executable")
    scratch = a.scratch or tempfile.mkdtemp(prefix="lease-retention-probe-")
    version = subprocess.run([a.claude, "--version"], capture_output=True, text=True, timeout=30).stdout.strip()
    results = {"claude": version}
    try:
        for case, (sources, settings) in CASES.items():
            results[case] = start(a.claude, os.path.join(scratch, case), sources, settings)
    finally:
        if not a.scratch:
            shutil.rmtree(scratch, ignore_errors=True)
    print(json.dumps(results, indent=1))
    if results["lease"]["deleted"]:
        print(f"the lease-shaped start DELETED {results['lease']['deleted']}", file=sys.stderr)
        return 1
    unstarted = [case for case in CASES if not results[case]["started"]]
    if unstarted:
        # A start that never reached system/init measured nothing: keeping every transcript is
        # then no answer (second review rv-20261021T185839Z-93ecee44-dd52, P3).
        print(f"no system/init frame from {unstarted}: that start measured nothing, so this probe cannot tell",
              file=sys.stderr)
        return 2
    if "age060" not in results["control-default"]["deleted"]:
        print("no sweep was seen in control-default either, so this probe cannot tell", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
