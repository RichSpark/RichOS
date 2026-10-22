// The relay's only GitHub access: a GitHub App installation token limited to Issues write on the
// one fixed repository, minted per attempt from the server-held App private key. Nothing here is
// ever logged or returned to a client.
export const API = 'https://api.github.com';
const VERSION = '2022-11-28';
const utf8 = new TextEncoder();
const b64url = bytes => btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const fromB64 = text => Uint8Array.from(atob(text), c => c.charCodeAt(0));

// GitHub hands out PKCS#1 ("RSA PRIVATE KEY"); WebCrypto imports PKCS#8. Wrapping is fixed DER.
function derLength(n) { return n < 128 ? [n] : n < 256 ? [0x81, n] : n < 65536 ? [0x82, n >> 8, n & 255] : [0x83, n >> 16, (n >> 8) & 255, n & 255]; }
function pkcs8(pem) {
  const match = /-----BEGIN (RSA )?PRIVATE KEY-----([\s\S]+?)-----END (RSA )?PRIVATE KEY-----/.exec(pem || '');
  if (!match) throw Error('app_key_invalid');
  const der = fromB64(match[2].replace(/\s+/g, ''));
  if (!match[1]) return der;
  const algorithm = [0x30, 0x0d, 0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01, 0x05, 0x00];
  const inner = [0x02, 0x01, 0x00, ...algorithm, 0x04, ...derLength(der.length)];
  return Uint8Array.from([0x30, ...derLength(inner.length + der.length), ...inner, ...der]);
}

export function issueUrl(repository, number) { return `https://github.com/${repository}/issues/${number}`; }
function issueOf(repository, data) {
  if (!data || !Number.isSafeInteger(data.number) || data.number < 1) return null;
  const url = issueUrl(repository, data.number);
  return data.html_url === url ? { number: data.number, url } : null;
}
function retryAfter(response, now) {
  const seconds = Number(response.headers.get('retry-after'));
  if (Number.isFinite(seconds) && seconds > 0) return Math.min(Math.ceil(seconds), 86400);
  const reset = Number(response.headers.get('x-ratelimit-reset'));
  if (Number.isFinite(reset) && reset * 1000 > now) return Math.min(Math.ceil((reset * 1000 - now) / 1000), 86400);
  return 60;
}

export class GitHub {
  constructor(env, { fetch: fetchImpl = fetch, now = Date.now, footer } = {}) {
    // Called unbound: workerd's fetch throws "Illegal invocation" when called as a method of this.
    this.env = env; this.fetch = (...args) => fetchImpl(...args); this.now = now; this.footer = footer;
    this.repository = env.GITHUB_REPOSITORY;
  }
  headers(token) {
    return { Authorization: `Bearer ${token}`, Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': VERSION,
      'User-Agent': 'richos-report-relay', 'Content-Type': 'application/json' };
  }
  async jwt() {
    const key = await crypto.subtle.importKey('pkcs8', pkcs8(this.env.GITHUB_APP_PRIVATE_KEY), { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['sign']);
    const iat = Math.floor(this.now() / 1000) - 60;
    const input = b64url(utf8.encode(JSON.stringify({ alg: 'RS256', typ: 'JWT' }))) + '.'
      + b64url(utf8.encode(JSON.stringify({ iat, exp: iat + 540, iss: String(this.env.GITHUB_APP_ID) })));
    return input + '.' + b64url(await crypto.subtle.sign('RSASSA-PKCS1-v1_5', key, utf8.encode(input)));
  }
  // An installation token for this repository and Issues write only. Throws before anything is sent.
  // `scoped: false` is only for the operator's check of what the installation itself can reach.
  async authorize({ scoped = true } = {}) {
    const response = await this.fetch(`${API}/app/installations/${this.env.GITHUB_INSTALLATION_ID}/access_tokens`, {
      method: 'POST', redirect: 'manual', signal: AbortSignal.timeout(10000), headers: this.headers(await this.jwt()),
      body: JSON.stringify(scoped ? { repositories: [this.repository.split('/')[1]], permissions: { issues: 'write' } } : {}),
    });
    const data = response.status === 201 ? await response.json().catch(() => null) : null;
    if (typeof data?.token !== 'string' || !data.token) throw Object.assign(Error('token_unavailable'), { retryAfter: retryAfter(response, this.now()) });
    return data.token;
  }
  // { kind: 'created', issue } | { kind: 'refused', retryAfter } | { kind: 'unknown' }.
  // Only a 4xx is a definitive refusal; a lost, malformed, redirected or 5xx answer is unknown.
  async create(token, title, body) {
    let response;
    try {
      response = await this.fetch(`${API}/repos/${this.repository}/issues`, {
        method: 'POST', redirect: 'manual', signal: AbortSignal.timeout(20000), headers: this.headers(token),
        body: JSON.stringify({ title, body, labels: [this.env.MODERATION_LABEL] }),
      });
    } catch { return { kind: 'unknown' }; }
    if (response.status >= 400 && response.status < 500) { await response.body?.cancel(); return { kind: 'refused', retryAfter: retryAfter(response, this.now()) }; }
    if (response.status !== 201) { await response.body?.cancel(); return { kind: 'unknown' }; }
    const issue = issueOf(this.repository, await response.json().catch(() => null));
    return issue ? { kind: 'created', issue } : { kind: 'unknown' };
  }
  // Reconciliation (PRD §7): page through the fixed repository's issues created by the reporting
  // identity, newest first, back to `since` (the attempt-time look-back), and match the exact
  // footer. Never search indexing, and never GitHub's own `since` filter: on 2026-10-22 it
  // answered an empty list for issues minutes old that the same listing without it returned.
  // A failed or incomplete listing is { kind: 'incomplete' }, never proof nothing was created.
  async find(token, reportId, since, maxPages = 10) {
    const query = new URLSearchParams({ creator: this.env.GITHUB_BOT_LOGIN, state: 'all', per_page: '100', sort: 'created', direction: 'desc' });
    let url = `${API}/repos/${this.repository}/issues?${query}`, found = null;
    for (let page = 0; url; page++) {
      if (page === maxPages) return { kind: 'incomplete' };
      let response, items;
      try {
        response = await this.fetch(url, { redirect: 'manual', signal: AbortSignal.timeout(10000), headers: this.headers(token) });
        items = response.status === 200 ? await response.json() : null;
      } catch { return { kind: 'incomplete' }; }
      if (!Array.isArray(items)) { await response.body?.cancel().catch(() => {}); return { kind: 'incomplete' }; }
      let covered = false;
      for (const item of items) {
        const created = Date.parse(item?.created_at);
        if (!Number.isFinite(created)) return { kind: 'incomplete' };
        if (created < since) { covered = true; break; }
        if (item.pull_request || item.user?.login !== this.env.GITHUB_BOT_LOGIN || !this.footer(item.body, reportId)) continue;
        const issue = issueOf(this.repository, item);
        if (issue && (!found || issue.number < found.number)) found = issue;
      }
      if (covered) break;
      const next = /<([^>]+)>;\s*rel="next"/.exec(response.headers.get('link') || '')?.[1] || null;
      if (next && !next.startsWith(API + '/')) return { kind: 'incomplete' };
      url = next;
    }
    return found ? { kind: 'found', issue: found } : { kind: 'absent' };
  }
}
