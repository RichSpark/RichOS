// The one approved production smoke test (RichDoctor PRD §10): one harmless report with one
// screenshot through the production relay into the production repository, exactly as the app
// sends it (register, upload, complete). It refuses anything but a production profile, refuses
// without the explicit confirmation word and refuses if a smoke-test issue by the relay's bot
// already exists, so it can file at most one. It prints the issue and the staff-only link, which
// a support engineer then opens after signing in; the issue is closed by hand after that.
//
//   RICHOS_REPORT_PROFILE=<production profile.json> \
//     node richos/mobile/dev/report-smoke-production.mjs one-production-report
import { readFileSync } from 'node:fs';
import { checkProfile } from '../cli/report.mjs';
import { registerBody, screenshotReport, uploadInit } from './report-client.mjs';

export const SMOKE_TITLE = 'RichDoctor production smoke test: harmless (test, will be closed)';
export const CONFIRM = 'one-production-report';
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));

export async function smoke(profile, { confirm, fetchImpl = fetch, screenshot, wait = pause } = {}) {
  checkProfile(profile);
  if (profile.stage !== 'production') throw Error('The smoke test is the production route only; A9 runs on the test relay');
  if (confirm !== CONFIRM) throw Error(`Name the confirmation word: ${CONFIRM}`);
  const base = `https://${profile.hostname}`;
  const gh = path => fetchImpl(`https://api.github.com${path}`, { redirect: 'error', headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'richos-report-smoke', 'X-GitHub-Api-Version': '2022-11-28' } });
  // Every issue the bot ever made, every page: an earlier smoke report on any page refuses.
  let path = `/repos/${profile.repository}/issues?creator=${encodeURIComponent(profile.botLogin)}&state=all&per_page=100`, pages = 0;
  for (; path; pages++) {
    if (pages === 10) throw Error('The bot has more than ten pages of issues; nothing was sent');
    const earlier = await gh(path);
    if (!earlier.ok) throw Error(`GitHub listing failed (HTTP ${earlier.status}); nothing was sent`);
    const sent = (await earlier.json()).find(issue => issue.title === SMOKE_TITLE);
    if (sent) throw Error(`The smoke test was already sent: ${sent.html_url}`);
    const next = /<https:\/\/api\.github\.com([^>]+)>;\s*rel="next"/.exec(earlier.headers.get('link') || '');
    path = next ? next[1] : null;
  }
  const health = await (await fetchImpl(base + '/v1/health', { redirect: 'error' })).json().catch(() => null);
  if (!health?.ok || !health.accepting) throw Error('The production relay is not accepting reports; switch it on first');

  const payload = screenshotReport({ origin: base, files: [{ bytes: screenshot, type: 'image/png' }], title: SMOKE_TITLE,
    lines: ['### What happened', 'A one-time harmless test of RichDoctor reporting, sent by the RichOS team\\. Nothing is broken\\.'] });
  // "limited" means the request was not processed: wait as told and send the same request again.
  const send = async (path, init) => {
    for (let tries = 0; ; tries++) {
      const response = await fetchImpl(base + path, { redirect: 'error', ...init });
      const body = await response.json().catch(() => null);
      if (body?.state !== 'limited' || tries === 5) return body;
      await wait(1000 * (body.retry_after_seconds || 60));
    }
  };
  const json = body => ({ method: 'POST', headers: { 'Content-Type': 'application/json' }, body });
  const receipt = JSON.stringify({ receipt_secret: payload.receipt_secret });
  const registered = await send('/v1/reports', json(registerBody(payload)));
  if (registered?.state !== 'receiving') throw Error(`Registration answered ${JSON.stringify(registered)}`);
  const stored = await send(`/v1/reports/${payload.report_id}/uploads/1`, uploadInit(payload, 1));
  if (stored?.state !== 'ready') throw Error(`The upload answered ${JSON.stringify(stored)}`);
  let answer = await send(`/v1/reports/${payload.report_id}/complete`, json(receipt));
  for (let i = 0; i < 24 && answer?.state === 'pending'; i++) { await wait(5000); answer = await send(`/v1/reports/${payload.report_id}/status`, json(receipt)); }
  const url = `https://github.com/${profile.repository}/issues/${answer?.issue_number}`;
  if (answer?.state !== 'delivered' || answer.issue_url !== url || answer.images_confirmed !== 1 || answer.images_expected !== 1)
    throw Error(`Report ${payload.report_id} was not confirmed delivered: ${JSON.stringify(answer)}`);
  const link = `${base}/v1/reports/${payload.report_id}/attachments/1`;
  const anonymous = await fetchImpl(link, { redirect: 'manual' });
  if (![302, 401, 403].includes(anonymous.status) || (anonymous.headers.get('content-type') || '').startsWith('image/'))
    throw Error(`The screenshot link answered HTTP ${anonymous.status} without a staff sign-in`);
  return { ok: true, reportId: payload.report_id, issue: url, screenshotLink: link, withoutSignIn: anonymous.status };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    const profile = JSON.parse(readFileSync(process.env.RICHOS_REPORT_PROFILE || '', 'utf8'));
    const screenshot = readFileSync(new URL('../service/report/fixtures/relay/accept-rgb.png', import.meta.url));
    console.log(JSON.stringify(await smoke(profile, { confirm: process.argv[2], screenshot }), null, 2));
  } catch (error) { console.error(JSON.stringify({ ok: false, error: error.message })); process.exitCode = 1; }
}
