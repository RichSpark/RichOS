// The report relay on a real SQLite database with its own schema, a fake GitHub, a fake R2, a fake
// Cloudflare Access team and a virtual clock (sleep advances it). Proves the relay's logic; real
// delivery is the live A9 run.
const { DatabaseSync } = require('node:sqlite');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { createHash } = require('node:crypto');

function database(t) {
  const db = new DatabaseSync(':memory:'); t.after(() => db.close());
  db.exec(readFileSync(join(__dirname, '../service/report/schema.sql'), 'utf8'));
  let failing = null;
  const shim = {
    raw: db,
    // The next write whose SQL contains `text` fails, as a lost database write would.
    failNext(text) { failing = text; },
    prepare(sql) { return { bind(...args) { const query = db.prepare(sql); return {
      sql,
      first: async () => query.get(...args) || null,
      all: async () => ({ results: query.all(...args) }),
      run: async () => {
        if (failing && sql.includes(failing)) { failing = null; throw Error('d1_unavailable'); }
        return { meta: { changes: Number(query.run(...args).changes) } };
      },
    }; } }; },
    async batch(statements) {
      db.exec('BEGIN IMMEDIATE');
      try { const rows = []; for (const statement of statements) rows.push(await statement.run()); db.exec('COMMIT'); return rows; }
      catch (error) { db.exec('ROLLBACK'); throw error; }
    },
  };
  return shim;
}

const ALPHABET = '23456789ABCDEFGHJKMNPQRSTVWXYZ';
const pick = n => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => ALPHABET[b % 30]).join('');
const reportId = () => `RD-${pick(4)}-${pick(4)}`;
const secret = () => Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString('base64url');
const fixtures = join(__dirname, '../service/report/fixtures/relay');
const image = name => readFileSync(join(fixtures, name));
const hexOf = bytes => createHash('sha256').update(bytes).digest('hex');
const b64url = value => Buffer.from(typeof value === 'string' ? value : JSON.stringify(value)).toString('base64url');
const TEAM = 'https://richos-test.cloudflareaccess.com', AUD = 'a'.repeat(64);

async function fixture(t, options = {}) {
  const { handle, scheduled } = await import('../service/report-worker.mjs');
  const { Store } = await import('../service/report/store.mjs');
  const { contentHash, attachmentLines } = await import('../service/report/checks.mjs');
  const { checkImage } = await import('../service/report/images.mjs');
  const { FakeGitHub, appKey } = await import('../dev/report-fake-github.mjs');
  const { FakeR2 } = await import('../dev/report-fake-r2.mjs');
  const { _forgetKeys } = await import('../service/report/access.mjs');
  _forgetKeys();
  let time = Date.parse('2026-10-22T12:00:00Z');
  const now = () => time, db = database(t), key = await appKey(), r2 = new FakeR2();
  const repository = 'richos-test/report-relay-test', bot = 'richos-reports-test[bot]', origin = 'https://report.example.com';
  const github = new FakeGitHub({ repository, bot, appId: '123', installationId: '456', publicKey: key.publicKey, now, pageSize: options.pageSize || 100 });
  // The Access team's signing key and its published certificate list.
  const staff = await crypto.subtle.generateKey({ name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' }, true, ['sign', 'verify']);
  const jwk = { ...(await crypto.subtle.exportKey('jwk', staff.publicKey)), kid: 'staff-1' };
  async function accessToken(claims = {}, signer = staff.privateKey, kid = 'staff-1') {
    const header = b64url({ alg: 'RS256', kid, typ: 'JWT' }), seconds = Math.floor(time / 1000);
    const body = b64url({ iss: TEAM, aud: [AUD], exp: seconds + 3600, iat: seconds, email: 'support@richos.ceo', ...claims });
    const signature = Buffer.from(await crypto.subtle.sign('RSASSA-PKCS1-v1_5', signer, new TextEncoder().encode(header + '.' + body))).toString('base64url');
    return header + '.' + body + '.' + signature;
  }
  const allow = { limit: async () => ({ success: true }) };
  const env = { DB: db, IMAGES: r2, CREATE_LIMIT: allow, STATUS_LIMIT: allow, UPLOAD_LIMIT: allow, GITHUB_APP_ID: '123', GITHUB_INSTALLATION_ID: '456',
    GITHUB_APP_PRIVATE_KEY: options.pkcs1 ? key.pkcs1 : key.pkcs8, GITHUB_REPOSITORY: repository, GITHUB_BOT_LOGIN: bot, MODERATION_LABEL: 'richdoctor',
    ACCESS_TEAM_DOMAIN: TEAM, ACCESS_AUD: AUD };
  db.raw.exec(`UPDATE control SET paused=0, daily_cap=${options.cap ?? 100}, daily_bytes_cap=${options.bytesCap ?? 100 * 1048576}, operator='test'`);
  const waits = [], ctx = { waitUntil: promise => waits.push(promise) };
  // Like workerd's fetch, this one refuses to be called as a method of another object.
  const unbound = function (input, init) {
    'use strict';
    if (this !== undefined) throw new TypeError('Illegal invocation');
    const url = typeof input === 'string' ? input : input.url;
    if (url === `${TEAM}/cdn-cgi/access/certs`) return Promise.resolve(new Response(JSON.stringify({ keys: [jwk] }), { status: 200 }));
    return github.fetch(input, init);
  };
  const ports = { now, sleep: async ms => { time += ms; }, fetch: unbound };
  const send = request => handle(request, env, ctx, ports);
  // A report. `images` names fixture files (protocol 2); without it, protocol 1.
  async function report(overrides = {}, images = null) {
    const id = overrides.report_id || reportId(), title = overrides.title ?? 'RichOS does not open';
    const manifest = images && images.map((name, i) => {
      const bytes = image(name), type = name.endsWith('.png') ? 'image/png' : 'image/jpeg', size = checkImage(bytes, type);
      return { attachment_id: String(i + 1), order: i + 1, filename: `screenshot-${i + 1}.${type === 'image/png' ? 'png' : 'jpg'}`, media_type: type,
        width: size.width || 1, height: size.height || 1, bytes: bytes.length, sha256: hexOf(bytes) };
    });
    const lines = attachmentLines(origin, id, manifest ? manifest.length : 0);
    const body = overrides.body ?? ['### What happened', 'RichOS closed when it started\\.', '', ...lines, `Sent with RichDoctor 1.0 · Report ID ${id}`].join('\n');
    const payload = { schema: manifest ? 'richdoctor-report/2' : 'richdoctor-report/1', report_id: id, receipt_secret: secret(), content_sha256: '',
      installation_id: crypto.randomUUID(), app_version: '1.0.0', approved_at: time, title, body, ...(manifest ? { manifest } : {}), ...overrides };
    if (overrides.content_sha256 === undefined) payload.content_sha256 = await contentHash(payload.title, payload.body, payload.manifest || null);
    if (images) payload.files = images.map(image);
    return payload;
  }
  const strip = payload => { const { files, ...rest } = payload; return rest; };
  const post = (path, payload) => send(new Request(origin + path, { method: 'POST',
    headers: { 'Content-Type': 'application/json', 'cf-connecting-ip': '192.0.2.1' }, body: typeof payload === 'string' ? payload : JSON.stringify(strip(payload)) }));
  const create = payload => post('/v1/reports', payload);
  const status = (id, receipt) => post(`/v1/reports/${id}/status`, { receipt_secret: receipt });
  const complete = (id, receipt) => post(`/v1/reports/${id}/complete`, { receipt_secret: receipt });
  const retire = (id, receipt) => post(`/v1/reports/${id}/retire`, { receipt_secret: receipt });
  function upload(payload, n, bytes = payload.files[n - 1], headers = {}) {
    const item = payload.manifest[n - 1];
    return send(new Request(`${origin}/v1/reports/${payload.report_id}/attachments/${n}`, { method: 'PUT', body: bytes,
      headers: { 'Content-Type': item.media_type, 'X-Receipt-Secret': payload.receipt_secret, 'X-Content-SHA256': item.sha256, 'cf-connecting-ip': '192.0.2.1', ...headers } }));
  }
  const read = (id, n, token) => send(new Request(`${origin}/v1/reports/${id}/attachments/${n}`, { headers: token ? { 'Cf-Access-Jwt-Assertion': token } : {} }));
  const answer = async response => ({ status: response.status, body: await response.json(), retryAfter: response.headers.get('retry-after') });
  return { env, db, r2, github, ports, ctx, waits, send, report, create, status, complete, retire, upload, read, answer, accessToken, staff, origin,
    store: new Store(db, now), key, cleanup: () => scheduled(env, ports), image, hexOf,
    advance: ms => { time += ms; }, now, repository, bot, posts: () => github.calls.filter(call => call === `POST /repos/${repository}/issues`).length };
}

module.exports = { fixture, reportId, secret, TEAM, AUD };
