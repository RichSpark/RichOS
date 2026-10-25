// relay-log-check.test.mjs — relay-log-check.mjs on stand-in request logs written here: a bug
// report and an idea the real relay's checks take pass; a token on any request, a second report
// under another ID, a missing or an unexpected schema, and a broken fingerprint each fail.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const tool = join(here, '..', 'relay-log-check.mjs');
const { contentHash } = await import(pathToFileURL(join(here, '../../../../mobile/service/report/checks.mjs')).href);

async function registration(schema, reportId, title) {
  const body = `What happened.\n\nSent with RichDoctor 1.2.3 · Report ID ${reportId}`;
  return {
    method: 'POST', path: '/v1/reports', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      schema, report_id: reportId, receipt_secret: 'a'.repeat(43), content_sha256: await contentHash(title, body, []),
      installation_id: '0f8fad5b-d9cb-469f-a165-70867728950e', app_version: '1.2.3', approved_at: 1_761_000_000_000,
      title, body, manifest: [],
    }),
  };
}
const complete = id => ({ method: 'POST', path: `/v1/reports/${id}/complete`, headers: {}, body: '{"receipt_secret":"x"}' });

const root = mkdtempSync(join(process.env.TMPDIR || tmpdir(), 'relay-log-check-test-'));
function check(requests, ...schemas) {
  const log = join(root, `log-${Math.random().toString(36).slice(2)}.jsonl`);
  writeFileSync(log, requests.map(r => JSON.stringify(r)).join('\n') + '\n');
  const r = spawnSync('node', [tool, log, ...schemas], { encoding: 'utf8' });
  return { code: r.status, out: r.stdout + r.stderr };
}
try {
  const bug = await registration('richdoctor-report/2', 'RD-ABCD-EFGH', 'Names cut off');
  const idea = await registration('richos-idea/1', 'RD-JKMN-PQRS', 'A Friday summary');
  const both = ['richdoctor-report/2', 'richos-idea/1'];

  let r = check([bug, complete('RD-ABCD-EFGH'), idea, complete('RD-JKMN-PQRS')], ...both);
  assert.equal(r.code, 0, r.out);
  assert.match(r.out, /taken: RD-ABCD-EFGH richdoctor-report\/2/);
  assert.match(r.out, /taken: RD-JKMN-PQRS richos-idea\/1/);
  assert.match(r.out, /\nok\n$/);

  // The contract's same-ID replay is still one report.
  r = check([bug, bug, idea], ...both);
  assert.equal(r.code, 0, r.out);

  r = check([bug, { ...complete('RD-ABCD-EFGH'), headers: { authorization: 'Bearer x' } }, idea], ...both);
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /FAILED: POST \/v1\/reports\/RD-ABCD-EFGH\/complete carried an Authorization header/);

  const second = await registration('richdoctor-report/2', 'RD-TVWX-YZ23', 'Names cut off');
  r = check([bug, second, idea], ...both);
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /richdoctor-report\/2 registered under 2 report ID\(s\), expected one/);

  r = check([bug], ...both);
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /richos-idea\/1 registered under 0 report ID\(s\)/);

  r = check([bug, idea], 'richdoctor-report/2');
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /richos-idea\/1 registered under 1 report ID\(s\), not expected/);

  const broken = JSON.parse(bug.body);
  broken.title = 'Changed after approval';
  r = check([{ ...bug, body: JSON.stringify(broken) }], 'richdoctor-report/2');
  assert.equal(r.code, 1, r.out);
  assert.match(r.out, /refused: .*fingerprint does not match/);

  assert.equal(check([]).code, 2);
  console.log('relay-log-check: 7 cases ok');
} finally {
  rmSync(root, { recursive: true, force: true });
}
