#!/usr/bin/env python3
"""Exercise real macOS Tauri IPC during slow/failing native turns, with isolated data.

Builds an instrumented copy of the current source. No provider calls, personal
configuration or installed RichOS state. Requires macOS and the Rust toolchain.
Evidence and the temporary source tree remain at the printed directory.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
if sys.platform != "darwin":
    sys.exit("This test requires the actual macOS Tauri/WebKit runtime.")
OUT = Path(tempfile.mkdtemp(prefix="richos-native-regression-"))
APP = OUT / "app"
print(f"Evidence: {OUT}", flush=True)
APP.mkdir()
for name in ("src-tauri", "ui", "crates"):
    shutil.copytree(ROOT / "app" / name, APP / name,
                    ignore=shutil.ignore_patterns("target", "node_modules", "ui-dist", "shots*"))
for name in ("Cargo.toml", "Cargo.lock"):
    shutil.copy2(ROOT / "app" / name, APP / name)
conf = APP / "src-tauri/tauri.conf.json"
c = json.loads(conf.read_text())
c["identifier"] = "com.richos.native-regression"
conf.write_text(json.dumps(c, indent=2))
source = APP / "src-tauri/src/main.rs"
s = source.read_text()
s = s.replace("richos_core::company::install_central_root()", 'Some(PathBuf::from(std::env::var_os("RICHOS_NATIVE_TEST_DIR").unwrap()).join("central"))')
s = s.replace("tauri::generate_handler![", "tauri::generate_handler![native_test_record,", 1)
anchor = '            boot_marker(|marker| marker.setup_returned = true);'
i = s.index(anchor)
s = s[:i] + '''            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let root = PathBuf::from(std::env::var_os("RICHOS_NATIVE_TEST_DIR").unwrap());
                    for _ in 0..300 {
                        if root.join("started").exists() {
                            std::thread::sleep(std::time::Duration::from_millis(300));
                            let start = std::time::Instant::now();
                            let _ = handle.run_on_main_thread(move || {
                                let _ = std::fs::write(root.join("mainloop-ms"), start.elapsed().as_millis().to_string());
                            });
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                });
            }
''' + s[i:]
s += '''
#[tauri::command(async)]
fn native_test_record(app: tauri::AppHandle, payload: String) {
    let root = PathBuf::from(std::env::var_os("RICHOS_NATIVE_TEST_DIR").unwrap());
    std::fs::write(root.join("renderer.json"), payload).unwrap();
    app.exit(0);
}
'''
source.write_text(s)
ui = APP / "ui/main.js"
ui.write_text(ui.read_text() + r'''
(async function nativeRegression() {
  const pause = ms => new Promise(r => setTimeout(r, ms));
  while (!window.RichBridge || !window.__RICHOS_TIMELINE__ || !document.querySelector('.nav-thread')) await pause(100);
  if (window.RichHome?.isOpen()) window.RichHome.hide('native-test');
  const row = document.querySelector('.nav-thread');
  row.click();
  while (window.__RICHOS_TIMELINE__().threadId !== row.dataset.threadId) await pause(100);
  const initialOnboarding = await window.RichBridge.invoke('onboarding_view');
  let wrongCompanyRefused = false;
  try { await window.RichBridge.invoke('decline_onboarding', {entityId: 'wrong-company'}); }
  catch (_) { wrongCompanyRefused = true; }
  const afterWrongCompany = await window.RichBridge.invoke('onboarding_view');
  const scopeGuard = wrongCompanyRefused && initialOnboarding.state === 'not-yet' && afterWrongCompany.state === 'not-yet';
  const events = [], readings = [];
  const start = performance.now();
  let latestTurnId = null;
  let finished;
  const done = new Promise(r => { finished = r; });
  await window.RichBridge.listen('rich://turn-status', e => {
    latestTurnId = e.payload.turnId || e.payload.turn_id || latestTurnId;
    events.push({t: performance.now() - start, status: e.payload.status});
    if (['completed', 'failed', 'interrupted', 'stopped'].includes(e.payload.status)) finished();
  });
  const timer = setInterval(() => readings.push({t: performance.now()-start,
    wait: document.querySelector('.turn-wait')?.textContent || ''}), 100);
  document.getElementById('input').value = 'NATIVE_REGRESSION';
  document.getElementById('send').click();
  // Test the actual Stop IPC while a handshake/prime can be blocking the spine.
  await pause(1000);
  const stopStart = performance.now();
  const mode = location.hash.slice(1);
  // The fixture writes an explicit Stop instruction into a build-time scenario map.
  let stopped = null, staleStopRefused = true;
  if (window.__NATIVE_TEST_STOP__) {
    const stale = await window.RichBridge.invoke('stop_turn', {expectedTurnId: 'an-obsolete-turn'});
    staleStopRefused = stale.stopped === false;
    if (!latestTurnId) throw new Error('No authoritative turn id for Stop');
    stopped = await window.RichBridge.invoke('stop_turn', {expectedTurnId: latestTurnId});
  }
  const stopMs = performance.now() - stopStart;
  await Promise.race([done, pause(20000)]);
  await pause(300);
  clearInterval(timer);
  await window.RichBridge.invoke('native_test_record', {payload: JSON.stringify({events, readings, stopped, stopMs, scopeGuard, staleStopRefused})});
})();
''')
# Set before renderer initialization, in this test app's own WebKit data store.
s = source.read_text().replace(
    '.initialization_script(launch_init_script(kind, start_ordinal))',
    '.initialization_script(launch_init_script(kind, start_ordinal))\n'
    '                    .initialization_script(format!("window.__NATIVE_TEST_STOP__ = {};", std::env::var("RICHOS_NATIVE_TEST_STOP").ok().as_deref() == Some("1")))')
s = s.replace('.visible(activation.presentation == activation::Presentation::Regular)', '.visible(false)')
source.write_text(s)
fake = OUT / "fake-claude"
fake.write_text(r'''#!/usr/bin/env python3
import json, os, sys, threading, time
from pathlib import Path
root = Path(os.environ['RICHOS_NATIVE_TEST_DIR'])
mode = os.environ['RICHOS_NATIVE_TEST_MODE']
stop = threading.Event()
lock = threading.Lock()
def emit(data):
    with lock:
        print(json.dumps(data), flush=True)
def answer(text):
    root.joinpath('started').write_text('yes')
    emit({'type':'system','subtype':'init','tools':['mcp__richos_onboarding__save_company_notes','mcp__richos_onboarding__decline_onboarding']})
    visible = text == 'NATIVE_REGRESSION'
    root.joinpath('frames').open('a').write(('user' if visible else 'prime')+'\n')
    if mode == 'prime-failure' and not visible:
        emit({'type':'result', 'subtype':'error_during_execution', 'is_error':True, 'errors':['fixture prime failed']})
        return
    if mode == 'child-exit' and visible:
        os._exit(1)
    delay = 6 if (visible or mode == 'prime-stop') else 0
    for i in range(delay * 10):
        if stop.wait(.1):
            emit({'type':'result','subtype':'success','stop_reason':'interrupted','is_error':False})
            return
    if visible:
        emit({'type':'assistant','message':{'content':[{'type':'text','text':'Fixture reply.'}]}})
    emit({'type':'result','subtype':'success','stop_reason':'end_turn','is_error':False})
for line in sys.stdin:
    m=json.loads(line)
    if m.get('type') == 'control_request':
        sub=m.get('request',{}).get('subtype')
        if sub == 'initialize' and mode == 'handshake-stop':
            root.joinpath('started').write_text('yes')
            continue
        if sub == 'interrupt': stop.set()
        emit({'type':'control_response','response':{'subtype':'success','request_id':m['request_id'],'response':{}}})
    elif m.get('type') == 'user':
        content=m.get('message',{}).get('content','')
        text=content if isinstance(content,str) else ''.join(x.get('text','') for x in content)
        threading.Thread(target=answer,args=(text,),daemon=True).start()
''')
fake.chmod(0o755)
cargo = shutil.which("cargo") or str(Path.home() / ".cargo/bin/cargo")
env = os.environ.copy()
env["CARGO_TARGET_DIR"] = os.environ.get("RICHOS_NATIVE_TEST_TARGET", str(ROOT / "app/src-tauri/target"))
with (OUT / "build.log").open("w") as log:
    subprocess.run([cargo, "build", "--manifest-path", str(APP / "src-tauri/Cargo.toml")], env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
binary = Path(env["CARGO_TARGET_DIR"]) / "debug/richos-tauri"
results = []
for mode in ("slow-reply", "prime-stop", "handshake-stop", "prime-failure", "child-exit"):
    run = OUT / mode
    data = run / "data"
    engine = run / "engine"
    data.mkdir(parents=True)
    engine.mkdir()
    (data / "entities.json").write_text(json.dumps({"version":1,"entities":[{"id":"fixture","display_name":"Fixture","status":"active","roots":[str(engine)]}]}))
    env.update(RICHOS_NATIVE_TEST_DIR=str(run), RICHOS_NATIVE_TEST_MODE=mode,
               RICHOS_NATIVE_TEST_STOP="1" if mode.endswith("stop") else "0",
               RICHOS_TEST_DATA_DIR=str(data), RICHOS_ENGINE_DIR=str(engine),
               RICHOS_ENTITY="fixture", RICHOS_CLAUDE_BIN=str(fake),
               RICHOS_UPDATE_ENDPOINT="http://127.0.0.1:1/latest.json",
               LORO_ROOT=str(run / "absent-corpus"), RICHOS_LORO_DIR=str(run / "absent-tools"))
    with (run / "native.log").open("w") as log:
        subprocess.run([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT, timeout=45, check=True)
    report = json.loads((run / "renderer.json").read_text())
    assert report["staleStopRefused"], (mode, "stale Stop affected another turn", report)
    assert report["scopeGuard"], (mode, "stale onboarding company accepted", report)
    events = report["events"]
    working = next(e for e in events if e["status"] == "working")
    terminal = [e for e in events if e["status"] in ("completed", "failed", "interrupted", "stopped")]
    assert working["t"] < 1000, (mode, "late working", events)
    assert terminal, (mode, "no terminal outcome", report)
    loop_ms = int((run / "mainloop-ms").read_text())
    assert loop_ms < 500, (mode, "native mainloop blocked", loop_ms)
    if mode.endswith("stop"):
        assert report["stopMs"] < 500, (mode, "Stop IPC blocked", report)
        assert terminal[-1]["t"] < 2500, (mode, "Stop took too long", report)
        assert terminal[-1]["status"] != "completed", (mode, "Stop falsely completed", report)
        assert not (run / "frames").exists() or "user" not in (run / "frames").read_text(), (mode, "sent user after Stop")
    elif mode in ("prime-failure", "child-exit"):
        assert terminal[-1]["status"] != "completed", (mode, "failure falsely completed", report)
    else:
        assert terminal[-1]["status"] == "completed", report
        assert any(r["wait"] for r in report["readings"] if 200 < r["t"] < 1500), "No visible waiting feedback"
    results.append({"scenario":mode,"working_ms":working["t"],"terminal":terminal[-1],"mainloop_ms":loop_ms,"stop_ipc_ms":report["stopMs"]})
    print(json.dumps(results[-1]), flush=True)
(OUT / "results.json").write_text(json.dumps(results, indent=2))
print("PASS: all native responsiveness scenarios", flush=True)
