#!/usr/bin/env python3
"""Deterministic provider boundary for the native conversation permission walk.

Only the provider is a fixture. The installed application must create, render and
answer the permission through its normal policy. This script executes no tools.
It never logs prompts or provider account data. Use only in a disposable guest.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid

MARKER = 'S7_NATIVE_PERMISSION_'
REAL_PROVIDER = '/Users/admin/.local/bin/claude'
TOOLS = ['Bash', 'mcp__richos_onboarding__save_company_notes',
         'mcp__richos_onboarding__decline_onboarding',
         'mcp__richos_continuity__checkpoint', 'mcp__richos_continuity__inspect',
         'mcp__richos_assignments__record', 'mcp__richos_status__background_work']


def emit(value):
    print(json.dumps(value), flush=True)


def result(text=None):
    if text:
        emit({'type': 'assistant', 'message': {'role': 'assistant', 'content': [
            {'type': 'text', 'text': text}]}})
    emit({'type': 'result', 'stop_reason': 'end_turn'})


def serve(lines):
    pending = None
    for line in lines:
        frame = json.loads(line)
        kind = frame.get('type')
        if kind == 'control_request':
            subtype = frame.get('request', {}).get('subtype')
            emit({'type': 'control_response', 'response': {
                'subtype': 'success', 'request_id': frame['request_id'], 'response': {}}})
            if subtype == 'interrupt':
                pending = None
                result()
        elif kind == 'user':
            # The provider boundary includes its startup inventory. These are
            # fixture declarations; the scenario executes none of these tools.
            emit({'type': 'system', 'subtype': 'init', 'model': 's7-fixture',
                  'plugins': [{'name': 'rich-skills'}, {'name': 'richos-app-engine'}],
                  'permissionMode': 'bypassPermissions', 'tools': TOOLS})
            # The app also supplies hidden context. Only an explicit visible
            # walk marker asks for permission; other turns have no tool action.
            content = frame.get('message', {}).get('content', '')
            text = content if isinstance(content, str) else '\n'.join(
                part.get('text', '') for part in content if isinstance(part, dict))
            if MARKER in text:
                if pending is not None:
                    raise ValueError('another turn arrived before the permission decision')
                pending = 's7-' + uuid.uuid4().hex
                emit({'type': 'control_request', 'request_id': pending, 'request': {
                    'subtype': 'can_use_tool', 'tool_name': 'Bash',
                    'description': 'Synthetic permission acceptance check',
                    'input': {'command': 'printf s7-permission-fixture'}}})
            else:
                result('S7 independent conversation replied.' if 'S7_INDEPENDENT' in text else None)
        elif kind == 'control_response' and pending is not None:
            response = frame.get('response', {})
            if response.get('request_id') != pending:
                continue
            decision = response.get('response', {}).get('behavior')
            if decision not in ('allow', 'deny'):
                raise ValueError('permission response has no allow/deny verdict')
            # This is receipt evidence, not a claim a shell action ran.
            result('S7 provider received ' + decision + ' for ' + pending + '.')
            pending = None
    if pending is not None:
        raise ValueError('input closed before the permission decision')


def session_start(argv):
    """Invoke the app's declared lifecycle hook, as the replaced provider would.

    This fixture starts no workers. The real hook records that session's
    observation; the app's worker-settlement gate remains unchanged.
    """
    if '--session-id' not in argv:
        return  # The standalone protocol unit has no app profile.
    session = argv[argv.index('--session-id') + 1]
    plugins = [Path(argv[i + 1]) for i, arg in enumerate(argv[:-1]) if arg == '--plugin-dir']
    profiles = [p for p in plugins if json.loads((p / '.claude-plugin/plugin.json').read_text()).get('name') == 'richos-app-engine']
    if len(profiles) != 1:
        raise ValueError('S7 requires exactly one declared app engine profile')
    hooks = json.loads((profiles[0] / 'hooks/hooks.json').read_text())['hooks']['SessionStart']
    commands = [h for group in hooks for h in group['hooks']]
    if not commands or any(h.get('type') != 'command' for h in commands):
        raise ValueError('S7 requires the declared SessionStart command hook')
    payload = {'hook_event_name': 'SessionStart', 'session_id': session,
               'source': 'startup', 'cwd': os.getcwd()}
    for hook in commands:
        completed = subprocess.run(['/bin/sh', '-c', hook['command']],
                                   input=json.dumps(payload), text=True, capture_output=True,
                                   timeout=min(30, hook.get('timeout', 25)))
        if completed.returncode:
            raise RuntimeError('S7 SessionStart hook refused: exit ' + str(completed.returncode))


def main(argv):
    if argv == ['auth', 'status', '--json']:
        # Keep the real signed-in guest's auth gate. Never invent an account or
        # read/save its credentials; replace this process with the pinned CLI.
        os.execv(REAL_PROVIDER, [REAL_PROVIDER, *argv])
    elif argv[:1] == ['auth']:
        raise ValueError('the S7 fixture supports only read-only auth status')
    elif '--version' in argv:
        print('2.1.283 (synthetic S7 acceptance provider)')
    else:
        session_start(argv)
        serve(sys.stdin)


if __name__ == '__main__':
    main(sys.argv[1:])
