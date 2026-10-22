// PRD acceptance A9 (delivery recovery) on the exact relay upload inside workerd, the runtime
// Cloudflare runs, with a persisted local D1 and a fake GitHub as the designated test repository.
// The relay's 2-minute window is shortened to 4 seconds here; every other rule is the upload's.
// Skipped with a stated reason where no local runtime exists. It proves the relay's behavior under
// injected faults; real GitHub delivery is the live run (dev/report-a9-live.mjs).
const test = require('node:test');
const assert = require('node:assert/strict');
const { createHash, randomBytes, randomUUID } = require('node:crypto');
const { mkdtempSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join } = require('node:path');

const ALPHABET = '23456789ABCDEFGHJKMNPQRSTVWXYZ';
const reportId = () => 'RD-' + [4, 4].map(n => Array.from(randomBytes(n), b => ALPHABET[b % 30]).join('')).join('-');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
function report({ title = 'RichOS does not open', lines = ['### What happened', 'RichOS closed when it started\\.'], body, id = reportId() } = {}) {
  body ??= `${lines.join('\n')}\n\nSent with RichDoctor 1.0 · Report ID ${id}`;
  return { schema: 'richdoctor-report/1', report_id: id, receipt_secret: randomBytes(32).toString('base64url'),
    content_sha256: createHash('sha256').update(title + '\n\n' + body).digest('hex'), installation_id: randomUUID(),
    app_version: '1.0.0', approved_at: Date.now(), title, body };
}

test('A9 in workerd: interrupted delivery always ends in one issue or an honest state', { timeout: 120000 }, async t => {
  const { locateMiniflare } = await import('../dev/policy-worker-local.mjs');
  const { startLocalRelay } = await import('../dev/report-relay-local.mjs');
  const entry = locateMiniflare();
  if (!entry) return t.skip('Cloudflare local runtime not installed (no Wrangler with Miniflare on PATH; set RICHOS_MINIFLARE)');
  const state = mkdtempSync(join(tmpdir(), 'report-relay-'));
  t.after(() => rmSync(state, { recursive: true, force: true }));
  const relay = await startLocalRelay({ entry, state, reconcileSeconds: 4 });
  t.after(() => relay.dispose());
  t.diagnostic(`workerd compatibility date ${relay.compatibilityDate} (upload requests ${relay.runtime.requestedDate})`);
  const { github } = relay;
  const post = (path, body, init = {}) => relay.fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body), ...init });
  const create = async payload => (await post('/v1/reports', payload)).json();
  const status = async payload => (await post(`/v1/reports/${payload.report_id}/status`, { receipt_secret: payload.receipt_secret })).json();
  const issuesFor = payload => github.created().filter(issue => issue.body === payload.body);
  const posts = () => github.calls.filter(call => call === `POST /repos/${relay.repository}/issues`).length;
  async function until(payload, wanted, ms = 8000) {
    const end = Date.now() + ms;
    for (;;) {
      const answer = await status(payload);
      if (answer.state === wanted || Date.now() > end) return answer;
      await pause(250);
    }
  }

  // Disconnected before send: the relay never heard of it, so nothing exists and sending works.
  const before = report();
  assert.equal((await status(before)).state, 'unknown_id');
  let answer = await create(before);
  assert.equal(answer.state, 'delivered');
  assert.equal(answer.issue_url, `https://github.com/${relay.repository}/issues/${answer.issue_number}`);
  assert.deepEqual(issuesFor(before).map(i => [i.title, i.labels]), [[before.title, [{ name: 'richdoctor' }]]]);
  assert.equal((await create(before)).issue_url, answer.issue_url, 'exact replay');

  // Disconnected after the server accepted: the client goes away mid-attempt; waitUntil finishes it.
  const accepted = report();
  github.fault('create:hold');
  const abort = new AbortController(), gone = post('/v1/reports', accepted, { signal: abort.signal }).catch(error => error);
  while (!github.held.length) await pause(20);
  abort.abort(); await gone; github.release();
  answer = await until(accepted, 'delivered');
  assert.equal(answer.state, 'delivered'); assert.equal(issuesFor(accepted).length, 1);

  // GitHub created the issue, then the relay was restarted before answering. Retries and an
  // edited revision during the uncertainty create nothing; the footer reconciles it.
  const restarted = report(), postsBefore = posts();
  github.fault('create:hold');
  const lost = post('/v1/reports', restarted).catch(error => error);
  while (!github.held.length) await pause(20);
  assert.equal(issuesFor(restarted).length, 1, 'GitHub has created it');
  await relay.restart(); github.release(); await lost;
  assert.equal((await create(restarted)).state, 'pending');
  const edited = report({ id: restarted.report_id, title: 'Edited during uncertainty' });
  assert.equal((await create({ ...edited, receipt_secret: restarted.receipt_secret })).state, 'conflict');
  answer = await until(restarted, 'delivered');
  assert.equal(answer.state, 'delivered'); assert.equal(issuesFor(restarted).length, 1); assert.equal(posts(), postsBefore + 1);

  // The answer and every listing are lost: no recreation, operator review at the bound, then a
  // delayed success returns the existing issue.
  const blind = report();
  github.fault('create:lost'); for (let i = 0; i < 100; i++) github.fault('list:500');
  assert.equal((await create(blind)).state, 'pending');
  answer = await until(blind, 'unconfirmed');
  assert.equal(answer.state, 'unconfirmed');
  assert.equal((await create(blind)).state, 'unconfirmed', 'a replay never creates');
  const review = await relay.db.prepare("SELECT report_id FROM reports WHERE state='unconfirmed'").all();
  assert.deepEqual(review.results.map(r => r.report_id), [blind.report_id]);
  github.faults.length = 0;
  answer = await until(blind, 'delivered');
  assert.equal(answer.state, 'delivered'); assert.equal(issuesFor(blind).length, 1);

  // A recorded definitive refusal, then Try again with the same ID, secret and bytes once its cause cleared.
  const refused = report();
  github.fault('create:422');
  answer = await create(refused);
  assert.deepEqual([answer.state, answer.reason], ['rejected', 'delivery']);
  assert.equal(issuesFor(refused).length, 0);
  assert.equal((await create(refused)).state, 'delivered'); assert.equal(issuesFor(refused).length, 1);

  // Maximum body, the daily cap and the operator switch.
  const id = reportId(), footer = `\n\nSent with RichDoctor 1.0 · Report ID ${id}`;
  const largest = report({ id, body: 'a'.repeat(60000 - Buffer.byteLength(footer)) + footer });
  assert.equal((await create(largest)).state, 'delivered');
  const oversize = await post('/v1/reports', report({ body: 'a'.repeat(60001) }));
  assert.equal(oversize.status, 413); assert.equal((await oversize.json()).reason, 'size');
  const today = await relay.db.prepare('SELECT creates FROM daily').first();
  await relay.control(0, today.creates);
  answer = await create(report());
  assert.deepEqual([answer.state, answer.reason], ['closed', 'daily_cap']); assert(answer.retry_after_seconds > 0);
  await relay.control(1, 1000);
  assert.deepEqual(await create(report()), { state: 'closed', reason: 'operator_off' });
  assert.equal((await status(before)).state, 'delivered', 'known reports answer while switched off');
  const health = await (await relay.fetch('/v1/health')).json();
  assert.deepEqual(health, { ok: true, service: 'richos-report', protocol: 1, accepting: false });

  // One issue per delivered report, and nothing else was ever created.
  const delivered = [before, accepted, restarted, blind, refused, largest];
  assert.equal(github.created().length, delivered.length);
  for (const payload of delivered) assert.equal(issuesFor(payload).length, 1, payload.report_id);
  // The database holds receipt state only.
  const rows = JSON.stringify((await relay.db.prepare('SELECT * FROM reports').all()).results);
  for (const payload of delivered) { assert(!rows.includes(payload.receipt_secret)); assert(!rows.includes(payload.title)); }
});

test('the live A9 runner passes against the local stand-in and refuses a production profile', { timeout: 120000 }, async t => {
  const { locateMiniflare } = await import('../dev/policy-worker-local.mjs');
  const { startLocalRelay, LOCAL } = await import('../dev/report-relay-local.mjs');
  const { run } = await import('../dev/report-a9-live.mjs');
  const { PRODUCTION } = await import('../cli/report.mjs');
  const entry = locateMiniflare();
  if (!entry) return t.skip('Cloudflare local runtime not installed (no Wrangler with Miniflare on PATH; set RICHOS_MINIFLARE)');
  const state = mkdtempSync(join(tmpdir(), 'report-a9-'));
  t.after(() => rmSync(state, { recursive: true, force: true }));
  const relay = await startLocalRelay({ entry, state });
  t.after(() => relay.dispose());
  const profile = { stage: 'test', worker: 'richos-report-local', hostname: 'report-local.example.com', repository: LOCAL.repository,
    accountId: '0'.repeat(32), databaseId: '00000000-0000-0000-0000-000000000000', appId: LOCAL.appId, installationId: LOCAL.installationId,
    botLogin: LOCAL.bot, label: 'richdoctor' };
  // The runner's three destinations, routed to the stand-in: the relay, its D1 API and GitHub.
  const fetchImpl = async (url, init = {}) => {
    if (url.startsWith('https://report-local.example.com/')) return relay.fetch(url.slice('https://report-local.example.com'.length), init);
    if (url.startsWith('https://api.github.com/')) return relay.github.fetch(url, init);
    const { sql, params } = JSON.parse(init.body);
    const result = await relay.db.prepare(sql).bind(...params).all();
    return new Response(JSON.stringify({ success: true, result: [{ success: true, results: result.results, meta: result.meta }] }));
  };
  await assert.rejects(run({ ...profile, stage: 'production', ...PRODUCTION }, { token: 't', fetchImpl }), /test relay and test repository only/);
  const record = await run(profile, { token: 't', fetchImpl, settleMs: 0 });
  assert.equal(record.ok, true); assert(record.steps.every(step => step.ok));
  assert.equal(relay.github.created().length, 3);
  assert.equal((await relay.db.prepare('SELECT paused FROM control').first()).paused, 1, 'the run leaves the test relay switched off');
});
