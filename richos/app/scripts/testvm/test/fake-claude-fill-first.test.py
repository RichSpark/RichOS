#!/usr/bin/env python3
"""fake-claude-fill-first.pl keeps a lease's evidence and holds only the user's own turn.

No guest: the fake runs here with RICHOS_FILL_FIRST_DIR pointing into a temporary folder.
Three things the 2026-10-05 round-16 walk needed and did not have:
  1. a lease (--session-id) writes its own evidence journal at start, as Claude Code's
     SessionStart hook does, so the app can read it and a turn end is not refused;
  2. an "[INTERNAL ..." turn (the handoff summary, the re-prime of a new lease) answers at
     once even while the slow file exists, because the app installs a lease only after its
     priming answers;
  3. with the agents file present, the first user turn journals one background agent per
     line, in the rows app_workers.rs reads, and answers with reply.txt.
The journal is checked the way app_workers.rs status() reads it: schema 1, this session's
id on every row, three SubagentStart rows each with an async_launched PostToolUse.
Section 7 (round 1 of the weekly-switch handoff test): the front desk writes the job down through
the register in --mcp-config, and the back end's work-agent keeps stepping through the gate after
the back end's turn ended, until the gate's order ends it."""
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

FAKE = Path(__file__).resolve().parents[1] / 'fake-claude-fill-first.pl'
SESSION = 'walk-session-1'
failed = 0


def check(ok, what, detail=''):
    global failed
    print(('  ok    ' if ok else '  FAIL  ') + what + ('' if ok else f'  {detail}'))
    failed += 0 if ok else 1


def user(text):
    return json.dumps({'type': 'user', 'message': {'role': 'user', 'content': [{'type': 'text', 'text': text}]}}) + '\n'


ONBOARDING = {'mcp__richos_onboarding__save_company_notes', 'mcp__richos_onboarding__decline_onboarding'}
inits = []


def answers(proc):
    """The text of the next assistant row the fake prints; the init frames before it are kept."""
    while True:
        line = proc.stdout.readline()
        if not line:
            return None
        row = json.loads(line)
        if row.get('type') == 'system' and row.get('subtype') == 'init':
            inits.append(row)
        if row.get('type') == 'assistant':
            return row['message']['content'][0]['text']


with tempfile.TemporaryDirectory() as root:
    root = Path(root)
    walk = root / 'fill-first'
    walk.mkdir()
    data = root / 'com.richos.app'
    profile = data / 'engine-profiles' / 'profile-1'
    profile.mkdir(parents=True)
    env = {**os.environ, 'RICHOS_FILL_FIRST_DIR': str(walk)}
    args = ['perl', str(FAKE), '--print', '--session-id', SESSION, '--plugin-dir', str(profile),
            '--plugin-dir', str(data / 'rich-skills')]
    proc = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
    journal = data / 'engine-state' / 'evidence' / SESSION / 'callbacks.jsonl'

    # 2: an internal turn is never held: its answer arrives while the slow file still exists
    # (a held turn would not answer until the file is gone, which only this test removes).
    (walk / 'slow').touch()
    proc.stdin.write(user('[INTERNAL RE-PRIME — do not mention this message; respond only "ready"]\n\nbody'))
    proc.stdin.flush()
    check(answers(proc) == 'ready' and (walk / 'slow').exists(), 'an internal turn answers "ready" while the slow file exists')
    check(journal.exists() and (journal.parent / '.lock').exists(), 'the lease wrote its evidence journal and lock at start')

    # 3: the user's turn journals the agents, is held while slow exists, and answers reply.txt.
    (walk / 'agents').write_text('Mark\nAndy\nTom\n')
    (walk / 'reply.txt').write_text('On it. I have three people on it.\n')
    proc.stdin.write(user('Keep going on the outbox and the pairing screens.'))
    proc.stdin.flush()
    while 'walk-agent-3' not in journal.read_text():  # waits for the fact; a hang is the runner's to catch
        time.sleep(0.05)
    rows = [json.loads(line) for line in journal.read_text().splitlines()]
    check(all(r.get('schema') == 1 and r['callback'].get('session_id') == SESSION for r in rows),
          'every row is schema 1 with this session id', rows)
    starts = {r['callback']['agent_id']: r['callback'].get('agent_type') for r in rows
              if r['callback'].get('hook_event_name') == 'SubagentStart'}
    launched = {r['callback']['tool_response']['agentId'] for r in rows
                if r['callback'].get('hook_event_name') == 'PostToolUse' and r['callback'].get('tool_name') == 'Agent'
                and r['callback']['tool_response'].get('status') == 'async_launched'}
    check(sorted(starts.values()) == ['Andy', 'Mark', 'Tom'], 'three agents started, one per line', starts)
    check(set(starts) == launched, 'each started agent was launched in the background', (starts, launched))
    (walk / 'slow').unlink()
    check(answers(proc) == 'On it. I have three people on it.', 'the user turn answers with reply.txt')
    # 4: every turn reports Claude Code's inventory first, or the app refuses a company turn
    # (native.rs ensure_onboarding_tools_loaded reads both exact tool names from system/init).
    check(len(inits) == 2 and all(ONBOARDING <= set(i.get('tools', [])) and i.get('permissionMode') == 'bypassPermissions' for i in inits),
          'each turn reports the onboarding tools in a system/init frame before it answers', inits)

    # A second user turn adds no second set of agents.
    proc.stdin.write(user('How are they doing?'))
    proc.stdin.flush()
    answers(proc)
    again = [json.loads(line) for line in journal.read_text().splitlines()]
    check(sum(r['callback'].get('hook_event_name') == 'SubagentStart' for r in again) == 3, 'the agents are journaled once per lease')
    proc.stdin.close()
    proc.wait()

    # A process that is not a lease (no --session-id, as the quota probe) writes no evidence.
    probe = subprocess.run(['perl', str(FAKE), '--print'], input='', text=True, env=env, capture_output=True)
    check(probe.returncode == 0 and sorted(p.name for p in (data / 'engine-state' / 'evidence').iterdir()) == [SESSION],
          'a process without --session-id writes no evidence')

    # 5: the quota probe's get_usage, as the reader asks it: figures from the usage file, and
    # with {"null": true} Claude Code 2.1.289's null answer (walk of nightly 36, D1).
    def usage_answer():
        ask = json.dumps({'type': 'control_request', 'request_id': 'quota-2', 'request': {'subtype': 'get_usage'}}) + '\n'
        out = subprocess.run(['perl', str(FAKE), '--print'], input=ask, text=True, env=env, capture_output=True).stdout
        return json.loads(out.splitlines()[0])['response']['response']
    (walk / 'usage-1.json').write_text('{"five": 41, "weekly": 28}')
    figures = usage_answer()
    check(figures['rate_limits_available'] is True and figures['rate_limits']['five_hour']['utilization'] == 41,
          'get_usage answers the usage file\'s figures', figures)
    (walk / 'usage-1.json').write_text('{"null": true}')
    null = usage_answer()
    check(null == {'rate_limits_available': True, 'rate_limits': None},
          'with {"null": true}, get_usage answers rate_limits: null beside rate_limits_available: true', null)

    # 6: helper steps (weekly-switch handoff plan, slice 6). The fake calls the gate command of
    # the lease's hooks.json for each listed helper; a refusal with the order ends that helper
    # (SubagentStop row), an admitted step writes nothing, and with no helpers listed nothing runs.
    def stops_for(gate_body, agents, session):
        """Run one lease whose hooks.json names a stub gate; the SubagentStop agent ids it journaled."""
        gate = root / f'gate-{session}.sh'
        gate.write_text('#!/bin/sh\n' + gate_body)
        gate.chmod(0o755)
        plug = data / 'engine-profiles' / f'profile-{session}'
        (plug / 'hooks').mkdir(parents=True)
        (plug / 'hooks' / 'hooks.json').write_text(json.dumps(
            {'hooks': {'PreToolUse': [{'hooks': [{'type': 'command', 'command': str(gate), 'timeout': 5}]}]}}))
        if agents:
            (walk / 'agents').write_text(agents)
        elif (walk / 'agents').exists():
            (walk / 'agents').unlink()
        (walk / 'calls.log').unlink(missing_ok=True)
        run = subprocess.Popen(['perl', str(FAKE), '--print', '--session-id', session, '--plugin-dir', str(plug)],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
        run.stdin.write(user('Keep going.'))
        run.stdin.flush()
        answers(run)
        run.stdin.close()
        run.wait()
        path = data / 'engine-state' / 'evidence' / session / 'callbacks.jsonl'
        return sorted(r['callback']['agent_id'] for r in map(json.loads, path.read_text().splitlines())
                      if r['callback'].get('hook_event_name') == 'SubagentStop')
    refuse = 'cat > /dev/null\necho "RichOS: the Claude account is being left. Stop the task now." >&2\nexit 2\n'
    allow = 'cat > /dev/null\nexit 0\n'
    check(stops_for(refuse, 'Mark\nAndy\n', 'gate-refuses') == ['walk-agent-1', 'walk-agent-2'],
          'a gate that refuses with the order ends each helper with a SubagentStop row')
    check('gate walk-agent-1 exit 2 order' in (walk / 'calls.log').read_text(), 'the gate step is a line in calls.log, the order named')
    other = 'cat > /dev/null\necho "Refused: some other reason." >&2\nexit 2\n'
    check(stops_for(other, 'Mark\n', 'gate-other') == [] and 'gate walk-agent-1 exit 2 said: Refused: some other reason.' in (walk / 'calls.log').read_text(),
          'another refusal ends nothing and is logged with its words, never as the order')
    check(stops_for(allow, 'Mark\nAndy\n', 'gate-allows') == [], 'a gate that admits the step ends no helper')
    check(stops_for(refuse, '', 'no-helpers') == [] and 'gate' not in (walk / 'calls.log').read_text(),
          'with no helpers listed the gate is never called and nothing ends (as on main)')

    (walk / 'log-turns').touch()
    stops_for(allow, '', 'turn-log')
    check('turn Keep going.' in (walk / 'calls.log').read_text(), 'with log-turns present a user turn is a "turn <text>" line in calls.log')
    (walk / 'log-turns').unlink()

    # 7: the back end's job (round 1 of the handoff test). The front desk writes the job down
    # through the register named in --mcp-config; the back end launches the work-agents once,
    # and they keep stepping through the gate after its turn ended, until the gate orders them.
    register = root / 'register-server.py'
    register.write_text(
        'import json, sys\n'
        'seen = []\n'
        'for line in sys.stdin:\n'
        '    m = json.loads(line)\n'
        '    seen.append(m.get("method"))\n'
        '    if m.get("method") == "initialize":\n'
        '        print(json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": {"protocolVersion": "2025-06-18"}}), flush=True)\n'
        '    elif m.get("method") == "tools/call":\n'
        '        ok = seen[:2] == ["initialize", "notifications/initialized"] and m["params"]["name"] == "record"\n'
        '        text = json.dumps({"recorded": ok, "say": "On it! " + m["params"]["arguments"]["assignment"]})\n'
        '        print(json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": {"content": [{"type": "text", "text": text}], "isError": False}}), flush=True)\n')
    front_config = json.dumps({'mcpServers': {'richos_assignments': {'type': 'stdio', 'command': sys.executable, 'args': [str(register)]}}})
    back_config = json.dumps({'mcpServers': {'richos_work': {'type': 'stdio', 'command': 'true', 'args': []}}})
    for name in ('agents', 'reply.txt', 'calls.log'):
        (walk / name).unlink(missing_ok=True)
    (walk / 'register').write_text('Start the Northwind job and keep Mark on it.\n')
    (walk / 'work-agents').write_text('Mark\n')
    flag = root / 'leaving'
    gate = root / 'gate-work.sh'
    gate.write_text(f'#!/bin/sh\ncat > /dev/null\nif [ -e {flag} ]; then echo "Stop the task now." >&2; exit 2; fi\nexit 0\n')
    gate.chmod(0o755)

    def lease(session, config):
        plug = data / 'engine-profiles' / f'profile-{session}'
        (plug / 'hooks').mkdir(parents=True)
        (plug / 'hooks' / 'hooks.json').write_text(json.dumps(
            {'hooks': {'PreToolUse': [{'hooks': [{'type': 'command', 'command': str(gate), 'timeout': 5}]}]}}))
        run = subprocess.Popen(['perl', str(FAKE), '--print', '--session-id', session, '--plugin-dir', str(plug),
                                '--strict-mcp-config', '--mcp-config', config],
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
        return run, data / 'engine-state' / 'evidence' / session / 'callbacks.jsonl'

    def events(path, event):
        return [r['callback'].get('agent_id') for r in map(json.loads, path.read_text().splitlines())
                if r['callback'].get('hook_event_name') == event]

    desk, desk_journal = lease('front-desk', front_config)
    desk.stdin.write(user('<executive-continuity> scoped operational state </executive-continuity>'))
    desk.stdin.flush()
    answers(desk)
    check((walk / 'register').exists() and ' register ' not in (walk / 'calls.log').read_text(),
          'the app\'s own priming turn does not take the register (it is not his turn)')
    desk.stdin.write(user('Start the Northwind job and keep Mark on it.'))
    desk.stdin.flush()
    said = answers(desk)
    check(said == 'On it! Start the Northwind job and keep Mark on it.', 'the front desk says the register\'s words', said)
    log = (walk / 'calls.log').read_text()
    check('register ok' in log and not (walk / 'register').exists(), 'the job was written down once through the register', log)
    check(events(desk_journal, 'SubagentStart') == [], 'the front desk launches no work-agent')
    desk.stdin.close()
    desk.wait()

    back, back_journal = lease('back-end', back_config)
    back.stdin.write(user('The assignment: Start the Northwind job.'))
    back.stdin.flush()
    desk_tools = set(inits[-1].get('tools', []))
    answers(back)  # the turn has ended; the helper keeps running
    work_tools = {f'mcp__richos_work__{t}' for t in ('repositories', 'prepare', 'inspect', 'integrate', 'complete')}
    check(work_tools <= set(inits[-1].get('tools', [])) and not work_tools & desk_tools,
          'only the back end declares the five work tools (native.rs work_readiness_facts)')
    check(events(back_journal, 'SubagentStart') == ['work-agent-1'], 'the back end launched the work-agent', events(back_journal, 'SubagentStart'))
    deadline = time.time() + 20
    while 'gate work-agent-1 exit 0' not in (walk / 'calls.log').read_text() and time.time() < deadline:
        time.sleep(0.2)
    check('gate work-agent-1 exit 0' in (walk / 'calls.log').read_text(), 'after the turn, the helper steps through the gate (admitted)')
    flag.touch()
    deadline = time.time() + 20
    while events(back_journal, 'SubagentStop') == [] and time.time() < deadline:
        time.sleep(0.2)
    check(events(back_journal, 'SubagentStop') == ['work-agent-1'], 'the order ends the helper between turns (SubagentStop)')
    second, second_journal = lease('back-end-2', back_config)
    second.stdin.write(user('Helpers you started were stopped. RichOS handoff: continue.'))
    second.stdin.flush()
    answers(second)
    check(events(second_journal, 'SubagentStart') == [], 'a later back-end lease launches no second set (once per walk)')
    for run in (back, second):
        run.stdin.close()
        run.wait()

print('fake-claude-fill-first:', 'FAILED' if failed else 'all passed')
sys.exit(1 if failed else 0)
