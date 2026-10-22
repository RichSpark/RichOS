#!/usr/bin/env python3
"""skills-walk.py — a worker reaches the engine's shipped skills, and a standing order sticks.

Run by run-walk.py, which holds one guest slot for this run only, boots the guest and removes the clone.
The fixture home is made first, then the walk:

  skills-walk.py --seed-home FIXTURE_HOME
  run-walk.py --bundle ZIP --home FIXTURE_HOME --engine ENGINE --report REPORT --wait 600 -- \\
      skills-walk.py --out DIR --expect-sha SHA

run-walk.py passes the owned VM name as the first argument. `--steps a,b,c` runs a subset.

THE QUESTION. CEO §138, "the regular app needs to have EVERYTHING I USE AND HAVE"; richos-hq
docs/plans/2026-10-21-switching-to-the-app.md rows S and W. S: every lease loads the engine's
shipped skills (engine_profile.rs `shipped_skills`), so a teammate told to use `playwright-cli` can.
W: a "from now on" order given to Rich is saved in the user's own instructions file, so it sticks.

The fixture user has `playwright-cli` on their login PATH, the way a user who installed it does
(the CEO's Mac: /opt/homebrew/bin/playwright-cli). It is a STAND-IN that logs its arguments and
prints a page title: the guest has no Node or browser, and the question is whether the skill
reaches the worker and the worker runs it, not whether Playwright drives a browser.

WHAT IT DOES, in the guest, never on the host's screen (CEO ruling §65):
  identity        the running app says it was built from --expect-sha
  first-run       adopt-walk.py's: company "Acme" with a folder that is a Git repository
  connect         command-walk.py's: that folder connected
  skills          SKILLS_JOB typed: Rich hands it on, the fixture teammate walkqa lists its skills and
                  uses its playwright-cli skill. PASS needs all of: a lease's journaled system/init
                  lists richos-app-engine:playwright-cli; the assignment settled; the back end's hook
                  record shows walkqa's own Skill call for playwright-cli COMPLETED (PostToolUse), its
                  own Bash call that opened the requested page with playwright-cli completed, and the
                  page's title (printed by the stand-in only for that open) reached walkqa in a tool
                  result; the stand-in's log shows that open. The worker's answer and its transcript's skill names
                  are recorded beside them.
  standing-order  ORDER typed ("From now on, ..."); within --within seconds ~/myrichos/instructions.md
                  holds ORDER_WORD, and the copied rich.md and stu.md are unchanged.
Evidence in --out: report.json, facts.json, observed-*.json, the app's logs, callbacks, ledger.
It quits nothing: run-walk.py quits the app by its pid and deletes the clone (CEO §54).
"""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from relaunch import guest  # noqa: E402

_spec = importlib.util.spec_from_file_location('lease_setup_walk', HERE / 'lease-setup-walk.py')
lease_setup_walk = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lease_setup_walk)
StepFailed = lease_setup_walk.StepFailed
OPEN = lease_setup_walk.OPEN

STEPS = ['identity', 'first-run', 'connect', 'skills', 'standing-order']
TEAMMATE = 'walkqa'
WANT = 'richos-app-engine:playwright-cli'
URL = 'https://example.com/'
TITLE = 'Walk Stand-in Page'
SKILLS_JOB = (f'Please ask {TEAMMATE} to do this and tell me its answer: list every skill it can use, by name, '
              f'then use its playwright-cli skill to open {URL} and read the page title.')
ORDER_WORD = 'MARIGOLD'
ORDER = f'From now on, always end every reply with the word {ORDER_WORD}, in capitals.'
STAND_IN = f"""#!/bin/sh
# skills-walk.py's stand-in for playwright-cli: logs its arguments, prints a page.
echo "$(date +%s) $*" >> "$HOME/playwright-cli.log"
case "$1" in
  open|goto) echo "### Page"; echo "- Page URL: ${{2:-about:blank}}"; echo "- Page Title: {TITLE}" ;;
  snapshot) echo '- heading "{TITLE}" [level=1] [ref=e1]' ;;
  *) echo "ok" ;;
esac
"""
DEFINITION = (f'---\nname: {TEAMMATE}\ndescription: Fictional walk QA teammate who checks web pages in a browser.\n'
              f'model: sonnet\n---\n\n# Walkqa — Web QA Tester\n\nA fictional teammate written by skills-walk.py. '
              'For anything in a web browser, use the `playwright-cli` skill.\n')


def seed_home(home):
    """The fictional user's own setup, written into an empty fixture home."""
    home = Path(home)
    (home / 'fixture-bin').mkdir(parents=True, exist_ok=True)
    (home / '.zprofile').write_text('export PATH="$HOME/fixture-bin:$PATH"\n')
    tool = home / 'fixture-bin/playwright-cli'
    tool.write_text(STAND_IN)
    tool.chmod(0o755)
    team = home / 'Library/Application Support/com.richos.app/team'
    team.mkdir(parents=True, exist_ok=True)
    (team / f'{TEAMMATE}.md').write_text(DEFINITION)
    print(f'seeded {home}')
    return 0


def init_skills(raw_lines):
    """Each journaled `system/init` frame's skills, as (skills or None, truncated): video-skill-walk.py's
    reading, for this walk's skill (a payload over the journal's 32 KB cap is a string prefix)."""
    out = []
    for line in raw_lines:
        try:
            row = json.loads(line)
        except ValueError:
            continue
        payload = row.get('payload')
        if isinstance(payload, dict) and payload.get('type') == 'system' and payload.get('subtype') == 'init':
            out.append((list(payload.get('skills') or []), False))
        elif isinstance(payload, str) and '"subtype":"init"' in payload:
            out.append(([WANT] if f'"{WANT}"' in payload else None, True))
    return out


# The requested browser action, in a command and in the stand-in's log.
OPENED = re.compile(r'(?:^|\s)(?:playwright-cli )?(?:open|goto) https://example\.com')


def worker_use(calls):
    """The worker's SUCCESSFUL use of the skill, from the back end's hook record: (Skill calls for
    playwright-cli that completed, never a PostToolUseFailure; the worker's own shell calls that
    completed and opened the requested page with playwright-cli; the worker's tool results that
    carry the page's title, which the stand-in prints only for that open). A worker's shell call
    runs in the background in the app (run_in_background, empty stdout and a backgroundTaskId), so
    the title comes back in a later result, `agent_hold.py wait`'s, and not in the open call's own."""
    mine = [c for c in calls if c['agent_id']]
    used = [c for c in mine if c['event'] == 'PostToolUse' and c['tool'] == 'Skill' and 'playwright-cli' in c['input']]
    opened = [c for c in mine if c['event'] == 'PostToolUse' and c['tool'] == 'Bash'
              and 'playwright-cli' in c['input'] and OPENED.search(c['input'])]
    titled = [c for c in mine if TITLE in c['response']]
    return used, opened, titled


CALLBACKS = r"""import glob,json,sys
out=[]
for p in glob.glob(sys.argv[1]+"/engine-state/evidence/*/callbacks.jsonl"):
    for l in open(p, errors="replace"):
        try: c=json.loads(l).get("callback",{})
        except Exception: continue
        if c.get("hook_event_name") not in ("PreToolUse","PostToolUse","PostToolUseFailure"): continue
        i=c.get("tool_input") or {}
        r=c.get("tool_response")
        r=json.dumps(r) if r is not None else str(c.get("error") or "")
        out.append({"event":c.get("hook_event_name"),"tool":c.get("tool_name"),"agent_id":c.get("agent_id"),
                    "agent_type":c.get("agent_type"),"input":json.dumps(i)[:1500],"response":r[:6000]})
print(json.dumps(out))
"""

TRANSCRIPT_SKILLS = r"""import glob,re,sys
names=set()
for p in glob.glob(sys.argv[1]+"/.claude/projects/*/*/subagents/*.jsonl"):
    names.update(re.findall(r"richos-app-engine:[a-z0-9-]+", open(p, errors="replace").read()))
print("\n".join(sorted(names)))
"""


class SkillsWalk(lease_setup_walk.LeaseSetupWalk):
    def calls(self):
        return json.loads(guest(self.vm, 'python3 -c ' + shlex.quote(CALLBACKS) + ' ' + shlex.quote(self.data), 60))

    def skills(self):
        sent = self.send(SKILLS_JOB)
        self.facts['skills_sent_ms'] = sent
        self.save()
        end = time.monotonic() + self.a.within
        record, used, ran, titled = None, [], [], []
        while time.monotonic() < end:
            used, ran, titled = worker_use(self.calls())
            record = self.ours(sent)
            if record and record.get('state') not in OPEN:
                # Closed: read the hook record once more, so a call written as it closed counts.
                used, ran, titled = worker_use(self.calls())
                break
            if record:
                self.approve_if_asked(record.get('title', ''))
            time.sleep(5)
        raw = guest(self.vm, 'grep -hE ' + shlex.quote('subtype.{1,3}:.{1,3}init') + ' '
                    + shlex.quote(self.data) + '/machinery/*/*.raw.jsonl 2>/dev/null || true', timeout=60)
        frames = init_skills(raw.splitlines())
        listed = [s for s, _ in frames if s and WANT in s]
        log = guest(self.vm, 'cat ' + shlex.quote(self.home + '/playwright-cli.log') + ' 2>/dev/null || true')
        worker_names = guest(self.vm, 'python3 -c ' + shlex.quote(TRANSCRIPT_SKILLS) + ' ' + shlex.quote(self.home), 60).split()
        evidence = {'assignment': record, 'init_frames': len(frames),
                    'init_skills_engine': sorted(s for s in (listed[0] if listed else []) if s.startswith('richos-app-engine:')),
                    'worker_skill_calls': used, 'worker_playwright_commands': ran,
                    'worker_results_with_the_title': titled, 'stand_in_log': log,
                    'worker_transcript_skill_names': worker_names}
        (self.out / 'observed-skills.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = []
        if not listed:
            failures.append(f'no journaled system/init frame lists {WANT} ({len(frames)} frame(s))')
        if not record or record.get('state') != 'settled':
            failures.append(f"the assignment did not end settled: {record.get('state') if record else 'none registered'}")
        if not used:
            failures.append("no teammate's Skill call for playwright-cli completed (PostToolUse)")
        if not ran:
            failures.append(f"no teammate's own shell call that opened {URL} with playwright-cli completed")
        if not titled:
            failures.append(f"the opened page's title ({TITLE}) never reached the teammate in a tool result")
        if not OPENED.search(log):
            failures.append(f'the playwright-cli stand-in never opened {URL}: {log.strip()[:200]!r}')
        if failures:
            raise StepFailed('; '.join(failures))
        return {'init_engine_skills': len(evidence['init_skills_engine']), 'skill_call': used[0]['input'],
                'command': ran[0]['input'], 'stand_in_log': log.strip().splitlines()[:4],
                'worker_transcript_skill_names': len(worker_names),
                'state': record.get('state') if record else None}

    def standing_order(self):
        path = self.central() + '/instructions.md'
        before = {name: guest(self.vm, 'shasum -a 256 ' + shlex.quote(self.central() + '/' + name) + ' 2>&1 || true').split()[0:1]
                  for name in ('rich.md', 'stu.md')}
        reply = self.reply_to(ORDER)
        end = time.monotonic() + self.a.within
        text = ''
        while time.monotonic() < end:
            text = guest(self.vm, 'cat ' + shlex.quote(path) + ' 2>/dev/null || true')
            if ORDER_WORD in text:
                break
            time.sleep(5)
        after = {name: guest(self.vm, 'shasum -a 256 ' + shlex.quote(self.central() + '/' + name) + ' 2>&1 || true').split()[0:1]
                 for name in ('rich.md', 'stu.md')}
        touching = [c for c in self.calls() if 'instructions.md' in c['input']]
        evidence = {'order': ORDER, 'reply': reply, 'instructions_md': text, 'calls_naming_the_file': touching,
                    'definitions_before': before, 'definitions_after': after}
        (self.out / 'observed-standing-order.json').write_text(json.dumps(evidence, indent=2) + '\n')
        failures = []
        if ORDER_WORD not in text:
            failures.append(f'{path} does not hold the order within {self.a.within:g} s: {text[:300]!r}')
        if before != after:
            failures.append('the copied rich.md or stu.md changed')
        if failures:
            raise StepFailed('; '.join(failures))
        return {'instructions_md': text.strip()[-300:], 'reply': reply[-200:],
                'writers': sorted({(c['tool'], bool(c['agent_id'])) for c in touching if c['event'] == 'PostToolUse'})}


def main():
    if sys.argv[1:2] == ['--seed-home']:
        return seed_home(sys.argv[2])
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('vm')
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--expect-sha', required=True)
    p.add_argument('--within', type=float, default=480, help='seconds for each step\'s result')
    p.add_argument('--steps', default=','.join(STEPS))
    a = p.parse_args()
    a.task, a.approvals = SKILLS_JOB, 3
    os.environ.setdefault('TESTVM_AX_TIMEOUT', '40')
    steps = a.steps.split(',')
    unknown = [s for s in steps if s not in STEPS]
    if unknown:
        p.error('unknown step(s): ' + ', '.join(unknown))
    walk = SkillsWalk(a)
    report = {'vm': a.vm, 'expect_sha': a.expect_sha, 'steps': []}
    ok = True
    for step in steps:
        began = time.monotonic()
        row = {'step': step}
        try:
            row['evidence'] = getattr(walk, step.replace('-', '_'))()
            row['outcome'] = 'PASS'
        except (StepFailed, RuntimeError, subprocess.TimeoutExpired, ValueError, KeyError) as exc:
            row['outcome'] = 'FAIL'
            row['detail'] = str(exc)
            ok = False
        row['seconds'] = round(time.monotonic() - began, 1)
        report['steps'].append(row)
        (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(f"{step}: {row['outcome']}" + (f" — {row.get('detail')}" if row['outcome'] != 'PASS' else ''), flush=True)
        if not ok and step in ('identity', 'first-run', 'connect'):
            break
    try:
        report['logs_kept'] = walk.keep_logs()
    except (StepFailed, RuntimeError, subprocess.TimeoutExpired, OSError) as exc:
        report['logs_kept'] = 'the guest logs could not be copied: ' + str(exc)
    (a.out / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
