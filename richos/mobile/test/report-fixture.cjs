// The report relay on a real SQLite database with its own schema, a fake GitHub and a virtual
// clock (sleep advances it). Proves the relay's logic; real delivery is the live A9 run.
const { DatabaseSync } = require('node:sqlite');
const { readFileSync } = require('node:fs');
const { join } = require('node:path');

function database(t) {
  const db = new DatabaseSync(':memory:'); t.after(() => db.close());
  db.exec(readFileSync(join(__dirname, '../service/report/schema.sql'), 'utf8'));
  return {
    raw: db,
    prepare(sql) { return { bind(...args) { const query = db.prepare(sql); return {
      first: async () => query.get(...args) || null,
      all: async () => ({ results: query.all(...args) }),
      run: async () => ({ meta: { changes: Number(query.run(...args).changes) } }),
    }; } }; },
  };
}

const ALPHABET = '23456789ABCDEFGHJKMNPQRSTVWXYZ';
const pick = n => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => ALPHABET[b % 30]).join('');
const reportId = () => `RD-${pick(4)}-${pick(4)}`;
const secret = () => Buffer.from(crypto.getRandomValues(new Uint8Array(32))).toString('base64url');

async function fixture(t, options = {}) {
  const { handle } = await import('../service/report-worker.mjs');
  const { Store } = await import('../service/report/store.mjs');
  const { contentHash } = await import('../service/report/checks.mjs');
  const { FakeGitHub, appKey } = await import('../dev/report-fake-github.mjs');
  let time = Date.parse('2026-10-22T12:00:00Z');
  const now = () => time, db = database(t), key = await appKey();
  const repository = 'richos-test/report-relay-test', bot = 'richos-reports-test[bot]';
  const github = new FakeGitHub({ repository, bot, appId: '123', installationId: '456', publicKey: key.publicKey, now, pageSize: options.pageSize || 100 });
  const allow = { limit: async () => ({ success: true }) };
  const env = { DB: db, CREATE_LIMIT: allow, STATUS_LIMIT: allow, GITHUB_APP_ID: '123', GITHUB_INSTALLATION_ID: '456',
    GITHUB_APP_PRIVATE_KEY: options.pkcs1 ? key.pkcs1 : key.pkcs8, GITHUB_REPOSITORY: repository, GITHUB_BOT_LOGIN: bot, MODERATION_LABEL: 'richdoctor' };
  db.raw.exec(`UPDATE control SET paused=0, daily_cap=${options.cap ?? 100}, operator='test'`);
  const waits = [], ctx = { waitUntil: promise => waits.push(promise) };
  const ports = { now, sleep: async ms => { time += ms; }, fetch: github.fetch };
  const send = request => handle(request, env, ctx, ports);
  async function report(overrides = {}) {
    const id = overrides.report_id || reportId();
    const title = overrides.title ?? 'RichOS does not open';
    const body = overrides.body ?? `### What happened\nRichOS closed when it started\\.\n\nSent with RichDoctor 1.0 · Report ID ${id}`;
    const payload = { schema: 'richdoctor-report/1', report_id: id, receipt_secret: secret(), content_sha256: await contentHash(title, body),
      installation_id: crypto.randomUUID(), app_version: '1.0.0', approved_at: time, title, body, ...overrides };
    if (overrides.content_sha256 === undefined && (overrides.title !== undefined || overrides.body !== undefined)) payload.content_sha256 = await contentHash(payload.title, payload.body);
    return payload;
  }
  const post = (path, payload) => send(new Request('https://report.example.com' + path, { method: 'POST',
    headers: { 'Content-Type': 'application/json', 'cf-connecting-ip': '192.0.2.1' }, body: typeof payload === 'string' ? payload : JSON.stringify(payload) }));
  const create = payload => post('/v1/reports', payload);
  const status = (id, receipt) => post(`/v1/reports/${id}/status`, { receipt_secret: receipt });
  const answer = async response => ({ status: response.status, body: await response.json(), retryAfter: response.headers.get('retry-after') });
  return { env, db, github, ports, ctx, waits, send, report, create, status, answer, store: new Store(db, now), key,
    advance: ms => { time += ms; }, now, repository, bot, posts: () => github.calls.filter(call => call === `POST /repos/${repository}/issues`).length };
}

module.exports = { fixture, reportId, secret };
