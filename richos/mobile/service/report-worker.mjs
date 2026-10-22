// RichDoctor report relay (PRD §7): creates one public GitHub issue per approved report in one fixed
// repository, for users without a GitHub account. A separate Worker with its own D1; it shares no
// runtime, binding or credential with Connect. Contract: REPORT.md. It logs nothing.
import { checkCreate, contentProblem, hasFooterLine, sha256, REPORT_ID } from './report/checks.mjs';
import { Store } from './report/store.mjs';
import { GitHub } from './report/github.mjs';

const MAX_REQUEST_BYTES = 131072;
const OLD_UNKNOWN_MS = 15 * 60_000;
const CHECK_AFTER_SECONDS = 5;

function reply(status, body, retryAfter) {
  return new Response(JSON.stringify(body), { status, headers: {
    'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store',
    'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'no-referrer',
    'Content-Security-Policy': "default-src 'none'; frame-ancestors 'none'",
    ...(retryAfter ? { 'Retry-After': String(retryAfter) } : {}),
  } });
}
const limited = seconds => reply(429, { state: 'limited', retry_after_seconds: seconds }, seconds);
const unavailable = () => reply(503, { error: 'service_unavailable' });

export function configured(env) {
  return !!(env.DB && env.CREATE_LIMIT && env.STATUS_LIMIT && /^\d{1,12}$/.test(env.GITHUB_APP_ID || '')
    && /^\d{1,14}$/.test(env.GITHUB_INSTALLATION_ID || '') && typeof env.GITHUB_APP_PRIVATE_KEY === 'string'
    && /^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9._-]{1,100}$/.test(env.GITHUB_REPOSITORY || '')
    && /^[a-z0-9-]{1,34}\[bot\]$/.test(env.GITHUB_BOT_LOGIN || '') && /^[a-z0-9-]{1,50}$/.test(env.MODERATION_LABEL || ''));
}
// The bounded reconciliation window per attempt (PRD §7: two minutes). Only a test sets it lower.
const windowMs = env => 1000 * (/^\d{1,3}$/.test(env.RECONCILE_SECONDS || '') && Number(env.RECONCILE_SECONDS) >= 2
  && Number(env.RECONCILE_SECONDS) <= 120 ? Number(env.RECONCILE_SECONDS) : 120);

async function readBody(request, cap) {
  if (Number(request.headers.get('content-length') || 0) > cap) throw Error('body_too_large');
  if (!request.body) return '';
  const reader = request.body.getReader(), parts = []; let size = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > cap) { await reader.cancel(); throw Error('body_too_large'); }
      parts.push(value);
    }
  } finally { reader.releaseLock(); }
  const buffer = new Uint8Array(size); let offset = 0;
  for (const part of parts) { buffer.set(part, offset); offset += part.byteLength; }
  return new TextDecoder('utf-8', { fatal: true }).decode(buffer);
}

const DELIVERY = 'The reporting service could not post this report, and nothing was posted. You can try again.';
function view(record, now, created = false) {
  const base = { report_id: record.report_id };
  if (record.state === 'delivered') return reply(created ? 201 : 200, { ...base, state: 'delivered', issue_url: record.issue_url, issue_number: record.issue_number });
  if (record.state === 'pending') return reply(202, { ...base, state: 'pending', check_after_seconds: CHECK_AFTER_SECONDS });
  if (record.state === 'unconfirmed') return reply(200, { ...base, state: 'unconfirmed' });
  const seconds = record.retry_at ? Math.max(1, Math.ceil((record.retry_at - now) / 1000)) : null;
  return reply(created ? 503 : 200, { ...base, state: 'rejected', reason: 'delivery', detail: DELIVERY,
    ...(seconds ? { retry_after_seconds: seconds } : {}) }, seconds);
}

class Relay {
  constructor(env, ctx, ports) {
    this.env = env; this.ctx = ctx;
    this.now = ports.now || Date.now;
    this.sleep = ports.sleep || (ms => new Promise(resolve => setTimeout(resolve, ms)));
    this.store = ports.store || new Store(env.DB, this.now);
    this.github = ports.github || new GitHub(env, { fetch: ports.fetch, now: this.now, footer: hasFooterLine });
    // Every timing follows the window: the lease holds the create request's own time (token 10 s,
    // create 20 s), the owner keeps a grace before others list, and listings are spaced.
    this.window = windowMs(env); this.lease = this.window / 2; this.grace = this.window / 5;
  }
  async throttled(binding, key) { return !(await binding.limit({ key: await sha256(key) })).success; }

  // One attempt, owned by whoever wrote its pending record. The create that runs it is registered
  // with waitUntil (see handle), so a client that disconnects or quits does not stop it.
  async attempt(r, attemptId) {
    const { store, github } = this;
    let token;
    try { token = await github.authorize(); }
    catch (error) {
      await store.notCreated(r.reportId, attemptId, 'delivery', this.now() + 1000 * (error.retryAfter || 60));
      return store.get(r.reportId);
    }
    if (!await store.markSent(r.reportId, attemptId)) return store.get(r.reportId);
    const result = await github.create(token, r.title, r.body);
    if (result.kind === 'created') await store.deliver(r.reportId, result.issue);
    else if (result.kind === 'refused') await store.notCreated(r.reportId, attemptId, 'delivery', this.now() + 1000 * result.retryAfter);
    else {
      // The response was lost. Never create again: look for the footer, a few times, briefly.
      const record = await store.get(r.reportId);
      for (const delay of [0, this.window / 60, this.window / 24]) {
        await this.sleep(delay);
        const found = await github.find(token, r.reportId, record.attempt_at - 300_000);
        if (found.kind === 'found') { await store.deliver(r.reportId, found.issue); break; }
      }
    }
    return store.get(r.reportId);
  }

  // Moves a pending record forward without creating anything: the status rule, also used when a
  // create repeats an ID the relay already knows.
  async advance(record) {
    const { store } = this, now = this.now();
    if (record.state === 'pending' && !record.sent) {
      if (now < record.lease_until) return record;
      // The lease ran out before the create request was recorded as sent: nothing was created.
      await store.notCreated(record.report_id, record.attempt_id, 'delivery', null, true);
      return store.get(record.report_id);
    }
    if (record.state !== 'pending' && record.state !== 'unconfirmed') return record;
    // Leave the owner its create request's own time, then list at most once per interval.
    if (record.state === 'pending' && now < record.attempt_at + this.grace) return record;
    if (await store.claimCheck(record.report_id, record.state === 'pending' ? this.window / 12 : this.window / 2)) {
      let found = { kind: 'incomplete' };
      try { found = await this.github.find(await this.github.authorize(), record.report_id, record.attempt_at - 300_000); } catch { /* incomplete */ }
      if (found.kind === 'found') { await store.deliver(record.report_id, found.issue); return store.get(record.report_id); }
    }
    if (record.state === 'pending' && now >= record.attempt_at + this.window) {
      await store.unconfirm(record.report_id, record.attempt_id);
      return store.get(record.report_id);
    }
    return record;
  }

  async create(request) {
    const { env, store } = this, now = this.now();
    const ip = request.headers.get('cf-connecting-ip') || 'unknown';
    if (await this.throttled(env.CREATE_LIMIT, 'ip:' + ip)) return limited(60);
    let data;
    try { data = JSON.parse(await readBody(request, MAX_REQUEST_BYTES)); }
    catch (error) {
      if (error.message === 'body_too_large') return reply(413, { state: 'rejected', reason: 'size', detail: 'The report is longer than GitHub allows.' });
      return reply(400, { state: 'rejected', reason: 'schema', detail: 'The report is not in the expected format.' });
    }
    const checked = await checkCreate(data, now);
    if (checked.error) { const { status, ...body } = checked.error; return reply(status, body); }
    const r = checked.value;
    if (await this.throttled(env.CREATE_LIMIT, 'installation:' + r.installationId)) return limited(60);
    r.secretHash = await sha256(r.secret);
    let existing = await store.get(r.reportId);
    if (existing) {
      if (existing.secret_hash !== r.secretHash) return reply(403, { state: 'forbidden' });
      if (existing.content_hash !== r.contentHash) return reply(409, { state: 'conflict' });
      if (existing.state !== 'not_created') return view(await this.advance(existing), this.now());
    }
    const problem = contentProblem(r);
    if (problem) return reply(422, { state: 'rejected', reason: 'content', detail: problem.detail, item: problem.item });
    const control = await store.control();
    if (!control) return unavailable();
    if (control.paused) return reply(503, { state: 'closed', reason: 'operator_off' });
    if (!existing && r.approvedAt < now - OLD_UNKNOWN_MS) {
      // An older unknown ID: its footer is looked for before any create (PRD §7).
      let found;
      try { found = await this.github.find(await this.github.authorize(), r.reportId, Math.min(r.approvedAt, now) - 86_400_000); }
      catch { found = { kind: 'incomplete' }; }
      if (found.kind === 'incomplete') return reply(503, { state: 'limited', retry_after_seconds: 60 }, 60);
      if (found.kind === 'found') { await store.adopt(r, found.issue); return view(await store.get(r.reportId), this.now()); }
    }
    const day = new Date(now).toISOString().slice(0, 10);
    if (!await store.reserve(day, control.daily_cap)) {
      const midnight = Date.parse(day + 'T00:00:00Z') + 86_400_000, seconds = Math.ceil((midnight - now) / 1000);
      return reply(503, { state: 'closed', reason: 'daily_cap', retry_after_seconds: seconds }, seconds);
    }
    const attemptId = crypto.randomUUID();
    const owned = existing ? await store.retry(r.reportId, existing.attempt, attemptId, this.lease) : await store.begin(r, attemptId, this.lease);
    if (!owned) {
      // Another request for this ID got there first; it owns the attempt.
      await store.refund(day);
      existing = await store.get(r.reportId);
      if (existing.secret_hash !== r.secretHash) return reply(403, { state: 'forbidden' });
      if (existing.content_hash !== r.contentHash) return reply(409, { state: 'conflict' });
      return view(existing, this.now());
    }
    return view(await this.attempt(r, attemptId), this.now(), true);
  }

  async status(request, id) {
    const { env, store } = this;
    if (await this.throttled(env.STATUS_LIMIT, 'ip:' + (request.headers.get('cf-connecting-ip') || 'unknown'))) return limited(60);
    let data;
    try { data = JSON.parse(await readBody(request, 1024)); } catch { data = null; }
    if (!data || typeof data !== 'object' || Object.keys(data).length !== 1 || typeof data.receipt_secret !== 'string')
      return reply(400, { state: 'rejected', reason: 'schema', detail: 'The status request is not in the expected format.' });
    const record = await store.get(id);
    if (!record) return reply(404, { report_id: id, state: 'unknown_id' });
    if (record.secret_hash !== await sha256(data.receipt_secret)) return reply(403, { state: 'forbidden' });
    return view(await this.advance(record), this.now());
  }
}

export async function handle(request, env, ctx, ports = {}) {
  const url = new URL(request.url);
  // Plain HTTP only for the loopback stand-in (dev/report-relay-local.mjs); never on Cloudflare.
  const loopback = url.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname);
  if (url.protocol !== 'https:' && !loopback) return reply(400, { error: 'https_required' });
  if (url.search) return reply(404, { error: 'not_found' });
  try {
    if (request.method === 'GET' && url.pathname === '/v1/health') {
      const control = configured(env) ? await new Store(env.DB).control().catch(() => null) : null;
      return reply(control ? 200 : 503, { ok: !!control, service: 'richos-report', protocol: 1, accepting: !!control && !control.paused && control.daily_cap > 0 });
    }
    const status = /^\/v1\/reports\/([^/]+)\/status$/.exec(url.pathname);
    if (request.method !== 'POST' || !(url.pathname === '/v1/reports' || (status && REPORT_ID.test(status[1])))) return reply(404, { error: 'not_found' });
    if (!configured(env)) return unavailable();
    const relay = new Relay(env, ctx, ports);
    if (status) return await relay.status(request, status[1]);
    // The whole create is handed to waitUntil before its first await: a client that disconnects
    // or quits after sending cannot stop it between its pending record and its GitHub call
    // (seen live on 2026-10-22, when that gap left an accepted report "not posted").
    const work = relay.create(request);
    ctx?.waitUntil?.(work.catch(() => {}));
    return await work;
  } catch { return unavailable(); }
}

// The operator's pending list is read from D1 directly (cli/report.mjs); nothing runs on a schedule.
export default { fetch: (request, env, ctx) => handle(request, env, ctx) };
