#!/usr/bin/env bash
# run-tests: no-host-screen: reads published source and exercises a disposable Git fixture
# run-tests: inputs richos/app/scripts/lib/no_host_paths.py richos/app/scripts/runtime-sources.json richos/engine/scripts/lib/machine_paths.py richos/engine/scripts/lib/machine_paths.test.sh richos/engine/scripts/publication-completeness.py richos/engine/scripts/publication-machine-path-baseline.json richos/tools/myrichos
# run-tests: covers richos/tools/myrichos/build-myrichos.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd -P)"
ROOT="$(cd "$HERE/../../.." && pwd -P)"
bash "$ROOT/richos/engine/scripts/lib/machine_paths.test.sh"
python3 -B - "$ROOT" <<'PYFIXTURE'
from pathlib import Path
import os, shutil, subprocess, sys, tempfile
source=Path(sys.argv[1])
with tempfile.TemporaryDirectory(prefix='portable checkout ') as directory:
    # Resolved: Git reports a worktree by its real path, and macOS's default TMPDIR is under
    # /var, a symlink to /private/var, so the unresolved name never matched the pointer.
    base=Path(directory).resolve(); primary=base/'primary'; primary.mkdir()
    script=primary/'richos/tools/myrichos/build-myrichos.sh'
    script.parent.mkdir(parents=True);shutil.copy2(source/'richos/tools/myrichos/build-myrichos.sh',script)
    shutil.copytree(source/'richos/tools/myrichos/seed',script.parent/'seed')
    doctrine=primary/'richos/app/crates/richos-core/doctrine/inner-doctrine.md'
    doctrine.parent.mkdir(parents=True);doctrine.write_text('fixture doctrine')
    hooks=base/'femcboost/scripts/hooks';hooks.mkdir(parents=True)
    for name in ('ecs-session-start.sh','ecs-user-prompt-submit.sh','ecs-stop.sh','ecs-teammate-idle.sh','ecs-task-completed.sh'):
        (hooks/name).write_text('#!/bin/sh\ntrue\n')
    deeply=base/'deeply/scripts/hooks';deeply.mkdir(parents=True)
    (deeply/'deeply-design-brief-gate.sh').write_text('#!/bin/sh\ntrue\n')
    def git(*args):
        subprocess.run(['git','-C',str(primary),'-c','core.hooksPath=/dev/null', '-c','user.name=Fixture','-c','user.email=fixture@localhost',*args],check=True,capture_output=True)
    git('init','-b','main');git('add','.');git('commit','-m','fixture')
    worktree=base/'separate worktree';git('worktree','add','--detach',str(worktree))
    destination=base/'personal folder'
    env={k:v for k,v in os.environ.items() if k not in ('RICHOS_ROOT','FEMCBOOST_ROOT','DEEPLY_ROOT')}
    env['MYRICHOS_ROOT']=str(destination)
    result=subprocess.run(['bash',str(worktree/'richos/tools/myrichos/build-myrichos.sh')],env=env,capture_output=True,text=True)
    assert result.returncode==0,result.stderr
    pointer=(destination/'me/doctrine.source.md').read_text()
    assert 'canonical: '+str(doctrine) in pointer
    assert (destination/'companies/femcboost/guards/ecs-session-start.sh').read_text()=='#!/bin/sh\ntrue\n'
    git('worktree','remove',str(worktree))
    assert doctrine.is_file()
    assert 'canonical: '+str(doctrine) in (destination/'me/doctrine.source.md').read_text()
print('PASS primary-checkout discovery, sibling guard source and durable doctrine pointer')
PYFIXTURE

python3 -B - "$HERE/lib" <<'PYARTIFACT'
from pathlib import Path
import hashlib, sys, tempfile
sys.path.insert(0,sys.argv[1]);import no_host_paths as N
with tempfile.TemporaryDirectory(prefix='artifact exception ') as directory:
    root=Path(directory);runtime=root/'runtime';file=runtime/'bin/tool';file.parent.mkdir(parents=True)
    prefix='/Users/'+'upstream';blob=b'HEADER\0'+(prefix+'/source.c').encode()+b'\0TAIL'
    file.write_bytes(blob)
    row={'member':'bin/tool','sha256':hashlib.sha256(blob).hexdigest(),'reason':'pinned upstream fixture','prefixes':[prefix]}
    policy={'upstream_path_exemptions':[row]}
    assert N.scan(str(root),N.DEFAULT_ALLOWED,None)
    assert not N.scan(str(root),N.DEFAULT_ALLOWED,None,upstream_policy=policy,runtime_root=runtime)
    file.write_bytes(blob+b'changed')
    assert N.scan(str(root),N.DEFAULT_ALLOWED,None,upstream_policy=policy,runtime_root=runtime)
    file.write_bytes(blob)
    another=runtime/'another';another.write_bytes(blob)
    assert N.scan(str(root),N.DEFAULT_ALLOWED,None,upstream_policy=policy,runtime_root=runtime)
    another.unlink();file.write_bytes(b'machine-path-exempt: arbitrary reason\0'+blob)
    assert N.scan(str(root),N.DEFAULT_ALLOWED,None,reviewed_text=True)
    try:N.scan(str(root),N.DEFAULT_ALLOWED,None,upstream_policy=policy,runtime_root=root.parent)
    except ValueError:pass
    else:raise AssertionError('outside artifact exception root accepted')
print('PASS artifact exceptions bind exact bytes/member/prefix and binary markers never waive')
PYARTIFACT
