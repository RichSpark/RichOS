#!/usr/bin/env python3
"""Literal private settings, portable storage roots and named-agent scratch."""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import sys
import tempfile

KEYS = (
    'RICHOS_BULK_ROOT', 'CARGO_TARGET_DIR', 'RICHOS_CARGO_CACHE_ROOT',
    'SCCACHE_DIR', 'RICHOS_SCCACHE_TMPDIR', 'RICHOS_BUILD_CACHE',
    'RICHOS_RECOVERY_EVIDENCE_ROOT', 'RICHOS_RECOVERY_WIP_ROOT',
    'RICHOS_CPU_GUARD_STATE', 'RICHOS_ENGINE_PASS_DIR',
    'RICHOS_PROOF_RUN_DIR', 'RICHOS_AUTOCHECK_PROOF_ROOT',
    'RICHOS_TEST_RESULTS_ROOT', 'RICHOS_ANDROID_CACHES_ROOT',
    'RICHOS_NATIVE_IOS_CACHE', 'RICHOS_NATIVE_ANDROID_CACHE', 'RANDROID_CACHE',
    'RICHOS_GPL_SOURCE_CACHE',
    'RICHOS_MOBILE_CACHE',
    'RICHOS_PHONE_LOCK_DIR',
    'RICHOS_PHONE_WATCH_HOME',
    'RICHOS_PHYSICAL_STORE',
    'RICHOS_PHYSICAL_PRODUCTS',
    'GRADLE_USER_HOME',
    'RICHOS_ENGINE_CHECK_PROOF_DIR',
    'RICHOS_CAMPAIGN_PARENT',
)
RESOLVED = 'RICHOS_MACHINE_SETTINGS_RESOLVED'


def parse(text, source='<settings>'):
    result = {}
    for number, raw in enumerate(text.splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith('#'):
            continue
        key, separator, value = line.partition('=')
        key, value = key.strip(), value.strip()
        if not separator or key not in KEYS or key in result:
            raise ValueError(f'{source}:{number}: malformed, duplicate or unknown key {key}')
        if not value or '\x00' in value or not Path(value).is_absolute():
            raise ValueError(f'{source}:{number}: {key} requires a nonempty literal absolute path')
        result[key] = value
    return result


def load(environment=None, home=None):
    env = os.environ if environment is None else environment
    result = {}
    if env.get(RESOLVED) != '1':
        home = Path(home or env.get('HOME') or Path.home())
        explicit = env.get('RICHOS_ENV_FILE')
        source = Path(explicit) if explicit is not None else home / 'Library/Application Support/RichOS/.env'
        if not source.is_absolute():
            raise ValueError('RICHOS_ENV_FILE must be an absolute path')
        try:
            result = parse(source.read_text(encoding='utf-8'), str(source))
        except FileNotFoundError:
            if explicit is not None:
                raise ValueError(f'explicit settings file does not exist: {source}') from None
        except OSError as exc:
            raise ValueError(f'cannot read settings file {source}: {exc}') from exc
    for key in KEYS:
        if key in env:
            value = env[key]
            if not value or '\x00' in value or not Path(value).is_absolute():
                raise ValueError(f'{key} requires a nonempty literal absolute path')
            result[key] = value
    # Keep existing private files valid. The current consumer name wins when both
    # spellings are present, as with the explicit Cargo alias priority below.
    if 'RICHOS_NATIVE_ANDROID_CACHE' in result:
        result.setdefault('RANDROID_CACHE', result['RICHOS_NATIVE_ANDROID_CACHE'])
    return result


def apply():
    os.environ.update(environment())


def environment(environment=None, home=None):
    env = dict(os.environ if environment is None else environment)
    env.update(load(env, home))
    env[RESOLVED] = '1'
    return env


def validate(path, require_directory=False):
    path = Path(path)
    if not path.is_absolute():
        raise ValueError(f'storage path must be absolute: {path}')
    path = path.resolve()
    if path.parts[:2] == ('/', 'Volumes') and len(path.parts) >= 3:
        volume = Path(*path.parts[:3])
        if not volume.is_mount():
            raise ValueError(f'storage volume is not mounted: {volume}')
    if require_directory and not path.is_dir():
        raise ValueError(f'configured bulk root must already be a directory: {path}')
    return path


def storage(kind, name='', key=None, default=None, environment=None):
    env = os.environ if environment is None else environment
    values = load(env)
    aliases = ('RICHOS_CARGO_CACHE_ROOT', 'CARGO_TARGET_DIR') if key == 'CARGO_TARGET_DIR' else (key,)
    for alias in aliases:
        if alias and alias in values:
            return validate(values[alias])
    bulk = values.get('RICHOS_BULK_ROOT')
    if bulk:
        root = validate(bulk, require_directory=True)
        root = root / {'cache': 'caches', 'state': 'state/richos', 'agents': 'tmp/claude'}[kind]
    elif default is not None:
        return validate(default)
    elif kind == 'agents':
        root = Path(env.get('TMPDIR') or tempfile.gettempdir()) / 'richos-agents'
    elif kind == 'cache':
        home = Path(env.get('HOME') or Path.home())
        root = home / ('Library/Caches/RichOS' if sys.platform == 'darwin' else '.cache/richos')
    else:
        root = Path(env.get('HOME') or Path.home()) / '.richos-nightly'
    return validate(root / name)


def cache(name='', key=None, environment=None):
    return storage('cache', name, key, environment=environment)


def state(name='', key=None, default=None, environment=None):
    return storage('state', name, key, default, environment)


def agent_root(environment=None):
    env = os.environ if environment is None else environment
    root = storage('agents', environment=env)
    allocator = (Path(env.get('TMPDIR') or tempfile.gettempdir()) / 'richos-scratch').resolve()
    if root.is_relative_to(allocator):
        raise ValueError('named-agent scratch must be outside the process allocator tree')
    return root


def scratch(name, environment=None):
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,127}', name) or name in ('.', '..'):
        raise ValueError('agent name must be one safe path component')
    root = agent_root(environment)
    target = root / name
    resolved = target.resolve()
    if not resolved.is_relative_to(root) or resolved == root:
        raise ValueError('named scratch escapes its configured root')
    resolved.mkdir(mode=0o700, parents=True, exist_ok=True)
    return resolved


def claude_home(environment=None):
    """The ONE Claude home of this machine, as a string: where the engine's own stores live
    (land locks and land records, the fence settings beside them, second-review records, the
    workspace registry's fallback).

    `CLAUDE_CONFIG_DIR`, else `~/.claude`, EXCEPT where `CLAUDE_CONFIG_DIR` is the added Claude
    account's folder the app gave this lease (`RICHOS_CLAUDE_ACCOUNT_DIR`): then
    `RICHOS_CLAUDE_HOME`, Account 1's folder. On an added account the app points
    `CLAUDE_CONFIG_DIR` at that account's folder, and a store derived from it split by account:
    a Work lease's land lease, fence settings and review records went to claude-accounts/2 while
    the installed fence and a terminal read ~/.claude (richos-hq follow-ups, 2026-10-28; Rich's
    decision 23:58Z: one machine-wide home, the same for terminal and app). A terminal sets none
    of the three and resolves exactly as before.

    WHY THE PAIR, NOT RICHOS_CLAUDE_HOME ALONE. Every test sandbox isolates the engine's stores
    with its own `CLAUDE_CONFIG_DIR` (global-state-witness.sh: "CLAUDE_CONFIG_DIR=$(mktemp -d)").
    Run from an app shell, it inherits the lease's `RICHOS_CLAUDE_HOME`; were that to win, the
    sandbox's land locks and review ledger would be the live ones. A `CLAUDE_CONFIG_DIR` other
    than the one the app set is somebody's own choice, and it wins, as it always has.

    NOT for what Claude Code itself writes (sessions/, projects/): those live under the
    account's own `CLAUDE_CONFIG_DIR`, and their readers keep reading it there.

    Standard library only: install copies this file beside the operator fence (PROGRAM_FILES)."""
    env = os.environ if environment is None else environment
    config = (env.get('CLAUDE_CONFIG_DIR') or '').strip()
    home = (env.get('RICHOS_CLAUDE_HOME') or '').strip()
    account = (env.get('RICHOS_CLAUDE_ACCOUNT_DIR') or '').strip()
    if config and home and account and os.path.normpath(config) == os.path.normpath(account):
        return home
    return config or os.path.join(os.path.expanduser('~'), '.claude')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('command', choices=['show', 'values', 'env', 'scratch', 'cache', 'state', 'agents', 'validate'])
    parser.add_argument('name', nargs='?', default='')
    parser.add_argument('--key')
    parser.add_argument('--array', action='store_true')
    args = parser.parse_args()
    try:
        if args.command == 'values':
            print(json.dumps({**load(), RESOLVED: '1'}))
        elif args.command == 'env':
            values = load()
            for key, value in {**values, RESOLVED: '1'}.items():
                print(f'export {key}={shlex.quote(value)}')
            if args.array:
                print('RICHOS_MACHINE_ENV=(' + ' '.join(shlex.quote(key+'='+value) for key,value in {**values, RESOLVED: '1'}.items()) + ')')
        elif args.command == 'show':
            print(json.dumps({'settings': load(), 'agents': str(agent_root()),
                              'cache': str(cache()), 'state': str(state())}, indent=2))
        elif args.command == 'scratch':
            print(scratch(args.name))
        elif args.command == 'validate':
            print(validate(args.name))
        else:
            print(storage(args.command if args.command != 'agents' else 'agents', args.name, args.key))
    except (ValueError, OSError) as exc:
        parser.exit(2, f'machine settings: {exc}\n')


if __name__ == '__main__':
    main()
