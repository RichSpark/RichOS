#!/usr/bin/env bash
# run-tests: no-host-screen: pinned archive and cache validation only
# run-tests: inputs richos/engine/scripts/lib/machine_settings.py richos/app/scripts/lib/machine_storage.py richos/app/scripts/connect-helper.test.sh richos/app/scripts/prepare-connect-helper.py richos/app/third-party/cloudflared
# run-tests: covers richos/app/scripts/prepare-connect-helper.py
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
python3 - "$here" <<'PY'
import importlib.util, io, os, sys, tarfile, tempfile, shutil, hashlib, json
from pathlib import Path
from unittest.mock import patch
spec=importlib.util.spec_from_file_location('helper',Path(sys.argv[1])/'prepare-connect-helper.py')
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
scratch=Path(tempfile.mkdtemp(prefix='connect-helper-'))
try:
    binary=b'fixture executable'
    data=io.BytesIO()
    with tarfile.open(fileobj=data,mode='w:gz') as tf:
        member=tarfile.TarInfo('cloudflared'); member.size=len(binary); tf.addfile(member,io.BytesIO(binary))
    archive=data.getvalue(); m.PINS={'arm64':('aarch64-apple-darwin',hashlib.sha256(archive).hexdigest())}
    calls=[]
    def download(*args,**kwargs): calls.append(args[0]); return io.BytesIO(archive)
    with patch.object(m.urllib.request,'urlopen',download):
        base=m.prepare(scratch,'arm64'); target=Path(str(base)+'-aarch64-apple-darwin')
        assert target.read_bytes()==binary
        m.prepare(scratch,'arm64'); assert len(calls)==1, 'verified cache was downloaded again'
        target.write_bytes(b'changed'); m.prepare(scratch,'arm64')
        assert len(calls)==2 and target.read_bytes()==binary, 'tampered cache trusted'
    target.unlink()
    with patch.object(m.urllib.request,'urlopen',lambda *a,**k:io.BytesIO(b'wrong archive')):
        try: m.prepare(scratch,'arm64')
        except RuntimeError: pass
        else: raise AssertionError('unverified download accepted')
    assert not target.exists()
    print('  PASS  helper digest, verified cache reuse, tamper recovery and refusal before extraction')
finally: shutil.rmtree(scratch)
PY
