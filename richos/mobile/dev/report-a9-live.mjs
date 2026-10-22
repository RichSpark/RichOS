// PRD acceptance A9 against a DEPLOYED TEST relay and its real test repository on GitHub. It
// refuses a production profile, so it can never create an issue in the production repository.
//
//   RICHOS_REPORT_PROFILE=<test profile.json> RICHOS_REPORT_TOKEN_FILE=<0600 D1 token file> \
//     node richos/mobile/dev/report-a9-live.mjs
//
// Optional RICHOS_REPORT_A9_GITHUB_TOKEN_FILE (0600) is a read-only token for a private test
// repository's issue listing. What real GitHub cannot be made to do (lose an answer after
// creating, a relay restart mid-request) is proven in workerd by test/report-relay-runtime.test.js;
// this run proves the same rules end to end on the real service: one issue per report, exact
// words, replay, conflict, a client gone after acceptance, the size limit, the cap and the switch.
// It turns the test relay's switch off when it ends. Its output is one JSON record.
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { readFileSync, statSync } from 'node:fs';
import { checkProfile, d1, readToken } from '../cli/report.mjs';

const ALPHABET = '23456789ABCDEFGHJKMNPQRSTVWXYZ';
const reportId = () => 'RD-' + [4, 4].map(n => Array.from(randomBytes(n), b => ALPHABET[b % 30]).join('')).join('-');
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

export function report({ title = 'RichDoctor A9 live test: harmless', body, id = reportId(), started }) {
  body ??= `### What happened\nThis is an automated delivery test of the RichDoctor report relay, run ${started}\\. Nothing is broken\\.\n\nSent with RichDoctor 1.0 · Report ID ${id}`;
  return { schema: 'richdoctor-report/1', report_id: id, receipt_secret: randomBytes(32).toString('base64url'),
    content_sha256: createHash('sha256').update(title + '\n\n' + body).digest('hex'), installation_id: randomUUID(),
    app_version: '1.0.0', approved_at: Date.now(), title, body };
}

export async function run(profile, { token, githubToken, fetchImpl = fetch } = {}) {
  checkProfile(profile);
  if (profile.stage !== 'test') throw Error('The live A9 run uses a test relay and test repository only');
  const base = `https://${profile.hostname}`, query = d1(profile, token, fetchImpl), started = new Date().toISOString();
  const steps = [], check = (name, ok, detail = {}) => { steps.push({ name, ok: !!ok, ...detail }); if (!ok) throw Error(`A9 live step failed: ${name}`); };
  const call = async (path, body, init = {}) => {
    const response = await fetchImpl(base + path, { method: 'POST', redirect: 'error', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body), ...init });
    return { status: response.status, body: await response.json().catch(() => null) };
  };
  const create = (payload, init) => call('/v1/reports', payload, init);
  const status = payload => call(`/v1/reports/${payload.report_id}/status`, { receipt_secret: payload.receipt_secret });
  const gh = (path) => fetchImpl(`https://api.github.com${path}`, { redirect: 'error', headers: { Accept: 'application/vnd.github+json',
    'User-Agent': 'richos-report-a9', 'X-GitHub-Api-Version': '2022-11-28', ...(githubToken ? { Authorization: `Bearer ${githubToken}` } : {}) } });
  async function botIssues() {
    const all = [];
    for (let page = 1; page <= 10; page++) {
      const response = await gh(`/repos/${profile.repository}/issues?creator=${encodeURIComponent(profile.botLogin)}&state=all&since=${encodeURIComponent(started)}&per_page=100&page=${page}`);
      if (!response.ok) throw Error(`GitHub listing failed (HTTP ${response.status})`);
      const items = await response.json(); all.push(...items);
      if (items.length < 100) return all;
    }
    throw Error('More than 1000 issues since the run started');
  }
  const footer = id => `Sent with RichDoctor 1.0 · Report ID ${id}`;
  const count = (issues, id) => issues.filter(i => (i.body || '').replace(/\r\n/g, '\n').split('\n').includes(footer(id))).length;
  try {
    const health = await (await fetchImpl(base + '/v1/health', { redirect: 'error' })).json();
    check('health', health.ok === true && health.service === 'richos-report', { health });
    await query('UPDATE control SET paused=0, operator=?1, daily_cap=1000, updated_at=?2 WHERE id=1', ['A9 live run', String(Date.now())]);

    const fresh = report({ started });
    check('disconnected before send: unknown', (await status(fresh)).body?.state === 'unknown_id');
    const first = await create(fresh);
    check('delivered with a real issue URL', first.body?.state === 'delivered'
      && first.body.issue_url === `https://github.com/${profile.repository}/issues/${first.body.issue_number}`, { issue: first.body?.issue_url });
    const issue = await (await gh(`/repos/${profile.repository}/issues/${first.body.issue_number}`)).json();
    check('the issue holds the exact approved words, the bot and the label', issue.title === fresh.title && issue.body === fresh.body
      && issue.user?.login === profile.botLogin && (issue.labels || []).some(l => l.name === profile.label));
    check('exact replay returns the same issue', (await create(fresh)).body?.issue_url === first.body.issue_url);
    const edited = report({ started, id: fresh.report_id, title: 'RichDoctor A9 live test: edited' });
    check('changed bytes under the same ID are refused', (await create({ ...edited, receipt_secret: fresh.receipt_secret })).body?.state === 'conflict');
    check('a wrong receipt is refused', (await status({ ...fresh, receipt_secret: edited.receipt_secret })).body?.state === 'forbidden');

    // The client goes away 150 ms after sending. If the relay had already accepted it, the relay
    // finishes the exact attempt; if not, status says unknown_id and the app may send again.
    const gone = report({ started });
    await create(gone, { signal: AbortSignal.timeout(150) }).catch(() => null);
    let late = await status(gone), resent = false;
    if (late.body?.state === 'unknown_id') { resent = true; late = await create(gone); }
    for (let i = 0; i < 40 && late?.body?.state !== 'delivered'; i++) { await pause(3000); late = await status(gone); }
    check('a client gone after sending still gets exactly one issue', late?.body?.state === 'delivered', { issue: late?.body?.issue_url, resent });

    const id = reportId(), tail = `\n\n${footer(id)}`;
    const largest = report({ started, id, body: 'A9 maximum body\\. ' + 'x'.repeat(60000 - Buffer.byteLength(tail) - 18) + tail });
    check('the largest allowed body is delivered', (await create(largest)).body?.state === 'delivered');
    const over = await create(report({ started, body: 'x'.repeat(60001) }));
    check('one byte more is refused for size', over.status === 413 && over.body?.reason === 'size');

    const day = new Date().toISOString().slice(0, 10);
    const used = (await query('SELECT creates FROM daily WHERE day=?1', [day])).results?.[0]?.creates || 0;
    await query('UPDATE control SET daily_cap=CAST(?1 AS INTEGER) WHERE id=1', [String(used)]);
    const capped = await create(report({ started }));
    check('the daily cap closes new reports honestly', capped.body?.state === 'closed' && capped.body.reason === 'daily_cap');
    await query('UPDATE control SET paused=1 WHERE id=1');
    const off = await create(report({ started }));
    check('the switch closes new reports honestly', off.body?.state === 'closed' && off.body.reason === 'operator_off');
    check('known reports still answer while switched off', (await status(fresh)).body?.state === 'delivered');

    const issues = await botIssues();
    for (const payload of [fresh, gone, largest]) check(`exactly one issue for ${payload.report_id}`, count(issues, payload.report_id) === 1);
    check('nothing else was created', issues.length === 3, { created: issues.map(i => i.html_url) });
    return { ok: true, started, finished: new Date().toISOString(), relay: base, repository: profile.repository, steps };
  } finally {
    await query('UPDATE control SET paused=1, updated_at=?1 WHERE id=1', [String(Date.now())]).catch(() => {});
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    const profile = JSON.parse(readFileSync(process.env.RICHOS_REPORT_PROFILE || '', 'utf8'));
    const file = process.env.RICHOS_REPORT_A9_GITHUB_TOKEN_FILE;
    if (file && (statSync(file).mode & 0o077) !== 0) throw Error('The GitHub token file must be private (mode 0600)');
    const githubToken = file ? readFileSync(file, 'utf8').trim() : undefined;
    console.log(JSON.stringify(await run(profile, { token: readToken(process.env.RICHOS_REPORT_TOKEN_FILE), githubToken }), null, 2));
  } catch (error) { console.error(JSON.stringify({ ok: false, error: error.message })); process.exitCode = 1; }
}
