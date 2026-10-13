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
