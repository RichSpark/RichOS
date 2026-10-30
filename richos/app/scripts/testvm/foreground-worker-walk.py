#!/usr/bin/env python3
"""foreground-worker-walk.py — in the regular app, a worker's ordinary command answers directly,
as in the terminal (2026-10-30, engine docs/foreground-bash.md).

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone:

  run-walk.py --bundle ZIP --home EMPTY_DIR --engine ENGINE_TARBALL --report REPORT -- \\
      foreground-worker-walk.py --out DIR --expect-sha SHA

The engine is the one under test (`--engine`, RICHOS_ENGINE_DIR in the guest), so a published
nightly bundle with a branch's engine tarball shows that branch's engine in the regular app.

It reuses stu-reminder-walk.py's class and its first steps; only the job's words and the last step
are this walk's.

STEPS, in the guest, never on the host's screen (CEO ruling 65):
  identity, first-run, relaunch   as stu-reminder-walk.py (first-run also seeds a git identity)
  job         typed: a job for the back end (Stu) in which one teammate he starts runs exactly
              `sleep 3; echo done` and he reports what it printed. PASS needs the job SETTLED.
  worker      the guest's own Claude Code transcripts: a WORKER's (subagent transcript) Bash call
              whose command is that one. PASS needs its tool result to be `done`, delivered
              directly, and neither a background receipt ("Command running in background with ID")
              nor an agent_hold.py wait instruction in that worker's transcript.
              NOT REACHED (a failure that says so) when no worker ran it (Stu ran it himself: his
              own commands stay in the background by the CEO's approval, ruling 129).

Exit 0 only when every step passes. Every app instance is quit by run-walk.py's stop.sh (CEO 54).
"""
import importlib.util
import json
from pathlib import Path
import shlex
import sys

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

# token-waste-walk.py's walk (its git identity seeding) and the stu-reminder-walk.py module it
# loaded: one module, so its StepFailed is the one main() catches.
_tw = importlib.util.spec_from_file_location('token_waste_walk', HERE / 'token-waste-walk.py')
token_waste = importlib.util.module_from_spec(_tw)
_tw.loader.exec_module(token_waste)
base = token_waste.base
StepFailed = base.StepFailed

COMMAND = 'sleep 3; echo done'
STEPS = ['identity', 'first-run', 'relaunch', 'job', 'worker']
JOB = ('Please start one teammate and have that teammate (not you) run exactly this shell command in the '
       'Acme repository, as one ordinary Bash command: %s . Then tell me exactly what it printed. '
       'Nothing needs to be changed, committed or landed.' % COMMAND)

# Read in the guest: every worker (subagent) transcript's Bash call of COMMAND, its result text and
# whether that worker's transcript carries a background receipt for it or a wait instruction.
FIND = r'''
import glob, json, sys
home, command = sys.argv[1], sys.argv[2]
out = []
for path in glob.glob(home + "/.claude/projects/*/*/subagents/agent-*.jsonl"):
    calls, results, text = {}, {}, ""
    for line in open(path, errors="replace"):
        text += line if "agent_hold.py wait" in line else ""
        try:
            row = json.loads(line)
        except ValueError:
            continue
        content = (row.get("message") or {}).get("content") if isinstance(row, dict) else None
        for item in content if isinstance(content, list) else []:
            if not isinstance(item, dict):
                continue
            if item.get("type") == "tool_use" and item.get("name") == "Bash":
                cmd = str((item.get("input") or {}).get("command") or "")
                if cmd.strip().rstrip(";").endswith(command):
                    calls[item.get("id")] = {"command": cmd, "input": item.get("input")}
            if item.get("type") == "tool_result" and item.get("tool_use_id"):
                body = item.get("content")
                if isinstance(body, list):
                    body = " ".join(str(b.get("text", "")) for b in body if isinstance(b, dict))
                results[item["tool_use_id"]] = str(body)
    for tid, call in calls.items():
        out.append({"transcript": path, "tool_use_id": tid, "command": call["command"],
                    "input": call["input"], "result": results.get(tid),
                    "wait_instruction_seen": bool(text)})
print(json.dumps(out))
'''


def judge(rows):
    """(verdict, why) over the worker calls found in the guest."""
    if not rows:
        return 'NOT REACHED', 'no worker ran `%s` (Stu may have run it himself; his own commands stay in the background, ruling 129)' % COMMAND
    for r in rows:
        result = r.get('result')
        if result is None:
            return 'FAIL', 'the worker call %s has no result in its transcript' % r['tool_use_id']
        if 'Command running in background' in result or 'moved to the background' in result:
            return 'FAIL', 'the worker call came back as a background task: %r' % result[:200]
        if result.strip() != 'done':
            return 'FAIL', 'the worker call returned %r, not "done"' % result[:200]
        if r.get('input', {}).get('run_in_background'):
            return 'FAIL', 'the worker asked for the background itself, so nothing was measured'
        if r.get('wait_instruction_seen'):
            return 'FAIL', 'the worker transcript carries an agent_hold.py wait instruction'
    return 'PASS', '%d worker call(s) of `%s` answered "done" directly' % (len(rows), COMMAND)


class ForegroundWorkerWalk(token_waste.TokenWasteWalk):
    def worker(self):
        made = self.payload + '/foreground-worker'
        guest(self.vm, 'mkdir -p ' + shlex.quote(made), 60)
        script = made + '/find.py'
        guest(self.vm, 'cat > %s <<\'PY\'\n%s\nPY' % (shlex.quote(script), FIND), 60)
        rows = json.loads(guest(self.vm, '/usr/bin/python3 %s %s %s' % (
            shlex.quote(script), shlex.quote(self.home), shlex.quote(COMMAND)), 120) or '[]')
        (self.out / 'worker-calls.json').write_text(json.dumps(rows, indent=2) + '\n')
        verdict, why = judge(rows)
        if verdict != 'PASS':
            raise StepFailed('%s: %s; see worker-calls.json' % (verdict, why))
        return {'calls': rows, 'why': why}


def main():
    base.STEPS = STEPS
    base.JOB = JOB
    base.StuReminderWalk = ForegroundWorkerWalk
    return base.main()


if __name__ == '__main__':
    sys.exit(main())
