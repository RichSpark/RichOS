#!/usr/bin/env python3
"""Prepare a pinned Cloudflare helper outside the checkout. Print a Tauri overlay.

Use GitHub's asset digest: upstream's release prose hashes are pre-signing hashes
and do not match the published macOS archives. Never accept either hash at runtime.
"""
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import machine_storage as M
import platform
import tarfile
import urllib.request

VERSION = '2026.9.1'
PINS = {
    'arm64': ('aarch64-apple-darwin', 'c27ab8fd0aa489449e3d201eb02f957ef460a13b613662928b1b23394bf1bcfe'),
    'x86_64': ('x86_64-apple-darwin', 'ff0d3b51d5ff70eceef89d6b32145fee985018a2174596a5dbe405e2766e2ac4'),
}

def prepare(cache, machine):
    triple, expected = PINS[machine]
    cache.mkdir(parents=True, exist_ok=True)
    target = cache / ('cloudflared-' + triple)
    receipt = cache / ('verified-' + triple + '.json')
    if target.exists() and receipt.exists():
        saved = json.loads(receipt.read_text())
        if saved.get('archive') == expected and saved.get('binary') == hashlib.sha256(target.read_bytes()).hexdigest():
            return cache / 'cloudflared'
    arch = 'arm64' if machine == 'arm64' else 'amd64'
    url = f'https://github.com/cloudflare/cloudflared/releases/download/{VERSION}/cloudflared-darwin-{arch}.tgz'
    with urllib.request.urlopen(url, timeout=60) as response:
        archive = response.read(40 * 1024 * 1024 + 1)
    if hashlib.sha256(archive).hexdigest() != expected:
        raise RuntimeError('Cloudflare helper archive does not match the pinned asset digest')
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as tf:
        members = [m for m in tf.getmembers() if m.isfile() and Path(m.name).name == 'cloudflared']
        if len(members) != 1 or members[0].size > 100 * 1024 * 1024:
            raise RuntimeError('Unexpected helper archive layout')
        binary = tf.extractfile(members[0]).read()
    pending = target.with_suffix('.pending')
    pending.write_bytes(binary)
    pending.chmod(0o755)
    pending.replace(target)
    receipt.write_text(json.dumps({'version': VERSION, 'archive': expected, 'binary': hashlib.sha256(binary).hexdigest()}))
    return cache / 'cloudflared'

if __name__ == '__main__':
    if platform.system() != 'Darwin' or platform.machine() not in PINS:
        raise SystemExit('The Connect helper currently supports macOS arm64 and x86_64')
    helper_root = M.cache('richos-connect/helper')
    directory = helper_root / VERSION
    directory.mkdir(parents=True, exist_ok=True)
    with (directory / 'prepare.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        base = prepare(directory, platform.machine())
    app = Path(__file__).resolve().parent.parent
    link = app / 'src-tauri/connect-helper'
    if link.is_symlink():
        if not link.resolve().is_relative_to(helper_root):
            raise SystemExit('Refusing to replace an unrelated helper link')
        if link.resolve() != directory:
            link.unlink(); link.symlink_to(directory, target_is_directory=True)
    elif link.exists():
        raise SystemExit('Refusing to replace an existing helper directory')
    else:
        link.symlink_to(directory, target_is_directory=True)
    license_file = app / 'third-party/cloudflared/LICENSE'
    # Tauri retains externalBin in generated application code. A relative source link
    # keeps build-machine paths out of the executable while bulk bytes stay on SSD.
    print(json.dumps({'bundle': {'externalBin': ['connect-helper/cloudflared'], 'resources': {str(license_file): 'licenses/cloudflared-LICENSE'}}}))
