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
Section 7: the front desk writes the job down through the register in --mcp-config, and the
back end's work-agent keeps stepping through the gate after the back end's turn ended, until it
finishes on its own (work-agents-finish) with a step the gate admits."""
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


hooks = []


def take_hook(proc):
    """Read up to the input hook the fake calls for its next turn and answer it as the app does;
    the turn goes on only after that answer."""
    while True:
        row = json.loads(proc.stdout.readline())
        if row.get('type') == 'system' and row.get('subtype') == 'init':
            inits.append(row)
        if row.get('type') == 'control_request' and row['request'].get('callback_id') == 'richos-input-context':
            hooks.append(row['request']['input'])
            proc.stdin.write(json.dumps({'type': 'control_response', 'response': {
                'subtype': 'success', 'request_id': row['request_id'], 'response': {}}}) + '\n')
            proc.stdin.flush()
            return


frames = []


def answers(proc):
    """The text of the next assistant text row the fake prints; every row before it is kept
    (`frames`, the init frames in `inits`), and the input hook it calls is answered as the app
    answers it (an empty success)."""
    while True:
        line = proc.stdout.readline()
        if not line:
            return None
        row = json.loads(line)
        frames.append(row)
        if row.get('type') == 'system' and row.get('subtype') == 'init':
            inits.append(row)
        if row.get('type') == 'control_request' and row['request'].get('callback_id') == 'richos-input-context':
            hooks.append(row['request']['input'])
            proc.stdin.write(json.dumps({'type': 'control_response', 'response': {
                'subtype': 'success', 'request_id': row['request_id'], 'response': {}}}) + '\n')
            proc.stdin.flush()
        if row.get('type') == 'assistant' and 'text' in row['message']['content'][0]:
            return row['message']['content'][0]['text']


with tempfile.TemporaryDirectory() as root:
    root = Path(root)
    walk = root / 'fill-first'
    walk.mkdir()
    data = root / 'com.richos.app'
    profile = data / 'engine-profiles' / 'profile-1'
    profile.mkdir(parents=True)
    # HOME in the temporary folder: a lease keeps its native transcript under $HOME/.claude when no
    # account folder is named, and this test must not write into this Mac's own.
    env = {**os.environ, 'RICHOS_FILL_FIRST_DIR': str(walk), 'HOME': str(root)}
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
    start = json.loads(journal.read_text().splitlines()[0])['callback']
    named = Path(start.get('transcript_path', ''))
    check(start.get('hook_event_name') == 'SessionStart' and named.name == SESSION + '.jsonl' and named.is_file()
          and root in named.parents, 'SessionStart names the lease\'s native transcript, which exists, under HOME', start)

    # 3: the user's turn journals the agents, is held while slow exists, and answers reply.txt.
    (walk / 'agents').write_text('Mark\nAndy\nTom\n')
    (walk / 'reply.txt').write_text('On it. I have three people on it.\n')
    proc.stdin.write(user('Keep going on the outbox and the pairing screens.'))
    proc.stdin.flush()
    take_hook(proc)
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

    # 6: helper steps. The fake calls the gate command of the lease's hooks.json for each listed
    # helper; a refusal ends nothing and is logged with the gate's words, an admitted step writes
    # nothing, and with no helpers listed nothing runs.
    def stops_for(gate_body, agents, session, batch_body=None):
        """Run one lease whose hooks.json names a stub gate (and a stub PostToolBatch gate with
        batch_body); the SubagentStop agent ids it journaled."""
        def stub(name, body):
            path = root / f'{name}-{session}.sh'
            path.write_text('#!/bin/sh\n' + body)
            path.chmod(0o755)
            return [{'hooks': [{'type': 'command', 'command': str(path), 'timeout': 5}]}]
        registered = {'PreToolUse': stub('gate', gate_body)}
        if batch_body is not None:
            registered['PostToolBatch'] = stub('batch', batch_body)
        plug = data / 'engine-profiles' / f'profile-{session}'
        (plug / 'hooks').mkdir(parents=True)
        (plug / 'hooks' / 'hooks.json').write_text(json.dumps({'hooks': registered}))
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
    refuse = 'cat > /dev/null\necho "Refused: some reason." >&2\nexit 2\n'
    allow = 'cat > /dev/null\nexit 0\n'
    check(stops_for(refuse, 'Mark\nAndy\n', 'gate-refuses') == [] and 'gate walk-agent-1 exit 2 said: Refused: some reason.' in (walk / 'calls.log').read_text(),
          'a refusal ends no helper and is a line in calls.log with its words')
    check(stops_for(allow, 'Mark\nAndy\n', 'gate-allows') == [], 'a gate that admits the step ends no helper')
    check(stops_for(refuse, '', 'no-helpers') == [] and 'gate' not in (walk / 'calls.log').read_text(),
          'with no helpers listed the gate is never called and nothing ends (as on main)')
    # A step is one tool call as Claude Code runs it (the CEO's §153: the app holds a helper at
    # its batch, after the call ran): PreToolUse, the tool, then PostToolBatch, which gets the
    # helper's agent_id and the call. A call PreToolUse refused does not run, and its batch
    # still comes (measured on Claude Code 2.1.316, gate.rs HOLD_EVENT).
    seen = root / 'batch-seen.json'
    record = f'cat > {seen}\nexit 0\n'
    stops_for(allow, 'Mark\n', 'batch-allows', batch_body=record)
    steps = [line.split()[2:5] for line in (walk / 'calls.log').read_text().splitlines() if ' walk-agent-1' in line]
    check(steps[:4] == [['gate', 'walk-agent-1', 'exit'], ['ran', 'walk-agent-1'], ['batch-start', 'walk-agent-1'],
                        ['batch', 'walk-agent-1', 'exit']], 'gate, the tool, then its batch, in that order', steps)
    batch = json.loads(seen.read_text())
    check(batch['hook_event_name'] == 'PostToolBatch' and batch['agent_id'] == 'walk-agent-1'
          and [c['tool_name'] for c in batch['tool_calls']] == ['Bash'], 'the batch carries the helper and its call', batch)
    stops_for(refuse, 'Mark\n', 'batch-after-refusal', batch_body=record)
    log = (walk / 'calls.log').read_text()
    check('ran walk-agent-1' not in log and 'batch walk-agent-1 exit 0' in log,
          'a refused call does not run, and its batch still comes', log)
    (walk / 'thinking').touch()
    stops_for(allow, 'Mark\n', 'thinking', batch_body=record)
    (walk / 'thinking').unlink()
    check('walk-agent-1' not in (walk / 'calls.log').read_text(), 'while thinking exists no helper takes a step')

    (walk / 'log-turns').touch()
    stops_for(allow, '', 'turn-log')
    check('turn Keep going.' in (walk / 'calls.log').read_text(), 'with log-turns present a user turn is a "turn <text>" line in calls.log')
    (walk / 'log-turns').unlink()

    # 7: the back end's job. The front desk writes the job down through the register named in
    # --mcp-config; the back end launches the work-agents once, and they keep stepping through
    # the gate after its turn ended, until one admitted step with work-agents-finish present is
    # their last.
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
    status = root / 'status-server.py'
    status.write_text(
        'import json, sys\n'
        'for line in sys.stdin:\n'
        '    m = json.loads(line)\n'
        '    if m.get("method") == "initialize":\n'
        '        print(json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": {"protocolVersion": "2025-06-18"}}), flush=True)\n'
        '    elif m.get("method") == "tools/call":\n'
        '        ok = m["params"]["name"] == "delegate" and m["params"]["arguments"] == {"text": "Start the Northwind job and keep Mark on it."}\n'
        '        text = json.dumps({"saved": ok, "to": "stu"})\n'
        '        print(json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": {"content": [{"type": "text", "text": text}], "isError": not ok}}), flush=True)\n')
    front_config = json.dumps({'mcpServers': {'richos_assignments': {'type': 'stdio', 'command': sys.executable, 'args': [str(register)]},
                                              'richos_status': {'type': 'stdio', 'command': sys.executable, 'args': [str(status)]}}})
    back_config = json.dumps({'mcpServers': {'richos_work': {'type': 'stdio', 'command': 'true', 'args': []}}})
    for name in ('agents', 'reply.txt', 'calls.log'):
        (walk / name).unlink(missing_ok=True)
    (walk / 'register').write_text('Start the Northwind job and keep Mark on it.\n')
    (walk / 'work-agents').write_text('Mark\n')
    flag = walk / 'work-agents-finish'
    gate = root / 'gate-work.sh'
    gate.write_text('#!/bin/sh\ncat > /dev/null\nexit 0\n')
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
    check(hooks and hooks[-1] == {'hook_event_name': 'UserPromptSubmit', 'prompt': 'Start the Northwind job and keep Mark on it.'},
          'the turn called the app\'s input hook with its exact words before it went on', hooks[-1:])
    log = (walk / 'calls.log').read_text()
    check('register ok' in log and not (walk / 'register').exists(), 'the job was written down once through the register', log)
    check(log.count('delegate ok') == 1 and log.index('register ok') < log.index('delegate ok'),
          'then it was handed to Stu once, through richos_status delegate', log)
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
    leases = (walk / 'calls.log').read_text()
    check(' lease front-desk front-desk\n' in leases and ' lease back-end back-end\n' in leases,
          'each lease is one calls.log line naming its kind and session', leases)
    started = [f for f in frames if f.get('type') == 'system' and f.get('subtype') == 'task_started']
    calls = [b for f in frames if f.get('type') == 'assistant' for b in f['message']['content'] if b.get('type') == 'tool_use']
    check(len(started) == 1 and started[0].get('task_type') == 'local_agent' and started[0].get('task_id') == 'task-work-agent-1'
          and [(b['name'], b['id'], b['input']['name']) for b in calls] == [('Agent', started[0]['tool_use_id'], 'Mark')],
          'the launch is on the turn\'s stream: the Agent call and task_started (agent_tasks.rs counts helpers from them)', (started, calls))
    deadline = time.time() + 20
    while 'gate work-agent-1 exit 0' not in (walk / 'calls.log').read_text() and time.time() < deadline:
        time.sleep(0.2)
    check('gate work-agent-1 exit 0' in (walk / 'calls.log').read_text(), 'after the turn, the helper steps through the gate (admitted)')
    flag.touch()
    deadline = time.time() + 20
    while events(back_journal, 'SubagentStop') == [] and time.time() < deadline:
        time.sleep(0.2)
    check(events(back_journal, 'SubagentStop') == ['work-agent-1'], 'with work-agents-finish the helper finishes between turns (SubagentStop)')
    check('gate work-agent-1 exit 0 finished' in (walk / 'calls.log').read_text(), 'its last step is named in calls.log')
    after = []  # from the task_notification on: the turn's own result before it is the turn's
    while not after or after[-1].get('type') != 'result':
        line = back.stdout.readline()
        if not line:
            break
        row = json.loads(line)
        if after or row.get('subtype') == 'task_notification':
            after.append(row)
    check([(f.get('type'), f.get('subtype')) for f in after] == [('system', 'task_notification'), ('assistant', None), ('result', 'success')]
          and after[0].get('task_id') == 'task-work-agent-1' and after[0].get('status') == 'completed',
          'its end is on the stream between turns: task_notification completed, then the automatic turn and its result', after)
    second, second_journal = lease('back-end-2', back_config)
    second.stdin.write(user('{"event":"account_moved","from":"Home","to":"Work"}'))
    second.stdin.flush()
    answers(second)
    check(events(second_journal, 'SubagentStart') == [], 'a later back-end lease launches no second set (once per walk)')
    # The app resumes a session with the path of its native transcript, on another account too.
    plug = data / 'engine-profiles' / 'profile-resumed'
    (plug / 'hooks').mkdir(parents=True)
    (plug / 'hooks' / 'hooks.json').write_text((data / 'engine-profiles' / 'profile-back-end' / 'hooks' / 'hooks.json').read_text())
    third = subprocess.Popen(['perl', str(FAKE), '--print', '--resume', str(root / 'Work' / 'projects' / '-fill-first' / 'back-end.jsonl'),
                              '--plugin-dir', str(plug), '--strict-mcp-config', '--mcp-config', back_config],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env)
    third.stdin.write(user('{"event":"account_moved","from":"Home","to":"Work"}'))
    third.stdin.flush()
    answers(third)
    check(events(back_journal, 'SessionStart') == [None, None] and (walk / 'calls.log').read_text().count(' lease back-end back-end\n') == 2,
          'a lease resumed by its transcript\'s path is the same session: its journal and its lease line', events(back_journal, 'SessionStart'))
    for run in (back, second, third):
        run.stdin.close()
        run.wait()

print('fake-claude-fill-first:', 'FAILED' if failed else 'all passed')
sys.exit(1 if failed else 0)
