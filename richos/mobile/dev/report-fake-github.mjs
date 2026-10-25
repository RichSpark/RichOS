// A stand-in for the parts of GitHub's REST API the report relay uses: App installation tokens,
// issue creation and the creator-filtered, paginated issue listing, and the two GraphQL requests
// that post a RichOS feature idea as a Discussion under Ideas. It checks the App JWT's RS256
// signature and the token on every call, and injects one-shot faults for failure tests. It is a
// test double: it proves the relay's logic, never real delivery.
const json = (status, body, headers = {}) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json', ...headers } });
const fromB64url = text => Uint8Array.from(atob(text.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));

export class FakeGitHub {
  constructor({ repository, bot, appId, installationId, publicKey, pageSize = 100, now = Date.now }) {
    Object.assign(this, { repository, bot, appId: String(appId), installationId: String(installationId), publicKey, pageSize, now });
    this.issues = []; this.discussions = []; this.tokens = new Set(); this.scopes = new Map(); this.granted = new Map(); this.faults = []; this.calls = []; this.held = [];
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
      const scope = permission => JSON.stringify({ repositories: [this.repository.split('/')[1]], permissions: { [permission]: 'write' } });
      const scoped = JSON.stringify(body) === scope('issues') || JSON.stringify(body) === scope('discussions');
      if (!scoped && JSON.stringify(body) !== '{}') return json(422, { message: 'scope' });
      // As on GitHub, a token restricted to one repository sees only that repository.
      const token = 'ghs_' + crypto.randomUUID().replace(/-/g, ''); this.tokens.add(token); this.scopes.set(token, scoped ? [this.repository] : (this.reach || [this.repository]));
      this.granted.set(token, scoped ? body.permissions : null);
      return json(201, { token, expires_at: new Date(this.now() + 3600_000).toISOString() });
    }
    // The App's own identity and its installation, read with the App JWT.
    if (request.method === 'GET' && (url.pathname === '/app' || url.pathname === `/repos/${this.repository}/installation`)) {
      if (!await this.verifyJwt(auth)) return json(401, { message: 'Bad credentials' });
      if (url.pathname === '/app') return json(200, { id: Number(this.appId), slug: this.bot.replace(/\[bot\]$/, '') });
      return json(200, { id: Number(this.installationId), repository_selection: this.selection || 'selected', permissions: this.permissions || { discussions: 'write', issues: 'write', metadata: 'read' } });
    }
    if (request.method === 'GET' && url.pathname === '/installation/repositories') { // ?per_page is ignored
      if (!this.tokens.has((auth || '').replace(/^Bearer /, ''))) return json(401, { message: 'Bad credentials' });
      const repositories = this.scopes.get((auth || '').replace(/^Bearer /, '')).map(full_name => ({ full_name }));
      return json(200, { total_count: repositories.length, repositories });
    }
    // Reads of a public repository need no token, as on GitHub; a token that is sent must be valid.
    if ((auth || request.method !== 'GET') && !this.tokens.has((auth || '').replace(/^Bearer /, ''))) return json(401, { message: 'Bad credentials' });
    if (request.method === 'POST' && url.pathname === '/graphql') {
      const { query, variables } = await request.json(), fault = this.take('graphql:');
      // As on GitHub, a token minted for Issues write alone may not post a Discussion.
      if (this.granted.get(auth.replace(/^Bearer /, ''))?.discussions !== 'write') return json(200, { data: null, errors: [{ type: 'FORBIDDEN', message: 'Resource not accessible by integration' }] });
      if (fault === 'graphql:forbidden') return json(200, { data: null, errors: [{ type: 'FORBIDDEN', message: 'Resource not accessible by integration' }] });
      if (query.includes('discussionCategory')) {
        if (fault === 'graphql:lookup-500') return json(500, { message: 'error' });
        if (fault) this.faults.unshift(fault); // a create fault waits for the create
        const mine = variables.owner + '/' + variables.name === this.repository;
        return json(200, { data: { repository: mine ? { id: 'R_fake', discussionCategory: variables.slug === 'ideas' ? { id: 'DIC_fake_ideas' } : null } : null } });
      }
      if (query.includes('createDiscussion')) {
        if (variables.repositoryId !== 'R_fake' || variables.categoryId !== 'DIC_fake_ideas') return json(200, { data: { createDiscussion: null }, errors: [{ type: 'NOT_FOUND' }] });
        const number = this.issues.length + this.discussions.length + 1;
        const discussion = { number, url: `https://github.com/${this.repository}/discussions/${number}`, title: variables.title, body: variables.body, category: 'ideas' };
        this.discussions.push(discussion);
        if (fault === 'graphql:lost') throw new TypeError('network connection lost');
        return json(200, { data: { createDiscussion: { discussion: { number, url: discussion.url } } } });
      }
      return json(200, { errors: [{ message: 'unknown query' }] });
    }
    const single = new RegExp(`^/repos/${this.repository}/issues/(\\d+)$`).exec(url.pathname);
    if (request.method === 'GET' && single) { const issue = this.issues[Number(single[1]) - 1]; return issue ? json(200, issue) : json(404, { message: 'Not Found' }); }
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
      // GitHub's `since` filter answered [] for minutes-old issues on 2026-10-22; this stand-in
      // answers [] whenever it is used, so nothing can come to rely on it.
      if (url.searchParams.has('since')) return json(200, []);
      const creator = url.searchParams.get('creator'), desc = url.searchParams.get('direction') === 'desc';
      const mine = this.issues.filter(i => !creator || i.user.login === creator), all = desc ? [...mine].reverse() : mine;
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
