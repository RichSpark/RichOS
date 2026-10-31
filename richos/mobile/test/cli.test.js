const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const { mkdirSync, readFileSync, writeFileSync, existsSync, rmSync } = require('node:fs');
const { join, resolve } = require('node:path');
const { createScratch } = require('./storage.cjs');
const mobile = resolve(__dirname, '..');

function session(t) {
  // The real CLI enforces the mounted external volume. Never use a user's session.
  const dir = createScratch('cli');
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const cache = join(dir, 'cache');
  const env = { ...process.env, TMPDIR: dir + '/', RICHOS_MOBILE_CACHE: cache };
  function raw(...args) {
    const result = spawnSync(process.execPath, [join(mobile, 'cli/mobile.mjs'), ...args], { env, encoding: 'utf8', timeout: 15000 });
    assert.ifError(result.error);
    return result;
  }
  function run(...args) {
    const result = raw(...args);
    assert.equal(result.status, 0, result.stderr);
    const body = JSON.parse(result.stdout);
    assert.equal(body.ok, true);
    assert(Number.isFinite(body.elapsedMs));
    return body.result;
  }
  return { dir, cache, env, raw, run };
}

test('separate CLI processes persist the selected conversation, draft and queued send', (t) => {
  const { run } = session(t);
  run('headless', 'fixture', 'offline');
  run('headless', 'action', JSON.stringify({ type: 'select-thread', threadId: 'planning' }));
  run('headless', 'action', JSON.stringify({ type: 'compose', text: 'Persist across CLI processes' }));
  assert.equal(run('headless', 'state').state.draft, 'Persist across CLI processes');
  const sent = run('headless', 'action', '{"type":"send"}').state;
  assert.equal(sent.outbox.length, 1);
  assert.equal(sent.outbox[0].threadId, 'planning');
  assert.deepEqual(run('headless', 'restart').state.outbox, sent.outbox);
  const online = run('headless', 'action', '{"type":"network","online":true}').state;
  assert.equal(online.outbox.length, 0);
  assert.equal(online.environment.receipts[0].clientId, sent.outbox[0].clientId);
});

test('suite storage honors the selected TMPDIR', (t) => {
  const { dir } = session(t);
  const result = spawnSync(process.execPath, ['-e',
    'const fs=require("node:fs"); const {createScratch}=require(process.argv[1]); const p=createScratch("ordinary-shell"); console.log(p); fs.rmdirSync(p);',
    join(__dirname, 'storage.cjs')], { env: { ...process.env, TMPDIR: dir }, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  assert(result.stdout.trim().startsWith(dir + '/'));
  assert(!existsSync(result.stdout.trim()));
});

test('suite launcher forwards and removes its selected scratch and refuses an unusable allocator', (t) => {
  const { dir, env } = session(t);
  const bin = join(dir, 'bin');
  mkdirSync(bin);
  writeFileSync(join(bin, 'npm'), '#!/bin/sh\nprintf \'%s\\n\' "$TMPDIR"\n', { mode: 0o755 });
  const suite = join(__dirname, 'run-suite.cjs');
  for (const value of [undefined, dir]) {
    const childEnv = { ...env, PATH: `${bin}:${env.PATH}` };
    if (value) childEnv.TMPDIR = value;
    else delete childEnv.TMPDIR;
    // A controlled platform temp directory keeps the unset-TMPDIR case in our sandbox.
    const code = `require('node:os').tmpdir=()=>${JSON.stringify(dir)};process.argv=[process.execPath,${JSON.stringify(suite)},'headless'];require(${JSON.stringify(suite)});`;
    const result = spawnSync(process.execPath, ['-e',code], { env: childEnv, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    assert(result.stdout.trim().startsWith(dir + '/'));
    assert(!existsSync(result.stdout.trim()), 'Suite scratch must be removed after child exit');
  }
  const refused = spawnSync(process.execPath, [suite,'headless'], {env:{...env,TMPDIR:'/var/empty'},encoding:'utf8'});
  assert.equal(refused.status,2,refused.stderr);
  assert.match(refused.stdout,/NOT RUN.*Cannot allocate mobile scratch/);
});

test('CLI scenarios expose their real traces and durable results', (t) => {
  const { run } = session(t);
  for (const [name, steps] of [['offline-reconnect', 10], ['revoked', 4], ['interrupted', 2]]) {
    const result = run('headless', 'scenario', name);
    assert.equal(result.trace.length, steps);
    // lastSend is transient process output. The queue, session and simulated Mac
    // receipts are durable; reopening does not pretend to have just sent again.
    assert.deepEqual(run('headless', 'state').state, { ...result.state, lastSend: null });
  }
});

test('voice CLI scenario resumes the same file and message after a client restart', t => {
  const { run } = session(t);
  const result = run('client', 'scenario', 'voice-restart');
  const queued = result.trace.find(step => step.action === 'record-send').state.outbox[0];
  const restored = result.trace.find(step => step.action === 'relaunch').state.outbox[0];
  assert.equal(queued.kind, 'voice');
  assert.deepEqual(restored, queued);
  assert.equal(result.state.outbox.length, 0);
  assert.equal(result.state.lastSend.sent, 1);
  assert.equal(result.state.lastSend.accepted[0].item.clientId, queued.clientId);
  assert.equal(result.state.lastSend.accepted[0].item.fileId, queued.fileId);
});

test('invalid JSON, actions and modes fail without poisoning the CLI lock', (t) => {
  const { run, raw, cache } = session(t);
  for (const args of [['headless', 'action', '{'], ['headless', 'action', '{"type":"unknown"}'], ['unknown']]) {
    const result = raw(...args);
    assert.equal(result.status, 1);
    assert.equal(JSON.parse(result.stderr).ok, false);
    assert(!existsSync(join(cache, 'cli.lock')));
    assert.equal(run('headless', 'state').state.outbox.length, 0);
  }
  mkdirSync(join(cache, 'cli.lock'));
  writeFileSync(join(cache, 'cli.lock/owner.json'), '{"pid":123}');
  assert.match(JSON.parse(raw('headless', 'state').stderr).error, /Another mobile CLI command owns/);
  assert(existsSync(join(cache, 'cli.lock/owner.json')), 'Do not remove another process lock');
});

test('integration profile preserves the ordinary CLI session and uses a separate build cache', t => {
  const { run, env, cache } = session(t);
  run('headless', 'action', '{"type":"compose","text":"Ordinary session"}');
  const result = spawnSync(process.execPath, [join(mobile, 'cli/mobile.mjs'), 'headless', 'action', '{"type":"compose","text":"Integration session"}'], {
    env: { ...env, RICHOS_MOBILE_TEST_APP: 'integration' }, encoding: 'utf8', timeout: 15000
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).result.state.draft, 'Integration session');
  assert.equal(run('headless', 'state').state.draft, 'Ordinary session');
  assert(existsSync(join(cache, 'integration/headless.json')));
});

test('storage transport keeps a private-file snapshot across later child work', (t) => {
  const dir = createScratch('settings');
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const bulk = join(dir, 'bulk with spaces'); mkdirSync(bulk);
  const source = join(dir, 'settings'); writeFileSync(source, 'RICHOS_BULK_ROOT=' + bulk + '\n');
  const env = { ...process.env, HOME: dir, TMPDIR: dir, RICHOS_ENV_FILE: source };
  delete env.RICHOS_MACHINE_SETTINGS_RESOLVED; delete env.RICHOS_BULK_ROOT;
  const transport = join(mobile, 'cli/machine-settings.mjs');
  const code = `import { cacheDir, machineEnvironment } from ${JSON.stringify('file://' + transport)};
    import { writeFileSync } from 'node:fs';
    import { execFileSync } from 'node:child_process';
    const first = cacheDir('first');
    writeFileSync(process.env.RICHOS_ENV_FILE, 'UNKNOWN=/invalid\\n');
    const second = cacheDir('second');
    const child = execFileSync('python3', [${JSON.stringify(join(mobile, '../engine/scripts/lib/machine_settings.py'))}, 'cache', 'child'], { env: machineEnvironment(), encoding: 'utf8' }).trim();
    console.log(JSON.stringify([first, second, child]));`;
  const result = spawnSync(process.execPath, ['--input-type=module', '-e', code], { env, encoding: 'utf8', timeout: 15000 });
  assert.equal(result.status, 0, result.stderr);
  assert.deepEqual(JSON.parse(result.stdout), ['first', 'second', 'child'].map(name => join(bulk, 'caches', name)));
});


test('CLI cache uses the private setting on its first and later calls', (t) => {
  const dir = createScratch('simulator-settings');
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const selected = join(dir, 'selected cache');
  const settings = join(dir, 'settings');
  writeFileSync(settings, 'RICHOS_MOBILE_CACHE=' + selected + '\n');
  const env = { ...process.env, HOME: dir, TMPDIR: dir, RICHOS_ENV_FILE: settings };
  delete env.RICHOS_MACHINE_SETTINGS_RESOLVED;
  delete env.RICHOS_MOBILE_CACHE;
  const module = 'file://' + join(mobile, 'cli/cache.mjs');
  const code = `const {cacheRoot} = await import(${JSON.stringify(module)}); console.log(JSON.stringify([cacheRoot(),cacheRoot()]));`;
  const result = spawnSync(process.execPath, ['--input-type=module','-e',code], { env, encoding:'utf8', timeout:15000 });
  assert.equal(result.status,0,result.stderr);
  assert.deepEqual(JSON.parse(result.stdout),[selected,selected]);
});
