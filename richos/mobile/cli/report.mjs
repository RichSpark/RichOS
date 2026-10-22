// Operator side of the RichDoctor report relay (service/report-worker.mjs, contract REPORT.md).
// It prepares the exact upload and talks to the relay's own D1 through Cloudflare's API with a
// short-lived token read from a private file. Identifiers live in a private profile; the GitHub
// App private key is a Worker secret only. Nothing here deploys, and no token reaches argv,
// output or an error message.
import { createHash } from 'node:crypto';
import { readFileSync, statSync } from 'node:fs';

export const COMPATIBILITY_DATE = '2026-09-21';
export const MODULES = ['report-worker.mjs', 'report/checks.mjs', 'report/store.mjs', 'report/github.mjs'];
// The one production identity (PRD §7). A test profile may use none of these.
export const PRODUCTION = { worker: 'richos-report', hostname: 'report.richos.ceo', repository: 'WebDevBooster/richos' };
// Workers rate-limit namespaces are account-wide; Connect uses 18443.
const LIMITS = { production: ['18501', '18502'], test: ['18511', '18512'] };

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
  const production = Object.entries(PRODUCTION);
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
  const p = checkProfile(profile), [create, status] = LIMITS[p.stage];
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
        ...Object.entries({ GITHUB_APP_ID: String(p.appId), GITHUB_INSTALLATION_ID: String(p.installationId), GITHUB_REPOSITORY: p.repository,
          GITHUB_BOT_LOGIN: p.botLogin, MODERATION_LABEL: p.label }).map(([name, text]) => ({ name, type: 'plain_text', text })),
      ],
    },
    secrets: ['GITHUB_APP_PRIVATE_KEY'],
    modules: MODULES.map(name => { const source = read(name); return { name, type: 'application/javascript+module', source, sha256: sha256(source) }; }),
    subdomain: { enabled: false, previews_enabled: false },
    schema: { sql: schema, sha256: sha256(schema) },
  };
}

export function readToken(path) {
  if (!path) throw Error('Set RICHOS_REPORT_TOKEN_FILE to a private file holding a short-lived D1 token');
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
    const names = (await query("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('reports','control','daily') ORDER BY name")).results.map(r => r.name);
    if (names.join() !== 'control,daily,reports') throw Error('Hosted schema readback is incomplete');
    return { tables: names, sha256: sha256(read('report/schema.sql')) };
  }
  if (action === 'control') {
    const [mode, operator, cap] = args;
    if (mode === 'off') await query('UPDATE control SET paused=1, updated_at=?1 WHERE id=1', [String(Date.now())]);
    else if (mode === 'on' && /^[A-Za-z][A-Za-z .'-]{1,60}$/.test(operator || '') && /^\d{1,6}$/.test(cap || ''))
      await query('UPDATE control SET paused=0, operator=?1, daily_cap=CAST(?2 AS INTEGER), updated_at=?3 WHERE id=1', [operator, cap, String(Date.now())]);
    else throw Error('Expected: control off | control on <operator name> <daily cap>');
    return (await query('SELECT paused, daily_cap, operator, updated_at FROM control WHERE id=1')).results[0];
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

export async function command(action, file, rest = []) {
  if (action === 'artifact') {
    if (!file) throw Error('Provide the private relay profile JSON file');
    return reportArtifact(JSON.parse(readFileSync(file, 'utf8')));
  }
  const profile = JSON.parse(readFileSync(process.env.RICHOS_REPORT_PROFILE || '', 'utf8'));
  return operate(action, profile, [file, ...rest].filter(v => v !== undefined), { token: readToken(process.env.RICHOS_REPORT_TOKEN_FILE) });
}
