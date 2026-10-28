#!/usr/bin/env bash
# Terminal entrypoint authorization and state-path tests. No live account access.
# run-tests: inputs richos/app/scripts/terminal-quota.test.sh richos/app/Cargo.toml richos/app/Cargo.lock richos/app/crates/richos-core richos/engine/scripts/quota-reset.sh
# run-tests: covers richos/app/crates/richos-core/src/bin/richos-quota.rs richos/app/crates/richos-core/examples/terminal_quota_proof.rs richos/app/crates/richos-core/src/quota/terminal.rs
# run-tests: no-host-screen: richos-quota and terminal_quota_proof are command-line binaries, and the "RichOS.app" it runs is a two-line shell stand-in in a temporary HOME that prints its arguments and exits; nothing is drawn
# The wrapper's machine-wide slot (/Applications) is pointed at the temporary HOME too, so an
# app the user installed in /Applications is never run by this suite.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 - <<'PY'
import json, os, pathlib, subprocess, tempfile
# No clock of this suite's own around real work (audit R13, 2026-09-29): the build waits on
# Cargo's lock behind whatever else builds, and each command below is a real process start,
# so 240 s and 10 s were verdicts on how busy the Mac was. A hang is caught by the enclosing
# runner's per-suite deadline, which names this suite.
build=subprocess.run(['cargo','build','--quiet','-p','richos-core','--bin','richos-quota',
    '--example','terminal_quota_proof','--message-format=json'],capture_output=True,text=True,check=True)
exe={r['target']['name']:r['executable'] for line in build.stdout.splitlines()
     if (r:=json.loads(line)).get('reason')=='compiler-artifact' and r.get('executable')}
with tempfile.TemporaryDirectory(prefix='terminal-quota-cli-') as d:
    root=pathlib.Path(d)
    system_apps=root/'system-Applications'
    env={**os.environ,'HOME':d,'ANTHROPIC_API_KEY':'fixture-auth-override',
         'RICHOS_SYSTEM_APPLICATIONS':str(system_apps)}
    def call(*args):
        return subprocess.run([exe['richos-quota'],*args],env=env,stdin=subprocess.DEVNULL,
            capture_output=True,text=True)
    for args in [('approve','offer'),('approve','offer','--yes'),('--unknown',)]:
        r=call(*args);assert r.returncode==2,(args,r)
    assert not list(root.iterdir()),'refused approvals wrote state'
    r=call('status');assert r.returncode==2 and 'overrides' in r.stderr,r
    assert not list(root.iterdir()),'refused connection wrote state'
    # HOME no longer redirects the account-wide ledger. Never run revoke here:
    # the Rust isolated-store tests cover revocation and the separate-process
    # account-path test proves that a nightly HOME resolves to the same store.
    assert b'--richos-quota-v1' in pathlib.Path(exe['richos-quota']).read_bytes()
    r=subprocess.run([exe['terminal_quota_proof']],env=env,capture_output=True,text=True)
    assert r.returncode!=0 and 'Requires --live-no-redemption' in r.stderr
    fakebin=root/'bin';fakebin.mkdir()
    cargo=fakebin/'cargo'
    cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$@"\n');cargo.chmod(0o700)
    wrapper=pathlib.Path('../engine/scripts/quota-reset.sh').resolve()
    r=subprocess.run([str(wrapper),'tick'],env={**env,'PATH':str(fakebin)+':/usr/bin:/bin'},capture_output=True,text=True)
    assert r.returncode==0 and r.stdout.splitlines()[-2:]==['--','tick'],r
    import shutil
    installed=root/'installed/engine/scripts';installed.mkdir(parents=True)
    shutil.copy(wrapper,installed/'quota-reset.sh')
    app=root/'Applications/RichOS.app/Contents/MacOS/richos-tauri';app.parent.mkdir(parents=True)
    app.write_text('#!/bin/sh\necho GUI-WAS-LAUNCHED\n');app.chmod(0o700)
    r=subprocess.run([str(installed/'quota-reset.sh'),'tick'],env=env,capture_output=True,text=True)
    assert 'GUI-WAS-LAUNCHED' not in r.stdout,'old app was launched'
    assert r.returncode==2 and 'helper is unavailable' in r.stderr,r
    app.write_text('#!/bin/sh\n# --richos-quota-v1\nprintf "%s\\n" "$@"\n')
    r=subprocess.run([str(installed/'quota-reset.sh'),'tick'],env=env,capture_output=True,text=True)
    assert r.returncode==0 and r.stdout.splitlines()==['--richos-quota-v1','tick'],r
    # The machine-wide slot resolves to the fixture, never the real /Applications.
    app.write_text('#!/bin/sh\necho GUI-WAS-LAUNCHED\n')
    system_app=system_apps/'RichOS.app/Contents/MacOS/richos-tauri';system_app.parent.mkdir(parents=True)
    system_app.write_text('#!/bin/sh\n# --richos-quota-v1\necho SYSTEM-FIXTURE\nprintf "%s\\n" "$@"\n');system_app.chmod(0o700)
    r=subprocess.run([str(installed/'quota-reset.sh'),'tick'],env=env,capture_output=True,text=True)
    assert r.returncode==0 and r.stdout.splitlines()==['SYSTEM-FIXTURE','--richos-quota-v1','tick'],r
print('  PASS  noninteractive approval refusal, no yes flag, no credential read on refusal, live-proof opt-in and headless wrapper that runs no app outside its temporary HOME')
PY
