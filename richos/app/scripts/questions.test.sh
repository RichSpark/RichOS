#!/usr/bin/env bash
# Prove the question store/engine boundary and the opt-in verification executables.
# The executable checks use a local protocol fixture, never credentials or a model request.
# Real-provider results remain a separate acceptance record.
# run-tests: no-host-screen: Rust tests and pipe-only provider fixtures
# run-tests: inputs richos/app/scripts/questions.test.sh richos/app/crates/richos-core richos/app/Cargo.toml richos/app/Cargo.lock
# run-tests: covers richos/app/crates/richos-core/src/question_engine.rs richos/app/crates/richos-core/examples/questions_roundtrip.rs richos/app/crates/richos-core/examples/question_words_eval.rs
set -euo pipefail
cd "$(dirname "$0")/.."
case "${1:-}" in
  "") cargo test -p richos-core --test questions ;;
  --probes-only) ;; # Retry only the executable checks; keep the store suite's passing result.
  *) echo "usage: questions.test.sh [--probes-only]" >&2; exit 2 ;;
esac
python3 - <<'PY'
import json, os, pathlib, subprocess, sys, tempfile

build = subprocess.run(['cargo', 'build', '-p', 'richos-core', '--example', 'questions_roundtrip',
                        '--example', 'question_words_eval', '--message-format=json'],
                       capture_output=True, text=True, timeout=240, check=True)
executables = {}
for line in build.stdout.splitlines():
    row = json.loads(line)
    if row.get('reason') == 'compiler-artifact' and row.get('executable'):
        executables[row['target']['name']] = row['executable']

with tempfile.TemporaryDirectory(prefix='questions-proof-', dir=os.environ['TMPDIR']) as tmp:
    root = pathlib.Path(tmp)
    fixture = root / 'claude'
    fixture.write_text('#!' + sys.executable + '\n' + r'''
import json, os, pathlib, subprocess, sys
def emit(value): print(json.dumps(value), flush=True)
config = json.loads(sys.argv[sys.argv.index('--mcp-config') + 1]) if '--mcp-config' in sys.argv else {}
for line in sys.stdin:
    request = json.loads(line)
    if request['type'] == 'control_request':
        emit({'type':'control_response','response':{'subtype':'success','request_id':request['request_id'],'response':{}}})
        continue
    if request['type'] != 'user': continue
    emit({'type':'system','subtype':'init','plugins':[{'name':'rich-skills'}], 'permissionMode':'auto',
          'tools':['mcp__richos_onboarding__save_company_notes','mcp__richos_onboarding__decline_onboarding',
                   'mcp__richos_assignments__record','mcp__richos_status__background_work']})
    content = request.get('message', {}).get('content', '')
    prompt = content[-1]['text'] if isinstance(content, list) else content
    try:
        context = json.loads(prompt)
    except (TypeError, ValueError):
        context = None
    if isinstance(context, dict) and context.get('event') == 'context_restored':
        emit({'type':'result','stop_reason':'end_turn','subtype':'success'})
        continue
    if 'Classify as answer only' in prompt:
        rows = [{'index':i,'kind':'answer' if i < 20 else 'unrelated','option_ids':[]} for i in range(40)]
        if os.environ.get('QUESTION_BAD_CLASSIFICATION'): rows[0]['kind'] = 'unrelated'
        text = json.dumps(rows)
    elif 'synthetic UI verification' in prompt and not pathlib.Path(os.environ['QUESTION_ASKED']).exists():
        server = config['mcpServers']['richos_questions']
        frames = [{'jsonrpc':'2.0','id':1,'method':'initialize','params':{}},
                  {'jsonrpc':'2.0','id':2,'method':'tools/call','params':{'name':'ask','arguments':{'questions':[
                      {'text':'When should the release ship?','options':[
                          {'label':'Ship today','description':'Earlier fixes'},
                          {'label':'Ship tomorrow','description':'More testing'}]}]}}}]
        pathlib.Path(os.environ['QUESTION_ASKED']).write_text('asking')
        reply = subprocess.run([server['command'], *server['args']], input=''.join(json.dumps(f)+'\n' for f in frames),
                               capture_output=True, text=True, timeout=10, check=True)
        assert 'recorded' in reply.stdout, reply.stdout + reply.stderr
        text = 'The question is recorded.'
    else:
        text = 'Got it. On it!'
    emit({'type':'assistant','message':{'content':[{'type':'text','text':text}]}})
    emit({'type':'result','stop_reason':'end_turn','subtype':'success'})
''')
    fixture.chmod(0o700)
    env = {**os.environ, 'RICHOS_CLAUDE_BIN':str(fixture), 'QUESTION_ASKED':str(root / 'asked')}
    def run(name, tag, extra=None):
        return subprocess.run([executables[name], str(root / tag)], env={**env, **(extra or {})},
                              capture_output=True, text=True, timeout=60)
    result = run('questions_roundtrip', 'roundtrip')
    assert result.returncode == 0, result.stdout + result.stderr
    assert '"phone_answer_delivered_once":true' in result.stdout, result.stdout
    print('PASS production MCP ask, provider-owned turn end, restart and one receiving turn')
    result = run('question_words_eval', 'words')
    assert result.returncode == 0, result.stdout + result.stderr
    assert '"answer_shaped_correct":20' in result.stdout and '"unrelated_correct":20' in result.stdout
    result = run('question_words_eval', 'bad-words', {'QUESTION_BAD_CLASSIFICATION':'1'})
    assert result.returncode != 0 and '"answer_shaped_correct":19' in result.stdout, result.stdout + result.stderr
    print('PASS counted evaluation accepts 40 correct classifications and rejects a wrong classification')
PY
