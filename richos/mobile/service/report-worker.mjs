// RichDoctor report relay (PRD v6 §7): one public GitHub issue per approved report in one fixed
// repository, for users without a GitHub account, with the user's approved screenshots kept
// privately in R2 and linked for staff only. A separate Worker with its own D1 and bucket; it
// shares no runtime, binding or credential with Connect. Contract: RICHDOCTOR-RELAY.md. It logs nothing.
import { checkCreate, contentProblem, hasFooterLine, sha256, REPORT_ID, SECRET } from './report/checks.mjs';
import { checkImage, MAX_IMAGE_BYTES } from './report/images.mjs';
import { Store } from './report/store.mjs';
import { GitHub } from './report/github.mjs';
import { staffAllowed } from './report/access.mjs';

const MAX_REQUEST_BYTES = 131072;
const OLD_UNKNOWN_MS = 15 * 60_000;
const CHECK_AFTER_SECONDS = 5;
const DAY_MS = 86_400_000;
const HEADERS = { 'X-Content-Type-Options': 'nosniff', 'Referrer-Policy': 'no-referrer', 'Content-Security-Policy': "default-src 'none'; frame-ancestors 'none'" };

function reply(status, body, retryAfter) {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json; charset=utf-8', 'Cache-Control': 'no-store',
    ...HEADERS, ...(retryAfter ? { 'Retry-After': String(retryAfter) } : {}) } });
}
const text = (status, words) => new Response(words, { status, headers: { 'Content-Type': 'text/plain; charset=utf-8', 'Cache-Control': 'private, no-store', ...HEADERS } });
const limited = seconds => reply(429, { state: 'limited', retry_after_seconds: seconds }, seconds);
const unavailable = () => reply(503, { error: 'service_unavailable' });
const schemaReply = detail => reply(400, { state: 'rejected', reason: 'schema', detail });
const key = (id, attachment) => `${id}/${attachment.attachment_id}-${attachment.sha256}`;

export function configured(env) {
  return !!(env.DB && env.IMAGES && env.CREATE_LIMIT && env.STATUS_LIMIT && env.UPLOAD_LIMIT && /^\d{1,12}$/.test(env.GITHUB_APP_ID || '')
    && /^\d{1,14}$/.test(env.GITHUB_INSTALLATION_ID || '') && typeof env.GITHUB_APP_PRIVATE_KEY === 'string'
    && /^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9._-]{1,100}$/.test(env.GITHUB_REPOSITORY || '')
    && /^[a-z0-9-]{1,34}\[bot\]$/.test(env.GITHUB_BOT_LOGIN || '') && /^[a-z0-9-]{1,50}$/.test(env.MODERATION_LABEL || ''));
}
// A bounded number of seconds from the environment, or the production value. Only tests set them.
const seconds = (value, min, max, fallback) => /^\d{1,9}$/.test(value || '') && Number(value) >= min && Number(value) <= max ? Number(value) : fallback;

async function readBytes(request, cap) {
  if (Number(request.headers.get('content-length') || 0) > cap) throw Error('body_too_large');
  if (!request.body) return new Uint8Array(0);
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
  return buffer;
}
const readJson = async (request, cap) => JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(await readBytes(request, cap)));

const DELIVERY = 'The reporting service could not post this report, and nothing was posted. You can try again.';
const STAGE = { receiving: 'receiving', ready: 'ready', pending: 'creating', unconfirmed: 'unconfirmed', not_created: 'not_sent', retired: 'retired' };

class Relay {
  constructor(env, ctx, ports) {
    this.env = env; this.ctx = ctx; this.ports = ports;
    this.now = ports.now || Date.now;
    this.sleep = ports.sleep || (ms => new Promise(resolve => setTimeout(resolve, ms)));
    this.store = ports.store || new Store(env.DB, this.now);
    this.github = ports.github || new GitHub(env, { fetch: ports.fetch, now: this.now, footer: hasFooterLine });
    // Every timing follows the window: the lease holds the create request's own time (token 10 s,
    // create 20 s), the owner keeps a grace before others list, and listings are spaced.
    this.window = 1000 * seconds(env.RECONCILE_SECONDS, 2, 120, 120); this.lease = this.window / 2; this.grace = this.window / 5;
    this.retain = 1000 * seconds(env.RETAIN_SECONDS, 1, 400 * 86400, 365 * 86400);
    this.abandon = 1000 * seconds(env.ABANDON_SECONDS, 1, 30 * 86400, 7 * 86400);
  }
  async throttled(binding, k) { return !(await binding.limit({ key: await sha256(k) })).success; }
  ip(request) { return request.headers.get('cf-connecting-ip') || 'unknown'; }

  // A stored screenshot is one whose object is in the bucket with the approved size and hash; the
  // database's flag alone is never taken as proof (a write can fail after the object is stored).
  async present(id, attachment) {
    const head = await this.env.IMAGES.head(key(id, attachment));
    return !!head && head.size === attachment.bytes && head.customMetadata?.sha256 === attachment.sha256;
  }

  async view(record, { created = false } = {}) {
    const base = { report_id: record.report_id };
    const attachments = await this.store.attachments(record.report_id);
    const lists = { stored: attachments.filter(a => a.stored).map(a => a.attachment_id), missing: attachments.filter(a => !a.stored).map(a => a.attachment_id) };
    if (record.state === 'delivered') {
      const expected = attachments.length, issue = { issue_url: record.issue_url, issue_number: record.issue_number };
      const expire = record.expires_at ? { images_expire_at: new Date(record.expires_at).toISOString() } : {};
      if (expected && record.cleanup) return reply(200, { ...base, state: 'delivered', stage: 'delivered', ...issue, images_expected: expected, images_confirmed: 0, images_expired: true, ...expire });
      let confirmed = 0;
      for (const a of attachments) if (await this.present(record.report_id, a)) confirmed++;
      const state = confirmed === expected ? 'delivered' : 'created_checking_images';
      return reply(created ? 201 : 200, { ...base, state, stage: state, ...issue, images_expected: expected, images_confirmed: confirmed, ...expire });
    }
    if (record.state === 'pending') return reply(202, { ...base, state: 'pending', stage: 'creating', check_after_seconds: CHECK_AFTER_SECONDS, ...lists });
    if (record.state === 'not_created') {
      const wait = record.retry_at ? Math.max(1, Math.ceil((record.retry_at - this.now()) / 1000)) : null;
      return reply(created ? 503 : 200, { ...base, state: 'rejected', stage: 'not_sent', reason: 'delivery', detail: DELIVERY, ...lists, ...(wait ? { retry_after_seconds: wait } : {}) }, wait);
    }
    return reply(200, { ...base, state: record.state, stage: STAGE[record.state], ...lists });
  }

  // One attempt, owned by whoever wrote its pending record. The request that runs it is
  // registered with waitUntil (see handle), so a client that disconnects or quits does not stop it.
  async attempt(id, title, body, attemptId) {
    const { store, github } = this;
    let token;
    try { token = await github.authorize(); }
    catch (error) {
      await store.notCreated(id, attemptId, 'delivery', this.now() + 1000 * (error.retryAfter || 60));
      return store.get(id);
    }
    if (!await store.markSent(id, attemptId)) return store.get(id);
    const result = await github.create(token, title, body);
    if (result.kind === 'created') await store.deliver(id, result.issue, this.retain);
    else if (result.kind === 'refused') await store.notCreated(id, attemptId, 'delivery', this.now() + 1000 * result.retryAfter);
    else {
      // The response was lost. Never create again: look for the footer, a few times, briefly.
      const record = await store.get(id);
      for (const delay of [0, this.window / 60, this.window / 24]) {
        await this.sleep(delay);
        const found = await github.find(token, id, record.attempt_at - 300_000);
        if (found.kind === 'found') { await store.deliver(id, found.issue, this.retain); break; }
      }
    }
    return store.get(id);
  }

  // Moves a pending record forward without creating anything: the status rule, also used when a
  // request repeats an ID the relay already knows.
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
      if (found.kind === 'found') { await store.deliver(record.report_id, found.issue, this.retain); return store.get(record.report_id); }
    }
    if (record.state === 'pending' && now >= record.attempt_at + this.window) {
      await store.unconfirm(record.report_id, record.attempt_id);
      return store.get(record.report_id);
    }
    return record;
  }

  // Completion: under the receipt's lock, every approved screenshot is verified in the bucket,
  // then the creation attempt is claimed, then GitHub is called outside the lock. A missing
  // screenshot means no issue: the receipt is receiving again and names what to upload.
  async finish(record) {
    const { store } = this, id = record.report_id;
    if (!['receiving', 'ready', 'not_created'].includes(record.state)) return this.view(await this.advance(record));
    const control = await store.control();
    if (!control) return unavailable();
    if (control.paused) return reply(503, { report_id: id, state: 'closed', stage: 'not_sent', reason: 'operator_off' });
    const token = await store.acquire(id);
    if (!token) return limited(2);
    let claimed = null;
    try {
      const current = await store.get(id);
      if (current.cleanup === 'claimed') return limited(5);
      if (!['receiving', 'ready', 'not_created'].includes(current.state)) return this.view(current);
      const missing = [];
      for (const a of await store.attachments(id)) if (!await this.present(id, a)) missing.push(a.attachment_id);
      if (missing.length) { await store.markMissing(id, missing, token); return this.view(await store.get(id)); }
      const now = this.now(), day = new Date(now).toISOString().slice(0, 10);
      if (!await store.reserve(day, control.daily_cap)) {
        const wait = Math.ceil((Date.parse(day + 'T00:00:00Z') + DAY_MS - now) / 1000);
        return reply(503, { report_id: id, state: 'closed', stage: 'not_sent', reason: 'daily_cap', retry_after_seconds: wait }, wait);
      }
      const attemptId = crypto.randomUUID();
      if (!await store.claim(id, current.attempt, attemptId, this.lease, token)) { await store.refund(day); return this.view(await store.get(id)); }
      claimed = { attemptId, title: current.title, body: current.body };
    } finally { await store.release(id, token); }
    return this.view(await this.attempt(id, claimed.title, claimed.body, claimed.attemptId), { created: true });
  }

  // Register (protocol 2) or create (protocol 1, which completes at once). Never creates for
  // protocol 2: that is the separate completion step, after the screenshots.
  async create(request, origin) {
    const { env, store } = this, now = this.now();
    if (await this.throttled(env.CREATE_LIMIT, 'ip:' + this.ip(request))) return limited(60);
    let data;
    try { data = await readJson(request, MAX_REQUEST_BYTES); }
    catch (error) {
      if (error.message === 'body_too_large') return reply(413, { state: 'rejected', reason: 'size', detail: 'The report is longer than GitHub allows.' });
      return schemaReply('The report is not in the expected format.');
    }
    const checked = await checkCreate(data, now);
    if (checked.error) { const { status, ...body } = checked.error; return reply(status, body); }
    const r = checked.value;
    if (await this.throttled(env.CREATE_LIMIT, 'installation:' + r.installationId)) return limited(60);
    r.secretHash = await sha256(r.secret);
    const known = async existing => {
      if (existing.secret_hash !== r.secretHash) return reply(403, { state: 'forbidden' });
      if (existing.state === 'retired') return this.view(existing);
      if (existing.content_hash !== r.contentHash) return reply(409, { state: 'conflict' });
      if (r.schema === 1 && ['ready', 'not_created'].includes(existing.state)) return this.finish(existing);
      return this.view(await this.advance(existing));
    };
    const existing = await store.get(r.reportId);
    if (existing) return known(existing);
    const problem = contentProblem({ ...r, origin, count: r.manifest.length });
    if (problem) return reply(422, { state: 'rejected', reason: 'content', detail: problem.detail, item: problem.item });
    const control = await store.control();
    if (!control) return unavailable();
    if (control.paused) return reply(503, { state: 'closed', stage: 'not_sent', reason: 'operator_off' });
    if (r.approvedAt < now - OLD_UNKNOWN_MS) {
      // An older unknown ID: its footer is looked for before any create (PRD §7).
      let found;
      try { found = await this.github.find(await this.github.authorize(), r.reportId, Math.min(r.approvedAt, now) - DAY_MS); }
      catch { found = { kind: 'incomplete' }; }
      if (found.kind === 'incomplete') return reply(503, { state: 'limited', retry_after_seconds: 60 }, 60);
      if (found.kind === 'found') {
        // Its approved screenshots come back with it; view() counts only those in the bucket.
        const stored = [];
        for (const item of r.manifest) if (await this.present(r.reportId, item)) stored.push(item.attachment_id);
        await store.adopt(r, found.issue, this.retain, stored);
        return this.view(await store.get(r.reportId));
      }
    }
    if (!await store.register(r)) return known(await store.get(r.reportId));
    const record = await store.get(r.reportId);
    return r.schema === 1 ? this.finish(record) : this.view(record);
  }

  async receipt(request, id, limiter) {
    if (await this.throttled(limiter, 'ip:' + this.ip(request))) return { response: limited(60) };
    let data;
    try { data = await readJson(request, 1024); } catch { data = null; }
    if (!data || typeof data !== 'object' || Object.keys(data).length !== 1 || typeof data.receipt_secret !== 'string' || !SECRET.test(data.receipt_secret))
      return { response: schemaReply('The request is not in the expected format.') };
    return { hash: await sha256(data.receipt_secret), record: await this.store.get(id) };
  }

  async status(request, id) {
    const { response, hash, record } = await this.receipt(request, id, this.env.STATUS_LIMIT);
    if (response) return response;
    if (!record) return reply(404, { report_id: id, state: 'unknown_id' });
    if (record.secret_hash !== hash) return reply(403, { state: 'forbidden' });
    return this.view(await this.advance(record));
  }

  async complete(request, id) {
    const { response, hash, record } = await this.receipt(request, id, this.env.CREATE_LIMIT);
    if (response) return response;
    if (!record) return reply(404, { report_id: id, state: 'unknown_id' });
    if (record.secret_hash !== hash) return reply(403, { state: 'forbidden' });
    return this.finish(record);
  }

  // Retirement before a changed revision is sent (PRD §7). Acknowledged only once recorded.
  async retire(request, id) {
    const { store } = this, { response, hash, record: found } = await this.receipt(request, id, this.env.STATUS_LIMIT);
    if (response) return response;
    let record = found;
    if (!record) { await store.retireUnseen(id, hash); record = await store.get(id); }
    if (record.secret_hash !== hash) return reply(403, { state: 'forbidden' });
    if (!['receiving', 'ready', 'not_created'].includes(record.state)) return this.view(await this.advance(record));
    const token = await store.acquire(id);
    if (!token) return limited(2);
    try { await store.retire(id, token); } finally { await store.release(id, token); }
    return this.view(await store.get(id));
  }

  async upload(request, id, attachmentId) {
    const { env, store } = this;
    if (await this.throttled(env.UPLOAD_LIMIT, 'ip:' + this.ip(request))) return limited(60);
    const secret = request.headers.get('x-receipt-secret') || '';
    if (!SECRET.test(secret)) return schemaReply('The upload has no valid receipt.');
    const record = await store.get(id);
    if (!record) return reply(404, { report_id: id, state: 'unknown_id' });
    if (record.secret_hash !== await sha256(secret)) return reply(403, { state: 'forbidden' });
    if (!['receiving', 'ready', 'not_created'].includes(record.state)) return this.view(await this.advance(record));
    const attachment = (await store.attachments(id)).find(a => a.attachment_id === attachmentId);
    const refuse = detail => reply(422, { report_id: id, state: 'rejected', reason: 'attachment', item: `screenshot ${attachmentId}`, detail });
    if (!attachment) return refuse('This report has no such screenshot.');
    if ((request.headers.get('content-type') || '').split(';')[0].trim().toLowerCase() !== attachment.media_type) return refuse('The screenshot is not the approved file type.');
    if ((request.headers.get('x-content-sha256') || '').toLowerCase() !== attachment.sha256) return refuse('The screenshot is not the approved file.');
    let bytes;
    try { bytes = await readBytes(request, Math.min(MAX_IMAGE_BYTES, attachment.bytes)); }
    catch { return refuse('The screenshot is larger than the approved file.'); }
    if (bytes.byteLength !== attachment.bytes || await sha256(bytes) !== attachment.sha256) return refuse('The screenshot is not the approved file.');
    const image = checkImage(bytes, attachment.media_type);
    if (image.error) return refuse(image.error);
    if (image.width !== attachment.width || image.height !== attachment.height) return refuse('The screenshot is not the approved size.');
    const token = await store.acquire(id);
    if (!token) return limited(2);
    try {
      const current = await store.get(id);
      if (current.cleanup === 'claimed') return limited(5);
      if (!['receiving', 'ready', 'not_created'].includes(current.state)) return this.view(current);
      // Written once per hash: an identical retry finds the object and only records it.
      if (!await this.present(id, attachment)) {
        const control = await store.control(), day = new Date(this.now()).toISOString().slice(0, 10);
        if (!control || !await store.reserveBytes(day, attachment.bytes, control.daily_bytes_cap)) return reply(503, { report_id: id, state: 'closed', stage: 'not_sent', reason: 'daily_cap' });
        await env.IMAGES.put(key(id, attachment), bytes, { sha256: attachment.sha256, httpMetadata: { contentType: attachment.media_type }, customMetadata: { sha256: attachment.sha256 } });
      }
      await store.markStored(id, attachmentId, token);
    } finally { await store.release(id, token); }
    return this.view(await store.get(id));
  }

  // The staff link: the image itself, after a verified Access token. No page, no listing.
  async image(request, id, attachmentId) {
    if (!await staffAllowed(request, this.env, { fetch: this.ports.fetch, now: this.now })) return text(403, 'RichOS support staff sign-in is required to see this screenshot.');
    const record = await this.store.get(id), attachment = record && (await this.store.attachments(id)).find(a => a.attachment_id === attachmentId);
    if (!attachment) return text(404, 'There is no such screenshot.');
    // Retirement withdraws a receipt's screenshots at once, before its cleanup deletes them.
    if (record.state === 'retired') return text(410, 'This screenshot was withdrawn: its report was replaced by a changed version before it was sent.');
    if (attachment.deleted_at) {
      const when = new Date(attachment.deleted_at).toISOString().slice(0, 10);
      return text(410, record.state === 'delivered' ? `This screenshot was deleted on ${when}, when its one-year retention ended.` : `This screenshot was deleted on ${when}, because its report was never sent.`);
    }
    const object = await this.env.IMAGES.get(key(id, attachment));
    if (!object) return text(404, 'This screenshot is not stored.');
    return new Response(object.body, { status: 200, headers: { 'Content-Type': attachment.media_type, 'Cache-Control': 'private, no-store', ...HEADERS,
      'Content-Disposition': `inline; filename="screenshot-${attachment.attachment_id}.${attachment.media_type === 'image/png' ? 'png' : 'jpg'}"` } });
  }

  // Scheduled cleanup (PRD v6 §7), serialized per receipt with upload, completion and retirement:
  // re-checked under the lock, marked unavailable before any object is deleted, and resumed after
  // a crash before anything may be uploaded again.
  async cleanup() {
    const { store } = this;
    for (const { report_id: id } of await store.cleanupCandidates(this.abandon)) {
      const token = await store.acquire(id, 60_000);
      if (!token) continue;
      try {
        if (!await store.claimCleanup(id, token, this.abandon)) continue;
        const keys = (await store.attachments(id)).filter(a => !a.deleted_at).map(a => key(id, a));
        if (keys.length) await this.env.IMAGES.delete(keys);
        await store.finishCleanup(id, token);
      } catch { /* the claim stays; the next run resumes it */ } finally { await store.release(id, token); }
    }
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
      return reply(control ? 200 : 503, { ok: !!control, service: 'richos-report', protocol: 2, accepting: !!control && !control.paused && control.daily_cap > 0 });
    }
    const route = /^\/v1\/reports(?:\/([^/]+)\/(status|complete|retire|attachments\/(\d{1,2})))?$/.exec(url.pathname);
    const [, id, action, attachmentId] = route || [];
    const allowed = route && (id === undefined ? request.method === 'POST' : REPORT_ID.test(id)
      && (attachmentId ? ['PUT', 'GET'].includes(request.method) : request.method === 'POST'));
    if (!allowed) return reply(404, { error: 'not_found' });
    if (!configured(env)) return unavailable();
    const relay = new Relay(env, ctx, ports);
    if (attachmentId) return await (request.method === 'GET' ? relay.image(request, id, attachmentId) : relay.upload(request, id, attachmentId));
    if (action === 'status') return await relay.status(request, id);
    // Requests that may create or retire are handed to waitUntil before their first await: a
    // client that disconnects after sending cannot stop one between its pending record and its
    // GitHub call (seen live on 2026-10-22, when that gap left an accepted report "not posted").
    const work = action === 'retire' ? relay.retire(request, id) : action === 'complete' ? relay.complete(request, id) : relay.create(request, url.origin);
    ctx?.waitUntil?.(work.catch(() => {}));
    return await work;
  } catch { return unavailable(); }
}

export async function scheduled(env, ports = {}) {
  if (configured(env)) await new Relay(env, null, ports).cleanup();
}
// The operator's lists are read from D1 directly (cli/report.mjs). Cleanup runs on the schedule.
export default {
  fetch: (request, env, ctx) => handle(request, env, ctx),
  scheduled: (_event, env, ctx) => ctx.waitUntil(scheduled(env)),
};
