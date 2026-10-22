// RichDoctor report relay (PRD §7, acceptance A9 logic): schema, limits, content rules, durable
// pending state before GitHub, exact replay, status by ID, the state-based retry after an
// authoritative non-creation, reconciliation by footer, the operator off switch and the daily cap.
const test = require('node:test'), assert = require('node:assert/strict');
const { fixture, reportId } = require('./report-fixture.cjs');

test('health reports configuration and the off switch; a new database starts switched off', async t => {
  const f = await fixture(t);
  const health = async () => f.answer(await f.send(new Request('https://report.example.com/v1/health')));
  assert.deepEqual((await health()).body, { ok: true, service: 'richos-report', protocol: 2, accepting: true });
  f.db.raw.exec('UPDATE control SET paused=1');
  assert.equal((await health()).body.accepting, false);
  f.db.raw.exec('DELETE FROM control');
  assert.equal((await health()).status, 503);
  const { handle } = await import('../service/report-worker.mjs');
  assert.equal((await handle(new Request('https://report.example.com/v1/reports', { method: 'POST', body: '{}' }), {}, null)).status, 503);
  const fresh = await fixture(t); fresh.db.raw.exec('DELETE FROM control');
  fresh.db.raw.exec(require('node:fs').readFileSync(require('node:path').join(__dirname, '../service/report/schema.sql'), 'utf8'));
  assert.deepEqual({ ...fresh.db.raw.prepare('SELECT paused, daily_cap FROM control').get() }, { paused: 1, daily_cap: 0 });
});

test('an approved report becomes exactly one issue with its exact words; replay and status return it', async t => {
  const f = await fixture(t), payload = await f.report();
  const first = await f.answer(await f.create(payload));
  assert.equal(first.status, 201);
  assert.deepEqual(first.body, { report_id: payload.report_id, state: 'delivered', stage: 'delivered', issue_url: `https://github.com/${f.repository}/issues/1`,
    issue_number: 1, images_expected: 0, images_confirmed: 0, images_expire_at: new Date(f.now() + 365 * 86_400_000).toISOString() });
  const [issue] = f.github.created();
  assert.equal(issue.title, payload.title); assert.equal(issue.body, payload.body);
  assert.deepEqual(issue.labels, [{ name: 'richdoctor' }]);
  assert.equal(f.waits.length, 1, 'the attempt is registered to finish even if the client goes away');
  const replay = await f.answer(await f.create(payload));
  assert.equal(replay.status, 200); assert.equal(replay.body.issue_url, first.body.issue_url);
  assert.deepEqual((await f.answer(await f.status(payload.report_id, payload.receipt_secret))).body, { ...first.body });
  assert.equal(f.posts(), 1);
  // Once posted, the database keeps receipt state only: no words, no secret, no installation ID.
  const row = JSON.stringify(f.db.raw.prepare('SELECT * FROM reports').all());
  for (const value of [payload.title, 'RichOS closed', payload.receipt_secret, payload.installation_id]) assert(!row.includes(value), value);
  assert(!issue.body.includes(payload.receipt_secret));
});

test('a create is handed to waitUntil before its first await, so a client gone after acceptance cannot strand it', async t => {
  // Live on 2026-10-22: the client went away 150 ms after sending; the pending record had been
  // written but the attempt never started, so the report ended "not posted" instead of posted.
  const f = await fixture(t), payload = await f.report();
  const reply = f.create(payload);
  assert.equal(f.waits.length, 1, 'registered synchronously, before any database or GitHub call');
  assert.equal((await f.answer(await reply)).body.state, 'delivered');
  assert.equal((await f.waits[0]).status, 201);
  // Status requests create nothing, so they are not kept alive.
  await f.status(payload.report_id, payload.receipt_secret);
  assert.equal(f.waits.length, 1);
});

test('the schema, sizes and fingerprint are checked and nothing is stored for a refused request', async t => {
  const f = await fixture(t);
  const cases = [
    [{ extra: 1 }, 400, 'schema'], [{ report_id: 'RD-0000-AAAA' }, 400, 'schema'], [{ receipt_secret: 'short' }, 400, 'schema'],
    [{ schema: 'richdoctor-report/2' }, 400, 'schema'], [{ installation_id: 'not-a-uuid' }, 400, 'schema'],
    [{ approved_at: f.now() + 2 * 86_400_000 }, 400, 'schema'], [{ content_sha256: 'a'.repeat(64) }, 400, 'schema'],
    [{ title: 'x'.repeat(201) }, 413, 'size'],
  ];
  for (const [change, code, reason] of cases) {
    const payload = { ...await f.report(change.title ? { title: change.title } : {}), ...change };
    const reply = await f.answer(await f.create(payload));
    assert.equal(reply.status, code, JSON.stringify(change)); assert.equal(reply.body.state, 'rejected'); assert.equal(reply.body.reason, reason);
  }
  assert.equal((await f.answer(await f.create('{not json'))).body.reason, 'schema');
  // The largest body PRD §10 allows is accepted; one more byte is not.
  const id = reportId(), footer = `\n\nSent with RichDoctor 1.0 · Report ID ${id}`;
  const fits = 'a'.repeat(60000 - Buffer.byteLength(footer)) + footer;
  assert.equal((await f.answer(await f.create(await f.report({ report_id: id, body: fits })))).body.state, 'delivered');
  const over = await f.answer(await f.create(await f.report({ report_id: reportId(), body: 'a' + fits })));
  assert.equal(over.status, 413); assert.equal(over.body.reason, 'size');
  assert.equal(f.db.raw.prepare('SELECT count(*) AS n FROM reports').get().n, 1);
});

test('content rules reject unsafe structure and never rewrite the words; approved words are never vetoed for privacy', async t => {
  const f = await fixture(t);
  const id = reportId(), footer = `Sent with RichDoctor 1.0 · Report ID ${id}`;
  const rejected = {
    markup: ['A picture ![x](https://evil.example/p.png)', 'A link [click](https://evil.example)', 'HTML <img src=x>'],
    mention: ['Ask @octocat'],
    control: ['carriage\rreturn'],
  };
  for (const [kind, lines] of Object.entries(rejected)) for (const line of lines) {
    const reply = await f.answer(await f.create(await f.report({ report_id: id, body: `${line}\n\n${footer}` })));
    assert.equal(reply.status, 422, line); assert.equal(reply.body.reason, 'content');
    assert.equal(reply.body.item, 'line 1', line); assert(!JSON.stringify(reply.body).includes(line), 'the words are never echoed');
  }
  const noFooter = await f.answer(await f.create(await f.report({ report_id: id, body: 'No footer' })));
  assert.equal(noFooter.body.reason, 'content'); assert.equal(noFooter.body.item, 'line 1');
  assert.equal(f.posts(), 0); assert.equal(f.db.raw.prepare('SELECT count(*) AS n FROM reports').get().n, 0);
  // The escaped forms Bust a bug writes, scrubbed stand-ins and the support address all pass.
  const safe = ['A picture \\!\\[x\\](https://example.com)', 'Opened /Users/\\[a person\\]/Library/Logs', 'Write to support@​richos\\.ceo',
    'Thanks @​team', 'Shared /Users/Shared/RichOS', 'password: \\[removed\\]'];
  const ok = await f.answer(await f.create(await f.report({ report_id: id, body: `${safe.join('\n')}\n\n${footer}` })));
  assert.equal(ok.body.state, 'delivered');
  assert.equal(f.github.created()[0].body, `${safe.join('\n')}\n\n${footer}`, 'delivered byte for byte');
  // Words the user's Claude reviewed and the user approved are posted as approved, whatever they look like.
  const approved = ['token ghp\\_' + 'a'.repeat(36), 'Authorization: Basic abc', 'Mail jane\\.doe@​example\\.com', 'Opened /Users/jane/Library/Logs'];
  const other = await f.report({ title: 'From jane@example.com' }), privateBody = `${approved.join('\n')}\n\n${other.body.split('\n').pop()}`;
  const { contentHash } = await import('../service/report/checks.mjs');
  const sent = await f.answer(await f.create({ ...other, body: privateBody, content_sha256: await contentHash(other.title, privateBody) }));
  assert.equal(sent.body.state, 'delivered', 'the relay never vetoes approved words');
  assert.equal(f.github.created()[1].body, privateBody); assert.equal(f.posts(), 2);
});

test('an ID is bound to its receipt and bytes: wrong secret, changed bytes and unknown IDs', async t => {
  const f = await fixture(t), payload = await f.report();
  await f.create(payload);
  const other = await f.report({ report_id: payload.report_id });
  assert.deepEqual((await f.answer(await f.create({ ...payload, receipt_secret: other.receipt_secret }))).body, { state: 'forbidden' });
  const edited = await f.report({ report_id: payload.report_id, title: 'Edited after approval' });
  const conflict = await f.answer(await f.create({ ...edited, receipt_secret: payload.receipt_secret }));
  assert.equal(conflict.status, 409); assert.equal(conflict.body.state, 'conflict');
  assert.equal((await f.answer(await f.status(payload.report_id, other.receipt_secret))).status, 403);
  const unknown = await f.answer(await f.status(reportId(), other.receipt_secret));
  assert.equal(unknown.status, 404); assert.equal(unknown.body.state, 'unknown_id');
  assert.equal(f.posts(), 1);
});

test('A9: GitHub created the issue but its answer was lost; the footer reconciles it, never a second create', async t => {
  for (const fault of ['create:lost', 'create:502-after']) {
    const f = await fixture(t), payload = await f.report();
    f.github.fault(fault);
    const reply = await f.answer(await f.create(payload));
    assert.equal(reply.body.state, 'delivered', fault); assert.equal(reply.body.issue_number, 1);
    assert.equal(f.posts(), 1); assert.equal(f.github.created().length, 1);
  }
});

test('A9: an incomplete or failing listing never permits recreation; the bound ends in operator review, and a late find delivers', async t => {
  const f = await fixture(t, { pageSize: 1 }), payload = await f.report();
  // The answer is lost and the relay's own three listings fail.
  f.github.fault('create:lost'); for (let i = 0; i < 3; i++) f.github.fault('list:500');
  let reply = await f.answer(await f.create(payload));
  assert.equal(reply.status, 202); assert.deepEqual(reply.body, { report_id: payload.report_id, state: 'pending', stage: 'creating', check_after_seconds: 5, stored: [], missing: [] });
  // Two newer bot issues fill pages 1 and 2 of the newest-first listing, so the match is on
  // page 3 behind a failing page 2.
  f.github.add('later', 'one'); f.github.add('later', 'two'); f.github.fault('list:page2');
  // Retries during uncertainty, including edited bytes, never create.
  assert.equal((await f.answer(await f.create(payload))).body.state, 'pending');
  const edited = await f.report({ report_id: payload.report_id, title: 'Edited' });
  assert.equal((await f.answer(await f.create({ ...edited, receipt_secret: payload.receipt_secret }))).body.state, 'conflict');
  for (let i = 0; i < 20; i++) f.github.fault('list:500');
  f.advance(30_000);
  assert.equal((await f.answer(await f.status(payload.report_id, payload.receipt_secret))).body.state, 'pending');
  f.advance(100_000);
  reply = await f.answer(await f.status(payload.report_id, payload.receipt_secret));
  assert.equal(reply.body.state, 'unconfirmed', 'two minutes after the attempt, it is in operator review');
  assert.deepEqual((await f.store.unconfirmed()).map(row => row.report_id), [payload.report_id]);
  assert.equal((await f.answer(await f.create(payload))).body.state, 'unconfirmed', 'a replay of an unconfirmed report never creates');
  f.github.faults.length = 0; f.advance(61_000);
  reply = await f.answer(await f.status(payload.report_id, payload.receipt_secret));
  assert.equal(reply.body.state, 'delivered', 'a delayed success returns the existing issue');
  assert.equal(reply.body.issue_number, 1);
  assert.equal(f.posts(), 1); assert.equal(f.github.created().length, 3);
});

test('A9: a recorded definitive refusal permits one new serialized attempt with the same ID, secret and bytes', async t => {
  const f = await fixture(t), payload = await f.report();
  f.github.fault('create:403-retry');
  let reply = await f.answer(await f.create(payload));
  assert.equal(reply.body.state, 'rejected'); assert.equal(reply.body.reason, 'delivery');
  assert.equal(reply.body.retry_after_seconds, 90); assert.equal(reply.retryAfter, '90');
  assert.equal((await f.answer(await f.status(payload.report_id, payload.receipt_secret))).body.state, 'rejected');
  // The cause cleared: Try again with the same ID creates exactly one issue.
  reply = await f.answer(await f.create(payload));
  assert.equal(reply.body.state, 'delivered');
  assert.equal(f.db.raw.prepare('SELECT attempt FROM reports').get().attempt, 2);
  assert.equal((await f.answer(await f.create(payload))).body.state, 'delivered');
  assert.equal(f.github.created().length, 1);
  f.github.fault('create:422');
  const second = await f.report();
  assert.equal((await f.answer(await f.create(second))).body.reason, 'delivery');
  assert.equal(f.github.created().length, 1, 'a 422 created nothing');
});

test('A9: the relay stopped mid-attempt; status decides from the persisted record alone', async t => {
  const f = await fixture(t);
  // A pending record exactly as a create writes it before calling GitHub.
  const begin = async (payload, attemptId) => {
    const { sha256 } = await import('../service/report/checks.mjs');
    await f.store.register({ reportId: payload.report_id, secretHash: await sha256(payload.receipt_secret), contentHash: payload.content_sha256, schema: 1,
      title: payload.title, body: payload.body, approvedAt: payload.approved_at, manifest: [] });
    const token = await f.store.acquire(payload.report_id);
    const claimed = await f.store.claim(payload.report_id, 0, attemptId, 60_000, token);
    await f.store.release(payload.report_id, token);
    return claimed;
  };
  // Stopped before the create request was recorded as sent: authoritative non-creation.
  const before = await f.report();
  assert(await begin(before, 'attempt-a'));
  assert.equal((await f.answer(await f.status(before.report_id, before.receipt_secret))).body.state, 'pending');
  f.advance(60_000);
  assert.equal((await f.answer(await f.status(before.report_id, before.receipt_secret))).body.reason, 'delivery');
  assert.equal((await f.answer(await f.create(before))).body.state, 'delivered');
  assert.equal(f.posts(), 1);
  // Stopped after sending, and GitHub had created it: found by its footer.
  const after = await f.report();
  assert(await begin(after, 'attempt-b')); assert(await f.store.markSent(after.report_id, 'attempt-b'));
  f.github.add(after.title, after.body);
  f.advance(26_000);
  assert.equal((await f.answer(await f.status(after.report_id, after.receipt_secret))).body.state, 'delivered');
  // Stopped after sending, and nothing is found: never "not created", only unconfirmed.
  const lost = await f.report();
  assert(await begin(lost, 'attempt-c')); assert(await f.store.markSent(lost.report_id, 'attempt-c'));
  f.advance(121_000);
  assert.equal((await f.answer(await f.status(lost.report_id, lost.receipt_secret))).body.state, 'unconfirmed');
  assert.equal((await f.answer(await f.create(lost))).body.state, 'unconfirmed');
  assert.equal(f.posts(), 1);
  // A lapsed lease cannot be used to send late.
  const late = await f.report();
  assert(await begin(late, 'attempt-d')); f.advance(60_000);
  assert.equal(await f.store.markSent(late.report_id, 'attempt-d'), false);
});

test('concurrent creates for one ID serialize to one GitHub request', async t => {
  const f = await fixture(t), payload = await f.report();
  f.github.fault('create:hold');
  const racing = [f.create(payload), f.create(payload), f.create(payload)];
  await new Promise(resolve => setTimeout(resolve, 50)); f.github.release();
  const states = (await Promise.all(racing.map(async r => (await f.answer(await r)).body.state))).sort();
  assert.deepEqual(states, ['delivered', 'pending', 'pending']);
  assert.equal(f.posts(), 1);
});

test('the off switch and the daily cap close new reports honestly and never block known ones', async t => {
  const f = await fixture(t, { cap: 1 }), first = await f.report();
  assert.equal((await f.answer(await f.create(first))).body.state, 'delivered');
  const capped = await f.answer(await f.create(await f.report()));
  assert.equal(capped.status, 503); assert.equal(capped.body.state, 'closed'); assert.equal(capped.body.reason, 'daily_cap');
  assert.equal(capped.body.retry_after_seconds, 12 * 3600, 'until the next UTC midnight');
  f.db.raw.exec('UPDATE control SET paused=1');
  const off = await f.answer(await f.create(await f.report()));
  assert.deepEqual(off.body, { state: 'closed', stage: 'not_sent', reason: 'operator_off' });
  assert.equal((await f.answer(await f.create(first))).body.state, 'delivered', 'a replay is answered while switched off');
  assert.equal((await f.answer(await f.status(first.report_id, first.receipt_secret))).body.state, 'delivered');
  // The capped report is kept as approved and not begun (ready); the switched-off one was never stored.
  assert.deepEqual(f.db.raw.prepare('SELECT state FROM reports ORDER BY created_at').all().map(r => r.state), ['delivered', 'ready']);
  f.db.raw.exec('UPDATE control SET paused=0, daily_cap=0');
  assert.equal((await f.answer(await f.create(await f.report()))).body.reason, 'daily_cap', 'a cap of zero creates nothing');
  assert.equal(f.posts(), 1);
});

test('an older unknown ID is reconciled by its footer before any create; a copied footer from another account is ignored', async t => {
  const f = await fixture(t), old = await f.report();
  f.github.add(old.title, old.body);
  f.advance(3600_000);
  const reply = await f.answer(await f.create(old));
  assert.equal(reply.body.state, 'delivered'); assert.equal(reply.body.issue_number, 1); assert.equal(f.posts(), 0);
  const copied = await f.report(); f.github.addForeign(copied.title, copied.body); f.advance(3600_000);
  assert.equal((await f.answer(await f.create(copied))).body.issue_number, 3);
  assert.equal(f.posts(), 1);
  const blind = await f.report(); f.advance(3600_000); f.github.fault('list:500');
  const unsure = await f.answer(await f.create(blind));
  assert.equal(unsure.body.state, 'limited', 'an incomplete listing permits no create'); assert.equal(f.posts(), 1);
});

test('a report sent offline and retried an hour later is filed exactly once', async t => {
  // The user pressed Send with no connection, so the first send never reached the relay; an hour
  // later Try again sends the same approved report, which the relay has never seen.
  const f = await fixture(t), offline = await f.report();
  f.advance(3600_000);
  const retried = await f.answer(await f.create(offline));
  assert.equal(retried.body.state, 'delivered'); assert.equal(retried.body.issue_number, 1);
  assert.equal(f.github.created()[0].body, offline.body); assert.equal(f.posts(), 1);
  assert.equal((await f.answer(await f.create(offline))).body.issue_number, 1, 'a second Try again returns the same issue');
  assert.equal((await f.answer(await f.status(offline.report_id, offline.receipt_secret))).body.state, 'delivered');
  assert.equal(f.posts(), 1, 'filed once');
});

test('a token failure is an authoritative non-creation; throttling says nothing about the report; a PKCS#1 App key works', async t => {
  const f = await fixture(t, { pkcs1: true }), payload = await f.report();
  f.github.fault('token:500');
  const reply = await f.answer(await f.create(payload));
  assert.equal(reply.body.reason, 'delivery'); assert.equal(reply.body.retry_after_seconds, 60); assert.equal(f.posts(), 0);
  assert.equal((await f.answer(await f.create(payload))).body.state, 'delivered', 'the PKCS#1 key signs a JWT GitHub accepts');
  f.env.CREATE_LIMIT = { limit: async () => ({ success: false }) };
  const throttled = await f.answer(await f.create(await f.report()));
  assert.equal(throttled.status, 429); assert.deepEqual(throttled.body, { state: 'limited', retry_after_seconds: 60 }); assert.equal(throttled.retryAfter, '60');
  assert.equal((await f.send(new Request('https://report.example.com/v1/reports?target=x', { method: 'POST' }))).status, 404);
  assert.equal((await f.send(new Request('https://report.example.com/v1/reports/RD-AAAA-AAAA/status', { method: 'GET' }))).status, 404);
});

test('plain HTTP is refused except on loopback, for the local stand-in only', async t => {
  const f = await fixture(t);
  assert.equal((await f.send(new Request('http://report.example.com/v1/health'))).status, 400);
  assert.equal((await f.send(new Request('http://127.0.0.1:8787/v1/health'))).status, 200);
});

test('the upload artifact is exact, carries no secret, and a test profile can never reach production', async () => {
  const { reportArtifact, checkProfile, PRODUCTION, schemaStatements } = await import('../cli/report.mjs');
  const { createHash } = require('node:crypto'), { readFileSync } = require('node:fs'), { join } = require('node:path');
  const base = { accountId: 'a'.repeat(32), databaseId: '11111111-2222-3333-4444-555555555555', appId: '9', installationId: '8',
    botLogin: 'richos-reports[bot]', label: 'richdoctor' }, production = { ...base, stage: 'production', ...PRODUCTION, bucket: 'richos-report' };
  const artifact = reportArtifact(production);
  assert.deepEqual(artifact.modules.map(m => m.name), ['report-worker.mjs', 'report/checks.mjs', 'report/images.mjs', 'report/store.mjs', 'report/github.mjs', 'report/access.mjs']);
  for (const module of artifact.modules) {
    const source = readFileSync(join(__dirname, '../service', module.name), 'utf8');
    assert.equal(module.source, source); assert.equal(module.sha256, createHash('sha256').update(source).digest('hex'));
  }
  assert.deepEqual(artifact.metadata.keep_bindings, ['secret_text']); assert.deepEqual(artifact.secrets, ['GITHUB_APP_PRIVATE_KEY']);
  assert.equal(artifact.metadata.observability.enabled, false); assert.equal(artifact.metadata.logpush, false);
  assert.deepEqual(artifact.subdomain, { enabled: false, previews_enabled: false });
  const vars = Object.fromEntries(artifact.metadata.bindings.map(b => [b.name, b.text ?? b.id ?? b.simple]));
  assert.equal(vars.GITHUB_REPOSITORY, 'WebDevBooster/richos'); assert.equal(vars.RECONCILE_SECONDS, undefined, 'production keeps the 2-minute window');
  assert(!artifact.metadata.bindings.some(b => b.name === 'GITHUB_APP_PRIVATE_KEY'));
  assert(!artifact.metadata.bindings.some(b => /connect/i.test(JSON.stringify(b))), 'nothing of Connect');
  assert.throws(() => checkProfile({ ...production, repository: 'WebDevBooster/other' }), /production/);
  for (const [key, value] of Object.entries(PRODUCTION)) {
    const test = { ...base, stage: 'test', worker: 'richos-report-test', hostname: 'report-test.richos.ceo', repository: 'richos-test/report-relay-test', bucket: 'richos-report-test', [key]: value };
    assert.throws(() => checkProfile(test), /test profile/, key);
  }
  assert.equal(reportArtifact({ ...base, stage: 'test', worker: 'richos-report-test', hostname: 'report-test.richos.ceo', repository: 'richos-test/report-relay-test', bucket: 'richos-report-test' }).repository, 'richos-test/report-relay-test');
  assert.throws(() => checkProfile({ ...base, stage: 'test', worker: 'richos-report-test', hostname: 'report-test.richos.ceo', repository: 'richos-test/report-relay-test', bucket: 'richos-report' }), /test profile/, 'nor the production bucket');
  assert.deepEqual(artifact.schedules, [{ cron: '17 * * * *' }]);
  assert.deepEqual(artifact.metadata.bindings.find(b => b.name === 'IMAGES'), { name: 'IMAGES', type: 'r2_bucket', bucket_name: 'richos-report' });
  // Four tables, two indexes and the switched-off control row.
  assert.equal(schemaStatements().length, 7);
});

test('operator resolve accepts only an issue the relay bot made with this report footer', async t => {
  const f = await fixture(t), payload = await f.report();
  f.github.fault('create:lost'); for (let i = 0; i < 10; i++) f.github.fault('list:500');
  await f.create(payload); f.github.faults.length = 0;
  const { operate } = await import('../cli/report.mjs');
  const profile = { stage: 'test', worker: 'richos-report-test', hostname: 'report-test.richos.ceo', repository: f.repository, accountId: 'a'.repeat(32),
    databaseId: '11111111-2222-3333-4444-555555555555', appId: '123', installationId: '456', botLogin: f.bot, label: 'richdoctor', bucket: 'richos-report-test' };
  const statements = [];
  const fake = async (url, init) => {
    if (url.startsWith('https://api.github.com/')) {
      const number = Number(url.split('/').pop()), issue = f.github.issues[number - 1];
      return new Response(JSON.stringify(issue || {}), { status: issue ? 200 : 404 });
    }
    const { sql, params } = JSON.parse(init.body); statements.push(sql);
    const changes = Number(f.db.raw.prepare(sql).run(...params).changes);
    return new Response(JSON.stringify({ success: true, result: [{ success: true, results: [], meta: { changes } }] }));
  };
  f.github.addForeign(payload.title, payload.body);
  await assert.rejects(operate('resolve', profile, [payload.report_id, '2'], { token: 't', fetch: fake }), /not made by the relay/);
  const done = await operate('resolve', profile, [payload.report_id, '1'], { token: 't', fetch: fake });
  assert.deepEqual(done, { reportId: payload.report_id, issueUrl: `https://github.com/${f.repository}/issues/1`, updated: true });
  assert.equal((await f.answer(await f.status(payload.report_id, payload.receipt_secret))).body.issue_number, 1);
  // The full delivery transition (review rv-20261022T154816Z): words dropped, retention clock started.
  const row = f.db.raw.prepare('SELECT title, body, delivered_at, expires_at FROM reports WHERE report_id=?').get(payload.report_id);
  assert.equal(row.title, null); assert.equal(row.body, null);
  assert(row.delivered_at > 0); assert.equal(row.expires_at - row.delivered_at, 365 * 86_400_000);
  assert.equal(statements.length, 1);
});

test('deploy applies the schema, uploads the exact modules, installs the key as a secret and reads back', async t => {
  const { deploy, reportArtifact } = await import('../cli/report.mjs');
  const { appKey } = await import('../dev/report-fake-github.mjs');
  const key = (await appKey()).pkcs1;
  const profile = { stage: 'test', worker: 'richos-report-test', hostname: 'richos-report-test.example-sub.workers.dev', repository: 'richos-test/report-relay-test',
    accountId: 'a'.repeat(32), databaseId: '11111111-2222-3333-4444-555555555555', appId: '9', installationId: '8', botLogin: 'richos-reports-test[bot]', label: 'richdoctor', bucket: 'richos-report-test' };
  const artifact = reportArtifact(profile), seen = [];
  let parts, secret, subdomain, schedules;
  const ok = result => new Response(JSON.stringify({ success: true, result }));
  const fake = async (url, init = {}) => {
    const path = url.replace(`https://api.cloudflare.com/client/v4/accounts/${profile.accountId}`, ''), method = init.method || 'GET';
    seen.push(`${method} ${path.startsWith('/d1/') ? '/d1/query' : path}`);
    if (path === '/workers/subdomain') return ok({ subdomain: 'example-sub' });
    if (path.endsWith('/query')) {
      const { sql } = JSON.parse(init.body);
      return ok([{ success: true, results: /sqlite_master/.test(sql) ? [{ name: 'attachments' }, { name: 'control' }, { name: 'daily' }, { name: 'reports' }] : [], meta: { changes: 0 } }]);
    }
    if (method === 'PUT' && path === '/workers/scripts/richos-report-test') { parts = init.body; return ok({ id: 'deployment-1' }); }
    if (path.endsWith('/secrets')) { secret = JSON.parse(init.body); return ok({}); }
    if (path.endsWith('/subdomain')) { subdomain = JSON.parse(init.body); return ok({}); }
    if (path.endsWith('/schedules')) { schedules = JSON.parse(init.body); return ok({}); }
    if (path.endsWith('/settings')) return ok({ bindings: [...artifact.metadata.bindings, { name: 'GITHUB_APP_PRIVATE_KEY', type: 'secret_text' }], logpush: false, tail_consumers: [] });
    if (url === `https://${profile.hostname}/v1/health`) return new Response(JSON.stringify({ ok: true, service: 'richos-report', protocol: 1, accepting: false }));
    return new Response('{}', { status: 404 });
  };
  const result = await deploy(profile, { token: 't', appKey: key, fetch: fake });
  assert.deepEqual(seen.slice(0, 2), ['GET /workers/subdomain', 'POST /d1/query'], 'the schema goes first');
  assert.deepEqual(seen.slice(-6), ['PUT /workers/scripts/richos-report-test', 'PUT /workers/scripts/richos-report-test/secrets',
    'POST /workers/scripts/richos-report-test/subdomain', 'PUT /workers/scripts/richos-report-test/schedules', 'GET /workers/scripts/richos-report-test/settings', `GET https://${profile.hostname}/v1/health`]);
  assert.deepEqual(schedules, [{ cron: '17 * * * *' }]);
  assert.deepEqual(JSON.parse(await parts.get('metadata').text()), artifact.metadata);
  for (const module of artifact.modules) assert.equal(await parts.get(module.name).text(), module.source);
  assert(!(await parts.get('metadata').text()).includes('PRIVATE KEY'), 'the key never goes into the upload');
  assert.deepEqual(secret, { name: 'GITHUB_APP_PRIVATE_KEY', text: key, type: 'secret_text' });
  assert.deepEqual(subdomain, { enabled: true, previews_enabled: false });
  assert(!JSON.stringify(result).includes('PRIVATE KEY'));
  assert.equal(result.health.accepting, false, 'a deployed relay starts switched off');
  await assert.rejects(deploy({ ...profile, hostname: 'richos-report-test.other.workers.dev' }, { token: 't', appKey: key, fetch: fake }), /workers\.dev hostname is/);
});

test('a production deploy attaches report.richos.ceo as a Custom Domain before its health check', async () => {
  const { deploy, reportArtifact, PRODUCTION } = await import('../cli/report.mjs');
  const { appKey } = await import('../dev/report-fake-github.mjs');
  const key = (await appKey()).pkcs1;
  const profile = { stage: 'production', ...PRODUCTION, bucket: 'richos-report', accountId: 'a'.repeat(32), databaseId: '11111111-2222-3333-4444-555555555555',
    appId: '9', installationId: '8', botLogin: 'richos-reports[bot]', label: 'richdoctor', accessTeamDomain: 'https://richos.cloudflareaccess.com', accessAud: 'b'.repeat(64) };
  const artifact = reportArtifact(profile), seen = [];
  let domain = null, subdomain;
  const ok = result => new Response(JSON.stringify({ success: true, result }));
  const fake = async (url, init = {}) => {
    const path = url.replace(`https://api.cloudflare.com/client/v4/accounts/${profile.accountId}`, ''), method = init.method || 'GET';
    seen.push(`${method} ${path.startsWith('/d1/') ? '/d1/query' : path}`);
    if (path.endsWith('/query')) {
      const { sql } = JSON.parse(init.body);
      return ok([{ success: true, results: /sqlite_master/.test(sql) ? [{ name: 'attachments' }, { name: 'control' }, { name: 'daily' }, { name: 'reports' }] : [], meta: { changes: 0 } }]);
    }
    if (method === 'PUT' && path === '/workers/domains') { domain = JSON.parse(init.body); return ok({ id: 'domain-1' }); }
    if (path.endsWith('/subdomain')) { subdomain = JSON.parse(init.body); return ok({}); }
    if (path.endsWith('/settings')) return ok({ bindings: [...artifact.metadata.bindings, { name: 'GITHUB_APP_PRIVATE_KEY', type: 'secret_text' }], logpush: false, tail_consumers: [] });
    if (url === 'https://report.richos.ceo/v1/health') return domain ? new Response(JSON.stringify({ ok: true, service: 'richos-report', protocol: 2, accepting: false })) : new Response('', { status: 522 });
    if (method === 'PUT' || method === 'POST') return ok({ id: 'deployment-1' });
    return new Response('{}', { status: 404 });
  };
  const result = await deploy(profile, { token: 't', appKey: key, fetch: fake });
  assert.deepEqual(domain, { hostname: 'report.richos.ceo', service: 'richos-report', zone_name: 'richos.ceo' });
  assert(seen.indexOf('PUT /workers/domains') < seen.indexOf('GET https://report.richos.ceo/v1/health'));
  assert(!seen.includes('GET /workers/subdomain'), 'production is never served on workers.dev');
  assert.deepEqual(subdomain, { enabled: false, previews_enabled: false });
  assert(result.bindings.includes('ACCESS_AUD:plain_text') && result.bindings.includes('ACCESS_TEAM_DOMAIN:plain_text'));
});

test('staff-access guards only the screenshot links, for exactly the named staff, and finds its application again', async () => {
  const { staffAccess, accessPath } = await import('../cli/report.mjs');
  const account = 'a'.repeat(32), apps = [], calls = [];
  let team = 'richos.cloudflareaccess.com', owner = 'owner@example.com', drop = false;
  const ok = result => new Response(JSON.stringify({ success: true, result }));
  const fake = async (url, init = {}) => {
    const method = init.method || 'GET', path = url.replace(`https://api.cloudflare.com/client/v4/accounts/${account}`, '');
    calls.push(`${method} ${path}`);
    if (url === 'https://api.cloudflare.com/client/v4/user') return owner ? ok({ email: owner }) : new Response('{"success":false}', { status: 403 });
    if (path === '/access/organizations') return ok({ auth_domain: team });
    if (path === '/access/apps' && method === 'GET') return ok(apps);
    if (path === '/access/apps' && method === 'POST') {
      const body = JSON.parse(init.body), app = { id: 'app-' + (apps.length + 1), aud: 'c'.repeat(64), ...body, policies: body.policies.map(p => ({ id: 'p1', ...p })) };
      apps.push(app); return ok(app);
    }
    const app = apps.find(a => path === `/access/apps/${a.id}`);
    if (app) return ok(drop ? { ...app, policies: [] } : app);
    return new Response('{}', { status: 404 });
  };
  await assert.rejects(staffAccess(account, 'richos-report-test.sub.workers.dev', [], { token: 't', fetch: fake }), /Expected/);
  const first = await staffAccess(account, 'report.richos.ceo', ['staff@example.com', 'second@example.com'], { token: 't', fetch: fake });
  assert.deepEqual(first, { accessTeamDomain: 'https://richos.cloudflareaccess.com', accessAud: 'c'.repeat(64), path: 'report.richos.ceo/v1/reports/*/attachments/*',
    staff: ['second@example.com', 'staff@example.com'], created: true });
  assert.equal(apps[0].domain, accessPath('report.richos.ceo'), 'never the whole hostname: the app itself signs in to nothing');
  assert.deepEqual(apps[0].destinations, [{ type: 'public', uri: 'report.richos.ceo/v1/reports/*/attachments/*' }]);
  // Run again: the same application is found and checked, nothing new is made.
  assert.equal((await staffAccess(account, 'report.richos.ceo', ['second@example.com', 'staff@example.com'], { token: 't', fetch: fake })).created, false);
  assert.equal(apps.length, 1);
  await assert.rejects(staffAccess(account, 'report.richos.ceo', ['other@example.com'], { token: 't', fetch: fake }), /exactly the named staff/);
  drop = true;
  await assert.rejects(staffAccess(account, 'report.richos.ceo', ['staff@example.com', 'second@example.com'], { token: 't', fetch: fake }), /exactly the named staff/);
  drop = false; apps.length = 0;
  // No staff named: the token owner's own email.
  assert.deepEqual((await staffAccess(account, 'report.richos.ceo', [], { token: 't', fetch: fake })).staff, ['owner@example.com']);
  owner = null; apps.length = 0;
  await assert.rejects(staffAccess(account, 'report.richos.ceo', [], { token: 't', fetch: fake }), /Name the staff emails/);
  team = '';
  await assert.rejects(staffAccess(account, 'report.richos.ceo', ['staff@example.com'], { token: 't', fetch: fake }), /no team domain/);
});

test('app-identity reads the bot login and installation from GitHub and refuses a broader App', async t => {
  const f = await fixture(t), { appIdentity } = await import('../cli/report.mjs');
  const fetch = (url, init) => f.github.fetch(url, init);
  assert.deepEqual(await appIdentity('123', f.repository, f.env.GITHUB_APP_PRIVATE_KEY, { fetch }),
    { appId: '123', botLogin: f.bot, installationId: '456', repository: f.repository, permissions: 'issues:write,metadata:read' });
  f.github.permissions = { issues: 'write', metadata: 'read', contents: 'write' };
  await assert.rejects(appIdentity('123', f.repository, f.env.GITHUB_APP_PRIVATE_KEY, { fetch }), /Issues write only/);
  f.github.permissions = null; f.github.selection = 'all';
  await assert.rejects(appIdentity('123', f.repository, f.env.GITHUB_APP_PRIVATE_KEY, { fetch }), /one repository only/);
  f.github.selection = null; f.github.reach = [f.repository, 'WebDevBooster/richos'];
  await assert.rejects(appIdentity('123', f.repository, f.env.GITHUB_APP_PRIVATE_KEY, { fetch }), /reaches 2 repositories/);
});

test('the content checks take linear time on the largest adversarial body', async () => {
  const { contentProblem } = await import('../service/report/checks.mjs');
  const id = reportId(), footer = `\n\nSent with RichDoctor 1.0 · Report ID ${id}`, room = 60000 - Buffer.byteLength(footer);
  for (const filler of ['a'.repeat(room - 1) + '@', 'a@'.repeat(Math.floor(room / 2)), '\\'.repeat(room), 'password='.repeat(Math.floor(room / 9))]) {
    const started = performance.now();
    contentProblem({ title: 'x', body: filler + footer, reportId: id, secret: 'z'.repeat(43) });
    const spent = performance.now() - started;
    assert(spent < 500, `${filler.slice(0, 12)} took ${Math.round(spent)} ms`);
  }
});
