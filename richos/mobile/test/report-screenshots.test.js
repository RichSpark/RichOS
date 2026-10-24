// RichDoctor screenshots through the relay (PRD v6 §6-§8, acceptance A7/A9 logic): the shared
// image profile, register then upload then complete, no issue while an approved image is missing,
// staff-only reads behind a verified Access token, retirement, cleanup serialized with upload and
// completion, and one-year retention.
const test = require('node:test'), assert = require('node:assert/strict');
const { readdirSync, readFileSync, existsSync } = require('node:fs');
const { join } = require('node:path');
const { fixture } = require('./report-fixture.cjs');
const DAY = 86_400_000;
const keys = f => [...f.r2.objects.keys()].sort();

test('the shared profile accepts exactly the golden accept-* files and refuses every reject-* file, client and relay alike', async () => {
  const { checkImage } = await import('../service/report/images.mjs');
  let seen = 0;
  for (const dir of ['relay', 'client']) {
    const path = join(__dirname, '../service/report/fixtures', dir);
    if (!existsSync(path)) continue;
    for (const name of readdirSync(path).filter(n => /^(accept|reject)-.*\.(png|jpg)$/.test(n))) {
      const result = checkImage(readFileSync(join(path, name)), name.endsWith('.png') ? 'image/png' : 'image/jpeg');
      assert.equal(!result.error, name.startsWith('accept-'), `${dir}/${name}: ${JSON.stringify(result)}`);
      seen++;
    }
  }
  assert(seen >= 15, `at least the relay's own fixtures (${seen})`);
  assert.match(checkImage(new Uint8Array([1, 2, 3]), 'image/gif').error, /Only PNG and JPEG/);
});

test('register, upload, complete: one issue with every approved screenshot, linked for staff only', async t => {
  const f = await fixture(t), payload = await f.report({}, ['accept-rgb.png', 'accept-baseline.jpg']);
  let reply = await f.answer(await f.create(payload));
  assert.deepEqual(reply.body, { report_id: payload.report_id, state: 'receiving', stage: 'receiving', stored: [], missing: ['1', '2'] });
  assert.equal(f.posts(), 0, 'registering never creates');
  reply = await f.answer(await f.complete(payload.report_id, payload.receipt_secret));
  assert.deepEqual([reply.body.state, reply.body.missing], ['receiving', ['1', '2']]);
  assert.equal(f.posts(), 0, 'no text-only issue while a screenshot is missing');
  reply = await f.answer(await f.upload(payload, 1));
  assert.deepEqual([reply.body.state, reply.body.stored, reply.body.missing], ['receiving', ['1'], ['2']]);
  reply = await f.answer(await f.upload(payload, 2));
  assert.deepEqual([reply.body.state, reply.body.stored], ['ready', ['1', '2']]);
  assert.equal((await f.answer(await f.upload(payload, 2))).body.state, 'ready', 'an identical retry is acknowledged');
  assert.equal(f.r2.puts, 2, 'and written once');
  reply = await f.answer(await f.complete(payload.report_id, payload.receipt_secret));
  assert.equal(reply.status, 201);
  assert.deepEqual([reply.body.state, reply.body.images_expected, reply.body.images_confirmed], ['delivered', 2, 2]);
  assert.equal(reply.body.images_expire_at, new Date(f.now() + 365 * DAY).toISOString());
  const [issue] = f.github.created();
  assert.equal(issue.body, payload.body, 'the exact approved words, links included');
  assert(issue.body.endsWith(['Screenshots: 2, shared privately with RichOS support (staff sign-in required)',
    `1. ${f.origin}/v1/reports/${payload.report_id}/attachments/1`, `2. ${f.origin}/v1/reports/${payload.report_id}/attachments/2`,
    `Sent with RichDoctor 1.0 \u00B7 Report ID ${payload.report_id}`].join('\n')), 'the round 40 wording, byte for byte');
  assert.equal(f.posts(), 1);
  // Repeats change nothing: complete, register and upload all answer delivered.
  for (const again of [() => f.complete(payload.report_id, payload.receipt_secret), () => f.create(payload), () => f.upload(payload, 1)])
    assert.equal((await f.answer(await again())).body.state, 'delivered');
  assert.equal(f.posts(), 1); assert.equal(f.r2.puts, 2);
  // Staff read the image itself; nobody else reads anything.
  const staff = await f.read(payload.report_id, 1, await f.accessToken());
  assert.equal(staff.status, 200);
  assert.deepEqual(Buffer.from(await staff.arrayBuffer()), payload.files[0]);
  assert.equal(staff.headers.get('content-type'), 'image/png'); assert.equal(staff.headers.get('cache-control'), 'private, no-store');
  assert.equal(staff.headers.get('x-content-type-options'), 'nosniff');
  const other = await crypto.subtle.generateKey({ name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' }, true, ['sign']);
  for (const [why, token] of [['no token', null], ['wrong audience', await f.accessToken({ aud: ['b'.repeat(64)] })], ['wrong issuer', await f.accessToken({ iss: 'https://other.cloudflareaccess.com' })],
    ['expired', await f.accessToken({ exp: Math.floor(f.now() / 1000) - 1 })], ['forged', await f.accessToken({}, other.privateKey)], ['unknown key', await f.accessToken({}, other.privateKey, 'staff-9')]]) {
    const denied = await f.read(payload.report_id, 1, token);
    assert.equal(denied.status, 403, why); assert.doesNotMatch(await denied.text(), /PNG/);
  }
  const noAccess = { ...f.env }; delete noAccess.ACCESS_AUD;
  const { handle } = await import('../service/report-worker.mjs');
  const unconfigured = await handle(new Request(`${f.origin}/v1/reports/${payload.report_id}/attachments/1`, { headers: { 'Cf-Access-Jwt-Assertion': await f.accessToken() } }), noAccess, f.ctx, f.ports);
  assert.equal(unconfigured.status, 403, 'without an Access application every read is refused');
  // The database keeps no words once posted, and no receipt secret ever.
  const row = JSON.stringify(f.db.raw.prepare('SELECT * FROM reports').all());
  assert(!row.includes(payload.receipt_secret)); assert(!row.includes('RichOS closed'));
});

test('uploads accept only the approved bytes in the shared profile; the manifest has hard limits', async t => {
  const f = await fixture(t), payload = await f.report({}, ['accept-rgb.png', 'accept-palette.png']);
  await f.create(payload);
  const refused = async (response, pattern) => { const r = await f.answer(response); assert.equal(r.status, 422); assert.equal(r.body.reason, 'attachment'); assert.match(r.body.detail, pattern); return r; };
  await refused(await f.upload(payload, 1, payload.files[1]), /not the approved/);
  await refused(await f.upload(payload, 1, payload.files[0], { 'X-Content-SHA256': 'f'.repeat(64) }), /not the approved file/);
  await refused(await f.upload(payload, 1, payload.files[0], { 'Content-Type': 'image/jpeg' }), /file type/);
  assert.equal((await f.answer(await f.send(new Request(`${f.origin}/v1/reports/${payload.report_id}/attachments/3`, { method: 'PUT', body: payload.files[0],
    headers: { 'Content-Type': 'image/png', 'X-Receipt-Secret': payload.receipt_secret, 'X-Content-SHA256': payload.manifest[0].sha256 } })))).body.item, 'screenshot 3');
  assert.equal((await f.answer(await f.upload(payload, 1, payload.files[0], { 'X-Receipt-Secret': (await f.report()).receipt_secret }))).body.state, 'forbidden');
  assert.equal(f.r2.puts, 0);
  // A forbidden original is refused even when it is exactly the file the manifest names.
  for (const name of ['reject-text.png', 'reject-exif.jpg', 'reject-original-sips.jpg', 'reject-trailing.png']) {
    const bad = await f.report({}, [name]);
    await f.create(bad);
    const r = await refused(await f.upload(bad, 1), /PNG|JPEG/);
    assert.equal(r.body.item, 'screenshot 1', name);
  }
  // A manifest that misstates the size is refused at upload.
  const wrong = await f.report({}, ['accept-rgb.png']);
  wrong.manifest[0].width = 17;
  const { contentHash } = await import('../service/report/checks.mjs');
  wrong.content_sha256 = await contentHash(wrong.title, wrong.body, wrong.manifest);
  await f.create(wrong);
  await refused(await f.upload(wrong, 1), /approved size/);
  assert.equal(f.r2.puts, 0);
  // Limits: 10 images, 5 MB each, 25 MB together, 40 megapixels, neutral names in order.
  const item = n => ({ attachment_id: String(n), order: n, filename: `screenshot-${n}.png`, media_type: 'image/png', width: 10, height: 10, bytes: 100, sha256: 'a'.repeat(64) });
  const cases = [[Array.from({ length: 11 }, (_, i) => item(i + 1)), 413], [[{ ...item(1), bytes: 5 * 1048576 + 1 }], 413],
    [Array.from({ length: 6 }, (_, i) => ({ ...item(i + 1), bytes: 5 * 1048576 })), 413], [[{ ...item(1), width: 8000, height: 6000 }], 413],
    [[{ ...item(1), filename: 'IMG_0042.png' }], 400], [[item(2)], 400], [[{ ...item(1), extra: 1 }], 400]];
  for (const [manifest, code] of cases) {
    const p = await f.report({}, []);
    p.manifest = manifest; p.content_sha256 = await contentHash(p.title, p.body, manifest);
    assert.equal((await f.answer(await f.create(p))).status, code, JSON.stringify(manifest[0]).slice(0, 60));
  }
});

test('the screenshot lines before the footer must be exactly the generated ones', async t => {
  const f = await fixture(t), good = await f.report({}, ['accept-rgb.png']);
  const { contentHash } = await import('../service/report/checks.mjs');
  for (const change of [l => l.replace('attachments/1', 'attachments/2'), l => l.replace(f.origin, 'https://evil.example'), l => l.replace('Screenshots: 1', 'Screenshots: 2'), l => l.replace('(staff sign-in required)', '')]) {
    const lines = good.body.split('\n'), at = lines.findIndex(l => l.startsWith('Screenshots:'));
    const changed = { ...good, body: lines.map((l, i) => (i === at || i === at + 1) ? change(l) : l).join('\n') };
    changed.content_sha256 = await contentHash(changed.title, changed.body, changed.manifest);
    const r = await f.answer(await f.create(changed));
    assert.equal(r.body.reason, 'content'); assert.match(r.body.detail, /screenshot lines/);
  }
  assert.equal((await f.answer(await f.create(good))).body.state, 'receiving');
});

test('A9: a receipt write lost after the object was stored is recovered by the same upload, never a second object', async t => {
  const f = await fixture(t), payload = await f.report({}, ['accept-rgb.png']);
  await f.create(payload);
  f.db.failNext('UPDATE attachments SET stored=1');
  assert.equal((await f.upload(payload, 1)).status, 503);
  assert.equal(f.r2.puts, 1, 'the object was stored');
  assert.equal((await f.answer(await f.status(payload.report_id, payload.receipt_secret))).body.state, 'receiving', 'but not recorded');
  assert.equal((await f.answer(await f.upload(payload, 1))).body.state, 'ready');
  assert.equal(f.r2.puts, 1, 'the retry found it and only recorded it');
  // A stored flag whose object is gone is not proof: completion names it missing again.
  f.r2.objects.clear();
  let reply = await f.answer(await f.complete(payload.report_id, payload.receipt_secret));
  assert.deepEqual([reply.body.state, reply.body.missing], ['receiving', ['1']]);
  assert.equal(f.posts(), 0);
  await f.upload(payload, 1);
  reply = await f.answer(await f.complete(payload.report_id, payload.receipt_secret));
  assert.deepEqual([reply.body.state, reply.body.images_confirmed], ['delivered', 1]);
  // An image lost behind the relay after creation: the issue stands and is never recreated.
  f.r2.objects.clear();
  reply = await f.answer(await f.status(payload.report_id, payload.receipt_secret));
  assert.deepEqual([reply.body.state, reply.body.images_confirmed, reply.body.issue_number], ['created_checking_images', 0, 1]);
  assert.equal((await f.answer(await f.complete(payload.report_id, payload.receipt_secret))).body.state, 'created_checking_images');
  assert.equal(f.posts(), 1);
});

test('A9: retirement before a changed revision, including an ID the relay never saw', async t => {
  const f = await fixture(t), unseen = await f.report({}, ['accept-rgb.png']);
  let reply = await f.answer(await f.retire(unseen.report_id, unseen.receipt_secret));
  assert.deepEqual([reply.body.state, reply.body.stage], ['retired', 'retired']);
  assert.equal((await f.answer(await f.retire(unseen.report_id, unseen.receipt_secret))).body.state, 'retired', 'a lost acknowledgment is simply asked again');
  assert.equal((await f.answer(await f.retire(unseen.report_id, (await f.report()).receipt_secret))).body.state, 'forbidden');
  // The delayed first registration and everything after it cannot revive it.
  for (const late of [() => f.create(unseen), () => f.upload(unseen, 1), () => f.complete(unseen.report_id, unseen.receipt_secret)])
    assert.equal((await f.answer(await late())).body.state, 'retired');
  // A registered, unsent report retires; one whose creation began cannot.
  const ready = await f.report({}, ['accept-rgb.png']);
  await f.create(ready); await f.upload(ready, 1);
  assert.equal((await f.answer(await f.retire(ready.report_id, ready.receipt_secret))).body.state, 'retired');
  assert.equal((await f.answer(await f.complete(ready.report_id, ready.receipt_secret))).body.state, 'retired');
  const sent = await f.report({}, ['accept-rgb.png']);
  await f.create(sent); await f.upload(sent, 1);
  f.github.fault('create:lost'); for (let i = 0; i < 3; i++) f.github.fault('list:500');
  assert.equal((await f.answer(await f.complete(sent.report_id, sent.receipt_secret))).body.state, 'pending');
  assert.equal((await f.answer(await f.retire(sent.report_id, sent.receipt_secret))).body.state, 'pending', 'refused while creation is uncertain');
  f.github.faults.length = 0; f.advance(30_000);
  assert.equal((await f.answer(await f.retire(sent.report_id, sent.receipt_secret))).body.state, 'delivered', 'and answers the earlier outcome');
  assert.equal(f.posts(), 1);
  // A retire racing a held receipt lock is told to ask again, never answered retired.
  const busy = await f.report(); await f.create({ ...busy, schema: 'richdoctor-report/1' });
  const other = await f.report({}, ['accept-rgb.png']); await f.create(other);
  const token = await f.store.acquire(other.report_id);
  assert.equal((await f.answer(await f.retire(other.report_id, other.receipt_secret))).body.state, 'limited');
  await f.store.release(other.report_id, token);
  assert.equal((await f.answer(await f.retire(other.report_id, other.receipt_secret))).body.state, 'retired');
});

test('A9: cleanup races upload and completion under the receipt lock, resumes after a crash and never touches uncertain creation', async t => {
  const f = await fixture(t), abandoned = await f.report({}, ['accept-rgb.png', 'accept-palette.png']);
  await f.create(abandoned); await f.upload(abandoned, 1);
  // Creation began and is uncertain: cleanup must leave it alone however old it gets.
  const uncertain = await f.report({}, ['accept-baseline.jpg']);
  await f.create(uncertain); await f.upload(uncertain, 1);
  f.github.fault('create:lost'); for (let i = 0; i < 40; i++) f.github.fault('list:500');
  assert.equal((await f.answer(await f.complete(uncertain.report_id, uncertain.receipt_secret))).body.state, 'pending');
  f.advance(6 * DAY); await f.cleanup();
  assert.equal(f.r2.objects.size, 2, 'nothing is removed before seven days without activity');
  f.advance(2 * DAY);
  // The scheduled run is interrupted after its claim: images are unavailable, objects not yet gone.
  const lock = await f.store.acquire(abandoned.report_id);
  assert(await f.store.claimCleanup(abandoned.report_id, lock, 7 * DAY));
  await f.store.release(abandoned.report_id, lock);
  let reply = await f.answer(await f.status(abandoned.report_id, abandoned.receipt_secret));
  assert.deepEqual([reply.body.stored, reply.body.missing], [[], ['1', '2']], 'marked unavailable before any delete');
  assert.equal((await f.answer(await f.upload(abandoned, 2))).body.state, 'limited', 'no upload while a cleanup is claimed');
  assert.equal((await f.answer(await f.complete(abandoned.report_id, abandoned.receipt_secret))).body.state, 'limited', 'no completion either');
  // A run that finds the receipt locked skips it; the next resumes the claimed cleanup.
  const held = await f.store.acquire(abandoned.report_id);
  await f.cleanup();
  assert(keys(f).some(k => k.startsWith(abandoned.report_id)), 'skipped while the receipt is locked');
  await f.store.release(abandoned.report_id, held);
  await f.cleanup();
  assert(!keys(f).some(k => k.startsWith(abandoned.report_id)), 'resumed and finished');
  assert(keys(f).some(k => k.startsWith(uncertain.report_id)), 'the uncertain report kept its screenshot');
  // Completion after cleanup gets the missing list; the unsent report resumes by reuploading.
  reply = await f.answer(await f.complete(abandoned.report_id, abandoned.receipt_secret));
  assert.deepEqual([reply.body.state, reply.body.missing], ['receiving', ['1', '2']]);
  assert.equal(f.posts(), 1, 'only the uncertain report ever reached GitHub');
  await f.upload(abandoned, 1); await f.upload(abandoned, 2);
  reply = await f.answer(await f.complete(abandoned.report_id, abandoned.receipt_secret));
  assert.deepEqual([reply.body.state, reply.body.images_confirmed], ['delivered', 2]);
  assert.equal(f.github.created().filter(i => i.body.includes(abandoned.report_id)).length, 1);
  // A crash after the objects were deleted but before "done": the claim is resumed, not repeated wrongly.
  const crash = await f.report({}, ['accept-rgb.png']);
  await f.create(crash); await f.upload(crash, 1);
  f.advance(8 * DAY); f.r2.fault('delete');
  await f.cleanup();
  assert.equal((await f.store.get(crash.report_id)).cleanup, 'claimed');
  await f.cleanup();
  assert.equal((await f.store.get(crash.report_id)).cleanup, 'done');
  assert.deepEqual((await f.answer(await f.upload(crash, 1))).body.stored, ['1'], 'reupload after a finished cleanup works');
});

test('delivered screenshots are kept one year, can be extended before expiry, then deleted with an honest notice', async t => {
  const f = await fixture(t), payload = await f.report({}, ['accept-rgb.png']);
  await f.create(payload); await f.upload(payload, 1); await f.complete(payload.report_id, payload.receipt_secret);
  f.advance(364 * DAY); await f.cleanup();
  assert.equal(f.r2.objects.size, 1);
  // The operator's extension, through the same D1 statement the CLI sends.
  const { operate } = await import('../cli/report.mjs');
  const profile = { stage: 'test', worker: 'richos-report-test', hostname: 'report-test.richos.ceo', repository: f.repository, accountId: 'a'.repeat(32),
    databaseId: '11111111-2222-3333-4444-555555555555', appId: '123', installationId: '456', botLogin: f.bot, label: 'richdoctor', bucket: 'richos-report-test' };
  const d1 = async (_url, init) => {
    const { sql, params } = JSON.parse(init.body);
    const changes = Number(f.db.raw.prepare(sql).run(...params).changes);
    return new Response(JSON.stringify({ success: true, result: [{ success: true, results: [], meta: { changes } }] }));
  };
  const realNow = Date.now; Date.now = f.now;
  try { assert.match((await operate('extend', profile, [payload.report_id, '30'], { token: 't', fetch: d1 })).expiresAt, /^2027-/); } finally { Date.now = realNow; }
  f.advance(2 * DAY); await f.cleanup();
  assert.equal(f.r2.objects.size, 1, 'extended past the first year');
  f.advance(40 * DAY); await f.cleanup();
  assert.equal(f.r2.objects.size, 0);
  const reply = await f.answer(await f.status(payload.report_id, payload.receipt_secret));
  assert.deepEqual([reply.body.state, reply.body.images_expired, reply.body.images_confirmed], ['delivered', true, 0]);
  const gone = await f.read(payload.report_id, 1, await f.accessToken());
  assert.equal(gone.status, 410); assert.match(await gone.text(), /one-year retention ended/);
  Date.now = f.now;
  try { await assert.rejects(operate('extend', profile, [payload.report_id, '30'], { token: 't', fetch: d1 }), /Not extended/); } finally { Date.now = realNow; }
});

test('handover 2: retirement withdraws staff reads at once and its screenshots go at the next cleanup', async t => {
  const f = await fixture(t), payload = await f.report({}, ['accept-rgb.png']);
  await f.create(payload); await f.upload(payload, 1);
  assert.equal((await f.read(payload.report_id, 1, await f.accessToken())).status, 200, 'readable by staff while it may still be sent');
  assert.equal((await f.answer(await f.retire(payload.report_id, payload.receipt_secret))).body.state, 'retired');
  const withdrawn = await f.read(payload.report_id, 1, await f.accessToken());
  assert.equal(withdrawn.status, 410); assert.match(await withdrawn.text(), /withdrawn/);
  await f.cleanup();
  assert.equal(f.r2.objects.size, 0, 'deleted without waiting seven days');
  assert.equal((await f.read(payload.report_id, 1, await f.accessToken())).status, 410);
});

test('handover 3: an identical registration retry answers the current state; any mismatch is refused; retired stays retired', async t => {
  const f = await fixture(t), payload = await f.report({}, ['accept-rgb.png']);
  await f.create(payload);                                    // its reply is lost
  assert.deepEqual((await f.answer(await f.create(payload))).body.missing, ['1'], 'the same registration again: receiving, not an error');
  await f.upload(payload, 1);
  assert.equal((await f.answer(await f.create(payload))).body.state, 'ready');
  const { contentHash } = await import('../service/report/checks.mjs');
  const changed = { ...payload, manifest: [{ ...payload.manifest[0], sha256: 'b'.repeat(64) }] };
  changed.content_sha256 = await contentHash(changed.title, changed.body, changed.manifest);
  assert.equal((await f.answer(await f.create(changed))).body.state, 'conflict', 'a different manifest under the same ID');
  await f.retire(payload.report_id, payload.receipt_secret);
  assert.equal((await f.answer(await f.create(payload))).body.state, 'retired');
  assert.equal(f.posts(), 0);
});

test('the daily stored-bytes cap closes uploads honestly and stores nothing', async t => {
  const f = await fixture(t, { bytesCap: 100 }), payload = await f.report({}, ['accept-rgb.png']);
  await f.create(payload);
  const reply = await f.answer(await f.upload(payload, 1));
  assert.deepEqual([reply.status, reply.body.state, reply.body.reason], [503, 'closed', 'daily_cap']);
  assert.equal(f.r2.objects.size, 0);
  assert.equal((await f.answer(await f.status(payload.report_id, payload.receipt_secret))).body.state, 'receiving');
});

test('a recovered report keeps its approved screenshots: an older unknown ID adopted by its footer restores the manifest', async t => {
  const f = await fixture(t), kept = await f.report({}, ['accept-rgb.png']), lost = await f.report({}, ['accept-rgb.png', 'accept-palette.png']);
  for (const p of [kept, lost]) { await f.create(p); for (let n = 1; n <= p.manifest.length; n++) await f.upload(p, n); await f.complete(p.report_id, p.receipt_secret); }
  // The relay's receipt and manifest rows are gone; the issues and the stored objects remain,
  // except one of `lost`'s objects.
  for (const p of [kept, lost]) for (const table of ['attachments', 'reports']) f.db.raw.prepare(`DELETE FROM ${table} WHERE report_id=?`).run(p.report_id);
  f.r2.objects.delete(keys(f).find(k => k.startsWith(lost.report_id + '/2-')));
  f.advance(3600_000);
  const posts = f.posts();
  let reply = await f.answer(await f.create(kept));
  assert.deepEqual([reply.body.state, reply.body.images_expected, reply.body.images_confirmed], ['delivered', 1, 1]);
  assert.equal((await f.read(kept.report_id, 1, await f.accessToken())).status, 200, 'staff can open the recovered screenshot');
  reply = await f.answer(await f.create(lost));
  assert.deepEqual([reply.body.state, reply.body.images_expected, reply.body.images_confirmed], ['created_checking_images', 2, 1], 'a missing object is never claimed delivered');
  assert.equal(f.posts(), posts, 'nothing was created again');
  // The restored manifest carries the one-year retention like any delivery.
  f.advance(366 * DAY); await f.cleanup();
  assert.equal(f.r2.objects.size, 0);
  assert.equal((await f.read(kept.report_id, 1, await f.accessToken())).status, 410);
});

test('a claimed cleanup withdraws staff reads at once, even when deleting the objects failed', async t => {
  const f = await fixture(t), payload = await f.report({}, ['accept-rgb.png']);
  await f.create(payload); await f.upload(payload, 1); await f.complete(payload.report_id, payload.receipt_secret);
  f.advance(366 * DAY); f.r2.fault('delete');
  await f.cleanup();
  assert.equal((await f.store.get(payload.report_id)).cleanup, 'claimed');
  assert.equal(f.r2.objects.size, 1, 'the object is still in the bucket');
  const read = await f.read(payload.report_id, 1, await f.accessToken());
  assert.equal(read.status, 410); assert.match(await read.text(), /being deleted.*one-year retention ended/);
  await f.cleanup();
  const gone = await f.read(payload.report_id, 1, await f.accessToken());
  assert.equal(gone.status, 410); assert.match(await gone.text(), /was deleted on/);
});

test('uploads go to uploads/n, outside the staff-only links, so an Access application on the links never blocks the app', async t => {
  const f = await fixture(t), payload = await f.report({}, ['accept-rgb.png', 'accept-palette.png']);
  await f.create(payload);
  assert.deepEqual((await f.answer(await f.upload(payload, 1))).body.stored, ['1']);
  // Where no Access application sits in front (a test relay), the older PUT path still works.
  assert.equal((await f.answer(await f.upload(payload, 2, undefined, {}, 'attachments'))).body.state, 'ready');
  const token = await f.accessToken();
  const onUploads = await f.send(new Request(`${f.origin}/v1/reports/${payload.report_id}/uploads/1`, { headers: { 'Cf-Access-Jwt-Assertion': token } }));
  assert.equal(onUploads.status, 404, 'an image is read only from its staff link');
  assert.equal((await f.read(payload.report_id, 1, token)).status, 200);
  assert.equal((await f.read(payload.report_id, 1)).status, 403);
});

test('the production smoke test files exactly one report with one screenshot, and never a second', async t => {
  const { smoke, SMOKE_TITLE, CONFIRM } = await import('../dev/report-smoke-production.mjs');
  const { PRODUCTION } = await import('../cli/report.mjs');
  const f = await fixture(t, { repository: PRODUCTION.repository, bot: 'richos-reports[bot]' });
  const profile = { stage: 'production', ...PRODUCTION, bucket: 'richos-report', accountId: 'a'.repeat(32), databaseId: '11111111-2222-3333-4444-555555555555',
    appId: '123', installationId: '456', botLogin: 'richos-reports[bot]', label: 'richdoctor' };
  const fetchImpl = async (url, init = {}) => url.startsWith('https://api.github.com/') ? f.github.fetch(url, init)
    : url === 'https://report.richos.ceo/v1/health' ? new Response(JSON.stringify({ ok: true, service: 'richos-report', protocol: 2, accepting: true }))
    : f.send(new Request(url, init));
  const screenshot = readFileSync(join(__dirname, '../service/report/fixtures/relay/accept-rgb.png'));
  const deps = { fetchImpl, screenshot, wait: async () => {} };
  // The script stamps its approval with Date.now(); the relay here runs on the fixture's fixed
  // clock, so the script must read that clock too, or the stamp lands after it on a later date.
  const realNow = Date.now; Date.now = f.now; t.after(() => { Date.now = realNow; });
  await assert.rejects(smoke({ ...profile, stage: 'test', worker: 'richos-report-test', hostname: 'report-test.example.com', repository: 'a/b', bucket: 'richos-report-test' }, { ...deps, confirm: CONFIRM }), /production route only/);
  await assert.rejects(smoke(profile, deps), /confirmation word/);
  assert.equal(f.posts(), 0);
  const result = await smoke(profile, { ...deps, confirm: CONFIRM });
  const issue = f.github.created()[0];
  assert.equal(f.posts(), 1);
  assert.deepEqual([result.issue, issue.title, result.withoutSignIn], ['https://github.com/WebDevBooster/richos/issues/1', SMOKE_TITLE, 403]);
  assert(issue.body.includes(`1. ${result.screenshotLink}`) && result.screenshotLink.startsWith('https://report.richos.ceo/v1/reports/'));
  assert.deepEqual(keys(f).length, 1, 'its one screenshot is stored');
  await assert.rejects(smoke(profile, { ...deps, confirm: CONFIRM }), /already sent: https:\/\/github.com\/WebDevBooster\/richos\/issues\/1/);
  assert.equal(f.posts(), 1, 'never a second production report');
});

test('the production smoke test finds an earlier smoke report on a later page of the bot\'s issues', async t => {
  const { smoke, SMOKE_TITLE, CONFIRM } = await import('../dev/report-smoke-production.mjs');
  const { PRODUCTION } = await import('../cli/report.mjs');
  // One issue per page; the listing reaches the smoke report only on its fourth page.
  const f = await fixture(t, { repository: PRODUCTION.repository, bot: 'richos-reports[bot]', pageSize: 1 });
  for (let i = 0; i < 3; i++) f.github.add(`A real report ${i}`, 'words', 'richos-reports[bot]');
  f.github.add(SMOKE_TITLE, 'the earlier smoke report', 'richos-reports[bot]');
  const profile = { stage: 'production', ...PRODUCTION, bucket: 'richos-report', accountId: 'a'.repeat(32), databaseId: '11111111-2222-3333-4444-555555555555',
    appId: '123', installationId: '456', botLogin: 'richos-reports[bot]', label: 'richdoctor' };
  const fetchImpl = async (url, init = {}) => url.startsWith('https://api.github.com/') ? f.github.fetch(url, init)
    : url === 'https://report.richos.ceo/v1/health' ? new Response(JSON.stringify({ ok: true, service: 'richos-report', protocol: 2, accepting: true }))
    : f.send(new Request(url, init));
  const screenshot = readFileSync(join(__dirname, '../service/report/fixtures/relay/accept-rgb.png'));
  await assert.rejects(smoke(profile, { fetchImpl, screenshot, wait: async () => {}, confirm: CONFIRM }), /already sent: https:\/\/github.com\/WebDevBooster\/richos\/issues\/4/);
  assert.equal(f.posts(), 0, 'nothing was sent');
});
