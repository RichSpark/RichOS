#!/usr/bin/env node
// relay-log-check.mjs — what a walk's app sent to report-relay-stand-in.py, through the REAL
// relay's own checks (richos/mobile/service/report/checks.mjs: the schema, the fingerprint and
// the content rules, with the footer), on the host.
//
//   node relay-log-check.mjs <stand-in request log> <schema> [<schema> ...]
//
// The log is the stand-in's (one JSON line per request, written as it arrived). Every
// registration (POST /v1/reports) must pass checkCreate and contentProblem; each <schema> named
// must be registered under exactly one report ID (the contract's same-ID replay is one report),
// and no other schema may be; no request may carry an Authorization header (no token is on the
// Mac: the relay files the report). One line per registration, then "ok" or the problems; exit 0
// only when there are none.
//
// bug-report-walk.sh and feature-request-walk.sh run the same two checks inline for their own
// walks; this is the same rule for a steps-walk.py record that pulls the log
// (docs/verification/2026-10-25-bust-a-bug-relay-walk).
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const [log, ...schemas] = process.argv.slice(2);
if (!log || !schemas.length) {
  console.error('usage: relay-log-check.mjs <stand-in request log> <schema> [<schema> ...]');
  process.exit(2);
}
const checks = join(dirname(fileURLToPath(import.meta.url)), '../../../mobile/service/report/checks.mjs');
const { checkCreate, contentProblem } = await import(pathToFileURL(checks).href);
const ORIGIN = 'http://127.0.0.1:8765';

const requests = readFileSync(log, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l));
const problems = [];
for (const r of requests) {
  if (Object.keys(r.headers || {}).some(h => h.toLowerCase() === 'authorization')) problems.push(`${r.method} ${r.path} carried an Authorization header`);
}
const seen = {};
for (const r of requests.filter(r => r.method === 'POST' && r.path === '/v1/reports')) {
  const data = JSON.parse(r.body);
  (seen[data.schema] ||= new Set()).add(data.report_id);
  const checked = await checkCreate(data, Date.now());
  if (checked.error) { problems.push(`${data.report_id} (${data.schema}) refused: ${JSON.stringify(checked.error)}`); continue; }
  const problem = contentProblem({ ...checked.value, origin: ORIGIN, count: checked.value.manifest.length });
  if (problem) { problems.push(`${data.report_id} (${data.schema}) content refused: ${JSON.stringify(problem)}`); continue; }
  console.log(`taken: ${data.report_id} ${data.schema}, ${checked.value.manifest.length} screenshot(s), title ${JSON.stringify(data.title)}`);
}
for (const s of schemas) if ((seen[s]?.size || 0) !== 1) problems.push(`${s} registered under ${seen[s]?.size || 0} report ID(s), expected one`);
for (const s of Object.keys(seen)) if (!schemas.includes(s)) problems.push(`${s} registered under ${seen[s].size} report ID(s), not expected`);
console.log(`${requests.length} request(s), ${requests.filter(r => /authorization/i.test(Object.keys(r.headers || {}).join(' '))).length} with an Authorization header`);
if (problems.length) { for (const p of problems) console.log('FAILED: ' + p); process.exit(1); }
console.log('ok');
