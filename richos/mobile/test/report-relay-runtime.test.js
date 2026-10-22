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
  assert.deepEqual(await create(report()), { state: 'closed', stage: 'not_sent', reason: 'operator_off' });
  assert.equal((await status(before)).state, 'delivered', 'known reports answer while switched off');
  const health = await (await relay.fetch('/v1/health')).json();
  assert.deepEqual(health, { ok: true, service: 'richos-report', protocol: 2, accepting: false });

  // One issue per delivered report, and nothing else was ever created.
  const delivered = [before, accepted, restarted, blind, refused, largest];
  assert.equal(github.created().length, delivered.length);
  for (const payload of delivered) assert.equal(issuesFor(payload).length, 1, payload.report_id);
  // Once posted, the database holds receipt state only (a report's words stay only until it is sent).
  const rows = JSON.stringify((await relay.db.prepare("SELECT * FROM reports WHERE state='delivered'").all()).results);
  for (const payload of delivered) { assert(!rows.includes(payload.receipt_secret)); assert(!rows.includes(payload.title)); }
});

test('the live A9 runner passes against the local stand-in and refuses a production profile', { timeout: 240000 }, async t => {
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
    botLogin: LOCAL.bot, label: 'richdoctor', bucket: 'richos-report-local' };
  // The runner's three destinations, routed to the stand-in: the relay, its D1 API and GitHub.
  // The deployed relay allows five creates a minute per IP and a registration is one of them,
  // so every other create or registration is answered "limited" here, as the limiter would.
  let creates = 0, refused = 0;
  const fetchImpl = async (url, init = {}) => {
    if (url === 'https://report-local.example.com/v1/reports' && creates++ % 2 === 0) {
      refused++;
      return new Response(JSON.stringify({ state: 'limited', retry_after_seconds: 0.01 }), { status: 429, headers: { 'Content-Type': 'application/json' } });
    }
    // The full https URL is kept, so the relay sees the origin its screenshot links name.
    if (url.startsWith('https://report-local.example.com/')) return relay.fetch(url, init);
    if (url.startsWith('https://api.github.com/')) return relay.github.fetch(url, init);
    const { sql, params } = JSON.parse(init.body);
    const result = await relay.db.prepare(sql).bind(...params).all();
    return new Response(JSON.stringify({ success: true, result: [{ success: true, results: result.results, meta: result.meta }] }));
  };
  await assert.rejects(run({ ...profile, stage: 'production', ...PRODUCTION, bucket: 'richos-report' }, { token: 't', fetchImpl }), /test relay and test repository only/);
  const record = await run(profile, { token: 't', fetchImpl });
  assert.equal(record.ok, true); assert(record.steps.every(step => step.ok));
  // Every refused create and registration was sent again, protocol 2 registrations included.
  assert(refused >= 12, `only ${refused} creates were refused`); assert.equal(record.throttledAndResent, refused);
  assert.equal(relay.github.created().length, 4);
  assert.equal((await relay.db.prepare('SELECT paused FROM control').first()).paused, 1, 'the run leaves the test relay switched off');
});

test('A9 screenshots in workerd: slow, cut and lost uploads, a restart after creation, retirement and cleanup races', { timeout: 240000 }, async t => {
  const { locateMiniflare } = await import('../dev/policy-worker-local.mjs');
  const { startLocalRelay } = await import('../dev/report-relay-local.mjs');
  const { screenshotReport, registerBody, throttled, uploadInit } = await import('../dev/report-client.mjs');
  const { largePng } = await import('../dev/report-image-fixtures.mjs');
  const entry = locateMiniflare();
  if (!entry) return t.skip('Cloudflare local runtime not installed (no Wrangler with Miniflare on PATH; set RICHOS_MINIFLARE)');
  const state = mkdtempSync(join(tmpdir(), 'report-shots-'));
  t.after(() => rmSync(state, { recursive: true, force: true }));
  const relay = await startLocalRelay({ entry, state, reconcileSeconds: 4, abandonSeconds: 2 });
  t.after(() => relay.dispose());
  const { github } = relay, fixtures = join(__dirname, '../service/report/fixtures/relay');
  const file = (name, type) => ({ bytes: require('node:fs').readFileSync(join(fixtures, name)), type });
  const json = async response => response.json();
  const register = payload => relay.fetch('/v1/reports', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: registerBody(payload) }).then(json);
  const call = (payload, action) => relay.fetch(`/v1/reports/${payload.report_id}/${action}`, { method: 'POST', body: JSON.stringify({ receipt_secret: payload.receipt_secret }) }).then(json);
  const upload = (payload, n, body, init) => relay.fetch(`/v1/reports/${payload.report_id}/uploads/${n}`, uploadInit(payload, n, body, init));
  const staffRead = async (payload, n, token) => relay.fetch(`/v1/reports/${payload.report_id}/attachments/${n}`, { headers: token ? { 'Cf-Access-Jwt-Assertion': token } : {} });
  const objects = async prefix => (await (await relay.bucket()).list({ prefix })).objects.length;
  const posts = () => github.calls.filter(c => c === `POST /repos/${relay.repository}/issues`).length;

  // The largest screenshot at 1 Mbit/s takes well over 30 seconds, and arrives whole.
  const big = largePng(), started = Date.now();
  const slow = screenshotReport({ origin: relay.url, files: [{ bytes: big, type: 'image/png' }, file('accept-baseline.jpg', 'image/jpeg')] });
  assert(big.length > 5_000_000 && big.length <= 5 * 1048576, `largest fixture is ${big.length} bytes`);
  assert.equal((await register(slow)).state, 'receiving');
  let reply = await (await upload(slow, 1, throttled(big, 1_000_000))).json();
  const seconds = (Date.now() - started) / 1000;
  t.diagnostic(`5 MiB screenshot at 1 Mbit/s: accepted in ${seconds.toFixed(1)} s`);
  assert(seconds > 30, `took ${seconds} s`); assert.deepEqual(reply.stored, ['1']);
  // Cut off mid-upload: nothing stored for it; the same image again completes it.
  const cut = await upload(slow, 2, throttled(slow.files[1], 80_000, { stopAfter: 300 })).then(r => r.status, () => 'lost');
  t.diagnostic(`cut upload answered ${cut}`);
  assert.deepEqual((await call(slow, 'status')).missing, ['2']);
  // Stored, then its acknowledgment lost (the client never reads it): the retry is acknowledged, one object.
  await upload(slow, 2);
  reply = await (await upload(slow, 2)).json();
  assert.equal(reply.state, 'ready'); assert.equal(await objects(slow.report_id), 2);
  // GitHub created it, then the relay restarted before answering: reconciled, every image confirmed.
  github.fault('create:hold');
  const held = call(slow, 'complete').catch(() => null);
  while (!github.held.length) await pause(20);
  await relay.restart(); github.release(); await held;
  let status;
  for (let i = 0; i < 40 && status?.state !== 'delivered'; i++) { await pause(250); status = await call(slow, 'status'); }
  assert.deepEqual([status.state, status.images_expected, status.images_confirmed], ['delivered', 2, 2]);
  assert.equal(github.created().filter(i => i.body === slow.body).length, 1);
  // Staff see the pixels; a direct request without Access sees nothing.
  const pixels = await staffRead(slow, 1, await relay.accessToken());
  assert.equal(pixels.status, 200); assert.equal(Buffer.compare(Buffer.from(await pixels.arrayBuffer()), big), 0);
  assert.equal((await staffRead(slow, 1)).status, 403);
  assert.equal((await staffRead(slow, 1, await relay.accessToken({ aud: ['d'.repeat(64)] }))).status, 403);

  // A missing upload blocks creation; cleanup of the abandoned upload races its completion.
  const abandoned = screenshotReport({ origin: relay.url, files: [file('accept-rgb.png', 'image/png'), file('accept-palette.png', 'image/png')] });
  await register(abandoned); await upload(abandoned, 1);
  assert.deepEqual((await call(abandoned, 'complete')).missing, ['2']);
  const before = posts();
  await pause(2500);
  const [, completed] = await Promise.all([relay.cleanup(), upload(abandoned, 2).then(r => r.json())]);
  t.diagnostic(`upload racing cleanup answered ${completed.state}`);
  await relay.cleanup();
  reply = await call(abandoned, 'complete');
  if (reply.state === 'receiving') {
    // Cleanup won: the unsent report resumes by uploading the same images again.
    for (const n of reply.missing) await upload(abandoned, Number(n));
    reply = await call(abandoned, 'complete');
  }
  assert.deepEqual([reply.state, reply.images_confirmed], ['delivered', 2]);
  assert.equal(posts(), before + 1, 'one issue, with every approved image');

  // Retirement: never-seen first, a delayed registration stays retired, and a retired report's
  // stored screenshots are withdrawn at once and deleted at the next cleanup.
  const replaced = screenshotReport({ origin: relay.url, files: [file('accept-rgb.png', 'image/png')] });
  await register(replaced); await upload(replaced, 1);
  assert.equal((await staffRead(replaced, 1, await relay.accessToken())).status, 200);
  assert.equal((await call(replaced, 'retire')).state, 'retired');
  assert.equal((await call(replaced, 'retire')).state, 'retired');
  assert.equal((await staffRead(replaced, 1, await relay.accessToken())).status, 410);
  await relay.cleanup();
  assert.equal(await objects(replaced.report_id), 0);
  const unseen = screenshotReport({ origin: relay.url, files: [file('accept-rgb.png', 'image/png')] });
  assert.equal((await call(unseen, 'retire')).state, 'retired');
  assert.equal((await register(unseen)).state, 'retired');
  assert.equal((await call(unseen, 'complete')).state, 'retired');

  // Uncertain creation is never cleaned up.
  const uncertain = screenshotReport({ origin: relay.url, files: [file('accept-rgb.png', 'image/png')] });
  await register(uncertain); await upload(uncertain, 1);
  github.fault('create:lost'); for (let i = 0; i < 200; i++) github.fault('list:500');
  assert.equal((await call(uncertain, 'complete')).state, 'pending');
  await pause(2500); await relay.cleanup();
  assert.equal(await objects(uncertain.report_id), 1, 'kept while delivery is uncertain');
  github.faults.length = 0;
  // One issue per report that reached GitHub, and nothing else.
  assert.equal(github.created().length, 3);
});
