const test = require('node:test'), assert = require('node:assert/strict');
const { createClient } = require('../core/client.js');
const { harness } = require('../dev/client-runtime.js');
const fixture = require('../dev/update-fixture.js');
const turn = () => new Promise(resolve => setImmediate(resolve));
async function pair(app, h) { await app.dispatch({ type: 'pair', link: 'https://mac.example/#pair=x' }); await app.dispatch({ type: 'confirm-pair', matched: true }); await turn(); h.opened.at(-1).event('hello', { challenge: 'fresh', capabilities: ['text'], threads: [{ id: 'general', title: 'General' }, { id: 'other', title: 'Other' }] }); await app.settle(); }
test('hello history uses server order; switching preserves each draft and rejects late frames', async t => {
  const h = harness(), app = await createClient(h.ports); t.after(() => app.close()); await pair(app, h);
  const old = h.opened.at(-1);
  old.event('hello', { capabilities: ['text'], messages: [{ id: 'b', cursor: 9, text: 'later', role: 'rich' }, { id: 'a', cursor: 3, text: 'earlier', role: 'ceo' }] }); await app.settle();
  assert.deepEqual(app.state().messages.map(x => x.id), ['a', 'b']);
  await app.dispatch({ type: 'compose', text: 'general draft' });
  await app.dispatch({ type: 'select-thread', threadId: 'other' }); await turn();
  old.event('message', { id: 'late', cursor: 10, text: 'wrong thread', role: 'rich' }); await app.settle();
  assert.equal(app.state().messages.length, 0);
  await app.dispatch({ type: 'compose', text: 'other draft' });
  await app.dispatch({ type: 'select-thread', threadId: 'general' }); await turn();
  assert.equal(app.state().draft, 'general draft'); assert.deepEqual(app.state().messages.map(x => x.id), ['a', 'b']);
  app.close(); const restored = await createClient(h.ports); t.after(() => restored.close());
  await restored.dispatch({ type: 'select-thread', threadId: 'other' }); assert.equal(restored.state().draft, 'other draft');
});
test('a slow send cannot block composing, changing thread or suspension', async t => {
  const h = harness(), original = h.ports.fetch; let finish;
  h.ports.fetch = (url, args) => url.endsWith('/api/messages') ? new Promise(resolve => { finish = resolve; }) : original(url, args);
  const app = await createClient(h.ports); t.after(() => app.close()); await pair(app, h);
  await app.dispatch({ type: 'compose', text: 'sending' }); await app.dispatch({ type: 'send' }); await turn(); assert(finish);
  await app.dispatch({ type: 'compose', text: 'next draft' }); await app.dispatch({ type: 'select-thread', threadId: 'other' });
  await app.dispatch({ type: 'suspend' }); assert.equal(app.state().online, false);
  finish(Response.json({ message_id: 'intake_a', cursor: 7 })); await app.settle();
  assert.equal(app.state().messages.length, 0);
  await app.dispatch({ type: 'select-thread', threadId: 'general' }); assert.equal(app.state().draft, 'next draft'); assert.equal(app.state().messages[0].text, 'sending');
});
test('a blocking update stops recording safely and gates actions, with durable work retained', async t => {
  const h = harness(); h.ports.now = Date.now; const wrapped = await fixture.wrap(h.ports), app = await createClient(wrapped.ports); t.after(() => app.close()); await pair(app, h);
  await app.dispatch({ type: 'compose', text: 'keep this draft' }); await app.dispatch({ type: 'record-start' });
  const banner = fixture.policy('banner'); await wrapped.install(banner, app); assert.equal(app.state().updates.mode, 'none');
  await wrapped.install(fixture.policy('blocking', 2), app); await app.settle();
  assert.equal(app.state().recording, false); assert.equal(app.state().recordings.length, 1); assert.equal(app.state().draft, 'keep this draft');
  assert.equal(app.state().updates.mode, 'blocking'); await assert.rejects(app.dispatch({ type: 'send' }), /latest/);
  await assert.rejects(app.dispatch({ type: 'record-start' }), /latest/);
  await wrapped.install(fixture.policy('none', 3), app); assert.equal(app.state().updates.blocked, false);
});
test('feature switches gate already queued work and unsupported clients do not retry forever', async t => {
  const h = harness(); h.ports.now = Date.now; const wrapped = await fixture.wrap(h.ports), app = await createClient(wrapped.ports); t.after(() => app.close()); await pair(app, h);
  await app.dispatch({ type: 'suspend' }); await app.dispatch({ type: 'compose', text: 'queued safely' }); await app.dispatch({ type: 'send' });
  await wrapped.install({ ...fixture.policy('none'), features: { text: false } }, app);
  h.opened.at(-1).event('hello', { capabilities: ['text'] }); await app.settle();
  assert.equal(app.state().outbox.length, 1); assert.equal(h.requests.filter(x => x.url.endsWith('/api/messages')).length, 0);
  await wrapped.install(fixture.policy('none', 2), app);
  h.opened.at(-1).event('hello', { capabilities: ['text'], protocol_version: 9 }); await app.settle();
  await assert.rejects(app.dispatch({ type: 'retry' }), /compatible/); assert(app.state().unsupported);
});
test('unknown storage schemas and unresolved drafts prevent destructive migration', async t => {
  const h = harness(); await h.ports.save({ schema: 99, outbox: [{ text: 'untouched' }] });
  await assert.rejects(createClient(h.ports), /retained/); assert.equal(h.disk().outbox[0].text, 'untouched');
  await h.ports.save(null); const app = await createClient(h.ports); t.after(() => app.close()); await pair(app, h);
  await app.dispatch({ type: 'compose', text: 'unsent' }); await assert.rejects(app.dispatch({ type: 'forget-pair', confirm: true }), /unsent/);
  await app.dispatch({ type: 'compose', text: '' }); await app.dispatch({ type: 'forget-pair', confirm: true }); assert.equal(app.state().confirmed, false);
});

test('application and universal links accept only known destinations and cannot trigger pairing', () => {
  const { destination } = require('../core/links.js');
  assert.deepEqual(destination('richos://conversation/#thread=general&at=reply'), { threadId: 'general', messageId: 'reply' });
  assert.equal(destination('https://links.example/#thread=general', ['links.example']).threadId, 'general');
  for (const value of ['richos://pair/#pair=secret', 'richos://conversation/#pair=x', 'https://evil.example/#thread=x', 'richos://conversation/?q=x#thread=y', 'richos://conversation/#thread=x&thread=y']) assert.throws(() => destination(value, ['links.example']));
});
test('distribution configuration rejects missing listing and unsafe service destinations', async t => {
  const { configuration } = await import('../cli/release.mjs');
  const { writeFileSync: write, rmSync: remove } = require('node:fs'), { join } = require('node:path');
  const saved = process.env.RICHOS_MOBILE_RELEASE_CONFIG; delete process.env.RICHOS_MOBILE_RELEASE_CONFIG;
  t.after(() => { if (saved === undefined) delete process.env.RICHOS_MOBILE_RELEASE_CONFIG; else process.env.RICHOS_MOBILE_RELEASE_CONFIG = saved; });
  const dir = require('./storage.cjs').createScratch('release-config-distribution');
  t.after(() => remove(dir, { recursive: true, force: true }));
  const committed = configuration(undefined, true);
  assert.match(committed.appId, /^\d{6,15}$/, 'the committed file carries the listing ID'); assert.equal(committed.metrics, false);
  const check = (name, change) => { const file = join(dir, `${name}.json`); write(file, JSON.stringify({ ...committed, ...change })); return () => configuration(file, true); };
  assert.doesNotThrow(check('valid', {}));
  assert.throws(check('no-listing', { appId: null }), /Configure the verified RichOS Apple app ID/);
  assert.throws(check('no-support', { supportURL: null }), /Configure supportURL/);
  assert.throws(check('insecure-support', { supportURL: 'http://richos.ceo/support' }), /Invalid supportURL/);
  assert.throws(check('insecure-policy', { policyURL: 'http://updates.richos.ceo/v1/policy' }), /Invalid policyURL/);
  assert.throws(check('wrong-policy-path', { policyURL: 'https://updates.richos.ceo/other' }), /\/v1\/policy/);
});

test('Release carries the hosted update-policy address; development keeps its local override', async t => {
  const { configuration } = await import('../cli/release.mjs');
  const { writeFileSync: write, rmSync: remove } = require('node:fs'), { join } = require('node:path');
  const saved = process.env.RICHOS_MOBILE_RELEASE_CONFIG; delete process.env.RICHOS_MOBILE_RELEASE_CONFIG;
  t.after(() => { if (saved === undefined) delete process.env.RICHOS_MOBILE_RELEASE_CONFIG; else process.env.RICHOS_MOBILE_RELEASE_CONFIG = saved; });
  const release = configuration(undefined, false, 'Release');
  assert.equal(release.policyURL, 'https://updates.richos.ceo/v1/policy');
  // A first-level name under richos.ceo stays inside the zone's Universal SSL certificate; never the Connect host.
  assert.match(new URL(release.policyURL).hostname, /^[a-z0-9-]+\.richos\.ceo$/);
  assert.notEqual(new URL(release.policyURL).hostname, 'connect.richos.ceo');
  assert.equal(configuration().policyURL, release.policyURL, 'the default is the Release configuration');
  assert.equal(configuration(undefined, false, 'Debug').policyURL, null, 'development never reads production policy by default');
  assert.throws(() => configuration(undefined, false, 'Profile'), /Debug or Release/);
  const dir = require('./storage.cjs').createScratch('release-config');
  t.after(() => remove(dir, { recursive: true, force: true }));
  const file = join(dir, 'lab.json');
  write(file, JSON.stringify({ ...release, policyURL: 'https://policy-lab.example.net/v1/policy' }));
  process.env.RICHOS_MOBILE_RELEASE_CONFIG = file;
  assert.equal(configuration(undefined, false, 'Debug').policyURL, 'https://policy-lab.example.net/v1/policy');
  assert.equal(configuration(file, false, 'Release').policyURL, 'https://policy-lab.example.net/v1/policy');
});

// release-config.json is a version the earlier iPhone app (removed 2026-10-31) carried. The native iPhone
// app sets its version in native-ios/project.yml and the Android app in native-android/app/
// build.gradle.kts; nothing holds any of them to another (CEO ruling §91, 2026-09-26;
// app/scripts/phone-apps-independent.test.sh).
test('the development update fixture reports the version release-config.json ships and offers a newer one', () => {
  const { compare } = require('../core/updates.js');
  const { version, build } = require('../release-config.json');
  // The development update fixture stands in for the running app, so it reports the shipped identity
  // and offers a newer release.
  assert.deepEqual({ version: fixture.client.version, build: fixture.client.build }, { version, build }, 'dev/update-fixture.js client');
  assert.equal(compare(fixture.policy('banner').latest.version, version), 1, 'the fixture offers a newer version');
});

test('a failed durable enqueue cannot leave a ghost message for a later session write to resurrect', async t => {
  const h = harness(), save = h.ports.save; let failed = false, next = 0;
  h.ports.nextId = () => `message-${++next}`;
  h.ports.save = async value => { if (!failed && value.outbox.length) { failed = true; throw Error('Disk full'); } await save(value); };
  const app = await createClient(h.ports); t.after(() => app.close()); await pair(app, h); await app.dispatch({ type: 'suspend' });
  await app.dispatch({ type: 'compose', text: 'must stay a draft' }); await assert.rejects(app.dispatch({ type: 'send' }), /Disk full/);
  await app.dispatch({ type: 'compose', text: 'intentional second attempt' }); await app.dispatch({ type: 'send' });
  assert.equal(h.disk().outbox.length, 1); assert.equal(h.disk().outbox[0].text, 'intentional second attempt');
});

for (const projectionFirst of [true, false]) test(`send acknowledgement and projection produce one bubble across restart (projection first: ${projectionFirst})`, async t => {
  const h = harness(), fetch = h.ports.fetch; let finish;
  h.ports.fetch = (url, args) => url.endsWith('/api/messages') ? new Promise(resolve => { finish = resolve; }) : fetch(url, args);
  let app = await createClient(h.ports); t.after(() => app.close()); await pair(app, h);
  await app.dispatch({ type: 'compose', text: 'one logical message' }); await app.dispatch({ type: 'send' }); await turn();
  const projected = { id: 'turn:user', cursor: 2, role: 'ceo', text: 'one logical message', complete: true };
  if (projectionFirst) { h.opened.at(-1).event('message', projected); await turn(); }
  finish(Response.json({ message_id: 'intake_123', cursor: 2 })); await app.settle();
  assert.equal(app.state().messages.length, 1);
  app.close(); app = await createClient(h.ports); await turn(); assert.equal(app.state().messages.length, 1);
  h.opened.at(-1).event('message', projected); await app.settle();
  assert.equal(app.state().messages.length, 1); assert.equal(app.state().messages[0].id, 'turn:user');
});
