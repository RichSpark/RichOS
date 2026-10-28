"""Compatibility transport for the standalone Rust developer tools.

Builds only developer tooling, using the existing workspace-private Cargo cache.
The desktop runtime and installed terminal hooks never import this module.
"""
import functools
import json
import os
from pathlib import Path
import subprocess
import sys


def toolchain_environment(environment, cargo):
    """Keep rustup reachable when an engine suite gives itself a private HOME.

    A standard rustup proxy on PATH identifies its installation independently of
    HOME. Preserve explicit tool locations and never replace the isolated HOME.
    """
    result = dict(environment)
    cargo_home = Path(cargo).absolute().parent.parent
    if (cargo_home.name == '.cargo' and (cargo_home / 'bin/rustup').is_file()
            and (cargo_home.parent / '.rustup').is_dir()):
        result.setdefault('CARGO_HOME', str(cargo_home))
        result.setdefault('RUSTUP_HOME', str(cargo_home.parent / '.rustup'))
    return result


@functools.lru_cache(maxsize=1)
def binary():
    supplied = os.environ.get('RICHOS_DEVTOOLS_BIN')
    if supplied:
        path = Path(supplied)
        if not path.is_absolute() or not path.is_file():
            raise RuntimeError('RICHOS_DEVTOOLS_BIN must name an existing absolute executable')
        return str(path)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import cargo_identity
    manifest = Path(__file__).resolve().parents[2] / 'crates/richos-devtools/Cargo.toml'
    cargo = cargo_identity.cargo_executable(os.environ)
    if not cargo:
        raise RuntimeError('Rust developer tools need Cargo in the development environment')
    environment = cargo_identity.enable_environment(toolchain_environment(os.environ, cargo))
    _, target = cargo_identity.target_dir(manifest.parent, environment)
    result = subprocess.run([cargo, 'build', '--locked', '--manifest-path', str(manifest),
                             '--target-dir', str(target), '--message-format=json'],
                            env=environment, stdin=subprocess.DEVNULL,
                            stdout=subprocess.PIPE, text=True, check=True)
    for line in result.stdout.splitlines():
        row = json.loads(line)
        if row.get('reason') == 'compiler-artifact' and row.get('executable'):
            return row['executable']
    raise RuntimeError('Cargo returned no developer-tool executable')


def request(operation, value):
    result = subprocess.run([binary(), 'rpc', operation], input=json.dumps(value),
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    reply = json.loads(result.stdout) if result.stdout else {}
    if result.returncode:
        error = reply.get('error', result.stderr.strip() or 'developer tool refused input')
        if error.startswith('IO: '):
            raise OSError(error[4:])
        raise ValueError(error)
    return reply['result']


def cli(tool, args):
    return subprocess.call([binary(), tool, *map(str, args)])


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--binary':
        print(binary())
    elif len(sys.argv) > 1:
        raise SystemExit(cli(sys.argv[1], sys.argv[2:]))
    else:
        raise SystemExit('usage: devtools_bridge.py TOOL [ARGUMENTS]')
