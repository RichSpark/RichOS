// Operator side of the RichDoctor report relay (service/report-worker.mjs, contract RICHDOCTOR-RELAY.md).
// It prepares the exact upload and talks to the relay's own D1 through Cloudflare's API with a
// short-lived token read from a private file. Identifiers live in a private profile; the GitHub
// App private key is a Worker secret only. Nothing here deploys, and no token reaches argv,
// output or an error message.
import { createHash } from 'node:crypto';
import { readFileSync, statSync } from 'node:fs';

export const COMPATIBILITY_DATE = '2026-09-21';
export const MODULES = ['report-worker.mjs', 'report/checks.mjs', 'report/images.mjs', 'report/store.mjs', 'report/github.mjs', 'report/access.mjs'];
// Hourly: delivered screenshots past their year and abandoned uploads go, and an interrupted cleanup resumes.
export const SCHEDULE = '17 * * * *';
// The one production identity (PRD §7). A test profile may use none of these.
export const PRODUCTION = { worker: 'richos-report', hostname: 'report.richos.ceo', repository: 'WebDevBooster/richos' };
// Workers rate-limit namespaces are account-wide; Connect uses 18443.
const LIMITS = { production: ['18501', '18502', '18503'], test: ['18511', '18512', '18513'] };

const sha256 = text => createHash('sha256').update(text).digest('hex');
const read = name => readFileSync(new URL('../service/' + name, import.meta.url), 'utf8');

export function checkProfile(profile) {
  const p = profile && typeof profile === 'object' ? profile : {};
  if (!['production', 'test'].includes(p.stage)) throw Error('The profile stage must be production or test');
  if (!/^[a-f0-9]{32}$/.test(p.accountId || '')) throw Error('Invalid account ID');
  if (!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(p.databaseId || '')) throw Error('Invalid report database ID');
  if (!/^[a-z0-9-]+(\.[a-z0-9-]+)+$/.test(p.hostname || '')) throw Error('Invalid relay hostname');
  if (!/^richos-report[a-z0-9-]*$/.test(p.worker || '')) throw Error('Invalid Worker name');
  if (!/^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9._-]{1,100}$/.test(p.repository || '')) throw Error('Invalid repository');
  if (!/^\d{1,12}$/.test(String(p.appId || '')) || !/^\d{1,14}$/.test(String(p.installationId || ''))) throw Error('Invalid GitHub App or installation ID');
  if (!/^[a-z0-9-]{1,34}\[bot\]$/.test(p.botLogin || '')) throw Error('Invalid GitHub App bot login');
  if (!/^[a-z0-9-]{1,50}$/.test(p.label || '')) throw Error('Invalid moderation label');
  if (!/^richos-report[a-z0-9-]*$/.test(p.bucket || '')) throw Error('Invalid screenshot bucket name');
  // Staff access: both or neither. Without them every screenshot read is refused.
  if (!!p.accessTeamDomain !== !!p.accessAud || (p.accessTeamDomain && (!/^https:\/\/[a-z0-9-]+\.cloudflareaccess\.com$/.test(p.accessTeamDomain) || !/^[a-f0-9]{64}$/.test(p.accessAud))))
    throw Error('Invalid Cloudflare Access team domain or audience');
  const production = Object.entries({ ...PRODUCTION, bucket: 'richos-report' });
  if (p.stage === 'production' && production.some(([k, v]) => p[k] !== v)) throw Error('A production profile names exactly the production Worker, hostname and repository');
  // A test relay can never reach the production repository, name or Worker.
  if (p.stage === 'test' && production.some(([k, v]) => p[k].toLowerCase() === v.toLowerCase())) throw Error('A test profile may not use the production Worker, hostname or repository');
  return p;
}

// One statement per CREATE/INSERT; the schema has no triggers.
export function schemaStatements() {
  return read('report/schema.sql').split('\n').filter(line => !line.startsWith('--')).join('\n')
    .split(';').map(statement => statement.trim()).filter(Boolean);
}

// The exact multipart upload. The secret GITHUB_APP_PRIVATE_KEY is installed separately into the
// Worker and kept across uploads by keep_bindings.
export function reportArtifact(profile) {
  const p = checkProfile(profile), [create, status, upload] = LIMITS[p.stage];
  const schema = read('report/schema.sql');
  return {
    stage: p.stage, worker: p.worker, hostname: p.hostname, repository: p.repository,
    metadata: {
      main_module: MODULES[0], compatibility_date: COMPATIBILITY_DATE,
      keep_bindings: ['secret_text'], observability: { enabled: false }, logpush: false, tail_consumers: [],
      bindings: [
        { name: 'DB', type: 'd1', id: p.databaseId },
        { name: 'CREATE_LIMIT', type: 'ratelimit', namespace_id: create, simple: { limit: 5, period: 60 } },
        { name: 'STATUS_LIMIT', type: 'ratelimit', namespace_id: status, simple: { limit: 60, period: 60 } },
        { name: 'UPLOAD_LIMIT', type: 'ratelimit', namespace_id: upload, simple: { limit: 30, period: 60 } },
        { name: 'IMAGES', type: 'r2_bucket', bucket_name: p.bucket },
        ...Object.entries({ GITHUB_APP_ID: String(p.appId), GITHUB_INSTALLATION_ID: String(p.installationId), GITHUB_REPOSITORY: p.repository,
          GITHUB_BOT_LOGIN: p.botLogin, MODERATION_LABEL: p.label,
          ...(p.accessTeamDomain ? { ACCESS_TEAM_DOMAIN: p.accessTeamDomain, ACCESS_AUD: p.accessAud } : {}) }).map(([name, text]) => ({ name, type: 'plain_text', text })),
      ],
    },
    secrets: ['GITHUB_APP_PRIVATE_KEY'],
    modules: MODULES.map(name => { const source = read(name); return { name, type: 'application/javascript+module', source, sha256: sha256(source) }; }),
    // Only a test relay may be served on workers.dev, which needs no DNS name.
    subdomain: { enabled: p.stage === 'test' && p.hostname.endsWith('.workers.dev'), previews_enabled: false },
    schedules: [{ cron: SCHEDULE }],
    schema: { sql: schema, sha256: sha256(schema) },
  };
}

export function readToken(path) {
  if (!path) throw Error('Set RICHOS_REPORT_TOKEN_FILE to a private file holding a short-lived Cloudflare token');
  if ((statSync(path).mode & 0o077) !== 0) throw Error('The report token file must be private (mode 0600)');
  const token = readFileSync(path, 'utf8').trim();
  if (!/^[A-Za-z0-9_-]{20,200}$/.test(token)) throw Error('The report token file does not hold a Cloudflare API token');
  return token;
}

export function d1(profile, token, fetchImpl = fetch) {
  const endpoint = `https://api.cloudflare.com/client/v4/accounts/${profile.accountId}/d1/database/${profile.databaseId}/query`;
  return async (sql, params = []) => {
    const response = await fetchImpl(endpoint, { method: 'POST', redirect: 'error',
      headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' }, body: JSON.stringify({ sql, params }) });
    const body = await response.json().catch(() => null), result = body?.result?.[0];
    if (!response.ok || body?.success !== true || result?.success !== true) {
      const codes = (body?.errors || []).map(error => error?.code).filter(Number.isInteger).join(',');
      throw Error(`Cloudflare D1 query failed (HTTP ${response.status}${codes ? ', codes ' + codes : ''})`);
    }
    return result;
  };
}

// Operator verbs. `control` is the off switch and the global daily cap; Rich names the operator
// and the cap. `pending` is the operator's review list. `resolve` records a report the operator
// found on GitHub: it re-reads that issue and accepts it only if the relay's bot made it and its
// footer names this report.
export async function operate(action, profile, args, { token, fetch: fetchImpl = fetch } = {}) {
  checkProfile(profile);
  const query = d1(profile, token, fetchImpl);
  if (action === 'schema') {
    for (const statement of schemaStatements()) await query(statement);
    const names = (await query("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('reports','attachments','control','daily') ORDER BY name")).results.map(r => r.name);
    if (names.join() !== 'attachments,control,daily,reports') throw Error('Hosted schema readback is incomplete');
    return { tables: names, sha256: sha256(read('report/schema.sql')) };
  }
  if (action === 'control') {
    const [mode, operator, cap, megabytes] = args;
    if (mode === 'off') await query('UPDATE control SET paused=1, updated_at=?1 WHERE id=1', [String(Date.now())]);
    else if (mode === 'on' && /^[A-Za-z][A-Za-z .'-]{1,60}$/.test(operator || '') && /^\d{1,6}$/.test(cap || '') && /^\d{1,7}$/.test(megabytes || ''))
      await query('UPDATE control SET paused=0, operator=?1, daily_cap=CAST(?2 AS INTEGER), daily_bytes_cap=CAST(?3 AS INTEGER) * 1048576, updated_at=?4 WHERE id=1',
        [operator, cap, megabytes, String(Date.now())]);
    else throw Error('Expected: control off | control on <operator name> <daily report cap> <daily screenshot megabytes>');
    return (await query('SELECT paused, daily_cap, daily_bytes_cap, operator, updated_at FROM control WHERE id=1')).results[0];
  }
  if (action === 'extend') {
    // Keeps a delivered report's screenshots longer; refused once their cleanup was claimed.
    const [id, days] = args;
    if (!/^RD-[2-9A-HJKMNP-TV-Z]{4}-[2-9A-HJKMNP-TV-Z]{4}$/.test(id || '') || !/^\d{1,3}$/.test(days || '')) throw Error('Expected: extend <report ID> <days from now>');
    const now = Date.now(), until = now + Number(days) * 86_400_000;
    const result = await query(`UPDATE reports SET expires_at=CAST(?1 AS INTEGER), updated_at=CAST(?2 AS INTEGER) WHERE report_id=?3 AND state='delivered'
      AND cleanup IS NULL AND busy_until < CAST(?2 AS INTEGER)`, [String(until), String(now), id]);
    if (result.meta?.changes !== 1) throw Error('Not extended: the report is not delivered, its screenshots are being or were deleted, or it is busy; try again');
    return { reportId: id, expiresAt: new Date(until).toISOString() };
  }
  if (action === 'pending') return (await query("SELECT report_id, attempt, attempt_at, approved_at FROM reports WHERE state IN ('unconfirmed','pending') ORDER BY attempt_at")).results;
  if (action === 'resolve') {
    const [id, number] = args;
    if (!/^RD-[2-9A-HJKMNP-TV-Z]{4}-[2-9A-HJKMNP-TV-Z]{4}$/.test(id || '') || !/^\d{1,9}$/.test(number || '')) throw Error('Expected: resolve <report ID> <issue number>');
    const response = await fetchImpl(`https://api.github.com/repos/${profile.repository}/issues/${number}`, { redirect: 'error',
      headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'richos-report-operator', 'X-GitHub-Api-Version': '2022-11-28' } });
    const issue = response.ok ? await response.json() : null;
    const { hasFooterLine } = await import('../service/report/checks.mjs');
    const url = `https://github.com/${profile.repository}/issues/${number}`;
    if (issue?.user?.login !== profile.botLogin || issue?.html_url !== url || !hasFooterLine(issue.body, id)) throw Error('That issue was not made by the relay for this report');
    const result = await query("UPDATE reports SET state='delivered', issue_number=CAST(?1 AS INTEGER), issue_url=?2, updated_at=?3 WHERE report_id=?4 AND state IN ('pending','unconfirmed')",
      [number, url, String(Date.now()), id]);
    return { reportId: id, issueUrl: url, updated: result.meta?.changes === 1 };
  }
  throw Error('Unknown report operator action');
}

function cloudflare(accountId, token, fetchImpl) {
  return async (method, path, body, type = 'application/json') => {
    const response = await fetchImpl(`https://api.cloudflare.com/client/v4/accounts/${accountId}${path}`, { method, redirect: 'error',
      headers: { Authorization: `Bearer ${token}`, ...(type && body !== undefined ? { 'Content-Type': type } : {}) },
      body: body === undefined ? undefined : type === 'application/json' ? JSON.stringify(body) : body });
    const data = await response.json().catch(() => null);
    if (!response.ok || data?.success !== true) {
      const codes = (data?.errors || []).map(error => error?.code).filter(Number.isInteger).join(',');
      throw Error(`Cloudflare ${method} ${path.split('/').slice(0, 4).join('/')} failed (HTTP ${response.status}${codes ? ', codes ' + codes : ''})`);
    }
    return data.result;
  };
}

// The profile's GitHub identifiers, read from GitHub with the App's own key, so nobody copies
// them by hand: the bot login and the installation ID. It refuses an App that can do anything
// but write issues (and read metadata), or whose installation reaches any other repository.
export async function appIdentity(appId, repository, appKey, { fetch: fetchImpl = fetch } = {}) {
  if (!/^\d{1,12}$/.test(appId || '') || !/^[A-Za-z0-9-]{1,39}\/[A-Za-z0-9._-]{1,100}$/.test(repository || '')) throw Error('Expected: app-identity <App ID> <owner/repository>');
  const { GitHub, API } = await import('../service/report/github.mjs');
  const app = new GitHub({ GITHUB_APP_ID: appId, GITHUB_APP_PRIVATE_KEY: appKey, GITHUB_REPOSITORY: repository }, { fetch: fetchImpl });
  const get = async (path, token) => {
    const response = await fetchImpl(API + path, { redirect: 'error', headers: app.headers(token) });
    if (!response.ok) throw Error(`GitHub ${path.split('/').slice(0, 3).join('/')} answered HTTP ${response.status}`);
    return response.json();
  };
  const jwt = await app.jwt(), about = await get('/app', jwt), installation = await get(`/repos/${repository}/installation`, jwt);
  const permissions = Object.entries(installation.permissions || {}).map(([k, v]) => `${k}:${v}`).sort().join(',');
  if (permissions !== 'issues:write,metadata:read') throw Error(`The App has permissions ${permissions}; it must have Issues write only`);
  if (installation.repository_selection !== 'selected') throw Error('The App is installed on all repositories; install it on the one repository only');
  app.env.GITHUB_INSTALLATION_ID = String(installation.id);
  // An unrestricted installation token: a token scoped to the one repository would see only it.
  const reach = await get('/installation/repositories?per_page=100', await app.authorize({ scoped: false }));
  const names = (reach.repositories || []).map(r => r.full_name);
  if (reach.total_count !== 1 || names[0] !== repository) throw Error(`The installation reaches ${reach.total_count} repositories; it must reach ${repository} only`);
  return { appId, botLogin: `${about.slug}[bot]`, installationId: String(installation.id), repository, permissions };
}

// The relay's own private R2 bucket for screenshots (public access stays off by default).
export async function createBucket(accountId, bucket, { token, fetch: fetchImpl = fetch } = {}) {
  if (!/^[a-f0-9]{32}$/.test(accountId || '') || !/^richos-report[a-z0-9-]*$/.test(bucket || '')) throw Error('Expected: create-bucket <account ID> <bucket name>');
  const result = await cloudflare(accountId, token, fetchImpl)('POST', '/r2/buckets', { name: bucket });
  return { bucket: result.name, created: result.creation_date || null };
}

// The relay's own D1 database, named after its Worker. Prints only the new ID for the profile.
export async function createDatabase(accountId, worker, { token, fetch: fetchImpl = fetch } = {}) {
  if (!/^[a-f0-9]{32}$/.test(accountId || '') || !/^richos-report[a-z0-9-]*$/.test(worker || '')) throw Error('Expected: create-database <account ID> <Worker name>');
  const result = await cloudflare(accountId, token, fetchImpl)('POST', '/d1/database', { name: worker });
  return { name: result.name, databaseId: result.uuid };
}

// The whole deployment of one relay, in order: schema, exact upload, the App key secret, the
// workers.dev setting, then readback of the bindings and the public health. It leaves the relay
// switched off (a new database starts off; an existing one keeps its switch).
export async function deploy(profile, { token, appKey, fetch: fetchImpl = fetch } = {}) {
  const artifact = reportArtifact(profile), api = cloudflare(profile.accountId, token, fetchImpl);
  if (!/-----BEGIN (RSA )?PRIVATE KEY-----/.test(appKey || '')) throw Error('The App key file does not hold a PEM private key');
  if (profile.hostname.endsWith('.workers.dev')) {
    const { subdomain } = await api('GET', '/workers/subdomain');
    if (profile.hostname !== `${profile.worker}.${subdomain}.workers.dev`) throw Error(`The workers.dev hostname is ${profile.worker}.${subdomain}.workers.dev`);
  }
  const schema = await operate('schema', profile, [], { token, fetch: fetchImpl });
  const form = new FormData();
  form.append('metadata', new Blob([JSON.stringify(artifact.metadata)], { type: 'application/json' }));
  for (const module of artifact.modules) form.append(module.name, new Blob([module.source], { type: module.type }), module.name);
  const uploaded = await api('PUT', `/workers/scripts/${profile.worker}`, form, null);
  await api('PUT', `/workers/scripts/${profile.worker}/secrets`, { name: 'GITHUB_APP_PRIVATE_KEY', text: appKey, type: 'secret_text' });
  await api('POST', `/workers/scripts/${profile.worker}/subdomain`, artifact.subdomain);
  await api('PUT', `/workers/scripts/${profile.worker}/schedules`, artifact.schedules);
  const settings = await api('GET', `/workers/scripts/${profile.worker}/settings`);
  const bindings = (settings.bindings || []).map(b => `${b.name}:${b.type}`).sort();
  const expected = [...artifact.metadata.bindings.map(b => `${b.name}:${b.type}`), 'GITHUB_APP_PRIVATE_KEY:secret_text'].sort();
  if (JSON.stringify(bindings) !== JSON.stringify(expected)) throw Error(`Bindings read back as ${bindings.join(', ')}`);
  if (settings.logpush || (settings.tail_consumers || []).length || settings.observability?.enabled) throw Error('Logs, Logpush or tails are not off');
  let health = null;
  for (let i = 0; i < 10 && !health?.ok; i++) {
    if (i) await new Promise(resolve => setTimeout(resolve, 3000));
    health = await (await fetchImpl(`https://${profile.hostname}/v1/health`, { redirect: 'error' }).catch(() => null))?.json().catch(() => null) ?? null;
  }
  if (!health?.ok) throw Error('The relay health check did not answer ok');
  return { worker: profile.worker, hostname: profile.hostname, repository: profile.repository, schema: schema.sha256, deployment: uploaded?.id || null,
    modules: artifact.modules.map(m => ({ name: m.name, sha256: m.sha256 })), bindings, health };
}

export async function command(action, file, rest = []) {
  if (action === 'artifact') {
    if (!file) throw Error('Provide the private relay profile JSON file');
    return reportArtifact(JSON.parse(readFileSync(file, 'utf8')));
  }
  if (action === 'create-bucket') return createBucket(file, rest[0], { token: readToken(process.env.RICHOS_REPORT_TOKEN_FILE) });
  if (action === 'create-database') return createDatabase(file, rest[0], { token: readToken(process.env.RICHOS_REPORT_TOKEN_FILE) });
  const appKey = () => {
    const keyFile = process.env.RICHOS_REPORT_APP_KEY_FILE;
    if (!keyFile || (statSync(keyFile).mode & 0o077) !== 0) throw Error('Set RICHOS_REPORT_APP_KEY_FILE to the private (mode 0600) GitHub App key file');
    return readFileSync(keyFile, 'utf8');
  };
  if (action === 'app-identity') return appIdentity(file, rest[0], appKey());
  if (action === 'deploy') {
    return deploy(JSON.parse(readFileSync(process.env.RICHOS_REPORT_PROFILE || '', 'utf8')),
      { token: readToken(process.env.RICHOS_REPORT_TOKEN_FILE), appKey: appKey() });
  }
  const profile = JSON.parse(readFileSync(process.env.RICHOS_REPORT_PROFILE || '', 'utf8'));
  return operate(action, profile, [file, ...rest].filter(v => v !== undefined), { token: readToken(process.env.RICHOS_REPORT_TOKEN_FILE) });
}
