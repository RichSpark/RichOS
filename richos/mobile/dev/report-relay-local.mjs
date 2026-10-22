// The exact report-relay upload in Cloudflare's local runtime (workerd through Miniflare), with a
// persisted local D1 and every GitHub call answered by the fake GitHub in this process. Nothing is
// installed: Miniflare comes from RICHOS_MINIFLARE or the Wrangler already on PATH. Loopback only;
// the workerd process belongs to the returned handle and ends with dispose().
//
// As a stand-in for the RichDoctor client:
//   node richos/mobile/dev/report-relay-local.mjs <empty state directory>
// prints the relay base (set RICHDOCTOR_REPORT_RELAY to it and RICHDOCTOR_REPORT_REPOSITORY to the
// printed repository) and a control base: GET /issues lists what the fake GitHub holds, POST
// /fault with a fault name (see report-fake-github.mjs) injects one failure, POST /release lets a
// held create answer, POST /restart restarts the relay on the same D1, POST /off and /on flip the
// operator switch and POST /cap/<n> sets the daily cap.
import { mkdirSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { locateMiniflare } from './policy-worker-local.mjs';
import { FakeGitHub, appKey } from './report-fake-github.mjs';
import { reportArtifact, schemaStatements } from '../cli/report.mjs';

const service = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'service');
export const LOCAL = { repository: 'richos-test/report-relay-local', bot: 'richos-reports-local[bot]', appId: '1001', installationId: '2002' };
// A local stand-in for the Cloudflare Access team, so staff reads can be proven against workerd.
export const LOCAL_ACCESS = { team: 'https://richos-local.cloudflareaccess.com', aud: 'c'.repeat(64) };

export async function startLocalRelay({ entry = locateMiniflare(), state, reconcileSeconds = 4, abandonSeconds = null, retainSeconds = null, pageSize = 100, port = 0 } = {}) {
  if (!entry) throw Error('Cloudflare local runtime not installed (no Wrangler with Miniflare on PATH; set RICHOS_MINIFLARE)');
  if (!state) throw Error('An explicit state directory is required');
  mkdirSync(state, { recursive: true, mode: 0o700 });
  const { Miniflare, supportedCompatibilityDate } = await import(pathToFileURL(entry).href);
  const artifact = reportArtifact({ stage: 'test', worker: 'richos-report-local', accountId: '0'.repeat(32), databaseId: '00000000-0000-0000-0000-000000000000',
    hostname: 'report-local.example.com', repository: LOCAL.repository, appId: LOCAL.appId, installationId: LOCAL.installationId, botLogin: LOCAL.bot, label: 'richdoctor',
    bucket: 'richos-report-local', accessTeamDomain: LOCAL_ACCESS.team, accessAud: LOCAL_ACCESS.aud });
  const compatibilityDate = artifact.metadata.compatibility_date <= supportedCompatibilityDate ? artifact.metadata.compatibility_date : supportedCompatibilityDate;
  const key = await appKey();
  const github = new FakeGitHub({ ...LOCAL, publicKey: key.publicKey, pageSize });
  const staff = await crypto.subtle.generateKey({ name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' }, true, ['sign', 'verify']);
  const certs = JSON.stringify({ keys: [{ ...(await crypto.subtle.exportKey('jwk', staff.publicKey)), kid: 'local-staff' }] });
  const b64 = value => Buffer.from(JSON.stringify(value)).toString('base64url');
  // A signed Access token for the local team; `claims` override the valid defaults.
  async function accessToken(claims = {}) {
    const header = b64({ alg: 'RS256', kid: 'local-staff' }), now = Math.floor(Date.now() / 1000);
    const body = b64({ iss: LOCAL_ACCESS.team, aud: [LOCAL_ACCESS.aud], exp: now + 600, iat: now, ...claims });
    return `${header}.${body}.${Buffer.from(await crypto.subtle.sign('RSASSA-PKCS1-v1_5', staff.privateKey, new TextEncoder().encode(header + '.' + body))).toString('base64url')}`;
  }
  const vars = Object.fromEntries(artifact.metadata.bindings.filter(b => b.type === 'plain_text').map(b => [b.name, b.text]));
  const ratelimits = Object.fromEntries(artifact.metadata.bindings.filter(b => b.type === 'ratelimit').map(b => [b.name, { simple: { limit: 1000, period: 60 } }]));
  const options = () => ({
    // cf: false keeps Miniflare from downloading Cloudflare's request-metadata file into ./.wrangler.
    modulesRoot: service, compatibilityDate, cf: false, host: '127.0.0.1', port,
    modules: artifact.modules.map(module => ({ type: 'ESModule', path: join(service, module.name), contents: module.source })),
    d1Databases: { DB: 'richos-report-local' }, d1Persist: join(state, 'd1'), ratelimits,
    r2Buckets: { IMAGES: 'richos-report-local' }, r2Persist: join(state, 'r2'),
    bindings: { ...vars, GITHUB_APP_PRIVATE_KEY: key.pkcs1, RECONCILE_SECONDS: String(reconcileSeconds),
      ...(abandonSeconds ? { ABANDON_SECONDS: String(abandonSeconds) } : {}), ...(retainSeconds ? { RETAIN_SECONDS: String(retainSeconds) } : {}) },
    outboundService: request => request.url === `${LOCAL_ACCESS.team}/cdn-cgi/access/certs` ? new Response(certs, { status: 200 }) : github.fetch(request),
  });
  let mf = new Miniflare(options()), url;
  const boot = async () => {
    url = await mf.ready;
    const db = await mf.getD1Database('DB');
    await db.batch(schemaStatements().map(statement => db.prepare(statement)));
    return db;
  };
  let db;
  try { db = await boot(); } catch (error) { await mf.dispose().catch(() => {}); throw error; }
  const handle = {
    github, compatibilityDate, repository: LOCAL.repository, runtime: { miniflare: entry, requestedDate: artifact.metadata.compatibility_date },
    get url() { return url.href.replace(/\/$/, ''); }, get db() { return db; },
    fetch: (path, init) => mf.dispatchFetch(new URL(path, url).href, init),
    accessToken,
    control: (paused, cap, operator = 'local-runtime', bytesCap = 200 * 1048576) => db.prepare('UPDATE control SET paused=?, daily_cap=?, daily_bytes_cap=?, operator=?, updated_at=? WHERE id=1')
      .bind(paused, cap, bytesCap, operator, Date.now()).run(),
    // One run of the scheduled cleanup, exactly as the cron trigger runs it.
    async cleanup() { await (await mf.getWorker()).scheduled({ cron: artifact.schedules[0].cron }); },
    bucket: async () => mf.getR2Bucket('IMAGES'),
    // Ends the workerd process mid-flight, then starts a new one on the same D1: a relay restart.
    async restart() { await mf.dispose(); mf = new Miniflare(options()); db = await boot(); },
    dispose: () => mf.dispose(),
  };
  await handle.control(0, 1000);
  return handle;
}

async function serve(state) {
  const relay = await startLocalRelay({ state, reconcileSeconds: 120 });
  const routes = {
    'GET /issues': () => relay.github.issues,
    'POST /release': () => relay.github.release(),
    'POST /restart': () => relay.restart(),
    'POST /off': () => relay.control(1, 1000),
    'POST /on': () => relay.control(0, 1000),
  };
  const control = createServer(async (request, response) => {
    let body = '';
    for await (const chunk of request) body += chunk;
    const key = `${request.method} ${request.url}`, cap = /^POST \/cap\/(\d+)$/.exec(key);
    try {
      const result = key === 'POST /fault' ? relay.github.fault(body.trim()) : cap ? await relay.control(0, Number(cap[1])) : routes[key] ? await routes[key]() : undefined;
      response.writeHead(routes[key] || cap || key === 'POST /fault' ? 200 : 404, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify(result ?? { ok: true }));
    } catch (error) { response.writeHead(500); response.end(JSON.stringify({ error: error.message })); }
  });
  await new Promise(done => control.listen(0, '127.0.0.1', done));
  const stop = async () => { control.close(); await relay.dispose(); process.exit(0); };
  process.on('SIGINT', stop); process.on('SIGTERM', stop);
  console.log(JSON.stringify({ relay: relay.url, repository: relay.repository, control: `http://127.0.0.1:${control.address().port}`, pid: process.pid }));
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv[2]) { console.error('Usage: node richos/mobile/dev/report-relay-local.mjs <empty state directory>'); process.exit(2); }
  await serve(resolve(process.argv[2]));
}
