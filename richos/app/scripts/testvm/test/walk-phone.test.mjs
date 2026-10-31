// The walk phone (testvm/walk-phone.mjs), run as the walks run it, against the harness
// Mac (richos/web/web-app/test/stub-mac.js): TLS from its own CA, trusted the way a system trust
// store is (NODE_EXTRA_CA_CERTS), each request's ECDSA signature verified, and the press on the
// Mac required before the phone may act. No virtual machine and no network beyond 127.0.0.1.
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const require = createRequire(import.meta.url);
const { createStubMac } = require('../../../../web/web-app/test/stub-mac.js');
const { phraseFromHex } = require('../../../../web/web-app/lib/fingerprint.js');
const PHONE = fileURLToPath(new URL('../walk-phone.mjs', import.meta.url));
const run = promisify(execFile);

test('the walk phone pairs, waits for the Mac, sends on the chosen conversation, and can refuse the words', { timeout: 120000 }, async t => {
  const dir = mkdtempSync(join(tmpdir(), 'walk-phone-'));
  const mac = createStubMac({ host: 'localhost', bindHost: '127.0.0.1', history: 0, macPress: 'manual',
    pairCode: 'walk-pair-code', capabilities: ['text'],
    threads: [{ id: 'thread-main', title: 'Rich' }, { id: 'thread-acme', title: 'Acme' }] });
  const port = await mac.listen(0);
  t.after(async () => { await mac.close(); rmSync(dir, { recursive: true, force: true }); });
  writeFileSync(join(dir, 'ca.pem'), mac.caPem);
  const env = { ...process.env, NODE_EXTRA_CA_CERTS: join(dir, 'ca.pem') };
  const phone = async (...args) => JSON.parse((await run('node', [PHONE, ...args], { env, timeout: 60000 })).stdout.trim());
  const link = `https://localhost:${port}/#pair=walk-pair-code`;

  const state = join(dir, 'phone.json');
  const paired = await phone('pair', '--state', state, '--link', link);
  assert.equal(paired.words, phraseFromHex(mac.caFingerprintHex), 'the phone shows the six words the Mac shows');
  assert.equal(paired.matched, true);
  assert.deepEqual(paired.threads.map(x => x.id), ['thread-main', 'thread-acme']);
  assert.equal(mac.state.devices.get(paired.device)?.confirmed, true, 'the phone answered They match');
  assert.equal(statSync(state).mode & 0o777, 0o600, 'the test key is readable by its owner only');

  mac.pressOnMac();  // the Mac's own They match (Sage F1)
  const text = 'Please add hello.txt ' + Date.now();
  assert.deepEqual(await phone('send', '--state', state, '--thread', 'thread-acme', '--text', text, '--seconds', '30'),
    { thread: 'thread-acme', text });
  const got = mac.received().filter(row => row.text === text);
  assert.equal(got.length, 1, JSON.stringify(mac.received()));
  assert.equal(got[0].thread_id, 'thread-acme', 'sent on the conversation the walk chose, not the first one');

  // A second phone that says the words do not match: the Mac forgets it, the first one stays.
  // A code pairs once, so it gets the next one, as "Set my phone up" draws a new one.
  mac.state.pairCode = 'walk-pair-code-2';
  const second = `https://localhost:${port}/#pair=walk-pair-code-2`;
  const refused = await phone('pair', '--state', join(dir, 'refused.json'), '--link', second, '--reject');
  assert.equal(refused.matched, false);
  assert.equal(mac.state.devices.has(refused.device), false);
  assert.equal(mac.state.devices.has(paired.device), true);

  // A Mac it cannot trust is a failure with the reason, never a pass.
  mac.state.pairCode = 'walk-pair-code-3';
  const third = `https://localhost:${port}/#pair=walk-pair-code-3`;
  const untrusted = { ...process.env }; delete untrusted.NODE_EXTRA_CA_CERTS;
  await assert.rejects(run('node', [PHONE, 'pair', '--state', join(dir, 'untrusted.json'), '--link', third], { env: untrusted, timeout: 60000 }),
    error => error.code === 1 && /walk-phone:/.test(error.stderr));
});

test('the walk phone switches notifications on and off, and the Mac holds its registration only while on', { timeout: 120000 }, async t => {
  const dir = mkdtempSync(join(tmpdir(), 'walk-phone-notify-'));
  const mac = createStubMac({ host: 'localhost', bindHost: '127.0.0.1', history: 0, macPress: 'manual',
    pairCode: 'walk-notify-code', capabilities: ['text', 'native-push'], threads: [{ id: 'thread-main', title: 'Rich' }] });
  const port = await mac.listen(0);
  t.after(async () => { await mac.close(); rmSync(dir, { recursive: true, force: true }); });
  writeFileSync(join(dir, 'ca.pem'), mac.caPem);
  const env = { ...process.env, NODE_EXTRA_CA_CERTS: join(dir, 'ca.pem') };
  const phone = async (...args) => JSON.parse((await run('node', [PHONE, ...args], { env, timeout: 60000 })).stdout.trim());
  const state = join(dir, 'phone.json');
  const paired = await phone('pair', '--state', state, '--link', `https://localhost:${port}/#pair=walk-notify-code`);
  mac.pressOnMac();  // the Mac's own They match (Sage F1)

  const on = await phone('notifications', '--state', state, '--seconds', '30');
  assert.deepEqual([on.on, on.status], [true, 'enabled'], JSON.stringify(on));
  const held = mac.state.devices.get(paired.device).nativePush;
  assert.equal(held?.topic, 'dev.richos.connect.perf', 'the registration names the TEST app topic: ' + JSON.stringify(held));

  const off = await phone('notifications', '--state', state, '--off', '--seconds', '30');
  assert.deepEqual([off.on, off.status], [false, 'off'], JSON.stringify(off));
  assert.equal(mac.state.devices.get(paired.device).nativePush, null, 'switching off removes the registration');
});
