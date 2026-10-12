const test = require('node:test');
const assert = require('node:assert/strict');
const { mkdtempSync, rmSync } = require('node:fs');
const { join } = require('node:path');
test('real policy service publishes audited revisions and signals connected clients without a Mac', async t => {
  const { publish, preview, readPolicy, serve } = await import('../service/policy.mjs');
  const directory = mkdtempSync(join(process.env.TMPDIR || require('node:os').tmpdir(), 'mobile-policy-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const now = Date.now();
  const policy = { schema: 1, revision: 1, issuedAt: new Date(now).toISOString(), expiresAt: new Date(now + 3600000).toISOString(), severity: 'none', title: '', message: '', allowDismiss: true, remindAfterSeconds: 60, features: { recording: false } };
  assert.throws(() => publish(directory, policy, { operator: 'test' }), /Preview/);
  const previewDigest = preview(policy, []).digest;
  publish(directory, policy, { operator: 'test', previewDigest });
  assert.throws(() => publish(directory, policy, { operator: 'test', previewDigest }), /increasing/);
  const instance = serve(directory, { signalInterval: 20 });
  await new Promise(resolve => instance.server.once('listening', resolve));
  t.after(() => instance.close());
  const origin = `http://127.0.0.1:${instance.server.address().port}`;
  assert.equal((await (await fetch(origin + '/v1/policy')).json()).revision, 1);
  assert.equal((await fetch(origin + '/v1/policy', { method: 'POST' })).status, 405);
  const abort = new AbortController(), stream = await fetch(origin + '/v1/events', { signal: abort.signal });
  const reader = stream.body.getReader(); await reader.read();
  const next = { ...policy, revision: 2, features: { recording: true } };
  publish(directory, next, { operator: 'test', previewDigest: preview(next, []).digest });
  let chunks = ''; const deadline = setTimeout(() => abort.abort(), 2000);
  try { while (!chunks.includes('data: 2')) chunks += new TextDecoder().decode((await reader.read()).value); }
  finally { clearTimeout(deadline); abort.abort(); }
  assert.equal(readPolicy(directory).features.recording, true);
  assert.match(chunks, /data: 2/);
});

test('operator CLI can publish while its independent service command is running', { timeout: 10000 }, async t => {
  const { spawn, spawnSync } = require('node:child_process'), { once } = require('node:events');
  const { createScratch } = require('./storage.cjs'); const { writeFileSync } = require('node:fs');
  const scratch = createScratch('policy-cli');
  const env = { ...process.env, RICHOS_MOBILE_CACHE: scratch, RICHOS_UPDATE_DIRECTORY: join(scratch, 'policy'), TMPDIR: scratch + '/' };
  const cli = join(__dirname, '../cli/mobile.mjs');
  const child = spawn(process.execPath, [cli, 'update', 'serve'], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  const stopped = once(child, 'close'); let stderr = ''; child.stderr.on('data', chunk => { stderr += chunk; });
  t.after(async () => { if (child.exitCode === null) child.kill('SIGTERM'); await stopped; rmSync(scratch, { recursive: true, force: true }); });
  const ready = await new Promise((resolve, reject) => {
    let output = ''; child.once('error', reject); child.once('exit', code => reject(Error(`Service exited ${code}: ${stderr}`)));
    child.stdout.on('data', chunk => { output += chunk; if (output.includes('\n')) resolve(JSON.parse(output.split('\n')[0])); });
  });
  const now = Date.now(), input = { operator: 'test', policy: { schema: 1, revision: 1, issuedAt: new Date(now).toISOString(), expiresAt: new Date(now + 3600000).toISOString(), severity: 'none', title: '', message: '', allowDismiss: true, remindAfterSeconds: 60 } };
  const file = join(scratch, 'request.json'); writeFileSync(file, JSON.stringify(input));
  const preview = spawnSync(process.execPath, [cli, 'update', 'preview', file], { env, encoding: 'utf8' }); assert.equal(preview.status, 0, preview.stderr);
  input.previewDigest = JSON.parse(preview.stdout).result.digest; writeFileSync(file, JSON.stringify(input));
  const publish = spawnSync(process.execPath, [cli, 'update', 'publish', file], { env, encoding: 'utf8' }); assert.equal(publish.status, 0, publish.stderr);
  assert.equal((await (await fetch(ready.policyService + '/v1/policy')).json()).revision, 1);
  const { version, build } = require('../release-config.json');
  const metric = { event: 'policy-visible', version, build, revision: 1 };
  assert.equal((await fetch(ready.policyService + '/v1/metrics', { method: 'POST', body: JSON.stringify(metric) })).status, 204);
  assert.equal((await fetch(ready.policyService + '/v1/metrics', { method: 'POST', body: JSON.stringify({ ...metric, conversation: 'must not be accepted' }) })).status, 400);
  child.kill('SIGTERM'); assert.equal((await stopped)[0], 0);
});
