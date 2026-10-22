// A stand-in for the parts of GitHub's REST API the report relay uses: App installation tokens,
// issue creation and the creator-filtered, paginated issue listing. It checks the App JWT's RS256
// signature and the token on every call, and injects one-shot faults for failure tests. It is a
// test double: it proves the relay's logic, never real delivery.
const json = (status, body, headers = {}) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json', ...headers } });
const fromB64url = text => Uint8Array.from(atob(text.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));

export class FakeGitHub {
  constructor({ repository, bot, appId, installationId, publicKey, pageSize = 100, now = Date.now }) {
    Object.assign(this, { repository, bot, appId: String(appId), installationId: String(installationId), publicKey, pageSize, now });
    this.issues = []; this.tokens = new Set(); this.faults = []; this.calls = []; this.held = [];
  }
  fault(name) { this.faults.push(name); }
  take(prefix) { const i = this.faults.findIndex(f => f.startsWith(prefix)); return i < 0 ? null : this.faults.splice(i, 1)[0]; }
  // An issue another account made with a copied footer: reconciliation must ignore it.
  addForeign(title, body, login = 'someone-else') { return this.add(title, body, login); }
  add(title, body, login = this.bot, labels = []) {
    const number = this.issues.length + 1, at = new Date(this.now()).toISOString();
    const issue = { number, title, body, labels: labels.map(name => ({ name })), user: { login, type: login.endsWith('[bot]') ? 'Bot' : 'User' },
      html_url: `https://github.com/${this.repository}/issues/${number}`, created_at: at, updated_at: at, state: 'open' };
    this.issues.push(issue); return issue;
  }
  created() { return this.issues.filter(issue => issue.user.login === this.bot); }
  release() { for (const resolve of this.held.splice(0)) resolve(); }
  async verifyJwt(header) {
    const [h, p, s] = (header || '').replace(/^Bearer /, '').split('.');
    if (!h || !p || !s) return false;
    const ok = await crypto.subtle.verify('RSASSA-PKCS1-v1_5', this.publicKey, fromB64url(s), new TextEncoder().encode(h + '.' + p));
    const payload = JSON.parse(new TextDecoder().decode(fromB64url(p)));
    return ok && payload.iss === this.appId && payload.exp > Math.floor(this.now() / 1000) - 600;
  }
  fetch = async (input, init = {}) => {
    // Miniflare's outbound requests are its own Request class, not this realm's.
    const request = typeof input === 'string' || input instanceof URL ? new Request(input, init) : input;
    const url = new URL(request.url), auth = request.headers.get('authorization');
    this.calls.push(request.method + ' ' + url.pathname);
    if (url.origin !== 'https://api.github.com') return json(404, { message: 'Not Found' });
    if (request.method === 'POST' && url.pathname === `/app/installations/${this.installationId}/access_tokens`) {
      if (this.take('token:500')) return json(500, { message: 'error' });
      if (!await this.verifyJwt(auth)) return json(401, { message: 'Bad credentials' });
      const body = await request.json();
      if (JSON.stringify(body) !== JSON.stringify({ repositories: [this.repository.split('/')[1]], permissions: { issues: 'write' } })) return json(422, { message: 'scope' });
      const token = 'ghs_' + crypto.randomUUID().replace(/-/g, ''); this.tokens.add(token);
      return json(201, { token, expires_at: new Date(this.now() + 3600_000).toISOString() });
    }
    if (!this.tokens.has((auth || '').replace(/^Bearer /, ''))) return json(401, { message: 'Bad credentials' });
    if (request.method === 'POST' && url.pathname === `/repos/${this.repository}/issues`) {
      const { title, body, labels } = await request.json();
      const fault = this.take('create:');
      if (fault === 'create:422') return json(422, { message: 'Validation Failed' });
      if (fault === 'create:403-retry') return json(403, { message: 'secondary rate limit' }, { 'Retry-After': '90' });
      if (fault === 'create:500-before') return json(500, { message: 'error' });
      const issue = this.add(title, body, this.bot, labels || []);
      if (fault === 'create:lost') throw new TypeError('network connection lost');
      if (fault === 'create:502-after') return json(502, { message: 'Bad Gateway' });
      if (fault === 'create:hold') await new Promise(resolve => this.held.push(resolve));
      return json(201, issue);
    }
    if (request.method === 'GET' && url.pathname === `/repos/${this.repository}/issues`) {
      const page = Number(url.searchParams.get('page') || 1), fault = this.take('list:');
      if (fault === 'list:500' || (fault === 'list:page2' && page === 2)) return json(500, { message: 'error' });
      if (fault === 'list:page2' && page !== 2) this.faults.unshift(fault);
      const since = Date.parse(url.searchParams.get('since') || 0), creator = url.searchParams.get('creator');
      const all = this.issues.filter(i => (!creator || i.user.login === creator) && Date.parse(i.updated_at) >= since);
      const items = all.slice((page - 1) * this.pageSize, page * this.pageSize);
      const headers = {};
      if (page * this.pageSize < all.length) { url.searchParams.set('page', String(page + 1)); headers.Link = `<${url}>; rel="next"`; }
      return json(200, items, headers);
    }
    return json(404, { message: 'Not Found' });
  };
}

// An RSA App key pair in the two forms GitHub and WebCrypto use, for tests and the local stand-in.
export async function appKey() {
  const { createPrivateKey } = await import('node:crypto');
  const pair = await crypto.subtle.generateKey({ name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' }, true, ['sign', 'verify']);
  const der = Buffer.from(await crypto.subtle.exportKey('pkcs8', pair.privateKey));
  // The PEM armor lines are assembled so no source file holds a key-shaped block.
  const armor = word => '-'.repeat(5) + word + ' PRIVATE KEY' + '-'.repeat(5);
  const pkcs8 = `${armor('BEGIN')}\n${der.toString('base64').match(/.{1,64}/g).join('\n')}\n${armor('END')}\n`;
  const pkcs1 = createPrivateKey(pkcs8).export({ type: 'pkcs1', format: 'pem' });
  return { publicKey: pair.publicKey, pkcs8, pkcs1 };
}
