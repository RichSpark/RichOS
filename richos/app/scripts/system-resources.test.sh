#!/usr/bin/env bash
# run-tests: no-host-screen: native host sampler and isolated provider configuration, no UI
# run-tests: inputs richos/app/scripts/system-resources.test.sh richos/app/crates/richos-core/src/system_resources.rs richos/app/crates/richos-core/src/status_tools.rs richos/app/crates/richos-core/examples/system_resources_probe.rs richos/app/crates/richos-core/Cargo.toml richos/engine/scripts/lib/second_review.py
# run-tests: covers richos/app/crates/richos-core/examples/system_resources_probe.rs
set -euo pipefail
APP="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=lib/cargo-cache-env.sh
source "$APP/scripts/lib/cargo-cache-env.sh"
# shellcheck source=../../engine/scripts/lib/scratch.sh
source "$APP/../engine/scripts/lib/scratch.sh"
RESOURCE_TEST_SCRATCH="$(scratch_new resource-read)"
trap 'scratch_release "$RESOURCE_TEST_SCRATCH" >/dev/null 2>&1 || true' EXIT
cargo build --manifest-path "$APP/Cargo.toml" -p richos-core --example system_resources_probe --message-format=json > "$RESOURCE_TEST_SCRATCH/build.jsonl"
PYTHONDONTWRITEBYTECODE=1 python3 - "$APP" "$RESOURCE_TEST_SCRATCH" <<'PY'
import importlib.util, json, os, pathlib, selectors, subprocess, sys, time
sys.dont_write_bytecode = True
app, scratch = map(pathlib.Path, sys.argv[1:])
rows = [json.loads(line) for line in (scratch / 'build.jsonl').read_text().splitlines()]
probe = next(row['executable'] for row in rows if row.get('executable') and row.get('target',{}).get('name')=='system_resources_probe')
# The test scratch's own native socket is removed by the bounded child on exit.
state = scratch / 'state'
if sys.platform == 'darwin':
    server = subprocess.Popen([probe,'serve',str(state),'3'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
    try:
        ready = selectors.DefaultSelector(); ready.register(server.stdout,selectors.EVENT_READ)
        assert ready.select(10), 'native sampler did not publish a sample within the hang guard'
        line = server.stdout.readline()
        assert line, server.communicate(timeout=5)
        first = json.loads(line)
        assert first['status']=='available' and first['current']['interval_ms']>=500
        started = time.monotonic()
        read = json.loads(subprocess.check_output([probe,'--system-resources',str(state)],text=True,timeout=5))
        elapsed = time.monotonic()-started
        assert read['instance_id']==first['instance_id'] and read['current'] and read['age_ms']<=3000
        requests = [dict(jsonrpc='2.0',id=1,method='initialize'),dict(jsonrpc='2.0',id=2,method='tools/list'),
                    dict(jsonrpc='2.0',id=3,method='tools/call',params=dict(name='system_resources',arguments={})),
                    dict(jsonrpc='2.0',id=4,method='tools/call',params=dict(name='system_resources',arguments={'path':'elsewhere'}))]
        replies = [json.loads(line) for line in subprocess.check_output([probe,'--resources-mcp',str(state)],
            input=''.join(json.dumps(r)+'\n' for r in requests),text=True,timeout=5).splitlines()]
        tools = replies[1]['result']['tools']
        assert len(tools)==1 and tools[0]['name']=='system_resources' and tools[0]['annotations']['readOnlyHint']
        observed = json.loads(replies[2]['result']['content'][0]['text'])
        assert observed['instance_id']==read['instance_id'] and observed['status']=='available'
        assert 'error' in replies[3], 'resource tool accepted an alternate path'
        print('PASS native sampler, separate-process cached read and read-only MCP; read_ms=%.3f' % (elapsed*1000))
        stdout, stderr = server.communicate(timeout=10)
        assert server.returncode==0,(stdout,stderr)
    finally:
        ready.close()
        if server.poll() is None: server.terminate(); server.communicate(timeout=5)
    missing = json.loads(subprocess.check_output([probe,'--system-resources',str(state)],text=True,timeout=5))
    assert missing['status']=='unavailable' and missing['current'] is None
    print('PASS stopped sampler is unavailable, never idle')
else:
    unavailable = json.loads(subprocess.check_output([probe,'--system-resources',str(state)],text=True,timeout=5))
    assert unavailable['status']=='unavailable' and unavailable['current'] is None
    print('PASS unsupported platform reports unavailable; native Mac sampling requires macOS')
# Both review providers receive the real reader through their own supported MCP configuration.
spec = importlib.util.spec_from_file_location('resource_review',app.parent/'engine/scripts/lib/second_review.py')
review = importlib.util.module_from_spec(spec);sys.modules[spec.name]=review;spec.loader.exec_module(review)
old = dict(os.environ)
try:
    os.environ['RICHOS_RESOURCE_READER']=probe;os.environ['RICHOS_APP_STATE']=str(state)
    expected = {'richos_status':{'command':probe,'args':['--resources-mcp',str(state)]}}
    assert review.resource_mcp()==expected
    claude = review.claude_argv('/fixture/claude')
    assert json.loads(claude[claude.index('--mcp-config')+1])['mcpServers']==expected
    codex = review.codex_argv('/fixture/codex','/fixture/export','/fixture/schema','/fixture/answer')
    configs = [codex[i+1] for i,arg in enumerate(codex[:-1]) if arg=='-c']
    assert 'mcp_servers.richos_status.command='+json.dumps(probe) in configs
    assert 'mcp_servers.richos_status.args='+json.dumps(['--resources-mcp',str(state)]) in configs
    os.environ.pop('RICHOS_RESOURCE_READER')
    assert review.resource_mcp()=={}
    assert '--mcp-config' not in review.claude_argv('/fixture/claude')
    print('PASS Claude and Codex review configuration expose only the shared host read')
finally:
    os.environ.clear();os.environ.update(old)
PY
