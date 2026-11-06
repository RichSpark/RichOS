#!/usr/bin/env node
// Loro public component. SPDX-License-Identifier: AGPL-3.0-only

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { dateFieldProblem, dateFieldProblems, deriveStatus, normalizeRecord, validateRecord } from '../lib/record.js';
import { inferKind, isCommentOnly, outboundLinks, slugify, splitMarkdownSections } from '../lib/sources.js';
import { corpusPaths, readCompanyManifest, repoPaths, resolveCorpusRoot } from '../lib/layout.js';
import { analyzeCoverage, candidateFor, citationOf, coverageByPage, relinkCandidates, sameHeadingText } from '../lib/coverage.js';
import { parseFrontMatter, setFields } from '../lib/frontmatter.js';
import { REF_RESOLVERS, appendRecord, correctRecord, createCompany, isWidening, locate, resolveRef, resolvePartition, setJsonlField, supersedeRecord } from '../writer/writer.js';
import { checkAgainstBaseline, readBaseline } from '../writer/coverage-write.js';
import { SOURCES, loadCorpus, resolveRoot } from '../lib/store.js';
import { loadSearchProjection } from '../lib/search.js';
import {
  AUDIENCE_SCOPES,
  assertAudience,
  assertNoScopeLeak,
  assertNoSideEffects,
  filterByAudience,
} from '../lib/privacy.js';
import { TUNING, applyFloor, buildIndex, expandQuery, groupKey, idf, queryStats, recencyFactor, scoreRecords, tokenize } from '../lib/relevance.js';
import { CEO_LANE, allocateChars, laneOf } from '../lib/partition.js';
import { SCALE_NOW, SCALE_TOPIC, flattenPartitions, fourCompanies, makeScaleCorpus } from './scale-corpus.js';
import { CONTEXT_SLICE_SCHEMA_VERSION, compileContext, fetchRecord, truncate } from '../lib/compile.js';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const LORO_DIR = path.resolve(HERE, '..');
const REPO_ROOT = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-public-fixture-'));
process.on('exit', () => fs.rmSync(REPO_ROOT, { recursive: true, force: true }));
fs.cpSync(path.join(HERE, 'fixtures/acme'), REPO_ROOT, { recursive: true });
for (const rel of ['loro/lib/store.js', 'loro/bin/loro-context.mjs']) {
  const target = path.join(REPO_ROOT, rel);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.writeFileSync(target, '// Synthetic product-layout marker.\n');
}
fs.mkdirSync(path.join(REPO_ROOT, 'loro/records'), { recursive: true });
for (let i = 0; i < 24; i += 1) {
  fs.writeFileSync(path.join(REPO_ROOT, `loro/records/pricing-${i}.md`),
    `---\nkind: decision\nscope: org-shared\n---\n\nManufacturing pricing policy ${i}: the fictional studio uses a written quote for each sensor installation.\n`);
}
fs.appendFileSync(path.join(REPO_ROOT, 'loro/memory/company.jsonl'), JSON.stringify({
  id: 'cited-pricing', kind: 'decision', scope: 'org-shared', text: 'Fictional seat pricing uses a written quote.',
  path: 'wiki/pricing.md', promotionMethod: 'fixture', promotionRef: 'wiki/pricing.md'
}) + '\n');
const FIX_ACME = path.join(HERE, 'fixtures', 'acme');
const FIX_EMPTY = path.join(HERE, 'fixtures', 'empty');
const FIX_CUSTOMER = path.join(HERE, 'fixtures', 'customer');
const CLI = path.join(LORO_DIR, 'bin', 'loro-context.mjs');
const WRITE_CLI = path.join(LORO_DIR, 'bin', 'loro-write.mjs');

const NOW = Date.parse('2026-08-24T12:00:00Z');

let passed = 0;
const failures = [];
function test(name, fn) {
  try {
    fn();
    passed += 1;
    console.log(`  ok  ${name}`);
  } catch (err) {
    failures.push({ name, err });
    console.log(`FAIL  ${name}\n      ${err.message}`);
  }
}

const acme = () => loadCorpus({ root: FIX_ACME, now: NOW });
const compileAcme = (opts) => compileContext({ corpus: acme(), now: NOW, ...opts });

const loadCorpusPageDirs = (root) => repoPaths(root).pageDirs.map((s) => s.dir);

function page(title, sections) {
  return [`# ${title}`, '', ...sections.flatMap(([h, b]) => [`## ${h}`, '', b, ''])].join('\n');
}

function withTempCorpus(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-corpus-'));
  const w = (rel, body) => {
    const p = path.join(dir, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, body);
  };
  w('ceo/pages/worldview.md', page('How I work', [
    ['How I decide', 'I decide slowly on things that are hard to reverse and fast on everything else. A decision I can undo in a week is not worth a meeting.'],
  ]));
  w('ceo/pages/private/health.md', page('Constraints', [
    ['Travel', 'No more than one long-haul trip a month, and never two weeks running. This is not negotiable and it is nobody else\'s business.'],
  ]));
  w('ceo/entities.json', JSON.stringify({
    schemaVersion: 1,
    version: '2026-08-26',
    entities: [{ canonical: 'Northwind Ceramics', type: 'company', aliases: ['Northwind'] }],
  }));
  w('companies/northwind/company.yaml', 'id: northwind\nname: Northwind Ceramics\nrole: owner\nstatus: active\n');
  w('companies/northwind/pages/strategy.md', page('Northwind strategy', [
    ['Direct only', 'We ship direct to independent studios and never through a distributor, because a distributor takes the relationship and then commoditizes the catalogue.'],
  ]));
  w('companies/northwind/evidence/meeting.md', page('Raw call note', [
    ['Transcript', 'EVIDENCE, not memory. If this ever appears in a compiled slice the evidence boundary has failed and raw material is being served as company truth.'],
  ]));
  // The same file at the two CEO-LEVEL paths, because the company path above is the happy case and
  // production does not use it: `evidenceRoot()` resolves to the unfiled branch whenever no company
  // is bound, which is the default. `ceo/unfiled/evidence/` is the path production wrote to before
  // 2026-09-17 and it sits INSIDE the `ceo/unfiled` record directory, which is walked recursively;
  // `ceo/evidence/unfiled/` is where it writes now, outside every allowlisted directory. Both must
  // stay out of the compiled corpus, and the first one only does because `neverWalk()` says so.
  w('ceo/unfiled/evidence/meetings/sess-1/transcript.md', page('Raw call note', [
    ['Transcript', 'EVIDENCE at the pre-2026-09-17 production path. A transcript inside a walked record directory, kept out by neverWalk() alone.'],
  ]));
  w('ceo/evidence/unfiled/meetings/sess-2/transcript.md', page('Raw call note', [
    ['Transcript', 'EVIDENCE at the current production path, outside every allowlisted page and record directory.'],
  ]));
  w('companies/northwind/inbox/dropped.md', page('Dropped note', [
    ['Untriaged', 'A drop in the inbox is transient by definition; the inbox is the only directory that is supposed to be empty, and it is never the record.'],
  ]));
  w('companies/northwind/mirrors/crm.md', page('CRM mirror', [
    ['Pointer', 'A mirror is a POINTER into an external store that keeps its own retention and egress policy. loro indexes it; loro never copies it.'],
  ]));
  try {
    return fn(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function withWritableCorpus(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-write-'));
  fs.mkdirSync(path.join(dir, 'ceo', 'records'), { recursive: true });
  fs.mkdirSync(path.join(dir, 'companies', 'northwind', 'records'), { recursive: true });
  const corpusRoot = resolveCorpusRoot({ corpus: dir, env: {} });
  try {
    return fn({
      dir,
      corpusRoot,
      corpus: () => loadCorpus({ ...corpusRoot, now: NOW }),
      read: (rel) => fs.readFileSync(path.join(dir, rel), 'utf8'),
      write: (rel, body) => {
        const f = path.join(dir, rel);
        fs.mkdirSync(path.dirname(f), { recursive: true });
        fs.writeFileSync(f, body);
      },
    });
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function writeCli(args, opts = {}) {
  try {
    const stdout = execFileSync(process.execPath, [WRITE_CLI, ...args], {
      encoding: 'utf8',
      input: opts.input || '',
      env: cleanEnv(),
      ...opts,
    });
    return { code: 0, stdout, stderr: '' };
  } catch (err) {
    return { code: err.status, stdout: err.stdout || '', stderr: err.stderr || '' };
  }
}

function cleanEnv() {
  const env = { ...process.env };
  delete env.LORO_CORPUS;
  delete env.LORO_ROOT;
  return env;
}

function cli(args, opts = {}) {
  try {
    const stdout = execFileSync(process.execPath, [CLI, ...args], { encoding: 'utf8', ...opts });
    return { code: 0, stdout, stderr: '' };
  } catch (err) {
    return { code: err.status, stdout: err.stdout || '', stderr: err.stderr || '' };
  }
}

test('record: a record with no declared scope is CEO-PRIVATE, never org-shared', () => {
  const rec = normalizeRecord({ id: 'x', title: 't', text: 'b' }, { now: NOW, source: 'memory' });
  assert.equal(rec.scope, 'ceo-private');
});

test('record: an unrecognized scope value is treated as CEO-PRIVATE, not passed through', () => {
  const rec = normalizeRecord({ id: 'x', title: 't', text: 'b', scope: 'public' }, { now: NOW, source: 'memory' });
  assert.equal(rec.scope, 'ceo-private');
});

test('record: an unrecognized kind degrades to passage — never promoted, never dropped', () => {
  const rec = normalizeRecord({ id: 'x', title: 't', text: 'b', kind: 'gospel' }, { now: NOW, source: 'memory' });
  assert.equal(rec.kind, 'passage');
});

test('record: supersededBy makes a record non-current so it can never read as truth', () => {
  assert.equal(deriveStatus({ supersededBy: 'other' }, NOW), 'superseded');
});

test('RichFinder: current memory retains citations and cannot cross a partition', () => withTempCorpus(dir => {
  const write = (rel, text) => {
    const file = path.join(dir, rel);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
  };
  const company = (id, status) => write(
    `companies/${id}/company.yaml`, `id: ${id}\nname: ${id}\nrole: owner\nstatus: ${status}\n`
  );
  const record = (id, file, fields) => write(
    `companies/${id}/records/${file}.md`, `---\nkind: decision\nscope: org-shared\n${fields}\n---\n\nBody for ${file}.\n`
  );
  company('rival', 'active');
  company('oldco', 'retired');
  record('northwind', 'old-rate', 'supersededBy: rec:companies/northwind/records/current-rate');
  record('northwind', 'current-rate', 'supersedes: rec:companies/northwind/records/old-rate\nvalidUntil: 2026-09-01');
  record('northwind', 'future-rate', 'validFrom: 2026-09-01');
  record('rival', 'stolen-company-label', 'company: northwind');
  record('rival', 'rival-rate', '');
  record('oldco', 'retired-rate', '');

  const grant = { audience: 'rich', companyIds: ['northwind', 'oldco'], includeCeo: false };
  const snapshot = loadSearchProjection({ corpusRoot: dir, grant, now: NOW });
  const current = snapshot.records.find(rec => rec.ref.endsWith('/current-rate'));
  assert.equal(current.supersedes, 'rec:companies/northwind/records/old-rate');
  assert.equal(current.provenance.path, 'companies/northwind/records/current-rate.md');
  assert.ok(snapshot.records.every(rec => rec.company === 'northwind'));
  assert.ok(!snapshot.records.some(rec => /old-rate|future-rate|stolen-company-label/.test(rec.ref)));

  const both = loadSearchProjection({ corpusRoot: dir, grant: { ...grant, companyIds: ['northwind', 'rival'] }, now: NOW });
  assert.ok(both.records.some(rec => rec.ref.endsWith('/rival-rate')));
  assert.ok(!both.records.some(rec => rec.ref.endsWith('/stolen-company-label')));
  const expired = loadSearchProjection({ corpusRoot: dir, grant, now: Date.parse('2026-09-02') });
  assert.ok(!expired.records.some(rec => rec.ref === current.ref));
  assert.deepEqual(loadSearchProjection({ corpusRoot: dir, grant: { ...grant, companyIds: [] }, now: NOW }).records, []);
  assert.throws(() => loadSearchProjection({ corpusRoot: dir, grant: { ...grant, audience: 'worker' }, now: NOW }), /explicit Rich audience/);
}));

test('record: a validUntil in the past makes a record expired', () => {
  assert.equal(deriveStatus({ validUntil: '2025-04-30T00:00:00Z' }, NOW), 'expired');
});

test('record: absence of a date is NOT staleness — an undated record stays current', () => {
  assert.equal(deriveStatus({}, NOW), 'current');
});

test('record: validation reports an id-less record instead of silently ranking it', () => {
  const rec = normalizeRecord({ title: 't', text: 'b' }, { now: NOW, source: 'memory' });
  assert.ok(validateRecord(rec).some((p) => /no id/.test(p)));
});

test('sources: a hard-rule heading infers a constraint, ordinary prose stays a passage', () => {
  assert.equal(inferKind('Hard rules — never violate these', 'body'), 'constraint');
  assert.equal(inferKind('Installation crews', 'two crews cover DACH'), 'passage');
});

test('sources: a heading naming a decision infers a decision', () => {
  assert.equal(inferKind('Naming decisions (recorded)', 'body'), 'decision');
});

test('sources: markdown splits into heading sections carrying the document title', () => {
  const sections = splitMarkdownSections('# Doc\n\nintro text here\n\n## First\n\nbody one\n\n## Second\n\nbody two');
  assert.deepEqual(sections.map((s) => s.heading), ['overview', 'First', 'Second']);
  assert.equal(sections[0].docTitle, 'Doc');
});

test('sources: a fenced code block never fabricates a heading section', () => {
  const sections = splitMarkdownSections('# D\n\n```\n## not-a-heading\n```\n\ntext body long enough');
  assert.deepEqual(sections.map((s) => s.heading), ['overview']);
});

test('sources: sibling markdown links become related refs (the one-hop link graph)', () => {
  assert.deepEqual(outboundLinks('see [pricing](pricing.md) and [ops](./operations.md#hours)'), [
    'wiki:pricing.md',
    'wiki:operations.md',
  ]);
});

test('sources: slugify produces a stable, ref-safe anchor', () => {
  assert.equal(slugify('How we talk about price'), 'how-we-talk-about-price');
});

test('pages: a customer install compiles the CEO second brain at engine/ceo-wiki/wiki', () => {
  const corpus = loadCorpus({ root: FIX_CUSTOMER, now: NOW });
  const fromSecondBrain = corpus.records.filter((r) => /engine\/ceo-wiki\/wiki\//.test(r.provenance.path));
  assert.ok(fromSecondBrain.length >= 3, `the second brain contributed ${fromSecondBrain.length} record(s)`);
  assert.ok(corpus.records.some((r) => r.id === 'wiki:engine/ceo-wiki/wiki/positioning.md#who-we-sell-to'));
});

test('pages: PROBE — the old single-wiki-directory behaviour finds NOTHING in that same install', () => {
  const corpus = loadCorpus({ root: FIX_CUSTOMER, now: NOW, wikiDir: path.join(FIX_CUSTOMER, 'wiki') });
  assert.equal(corpus.counts.total, 0, 'the pre-fix code path must be blind here, or the fix is untested');
});

test('pages: the legacy engine wiki directory is included in a repository layout', () => {
  const dirs = loadCorpusPageDirs(REPO_ROOT);
  assert.ok(
    dirs.includes(path.join(REPO_ROOT, 'engine', 'ceo-wiki', 'wiki')),
    `engine/ceo-wiki/wiki is not in the compiled prose directories: ${dirs.join(', ')}`,
  );
});

test('pages: an unfilled template section is NOT memory (scaffolding never compiles)', () => {
  const corpus = loadCorpus({ root: FIX_CUSTOMER, now: NOW });
  assert.deepEqual(corpus.records.filter((r) => /000_index/.test(r.id)).map((r) => r.id), []);
});

test('pages: PROBE — the excluded template sections are long enough to have compiled', () => {
  const md = fs.readFileSync(path.join(FIX_CUSTOMER, 'engine', 'ceo-wiki', 'wiki', '000_index.md'), 'utf8');
  const scaffolding = splitMarkdownSections(md).filter((s) => isCommentOnly(s.body));
  assert.ok(scaffolding.length >= 2, 'the fixture must contain comment-only sections');
  assert.ok(scaffolding.every((s) => s.body.length >= 60), 'each must clear the 60-char minimum on its own');
  assert.ok(!isCommentOnly('a real sentence the CEO wrote down'), 'prose must not read as scaffolding');
});

test('pages: every prose ref is unique across prose directories (a ref is never ambiguous)', () => {
  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  assert.deepEqual(corpus.problems.filter((p) => /duplicate record id/.test(p)), []);
});

test('pages: a page under pages/private/ is CEO-PRIVATE (the scope field prose otherwise lacks)', () => {
  const corpus = withTempCorpus((dir) => loadCorpus({ root: dir, layout: 'corpus', now: NOW }));
  const priv = corpus.byId.get('wiki:ceo/pages/private/health.md#travel');
  const shared = corpus.byId.get('wiki:ceo/pages/worldview.md#how-i-decide');
  assert.ok(priv, 'the private page must compile at all');
  assert.equal(priv.scope, 'ceo-private');
  assert.ok(shared, 'the ordinary page must compile');
  assert.equal(shared.scope, 'org-shared');
});

test('pages: PROBE — the same page outside private/ compiles org-shared', () => {
  const corpus = withTempCorpus((dir) => {
    fs.renameSync(
      path.join(dir, 'ceo', 'pages', 'private', 'health.md'),
      path.join(dir, 'ceo', 'pages', 'health.md'),
    );
    return loadCorpus({ root: dir, layout: 'corpus', now: NOW });
  });
  assert.equal(corpus.byId.get('wiki:ceo/pages/health.md#travel').scope, 'org-shared');
});

test('pages: a company partition contributes its own pages and records, tagged with the company', () => {
  const corpus = withTempCorpus((dir) => loadCorpus({ root: dir, layout: 'corpus', now: NOW }));
  assert.deepEqual(corpus.companies, ['northwind']);
  const page = corpus.byId.get('wiki:companies/northwind/pages/strategy.md#direct-only');
  assert.ok(page, 'the company page must compile');
  assert.equal(page.company, 'northwind');
});

test('pages: evidence, inbox and mirrors are NEVER compiled (evidence is not truth)', () => {
  const corpus = withTempCorpus((dir) => loadCorpus({ root: dir, layout: 'corpus', now: NOW }));
  const leaked = corpus.records.filter((r) => /\/(evidence|inbox|mirrors)\//.test(r.provenance.path || ''));
  assert.deepEqual(leaked.map((r) => r.id), []);
});

// The test above passed for the right reason only at the fixture's company path. This one pins the
// CEO-LEVEL paths, which is what production actually writes to, and it carries its own positive
// control: a negative assertion over a corpus that never received the files would pass for the
// wrong reason, so the fixture's own prose sections are asserted to compile first.
//
// Reproduced RED before `neverWalk()` gained `evidence`, with this fixture:
//   total records compiled: 1
//   LEAKED -> rec:ceo/unfiled/evidence/meetings/sess-1/transcript
//      path: ceo/unfiled/evidence/meetings/sess-1/transcript.md
//     scope: ceo-private | kind: passage
test('pages: a transcript at the CEO-LEVEL evidence path is NEVER compiled — pre-move and post-move', () => {
  const seen = withTempCorpus((dir) => {
    const corpus = loadCorpus({ root: dir, layout: 'corpus', now: NOW });
    return {
      paths: corpus.records.map((r) => r.provenance.path || ''),
      total: corpus.records.length,
      onDisk: [
        'ceo/unfiled/evidence/meetings/sess-1/transcript.md',
        'ceo/evidence/unfiled/meetings/sess-2/transcript.md',
      ].filter((rel) => fs.existsSync(path.join(dir, rel))),
    };
  });

  // POSITIVE CONTROL 1 — the files this test is about are really on disk in the fixture.
  assert.deepEqual(seen.onDisk, [
    'ceo/unfiled/evidence/meetings/sess-1/transcript.md',
    'ceo/evidence/unfiled/meetings/sess-2/transcript.md',
  ], 'the fixture must actually write both CEO-level transcripts, or this test proves nothing');
  // POSITIVE CONTROL 2 — the corpus compiled something, so an empty result is not the reason.
  assert.ok(seen.total > 0, 'the fixture corpus must compile SOME records');

  // ceo/unfiled/evidence/** — the pre-2026-09-17 path, inside a recursively-walked record
  // directory. Held out by `neverWalk()` and by nothing else.
  assert.deepEqual(seen.paths.filter((p) => p.startsWith('ceo/unfiled/evidence/')), []);
  // ceo/evidence/** — the current path, outside every allowlisted page and record directory.
  assert.deepEqual(seen.paths.filter((p) => p.startsWith('ceo/evidence/')), []);
});

test('corpus: a loro with no memory, no wiki and no entities loads as EMPTY, not as an error', () => {
  const corpus = loadCorpus({ root: FIX_EMPTY, now: NOW });
  assert.equal(corpus.counts.total, 0);
  assert.equal(corpus.records.length, 0);
});

test('corpus: typed memory records load with their DECLARED kind and scope', () => {
  const rec = acme().byId.get('mem:company:pricing-seat-based');
  assert.equal(rec.kind, 'decision');
  assert.equal(rec.kindInferred, false);
  assert.equal(rec.scope, 'org-shared');
});

test('corpus: wiki sections load with an INFERRED kind, always marked as inferred', () => {
  const rec = acme().byId.get('wiki:pricing.md#discount-authority');
  assert.ok(rec, 'expected a record for the discount-authority section');
  assert.equal(rec.kindInferred, true);
});

test('corpus: wiki/raw is EXCLUDED — raw evidence never compiles as company memory', () => {
  const refs = acme().records.map((r) => r.id);
  assert.ok(!refs.some((r) => r.includes('raw-call-note')), `raw evidence leaked into the corpus: ${refs.join(',')}`);
});

test('corpus: the fingerprint is stable across identical loads', () => {
  assert.equal(acme().fingerprint, acme().fingerprint);
});

test('corpus: the fingerprint CHANGES when a record changes (identity, not a timestamp)', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-fp-'));
  fs.mkdirSync(path.join(dir, 'loro', 'memory'), { recursive: true });
  const file = path.join(dir, 'loro', 'memory', 'm.jsonl');
  fs.writeFileSync(file, '{"id":"a","kind":"fact","title":"A","text":"one","scope":"org-shared"}\n');
  const before = loadCorpus({ root: dir, now: NOW }).fingerprint;
  fs.writeFileSync(file, '{"id":"a","kind":"fact","title":"A","text":"two","scope":"org-shared"}\n');
  const after = loadCorpus({ root: dir, now: NOW }).fingerprint;
  assert.notEqual(before, after);
  fs.rmSync(dir, { recursive: true, force: true });
});

test('corpus: a malformed jsonl line is REPORTED and skipped — one bad row never fails a compile', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-bad-'));
  fs.mkdirSync(path.join(dir, 'loro', 'memory'), { recursive: true });
  fs.writeFileSync(
    path.join(dir, 'loro', 'memory', 'm.jsonl'),
    '{"id":"a","kind":"fact","title":"A","text":"one","scope":"org-shared"}\n{ this is not json\n',
  );
  const corpus = loadCorpus({ root: dir, now: NOW });
  assert.equal(corpus.counts.total, 1);
  assert.ok(corpus.notes.some((n) => /malformed record/.test(n)));
  fs.rmSync(dir, { recursive: true, force: true });
});

test('corpus: resolveRoot finds the repo root from inside a subdirectory', () => {
  assert.equal(resolveRoot(path.join(FIX_ACME, 'wiki')), FIX_ACME);
});

test('root: an unconfigured corpus root is an ERROR — never a fallback to the binary\'s checkout', () => {
  assert.throws(() => resolveCorpusRoot({ env: {} }), /no corpus configured/);
});

test('root: PROBE — the same call with a root configured resolves cleanly', () => {
  const r = resolveCorpusRoot({ root: FIX_ACME, env: {} });
  assert.equal(r.root, FIX_ACME);
  assert.equal(r.layout, 'repo');
  assert.equal(r.rootSource, '--root');
});

test('root: the compiler itself refuses to compile without an explicit root (not just the CLI)', () => {
  assert.throws(() => compileContext({ topic: 'pricing', now: NOW, env: {} }), /no corpus configured/);
  assert.throws(() => fetchRecord({ ref: 'mem:company:x', now: NOW, env: {} }), /no corpus configured/);
});

test('root: a root that does not exist FAILS LOUDLY instead of compiling an empty corpus', () => {
  assert.throws(
    () => resolveCorpusRoot({ root: path.join(os.tmpdir(), 'loro-does-not-exist-9f3a'), env: {} }),
    /does not exist/,
  );
});

test('root: a corpus root with neither ceo/ nor companies/ is refused, not compiled as thin', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-notacorpus-'));
  try {
    assert.throws(() => resolveCorpusRoot({ corpus: dir, env: {} }), /is not a loro corpus/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('root: a corpus inside the RichOS product repo is REFUSED (the open-source boundary)', () => {

  assert.throws(() => resolveCorpusRoot({ corpus: path.join(REPO_ROOT, 'loro'), env: {} }), /product repo/);
});

test('root: PROBE — the same corpus outside the product repo is accepted', () => {
  const resolved = withTempCorpus((dir) => resolveCorpusRoot({ corpus: dir, env: {} }));
  assert.equal(resolved.layout, 'corpus');
  assert.deepEqual(resolved.companies, ['northwind']);
});

test('root: LORO_CORPUS and LORO_ROOT are honoured, and a flag outranks the environment', () => {
  const r = withTempCorpus((dir) => resolveCorpusRoot({ env: { LORO_CORPUS: dir } }));
  assert.equal(r.rootSource, 'LORO_CORPUS');
  assert.equal(resolveCorpusRoot({ env: { LORO_ROOT: FIX_ACME } }).rootSource, 'LORO_ROOT');
  assert.equal(resolveCorpusRoot({ root: FIX_ACME, env: { LORO_ROOT: FIX_EMPTY } }).root, FIX_ACME);
});

test('root: compiling the PRODUCT checkout says so in the slice — a dogfood corpus is never silent', () => {
  const corpus = loadCorpus({ ...resolveCorpusRoot({ root: REPO_ROOT, env: {} }), now: NOW });
  assert.equal(corpus.dogfood, true);
  assert.ok(corpus.notes.some((n) => /RichOS PRODUCT checkout/.test(n)), corpus.notes.join(' | '));
});

test('root: PROBE — a corpus that is not the product checkout carries no dogfood note', () => {
  const corpus = loadCorpus({ ...resolveCorpusRoot({ root: FIX_ACME, env: {} }), now: NOW });
  assert.equal(corpus.dogfood, false);
  assert.deepEqual(corpus.notes.filter((n) => /PRODUCT checkout/.test(n)), []);
});

test('root: a slice states which corpus it was compiled from (layout + how the root was given)', () => {
  const slice = compileContext({ root: FIX_ACME, now: NOW, topic: 'pricing', budgetChars: 1200, env: {} });
  assert.equal(slice.corpus.layout, 'repo');
  assert.equal(slice.corpus.rootSource, '--root');
});

test('cli: compile without any corpus root exits 2 and names the flags to set', () => {
  const r = cli(['compile', '--topic', 'pricing'], { env: cleanEnv() });
  assert.equal(r.code, 2);
  assert.ok(/no corpus configured/.test(r.stderr), r.stderr);
  assert.ok(/--corpus/.test(r.stderr) && /--root/.test(r.stderr), r.stderr);
});

test('cli: PROBE — the same invocation with --root exits 0', () => {
  const r = cli(['compile', '--topic', 'pricing', '--root', FIX_ACME, '--now', '2026-08-24T12:00:00Z'], {
    env: cleanEnv(),
  });
  assert.equal(r.code, 0, r.stderr);
});

test('cli: --corpus compiles a shipped-layout corpus end to end', () => {
  const out = withTempCorpus((dir) =>
    cli(['compile', '--topic', 'do we sell through a distributor', '--corpus', dir, '--now', '2026-08-24T12:00:00Z'], {
      env: cleanEnv(),
    }),
  );
  assert.equal(out.code, 0, out.stderr);
  const slice = JSON.parse(out.stdout);
  assert.equal(slice.corpus.layout, 'corpus');
  assert.ok(slice.items.some((i) => /companies\/northwind\/pages/.test(i.ref)), out.stdout);
});

test('privacy: an unknown audience THROWS rather than defaulting to the widest scope', () => {
  assert.throws(() => assertAudience('everyone'), /unknown audience/);
  assert.throws(() => assertAudience(undefined), /unknown audience/);
});

test('privacy: the worker audience is not permitted CEO-private scope at all', () => {
  assert.ok(!AUDIENCE_SCOPES.worker.includes('ceo-private'));
});

test('privacy: filtering removes CEO-private records for a worker and keeps them for Rich', () => {
  const records = acme().records;
  assert.ok(filterByAudience(records, 'rich').allowed.some((r) => r.scope === 'ceo-private'));
  assert.equal(filterByAudience(records, 'worker').allowed.filter((r) => r.scope === 'ceo-private').length, 0);
});

test('privacy: a compiled WORKER slice never contains a CEO-private record', () => {
  const slice = compileAcme({ topic: 'VP Sales pricing rollout', audience: 'worker', budgetChars: 4000 });
  assert.ok(!slice.items.some((i) => i.scope === 'ceo-private'));
  assert.ok(!slice.text.includes('privately doubts'));
});

test("privacy: the same CEO-private memory IS available to Rich's own slice", () => {
  const slice = compileAcme({ topic: 'VP Sales pricing rollout', audience: 'rich', budgetChars: 4000 });
  assert.ok(slice.items.some((i) => i.ref === 'mem:company:vp-sales-doubt'), 'Rich should see his own private memory');
});

test('privacy: withheld records are COUNTED in the slice, never silently dropped', () => {
  const slice = compileAcme({ topic: 'pricing', audience: 'worker', budgetChars: 4000 });
  assert.ok(slice.budget.withheldByScope >= 1);
  assert.ok(slice.notes.some((n) => /scope wall/.test(n)));
});

test('privacy: the finished slice is re-checked — a leaked scope throws instead of being emitted', () => {
  assert.throws(
    () => assertNoScopeLeak({ items: [{ ref: 'x', scope: 'ceo-private' }] }, 'worker'),
    /privacy invariant VIOLATED/,
  );
});

test('privacy: deep fetch enforces the SAME scope wall as compile (no ref-guessing around it)', () => {
  const corpus = acme();
  const asRich = fetchRecord({ corpus, ref: 'mem:company:vp-sales-doubt', audience: 'rich', now: NOW });
  const asWorker = fetchRecord({ corpus, ref: 'mem:company:vp-sales-doubt', audience: 'worker', now: NOW });
  assert.equal(asRich.found, true);
  assert.equal(asWorker.found, false);
  assert.equal(asWorker.denied, true);
});

function everyCompilerModule(dir = path.join(LORO_DIR, 'lib')) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...everyCompilerModule(full));
    else if (entry.name.endsWith('.js')) out.push(path.relative(LORO_DIR, full).split(path.sep).join('/'));
  }
  return out;
}

test('privacy: every compiler library module is READ-ONLY and makes no network call', () => {
  const modules = everyCompilerModule();
  const problems = [];
  for (const rel of modules) {
    problems.push(...assertNoSideEffects(fs.readFileSync(path.join(LORO_DIR, rel), 'utf8'), `loro/${rel}`));
  }
  assert.deepEqual(problems, []);

  for (const expected of ['lib/compile.js', 'lib/coverage.js', 'lib/layout.js', 'lib/partition.js', 'lib/privacy.js', 'lib/relevance.js', 'lib/sources.js', 'lib/store.js', 'lib/record.js', 'lib/frontmatter.js']) {
    assert.ok(modules.includes(expected), `the read-only scan did not cover ${expected} — it covered ${modules.join(', ')}`);
  }
});

test('privacy: the read-only scan reaches a module in a SUBDIRECTORY of lib', () => {

  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-scan-'));
  try {
    fs.mkdirSync(path.join(dir, 'lib', 'cache'), { recursive: true });
    fs.writeFileSync(path.join(dir, 'lib', 'clean.js'), 'export const a = 1;\n');
    fs.writeFileSync(path.join(dir, 'lib', 'cache', 'index.js'), "import fs from 'node:fs';\nfs.writeFileSync('x', 'y');\n");
    const found = everyCompilerModule(path.join(dir, 'lib')).map((f) => f.replace(/^.*lib\//, 'lib/'));
    const offenders = [];
    for (const rel of ['lib/clean.js', 'lib/cache/index.js']) {
      const full = path.join(dir, rel);
      offenders.push(...assertNoSideEffects(fs.readFileSync(full, 'utf8'), rel));
    }
    assert.ok(found.some((f) => /cache\/index\.js$/.test(f)), `a nested module was not enumerated: ${found.join(', ')}`);
    assert.ok(offenders.some((o) => /filesystem write/.test(o)), 'the scan did not flag a nested filesystem write');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('relevance: tokenizing keeps punctuated company terms (whisper.cpp) as one term', () => {
  assert.ok(tokenize('we benchmarked whisper.cpp today').includes('whisper.cpp'));
});

test('relevance: stopwords are dropped so they can never drive a match', () => {
  assert.deepEqual(tokenize('the and of pricing'), ['pricing']);
});

test('relevance: a term present in every record carries near-zero IDF (grep cannot do this)', () => {
  const index = buildIndex(acme().records);
  const common = idf('pricing', index);
  const rare = idf('halstead', index);
  assert.ok(rare > common, `expected rare term to outweigh common term (rare=${rare}, common=${common})`);
});

test("relevance: a topic dictated with a known ASR mangling still retrieves the entity's memory", () => {
  const corpus = acme();
  const query = expandQuery('what did we promise hall stead group', corpus.entities);
  assert.deepEqual(query.matchedEntities, ['Halstead Group']);
  const slice = compileContext({ corpus, now: NOW, topic: 'what did we promise hall stead group', budgetChars: 2000 });
  assert.ok(slice.items.some((i) => i.ref === 'mem:company:halstead-integration'));
});

test('relevance: a governing DECISION outranks a merely word-similar prose passage', () => {
  const slice = compileAcme({ topic: 'how is our pricing set', budgetChars: 4000 });
  const decisionAt = slice.items.findIndex((i) => i.ref === 'mem:company:pricing-seat-based');
  const passageAt = slice.items.findIndex((i) => i.provenance.source === 'wiki');
  assert.ok(decisionAt >= 0, 'the pricing decision must be in the slice');
  assert.ok(passageAt === -1 || decisionAt < passageAt, 'the decision must rank above wiki prose');
});

test('relevance: a SUPERSEDED decision is never returned — not ranked lower, absent', () => {
  const slice = compileAcme({ topic: 'pricing usage based per processed job', budgetChars: 4000 });
  assert.ok(!slice.items.some((i) => i.ref === 'mem:company:pricing-usage-based'));
  assert.ok(!slice.deepFetch.available.some((a) => a.ref === 'mem:company:pricing-usage-based'));
});

test('relevance: an EXPIRED record is never returned', () => {
  const slice = compileAcme({ topic: 'Hannover trade show booth', budgetChars: 4000 });
  assert.ok(!slice.items.some((i) => i.ref === 'mem:company:trade-show-booth'));
});

test('relevance: a record containing no query term at all is never a candidate (no padding)', () => {
  const corpus = acme();
  const index = buildIndex(corpus.records);
  const scored = scoreRecords({
    records: corpus.records,
    index,
    query: expandQuery('sensor supplier concentration', corpus.entities),
    now: NOW,
  });
  assert.ok(!scored.some((c) => c.ref === 'wiki:pricing.md#discount-authority'));
});

test('relevance: brevity below the length floor is not itself a relevance signal', () => {
  const mk = (id, text) =>
    normalizeRecord({ id, kind: 'fact', title: 'T', text, scope: 'org-shared', authority: 'self', confidence: 0.7 }, { now: NOW, source: 'memory' });
  const short = mk('short', 'zebra aa bb');
  const long = mk('long', `zebra ${Array.from({ length: 19 }, (_, i) => `w${i}`).join(' ')}`);
  const records = [short, long];
  const index = buildIndex(records);
  const scored = scoreRecords({ records, index, query: expandQuery('zebra', [], index), now: NOW });
  const byRef = Object.fromEntries(scored.map((c) => [c.ref, c.score]));
  assert.equal(byRef.short.toFixed(6), byRef.long.toFixed(6), 'a 3-token record must not outscore a 20-token one on the same term');
});

test('relevance: an entity alias that is AMBIENT in the corpus never expands the query', () => {
  const records = Array.from({ length: 8 }, (_, i) =>
    normalizeRecord({ id: `r${i}`, kind: 'fact', title: 'T', text: 'rich context about the company', scope: 'org-shared' }, { now: NOW, source: 'memory' }),
  );
  const index = buildIndex(records);
  const entities = [{ canonical: 'Rich Hanna', aliases: ['Rich'], mangled: [] }];
  assert.deepEqual(expandQuery('what did Rich decide', entities, index).matchedEntities, []);

  assert.deepEqual(
    expandQuery('what did Rich Hanna decide', entities, index).matchedEntities,
    ['Rich Hanna'],
  );
});

test('relevance: a promoted memory record is its own diversity group, a wiki section is not', () => {
  const corpus = acme();
  assert.equal(groupKey(corpus.byId.get('mem:company:pricing-seat-based')), 'mem:company:pricing-seat-based');
  assert.equal(groupKey(corpus.byId.get('wiki:pricing.md#discount-authority')), 'wiki/pricing.md');
});

test('relevance: no single wiki page may supply more than two items to a slice', () => {
  const slice = compileAcme({ topic: 'price discount packaging seat', budgetChars: 6000, maxItems: 3 });
  const perPage = new Map();
  for (const item of slice.items) {
    if (item.provenance.source !== 'wiki') continue;
    perPage.set(item.provenance.path, (perPage.get(item.provenance.path) || 0) + 1);
  }
  for (const [page, n] of perPage) assert.ok(n <= 2, `${page} supplied ${n} items`);
});

test('compile: the rendered slice NEVER exceeds the character budget, at any budget', () => {
  const corpus = acme();
  for (const budgetChars of [0, 1, 40, 120, 200, 400, 800, 1200, 2000, 5000]) {
    const slice = compileContext({ corpus, now: NOW, topic: 'pricing and customer commitments', budgetChars });
    assert.ok(
      slice.text.length <= budgetChars,
      `budget ${budgetChars} produced ${slice.text.length} chars`,
    );
    assert.equal(slice.budget.usedChars, slice.text.length);
  }
});

test('compile: a larger budget yields a SUPERSET of a smaller budget (monotone packing)', () => {
  const corpus = acme();
  const refsAt = (budgetChars) =>
    compileContext({ corpus, now: NOW, topic: 'pricing and customer commitments', budgetChars }).items.map((i) => i.ref);
  const small = refsAt(400);
  const medium = refsAt(1200);
  const large = refsAt(6000);
  for (const ref of small) assert.ok(medium.includes(ref), `${ref} vanished when the budget grew`);
  for (const ref of medium) assert.ok(large.includes(ref), `${ref} vanished when the budget grew`);
});

test('compile: the same corpus, topic and clock produce BYTE-IDENTICAL output (deterministic)', () => {
  const a = JSON.stringify(compileAcme({ topic: 'pricing strategy for mid-market', budgetChars: 1500 }));
  const b = JSON.stringify(compileAcme({ topic: 'pricing strategy for mid-market', budgetChars: 1500 }));
  assert.equal(a, b);
});

test('compile: a freshly loaded corpus compiles identically to a cached one (no load-order drift)', () => {

  const cached = loadCorpus({ root: FIX_ACME, now: NOW, layout: 'repo', rootSource: '--root' });
  const a = JSON.stringify(compileContext({ root: FIX_ACME, now: NOW, topic: 'onboarding', budgetChars: 1500 }));
  const b = JSON.stringify(compileContext({ corpus: cached, now: NOW, topic: 'onboarding', budgetChars: 1500 }));
  assert.equal(a, b);
});

test('compile: an EMPTY loro returns an honest thin slice that tells the reader not to assume', () => {
  const slice = compileContext({ root: FIX_EMPTY, now: NOW, topic: 'pricing', budgetChars: 1200 });
  assert.equal(slice.thin, true);
  assert.equal(slice.items.length, 0);
  assert.match(slice.text, /nothing recorded bears on/);
  assert.match(slice.text, /Do not assume company facts/);
  assert.ok(slice.notes.some((n) => /loro is EMPTY/.test(n)));
});

test('compile: a topic loro squarely covers is labelled DIRECT', () => {
  const slice = compileAcme({ topic: 'what did we promise hall stead group and when', budgetChars: 2000 });
  assert.equal(slice.coverage, 'direct');
  assert.match(slice.text, /^COMPANY MEMORY \(loro\) — bearing on:/);
});

test('compile: a topic loro can only answer ADJACENTLY says so in the injected text itself', () => {
  const slice = compileAcme({ topic: 'what is our refund policy for damaged sensor modules', budgetChars: 900 });
  assert.equal(slice.coverage, 'adjacent');
  assert.ok(slice.items.length > 0, 'the adjacent case still returns the nearest material');
  assert.match(slice.text, /nothing squarely covers/);
  assert.match(slice.text, /NOT as an answer/);
  assert.ok(slice.notes.some((n) => /ADJACENT ONLY/.test(n)));
});

test('compile: a question built mostly of terms loro has never heard is NEVER labelled direct', () => {
  const slice = compileAcme({ topic: 'Series B term sheet negotiation with a Patagonian underwriter', budgetChars: 900 });
  assert.equal(slice.coverage, 'adjacent');
  assert.match(slice.text, /nothing squarely covers/);
});

test('relevance: an unheard-of query term counts against coverage at half weight, not zero', () => {
  const records = [
    normalizeRecord({ id: 'a', kind: 'fact', title: 'T', text: 'pricing pricing pricing', scope: 'org-shared' }, { now: NOW, source: 'memory' }),
  ];
  const index = buildIndex(records);
  const known = queryStats(expandQuery('pricing', [], index), index);
  const withUnknown = queryStats(expandQuery('pricing zeppelin', [], index), index);
  assert.equal(known.unknownRatio, 0);
  assert.ok(withUnknown.unknownRatio > 0, 'an unheard-of term is reported');
  const expected = known.mass + TUNING.UNKNOWN_TERM_WEIGHT * idf('zeppelin', index);
  assert.ok(Math.abs(withUnknown.mass - expected) < 1e-9, `expected ${expected}, got ${withUnknown.mass}`);
});

test('compile: a topic loro knows nothing about returns thin, never a plausible-sounding filler', () => {
  const slice = compileAcme({ topic: 'zeppelin insurance underwriting in Patagonia', budgetChars: 1200 });
  assert.equal(slice.thin, true);
  assert.equal(slice.items.length, 0);
});

test('compile: a thin slice still fits its budget', () => {
  const slice = compileContext({ root: FIX_EMPTY, now: NOW, topic: 'pricing', budgetChars: 50 });
  assert.ok(slice.text.length <= 50);
});

test('compile: inferred kinds are flagged in the slice text and called out in the notes', () => {
  const slice = compileAcme({ topic: 'discount authority and support escalation', budgetChars: 4000 });
  assert.ok(slice.items.some((i) => i.kindInferred));
  assert.match(slice.text, /\?\]/);
  assert.ok(slice.notes.some((n) => /INFERRED from prose/.test(n)));
});

test('compile: relevant memory that did not fit is listed for on-demand deep fetch', () => {
  const slice = compileAcme({ topic: 'pricing', budgetChars: 300, maxItems: 8 });
  assert.ok(slice.deepFetch.available.length > 0);
  const included = new Set(slice.items.map((i) => i.ref));
  assert.ok(!slice.deepFetch.available.some((a) => included.has(a.ref)), 'deep-fetch must list what was NOT included');
});

test('compile: deep fetch returns the FULL untruncated record behind a truncated slice line', () => {
  const corpus = acme();
  const slice = compileContext({ corpus, now: NOW, topic: 'pricing', budgetChars: 400 });
  const truncatedItem = slice.items.find((i) => i.truncated);
  assert.ok(truncatedItem, 'expected at least one truncated item at a 400-char budget');
  const full = fetchRecord({ corpus, ref: truncatedItem.ref, audience: 'rich', now: NOW });
  assert.equal(full.found, true);
  assert.ok(full.record.text.length > 0);
});

test('compile: an unknown ref reports not-found rather than returning a near match', () => {
  const result = fetchRecord({ corpus: acme(), ref: 'mem:company:does-not-exist', audience: 'rich', now: NOW });
  assert.equal(result.found, false);
  assert.ok(!result.denied);
});

test('compile: the slice carries its contract version and the corpus fingerprint it came from', () => {
  const slice = compileAcme({ topic: 'pricing', budgetChars: 1200 });
  assert.equal(slice.schemaVersion, CONTEXT_SLICE_SCHEMA_VERSION);
  assert.match(slice.compiler, /^loro-context-compiler\//);
  assert.match(slice.corpus.fingerprint, /^sha256:[0-9a-f]{32}$/);
});

test('compile: truncation cuts on a word boundary and marks the cut', () => {
  assert.equal(truncate('alpha beta gamma', 12), 'alpha beta…');
  assert.equal(truncate('short', 40), 'short');
});

test('cli: compile emits a schema-stamped JSON slice and exits 0', () => {
  const r = cli(['compile', '--topic', 'pricing', '--root', FIX_ACME, '--now', '2026-08-24T12:00:00Z']);
  assert.equal(r.code, 0, r.stderr);
  const slice = JSON.parse(r.stdout);
  assert.equal(slice.schemaVersion, CONTEXT_SLICE_SCHEMA_VERSION);
  assert.ok(slice.items.length > 0);
});

test('cli: --format text emits exactly the injectable slice text and nothing else', () => {
  const args = ['compile', '--topic', 'pricing', '--root', FIX_ACME, '--now', '2026-08-24T12:00:00Z'];
  const asJson = JSON.parse(cli(args).stdout);
  const asText = cli([...args, '--format', 'text']).stdout;
  assert.equal(asText, `${asJson.text}\n`);
});

test('cli: --now makes two runs byte-identical (deterministic for the caller)', () => {
  const args = ['compile', '--topic', 'onboarding', '--root', FIX_ACME, '--now', '2026-08-24T12:00:00Z'];
  assert.equal(cli(args).stdout, cli(args).stdout);
});

test('cli: --budget-chars is honoured end-to-end through the CLI', () => {
  const r = cli(['compile', '--topic', 'pricing', '--root', FIX_ACME, '--now', '2026-08-24T12:00:00Z', '--budget-chars', '250']);
  const slice = JSON.parse(r.stdout);
  assert.ok(slice.text.length <= 250);
});

test('cli: an empty loro exits 0 with thin=true — thin is an answer, not a failure', () => {
  const r = cli(['compile', '--topic', 'pricing', '--root', FIX_EMPTY, '--now', '2026-08-24T12:00:00Z']);
  assert.equal(r.code, 0, r.stderr);
  assert.equal(JSON.parse(r.stdout).thin, true);
});

test('cli: a worker-audience compile never emits CEO-private memory', () => {
  const r = cli([
    'compile', '--topic', 'VP Sales pricing rollout', '--root', FIX_ACME,
    '--now', '2026-08-24T12:00:00Z', '--audience', 'worker', '--budget-chars', '4000',
  ]);
  assert.equal(r.code, 0, r.stderr);
  assert.ok(!r.stdout.includes('privately doubts'));
});

test('cli: an unknown audience is refused with a usage error, never silently widened', () => {
  const r = cli(['compile', '--topic', 'pricing', '--root', FIX_ACME, '--audience', 'everyone']);
  assert.notEqual(r.code, 0);
  assert.match(r.stderr, /unknown audience/);
});

test('cli: fetch of an unknown ref exits 3', () => {
  const r = cli(['fetch', '--ref', 'mem:company:nope', '--root', FIX_ACME, '--now', '2026-08-24T12:00:00Z']);
  assert.equal(r.code, 3);
});

test('cli: fetch of CEO-private memory as a worker exits 4 (refused by the scope wall)', () => {
  const r = cli([
    'fetch', '--ref', 'mem:company:vp-sales-doubt', '--root', FIX_ACME,
    '--now', '2026-08-24T12:00:00Z', '--audience', 'worker',
  ]);
  assert.equal(r.code, 4);
  assert.ok(!r.stdout.includes('privately doubts'));
});

test('cli: fetch of the same ref as Rich exits 0 and returns the record', () => {
  const r = cli([
    'fetch', '--ref', 'mem:company:vp-sales-doubt', '--root', FIX_ACME,
    '--now', '2026-08-24T12:00:00Z', '--audience', 'rich',
  ]);
  assert.equal(r.code, 0, r.stderr);
  assert.equal(JSON.parse(r.stdout).id, 'mem:company:vp-sales-doubt');
});

test('cli: compile without a topic is a usage error (a slice is always topical)', () => {
  const r = cli(['compile', '--root', FIX_ACME]);
  assert.equal(r.code, 2);
});

test('cli: corpus reports what loro actually holds, for a real repo root', () => {
  const r = cli(['corpus', '--root', REPO_ROOT, '--format', 'text']);
  assert.equal(r.code, 0, r.stderr);
  assert.match(r.stdout, /records: \d+/);
});

const proseRec = (p, anchor, title) =>
  normalizeRecord({ id: `wiki:${p}#${anchor}`, kindInferred: true, title, text: `Body of ${title}.`, path: p, anchor }, { now: NOW, source: 'wiki' });

const memRec = (id, p, anchor) =>
  normalizeRecord({ id: `mem:x:${id}`, kind: 'decision', scope: 'org-shared', title: id, text: id, path: p, anchor }, { now: NOW, source: 'memory' });

const COV_PAGE = [
  proseRec('wiki/a.md', 'overview', 'A — overview'),
  proseRec('wiki/a.md', 'the-ceo-s-ruling', 'A — the CEO’s ruling'),
  proseRec('wiki/a.md', 'delivery-warehouse-to-store-round-trip', 'A — delivery'),
  proseRec('wiki/b.md', 'pricing', 'B — pricing'),
];

test('coverage: a prose section with no promoted record is UNCOVERED, and the count is derived not typed', () => {
  const a = analyzeCoverage([...COV_PAGE, memRec('one', 'wiki/b.md', 'pricing')]);
  assert.equal(a.totals.sections, 4);
  assert.equal(a.totals.covered, 1);
  assert.deepEqual(a.uncovered, ['wiki/a.md#delivery-warehouse-to-store-round-trip', 'wiki/a.md#overview', 'wiki/a.md#the-ceo-s-ruling']);
  assert.deepEqual(a.citedBy.get('wiki/b.md#pricing'), ['mem:x:one']);
});

test('coverage: a JSONL row with no cited page is UNCITED, never a citation of its own store', () => {

  const selfPath = normalizeRecord(
    { id: 'mem:x:loose', kind: 'fact', scope: 'org-shared', title: 't', text: 't', path: 'loro/memory/x.jsonl' },
    { now: NOW, source: 'memory' },
  );
  assert.equal(citationOf(selfPath), null);
  const a = analyzeCoverage([...COV_PAGE, selfPath]);
  assert.deepEqual(a.uncited, ['mem:x:loose']);
  assert.equal(a.totals.danglingCitations, 0);
});

test('coverage: a citation whose anchor no longer exists is a DRIFTED ANCHOR, reported not ignored', () => {
  const a = analyzeCoverage([...COV_PAGE, memRec('stale', 'wiki/a.md', 'a-heading-that-was-deleted')]);
  assert.equal(a.totals.driftedAnchors, 1);
  assert.equal(a.totals.unreachableFiles, 0);
  assert.deepEqual(a.driftedAnchors[0].records, ['mem:x:stale']);
});

test('coverage: a citation naming a file outside the prose layer is UNREACHABLE, counted apart', () => {

  const a = analyzeCoverage([...COV_PAGE, memRec('offpage', 'README.md', 'the-concept-in-one-page')]);
  assert.equal(a.totals.unreachableFiles, 1);
  assert.equal(a.totals.driftedAnchors, 0);
  assert.match(relinkCandidates(a).unrepairable[0].why, /not in the compiled prose layer/);
});

test('coverage: a drifted anchor is repaired ONLY when the heading text is unchanged', () => {

  const punctuation = analyzeCoverage([...COV_PAGE, memRec('punct', 'wiki/a.md', 'the-ceos-ruling')]);
  assert.deepEqual(relinkCandidates(punctuation).repairable.map((r) => r.to), ['the-ceo-s-ruling']);

  const long = 'x'.repeat(70);
  const truncated = analyzeCoverage([
    proseRec('wiki/c.md', long, 'C — long'),
    memRec('trunc', 'wiki/c.md', `${long}-and-more`),
  ]);
  assert.deepEqual(relinkCandidates(truncated).repairable.map((r) => r.to), [long]);

  const renamed = analyzeCoverage([...COV_PAGE, memRec('renamed', 'wiki/a.md', 'delivery-expected-one-day-by-priority-truck')]);
  const r = relinkCandidates(renamed);
  assert.deepEqual(r.repairable, []);
  assert.match(r.unrepairable[0].why, /heading was rewritten or removed/);
});

test('coverage: an ambiguous anchor is never re-aimed silently', () => {
  const ambiguous = analyzeCoverage([
    proseRec('wiki/d.md', 'pricing-one', 'D — pricing one'),
    proseRec('wiki/d.md', 'pricing-one-two', 'D — pricing one two'),
    memRec('amb', 'wiki/d.md', 'pricingone'),
  ]);
  const r = relinkCandidates(ambiguous);
  assert.deepEqual(r.repairable, []);
  assert.match(r.unrepairable[0].why, /2 sections .* match this anchor/);

  const unambiguous = analyzeCoverage([proseRec('wiki/d.md', 'pricing-one', 'D'), memRec('amb', 'wiki/d.md', 'pricingone')]);
  assert.deepEqual(relinkCandidates(unambiguous).repairable.map((r2) => r2.to), ['pricing-one']);
});

test('coverage: the same records produce a byte-identical report (a coverage number has an identity)', () => {
  const recs = [...COV_PAGE, memRec('one', 'wiki/b.md', 'pricing'), memRec('stale', 'wiki/a.md', 'gone')];
  const a = JSON.stringify(analyzeCoverage(recs).totals) + analyzeCoverage(recs).uncovered.join('|');
  const b = JSON.stringify(analyzeCoverage(recs.slice().reverse()).totals) + analyzeCoverage(recs.slice().reverse()).uncovered.join('|');
  assert.equal(a, b);
});

test('coverage: a candidate declares NOTHING a promotion must adjudicate', () => {
  const c = candidateFor({ key: 'wiki/a.md#pricing', path: 'wiki/a.md', anchor: 'pricing', title: 'A — pricing', inferredKind: 'decision' }, 'body');
  assert.equal(c.candidate, true);

  assert.equal(c.kindGuess, 'decision');
  for (const f of ['kind', 'scope', 'authority', 'confidence', 'title', 'text']) {
    assert.equal(c[f], null, `a candidate pre-decided ${f}`);
    assert.ok(c.needs.includes(f));
  }
});

test('coverage: per-page rows surface a page at zero, which an aggregate hides', () => {
  const a = analyzeCoverage([...COV_PAGE, memRec('one', 'wiki/b.md', 'pricing')]);
  const rows = coverageByPage(a);
  assert.deepEqual(rows[0], { path: 'wiki/a.md', total: 3, covered: 0, uncovered: 3 });
  assert.deepEqual(rows[1], { path: 'wiki/b.md', total: 1, covered: 1, uncovered: 0 });
});

test('frontmatter: a record file parses into fields plus a body', () => {
  const doc = '---\nid: x\nkind: decision\ntags: [a, b]\nprovenance: { method: m, ref: null }\n---\n\nThe body.\n';
  const parsed = parseFrontMatter(doc);
  assert.equal(parsed.data.kind, 'decision');
  assert.deepEqual(parsed.data.tags, ['a', 'b']);
  assert.deepEqual(parsed.data.provenance, { method: 'm', ref: null });
  assert.equal(parsed.body.trim(), 'The body.');
});

test('frontmatter: a machine rewrite PRESERVES unknown fields, comments and order verbatim', () => {
  const doc = '---\nid: x\n# my own note\nmyField: keep-me\nkind: fact\n---\n\nBody.\n';
  const out = setFields(doc, { kind: 'decision' });
  assert.ok(out.includes('# my own note'), 'a comment must survive');
  assert.ok(out.includes('myField: keep-me'), 'an unknown field must survive');
  assert.ok(out.includes('kind: decision'));
  assert.ok(out.indexOf('myField') < out.indexOf('kind:'), 'key order must survive');
});

test('frontmatter: PROBE — a parse/reserialize round trip DOES lose them (so the test above bites)', () => {
  const doc = '---\nid: x\n# my own note\nmyField: keep-me\nkind: fact\n---\n\nBody.\n';
  const naive = Object.entries(parseFrontMatter(doc).data).map(([k, v]) => `${k}: ${v}`).join('\n');
  assert.ok(!naive.includes('# my own note'), 'the probe is wrong if a naive round trip keeps comments');
});

test('frontmatter: an unparsable line is REPORTED and kept, never silently dropped', () => {
  const doc = '---\nid: x\nthis line is not a mapping\nkind: fact\n---\n\nBody.\n';
  const parsed = parseFrontMatter(doc);
  assert.ok(parsed.problems.some((p) => /unparsable line/.test(p)));
  assert.equal(parsed.data.kind, 'fact');
  assert.ok(setFields(doc, { kind: 'decision' }).includes('this line is not a mapping'));
});

test('frontmatter: a fence a HUMAN broke is REPORTED, never mistaken for "he wrote no metadata"', () => {
  const good = '---\nid: x\nkind: decision\nscope: org-shared\nsupersededBy: rec:y\n---\n\nBody.\n';
  const damaged = {
    'a blank line pushed above the fence': '\n' + good,
    'a leading space': ' ' + good,
    'a trailing space (invisible)': good.replace(/^---\n/, '--- \n'),
    'a fourth hyphen': good.replace(/^---\n/, '----\n'),
    "TextEdit's smart dash": good.replace(/^---/, '—'),
    'a byte-order mark (invisible)': '﻿' + good,
    'the closing fence deleted': good.replace(/\n---\n\n/, '\n\n'),
  };
  for (const [what, doc] of Object.entries(damaged)) {
    const parsed = parseFrontMatter(doc);
    assert.equal(parsed.hasFrontMatter, false, `${what}: the parser is right to refuse this fence`);
    assert.equal(parsed.fenceProblems.length, 1, `${what} must be REPORTED, not silently ignored`);
    assert.match(parsed.fenceProblems[0], /front matter IGNORED/);
    assert.match(parsed.fenceProblems[0], /supersededBy/, `${what}: the report must name what was lost`);
    assert.match(parsed.fenceProblems[0], /Fix:/, `${what}: the report must say how to fix it`);
  }
});

test('frontmatter: PROBE — a file that never intended front matter is NOT reported as damage', () => {
  const innocent = {
    'a plain note': 'A note the CEO typed straight into a file with no metadata at all.\n',
    'an opening horizontal rule': '---\n\nA note that just happens to start with a rule.\n',
    'a setext heading': 'A title\n---\n\nThe note under it.\n',
    'empty': '',
  };
  for (const [what, doc] of Object.entries(innocent)) {
    assert.deepEqual(parseFrontMatter(doc).fenceProblems, [], `${what} must not be reported`);
  }
});

test('records: a broken fence reaches the SLICE as a problem, and the record is never dropped', () => {
  withWritableCorpus((ctx) => {

    ctx.write(
      'ceo/records/retracted.md',
      '\n---\nid: retracted\nkind: decision\nscope: org-shared\n' +
        'supersededBy: rec:ceo/records/replacement\n---\n\nWe are moving the office to Berlin.\n',
    );
    const corpus = ctx.corpus();
    const rec = corpus.byId.get('rec:ceo/records/retracted');
    assert.ok(rec, 'the record must survive — a hand edit never deletes memory');
    assert.equal(rec.supersededBy, null, 'the PROBE: the damage is real, the retraction WAS lost');
    assert.equal(rec.status, 'current', 'the PROBE: which is why a retracted belief reads as current');

    assert.equal(corpus.problems.filter((p) => /front matter IGNORED/.test(p)).length, 1);
    assert.match(corpus.problems.find((p) => /front matter IGNORED/.test(p)), /ceo\/records\/retracted\.md/);

    const slice = compileContext({ corpus, now: NOW, topic: 'moving the office to Berlin', budgetChars: 1200 });
    assert.ok(
      slice.notes.some((n) => /front matter IGNORED/.test(n)),
      'the damage must reach the compiled slice, not sit in a field nobody reads',
    );
  });
});

test('record: an AMBIGUOUS hand-typed date is REPORTED with BOTH readings, never silently resolved', () => {
  const problem = dateFieldProblem('observedAt', '01/09/2026');
  assert.ok(problem, 'an ambiguous date must never pass in silence — silence is what makes it wrong data');
  assert.match(problem, /AMBIGUOUS/);
  assert.match(problem, /2026-01-09/, 'the report must name the month-first reading');
  assert.match(problem, /2026-09-01/, 'the report must name the day-first reading');
  assert.match(problem, /month-first/, 'the report must say WHICH reading loro used');
  assert.match(problem, /Fix:/, 'the report must say how to fix it');
  assert.match(problem, /YYYY-MM-DD/, 'the fix is the ISO form, which has exactly one reading');

  for (const shape of ['01/09/2026', '01-09-2026', '01.09.2026', '1/9/26']) {
    assert.match(String(dateFieldProblem('observedAt', shape)), /AMBIGUOUS/, `${shape} has two readings`);
  }

  for (const field of ['occurredAt', 'validFrom', 'validUntil']) {
    assert.match(String(dateFieldProblem(field, '04/07/2026')), /AMBIGUOUS/, `${field} is read as a date`);
  }
});

test('record: PROBE — an unambiguous date is accepted SILENTLY, so the report can never be noise', () => {
  const silent = [
    '2026-09-01',
    '2026-08-30T00:00:00.000Z',
    '2026-09-01T12:36:12.546Z',
    '09/13/2026',
    '03/03/2026',
    '2026/09/01',
  ];
  for (const value of silent) {
    assert.equal(dateFieldProblem('observedAt', value), null, `${value} must pass in silence`);
  }

  for (const value of [undefined, null, '', '   ', []]) {
    assert.deepEqual(dateFieldProblems({ observedAt: value }), [], 'an absent date is not a problem');
  }
  assert.deepEqual(dateFieldProblems({ kind: 'decision', title: 'no dates here' }), []);
});

test('record: an UNREADABLE date is reported but still DEGRADES — never a crash, never a dropped record', () => {
  for (const value of ['13/09/2026', 'last Tuesday', '2026-13-45', 2026]) {
    const problem = dateFieldProblem('observedAt', value);
    assert.ok(problem, `${value} must be reported, not thrown away in silence`);
    assert.match(problem, /UNDATED/, 'the report must say what the degradation cost');
    assert.match(problem, /Fix:/);
  }

  const rec = normalizeRecord({ id: 'x', title: 'T', text: 'B', observedAt: 'last Tuesday' }, { now: NOW, source: 'records' });
  assert.equal(rec.status, 'current', 'an unreadable date NEVER expires or drops the record');
  assert.equal(recencyFactor(rec, NOW), 1, 'undated is not penalized — that behavior is right and stays');
});

test('records: an ambiguous hand-typed date reaches the SLICE as a problem, and the file is UNTOUCHED', () => {
  withWritableCorpus((ctx) => {
    const typed =
      '---\nid: berlin-office\nkind: decision\nscope: org-shared\nobservedAt: 01/09/2026\n---\n\n' +
      'We are moving the office to Berlin.\n';
    ctx.write('ceo/records/berlin-office.md', typed);
    const corpus = ctx.corpus();
    const rec = corpus.byId.get('rec:ceo/records/berlin-office');
    assert.ok(rec, 'the record must survive — a report never costs him a record');

    assert.equal(ctx.read('ceo/records/berlin-office.md'), typed, 'his file is read, never rewritten');
    assert.equal(rec.observedAt, '01/09/2026', 'the value is passed through exactly as he typed it');

    const found = corpus.problems.filter((p) => /AMBIGUOUS/.test(p));
    assert.equal(found.length, 1, 'exactly one report, on the problems channel');
    assert.match(found[0], /ceo\/records\/berlin-office\.md/, 'the report must name the FILE');

    const slice = compileContext({ corpus, now: NOW, topic: 'moving the office to Berlin', budgetChars: 1200 });
    assert.ok(
      slice.notes.some((n) => /AMBIGUOUS/.test(n)),
      'the ambiguity must reach the compiled slice, not sit in a field nobody reads',
    );
  });
});

test('records: PROBE — the same record dated in ISO compiles with NO problems at all', () => {
  withWritableCorpus((ctx) => {
    ctx.write(
      'ceo/records/berlin-office.md',
      '---\nid: berlin-office\nkind: decision\nscope: org-shared\nobservedAt: "2026-09-01"\n---\n\n' +
        'We are moving the office to Berlin.\n',
    );
    const corpus = ctx.corpus();
    assert.ok(corpus.byId.get('rec:ceo/records/berlin-office'));
    assert.deepEqual(corpus.problems, [], 'the ordinary case must be completely silent');
  });
});

test('writer: a record written by the writer is READ BACK by the compiler (the loop closes)', () => {
  withWritableCorpus((ctx) => {
    appendRecord({
      corpusRoot: ctx.corpusRoot, id: 'pricing-seat-based', kind: 'decision', scope: 'org-shared',
      title: 'Pricing moved to per-seat', body: 'Pricing moved to per-seat. Usage-based was rejected because buyers could not forecast it.',
      now: NOW,
    });
    const rec = ctx.corpus().byId.get('rec:ceo/records/pricing-seat-based');
    assert.ok(rec, 'the compiler must see what the writer wrote');
    assert.equal(rec.kind, 'decision');
    assert.equal(rec.kindInferred, false, 'a written record DECLARES its kind');
    assert.equal(rec.scope, 'org-shared');
    assert.equal(rec.provenance.method, 'explicit_ceo_instruction');
  });
});

test('writer: PROBE — the same corpus before any write holds no records at all', () => {
  withWritableCorpus((ctx) => assert.equal(ctx.corpus().counts.total, 0));
});

test('provenance: a record carries the ORIGIN it was written with all the way into a slice item', () => {
  // The writer has always written `--source-label` into front matter as `provenance.source`, and
  // the reader dropped it — so a record promoted from the CEO's Calendar arrived in a slice with
  // no way for a consumer to say where it came from. `origin` is that label; `source` stays the
  // loro family that read the record. Both are facts and neither substitutes for the other.
  withWritableCorpus((ctx) => {
    appendRecord({
      corpusRoot: ctx.corpusRoot, id: 'ws-google-calendar-pricing', kind: 'event', scope: 'org-shared',
      title: 'Q3 pricing review',
      body: 'Calendar entry for Tuesday, September 15, 2026 at 9:00 AM. The fictional studio walks its price ladder.',
      sourceLabel: 'workspace:google:calendar', method: 'rich_inferred', ref: 'google:calendar:evt_pricing',
      now: NOW,
    });
    const rec = ctx.corpus().byId.get('rec:ceo/records/ws-google-calendar-pricing');
    assert.ok(rec, 'the compiler must see what the writer wrote');
    assert.equal(rec.provenance.origin, 'workspace:google:calendar');
    assert.equal(rec.provenance.source, 'records', 'the loro family that read it is a different fact');
    assert.equal(rec.provenance.ref, 'google:calendar:evt_pricing');

    const slice = compileContext({ corpus: ctx.corpus(), now: NOW, topic: 'price ladder pricing review' });
    const item = slice.items.find((i) => i.ref === 'rec:ceo/records/ws-google-calendar-pricing');
    assert.ok(item, `the record must clear the floor for its own topic: ${JSON.stringify(slice.items)}`);
    assert.equal(item.provenance.origin, 'workspace:google:calendar');
  });
});

test('provenance: PROBE — origin is read, not invented: the writer default and a bare file differ', () => {
  withWritableCorpus((ctx) => {
    // The writer's own default for `--source-label` is "conversation" (writer/writer.js), so a
    // record the CEO dictated says so rather than saying nothing.
    appendRecord({
      corpusRoot: ctx.corpusRoot, id: 'no-origin', kind: 'decision', scope: 'org-shared',
      title: 'Written in conversation', body: 'The fictional studio decided this in a conversation, not from a source.',
      now: NOW,
    });
    assert.equal(ctx.corpus().byId.get('rec:ceo/records/no-origin').provenance.origin, 'conversation');
    // A hand-written file with no provenance block at all has no origin, and gets none invented.
    ctx.write('ceo/records/bare.md', '---\nid: bare\nkind: fact\nscope: org-shared\n---\n\nA fictional fact with no provenance block.\n');
    assert.equal(ctx.corpus().byId.get('rec:ceo/records/bare').provenance.origin, null);
  });
});

test('layout: the CEO layer is the directory `ceo/` — recognized, written to, and read back as `rec:ceo/`', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-ceo-layer-'));
  try {
    fs.mkdirSync(path.join(dir, 'ceo', 'records'), { recursive: true });

    const corpusRoot = resolveCorpusRoot({ corpus: dir, env: {} });
    appendRecord({
      corpusRoot, id: 'the-layer-is-named-ceo', kind: 'decision', scope: 'org-shared',
      title: 'The CEO layer', body: 'The layer holding the CEO\'s own synthesis is named for what it holds.',
      now: NOW,
    });

    assert.ok(fs.existsSync(path.join(dir, 'ceo', 'records', 'the-layer-is-named-ceo.md')),
      'the default partition must write to ceo/records/');
    assert.ok(!fs.existsSync(path.join(dir, 'person')), 'no person/ directory may be created');

    const corpus = loadCorpus({ ...corpusRoot, now: NOW });
    const rec = corpus.byId.get('rec:ceo/records/the-layer-is-named-ceo');
    assert.ok(rec, 'the compiler must read the record back under a rec:ceo/ ref');
    assert.equal(rec.company, null, 'the CEO layer is never a company lane');
    assert.equal(laneOf(rec), CEO_LANE, 'and its lane is the reserved CEO lane');
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('writer: append REFUSES to overwrite — a belief is superseded, never silently replaced', () => {
  withWritableCorpus((ctx) => {
    const args = { corpusRoot: ctx.corpusRoot, id: 'x', kind: 'fact', scope: 'org-shared', body: 'One.', now: NOW };
    appendRecord(args);
    assert.throws(() => appendRecord({ ...args, body: 'Two.' }), /already exists/);
    assert.ok(ctx.read('ceo/records/x.md').includes('One.'), 'the original body must be intact');
  });
});

test('writer: a record with no declared scope is CEO-PRIVATE on the way IN, not just on the way out', () => {
  withWritableCorpus((ctx) => {
    appendRecord({ corpusRoot: ctx.corpusRoot, id: 'hunch', kind: 'fact', body: 'A hunch about a hire.', now: NOW });
    assert.ok(ctx.read('ceo/records/hunch.md').includes('scope: ceo-private'));
    assert.equal(ctx.corpus().byId.get('rec:ceo/records/hunch').scope, 'ceo-private');
  });
});

test('writer: correct changes ONLY what it was asked to, and never the body', () => {
  withWritableCorpus((ctx) => {
    ctx.write('ceo/records/note.md', '---\nid: note\nkind: fact\nscope: org-shared\nmyField: keep-me\n---\n\nThe CEO wrote this sentence himself.\n');
    correctRecord({
      corpus: ctx.corpus(), corpusRoot: ctx.corpusRoot, ref: 'rec:ceo/records/note',
      kind: 'decision', why: 'it records a ruling, not an observation', now: NOW,
    });
    const after = ctx.read('ceo/records/note.md');
    assert.ok(after.includes('kind: decision'));
    assert.ok(after.includes('The CEO wrote this sentence himself.'), 'prose is his; metadata is ours');
    assert.ok(after.includes('myField: keep-me'));
    assert.ok(after.includes('correctionReason: '), 'a correction records WHY');
  });
});

test('writer: correct REFUSES without a reason — an unexplained edit is indistinguishable from a mistake', () => {
  withWritableCorpus((ctx) => {
    appendRecord({ corpusRoot: ctx.corpusRoot, id: 'y', kind: 'fact', scope: 'org-shared', body: 'A fact.', now: NOW });
    assert.throws(
      () => correctRecord({ corpus: ctx.corpus(), corpusRoot: ctx.corpusRoot, ref: 'rec:ceo/records/y', kind: 'decision', why: '', now: NOW }),
      /needs --why/,
    );
  });
});

test('writer: WIDENING a scope is refused without an explicit acknowledgement (the scope wall)', () => {
  withWritableCorpus((ctx) => {
    appendRecord({ corpusRoot: ctx.corpusRoot, id: 'doubt', kind: 'fact', body: 'I doubt the VP believes in this.', now: NOW });
    const base = { corpus: ctx.corpus(), corpusRoot: ctx.corpusRoot, ref: 'rec:ceo/records/doubt', scope: 'org-shared', why: 'x', now: NOW };
    assert.throws(() => correctRecord(base), /refusing to widen/);
    assert.equal(ctx.corpus().byId.get('rec:ceo/records/doubt').scope, 'ceo-private', 'still private after the refusal');
    correctRecord({ ...base, widenScope: true });
    assert.equal(ctx.corpus().byId.get('rec:ceo/records/doubt').scope, 'org-shared');
  });
});

test('writer: PROBE — narrowing a scope needs no acknowledgement', () => {
  withWritableCorpus((ctx) => {
    appendRecord({ corpusRoot: ctx.corpusRoot, id: 'z', kind: 'fact', scope: 'org-shared', body: 'A shared fact.', now: NOW });
    correctRecord({ corpus: ctx.corpus(), corpusRoot: ctx.corpusRoot, ref: 'rec:ceo/records/z', scope: 'ceo-private', why: 'it is not the org\'s business', now: NOW });
    assert.equal(ctx.corpus().byId.get('rec:ceo/records/z').scope, 'ceo-private');
    assert.ok(isWidening('ceo-private', 'org-shared') && !isWidening('org-shared', 'ceo-private'));
  });
});

test('write cli: supersession requires acknowledgement for every widening and leaves refused writes untouched', () => {
  for (const [from, to] of [['ceo-private', 'org-shared'], ['ceo-private', 'external'], ['org-shared', 'external']]) {
    withWritableCorpus((ctx) => {
      appendRecord({ corpusRoot: ctx.corpusRoot, id: 'secret', kind: 'fact', scope: from,
        body: 'Synthetic confidential acquisition budget is forty million.', now: NOW });
      const before = ctx.read('ceo/records/secret.md');
      const args = ['supersede', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/secret',
        '--id', 'replacement', '--kind', 'fact', '--scope', to, '--body',
        'Synthetic confidential acquisition budget is fifty million.', '--why', 'Budget correction', '--json'];
      for (const flags of [[], ['--dry-run']]) {
        const result = writeCli([...args, ...flags]);
        assert.equal(result.code, 5, result.stderr);
        assert.match(result.stderr, /refusing to widen/);
        assert.equal(ctx.read('ceo/records/secret.md'), before);
        assert.ok(!fs.existsSync(path.join(ctx.dir, 'ceo/records/replacement.md')));
      }
      if (from === 'ceo-private') {
        const slice = compileContext({ corpus: ctx.corpus(), topic: 'confidential acquisition budget', audience: 'worker', now: NOW });
        assert.deepEqual(slice.items, []);
      }
      const allowed = writeCli([...args, '--widen-scope']);
      assert.equal(allowed.code, 0, allowed.stderr);
      assert.equal(ctx.corpus().byId.get('rec:ceo/records/replacement').scope, to);
    });
  }
});

test('writer: supersede links BOTH directions and drops the old record out of live memory', () => {
  withWritableCorpus((ctx) => {
    appendRecord({ corpusRoot: ctx.corpusRoot, id: 'old-price', kind: 'decision', scope: 'org-shared', body: 'Pricing is per seat.', now: NOW });
    supersedeRecord({
      corpus: ctx.corpus(), corpusRoot: ctx.corpusRoot, ref: 'rec:ceo/records/old-price',
      id: 'new-price', kind: 'decision', scope: 'org-shared', body: 'Pricing is usage-based as of August.',
      why: 'we changed the model', now: NOW,
    });
    const corpus = ctx.corpus();
    const oldRec = corpus.byId.get('rec:ceo/records/old-price');
    const newRec = corpus.byId.get('rec:ceo/records/new-price');
    assert.equal(oldRec.supersededBy, 'rec:ceo/records/new-price');
    assert.equal(oldRec.status, 'superseded', 'a superseded record never reads as current');
    assert.equal(newRec.supersedes, 'rec:ceo/records/old-price');

    const fetched = fetchRecord({ corpus, ref: 'rec:ceo/records/old-price', audience: 'rich', now: NOW });
    assert.equal(fetched.found, true);
  });
});

test('writer: supersede preserves company, CEO and unfiled partitions and refuses an implicit move', () => {
  withWritableCorpus((ctx) => {
    for (const partition of ['northwind', 'ceo', 'unfiled']) {
      const old = appendRecord({ corpusRoot: ctx.corpusRoot, partition, id: `old-${partition}`,
        kind: 'fact', scope: 'org-shared', body: 'Fictional delivery uses the morning truck.', now: NOW });
      const options = { corpusRoot: ctx.corpusRoot, corpus: ctx.corpus(), ref: old.ref,
        id: `new-${partition}`, kind: 'fact', scope: 'org-shared', body: 'Fictional delivery uses the afternoon truck.', now: NOW };
      const before = fs.readFileSync(old.file, 'utf8');
      assert.throws(() => supersedeRecord({ ...options, partition: partition === 'ceo' ? 'northwind' : 'ceo' }),
        /partition.*separate/i);
      assert.equal(fs.readFileSync(old.file, 'utf8'), before);
      const replacement = supersedeRecord(options);
      assert.equal(path.dirname(replacement.file), path.dirname(old.file));
      const fresh = ctx.corpus();
      assert.equal(fresh.byId.get(replacement.ref).company, partition === 'northwind' ? 'northwind' : null);
      assert.equal(fresh.byId.get(old.ref).supersededBy, replacement.ref);
    }
  });
});

test('writer: supersede accepts an explicit equivalent company partition', () => {
  withWritableCorpus((ctx) => {
    const old = appendRecord({ corpusRoot: ctx.corpusRoot, partition: 'northwind', id: 'old-company',
      kind: 'fact', body: 'Fictional original fact.', now: NOW });
    const replacement = supersedeRecord({ corpusRoot: ctx.corpusRoot, corpus: ctx.corpus(), ref: old.ref,
      partition: 'companies/northwind', id: 'new-company', kind: 'fact', body: 'Fictional corrected fact.', now: NOW });
    assert.equal(replacement.ref, 'rec:companies/northwind/records/new-company');
  });
});

test('writer: supersede REFUSES an already-superseded record (no chains of dead records)', () => {
  withWritableCorpus((ctx) => {
    appendRecord({ corpusRoot: ctx.corpusRoot, id: 'a1', kind: 'fact', scope: 'org-shared', body: 'One.', now: NOW });
    const common = { corpusRoot: ctx.corpusRoot, ref: 'rec:ceo/records/a1', kind: 'fact', scope: 'org-shared', why: 'w', now: NOW };
    supersedeRecord({ ...common, corpus: ctx.corpus(), id: 'a2', body: 'Two.' });
    assert.throws(() => supersedeRecord({ ...common, corpus: ctx.corpus(), id: 'a3', body: 'Three.' }), /already superseded/);
  });
});

test('writer: a JSONL-promoted record is superseded by rewriting ONE line, byte-identical elsewhere', () => {
  const before =
    '{"id":"a","kind":"fact","title":"A","text":"one","scope":"org-shared"}\n' +
    '// a comment line the writer must not touch\n' +
    '{"id":"b","kind":"fact","title":"B","text":"two","scope":"org-shared"}\n';
  const after = setJsonlField(before, 'a', { supersededBy: 'rec:ceo/records/new' });
  const lines = after.split('\n');
  assert.ok(JSON.parse(lines[0]).supersededBy === 'rec:ceo/records/new');
  assert.equal(lines[1], '// a comment line the writer must not touch');
  assert.equal(lines[2], before.split('\n')[2], 'every other row must be byte-identical');
});

test('writer: a ref that names PROSE is refused with the file to open (the escape hatch)', () => {
  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  assert.throws(
    () => correctRecord({ corpus, corpusRoot: { root: REPO_ROOT, layout: 'repo' }, ref: 'wiki:pricing.md#discount-authority', why: 'w', now: NOW }),
    /PROSE section/,
  );
});

test('writer: an id that could escape its directory is refused before anything is written', () => {
  withWritableCorpus((ctx) => {
    for (const bad of ['../escape', 'Has Spaces', '/absolute', 'UPPER']) {
      assert.throws(
        () => appendRecord({ corpusRoot: ctx.corpusRoot, id: bad, kind: 'fact', scope: 'org-shared', body: 'x', now: NOW }),
        /not a usable record id/,
        `"${bad}" must be refused`,
      );
    }
  });
});

test('writer: an unknown company partition is refused and names the ones that exist', () => {
  withWritableCorpus((ctx) => {
    assert.throws(
      () => appendRecord({ corpusRoot: ctx.corpusRoot, partition: 'northwnid', id: 'x', kind: 'fact', scope: 'org-shared', body: 'x', now: NOW }),
      /no company "northwnid"/,
    );
    const out = appendRecord({ corpusRoot: ctx.corpusRoot, partition: 'northwind', id: 'x', kind: 'fact', scope: 'org-shared', body: 'A company fact.', now: NOW });
    assert.equal(out.ref, 'rec:companies/northwind/records/x');
    assert.equal(ctx.corpus().byId.get(out.ref).company, 'northwind');
  });
});

test('writer: filing never BLOCKS a write — unfiled is always available', () => {
  withWritableCorpus((ctx) => {
    const out = appendRecord({ corpusRoot: ctx.corpusRoot, partition: 'unfiled', id: 'stray', kind: 'fact', scope: 'org-shared', body: 'Something nobody could file.', now: NOW });
    assert.equal(out.ref, 'rec:ceo/unfiled/stray');
    assert.ok(ctx.corpus().byId.get('rec:ceo/unfiled/stray'));
  });
});

test('records: a hand-written file with NO front matter still compiles, private and untyped', () => {
  withWritableCorpus((ctx) => {
    ctx.write('ceo/records/scribble.md', 'A note the CEO typed straight into a file with no metadata at all.\n');
    const rec = ctx.corpus().byId.get('rec:ceo/records/scribble');
    assert.ok(rec, 'a file with no front matter must still be a record');
    assert.equal(rec.kind, 'passage');
    assert.equal(rec.scope, 'ceo-private', 'ambiguity resolves to the MORE private scope');
  });
});

test('records: the ref comes from the PATH, so it can never disagree with where the file lives', () => {
  withWritableCorpus((ctx) => {
    ctx.write('ceo/records/real-name.md', '---\nid: some-other-id\nkind: fact\nscope: org-shared\n---\n\nBody text here.\n');
    const corpus = ctx.corpus();
    assert.ok(corpus.byId.get('rec:ceo/records/real-name'));
    assert.ok(!corpus.byId.get('rec:ceo/records/some-other-id'));
    assert.ok(corpus.notes.some((n) => /disagrees with the filename/.test(n)), 'and the disagreement is REPORTED');
  });
});

test('privacy: the WRITER lives outside loro/lib, so the compiler read-only scan still covers it all', () => {
  const libFiles = fs.readdirSync(path.join(LORO_DIR, 'lib')).filter((f) => f.endsWith('.js'));
  assert.ok(libFiles.length >= 6, 'the scan must actually cover the compiler modules');
  assert.ok(!fs.existsSync(path.join(LORO_DIR, 'lib', 'writer.js')), 'the writer must not be a compiler module');
  assert.ok(fs.existsSync(path.join(LORO_DIR, 'writer', 'writer.js')));
  const problems = [];
  for (const file of libFiles) {
    problems.push(...assertNoSideEffects(fs.readFileSync(path.join(LORO_DIR, 'lib', file), 'utf8'), file));
  }
  assert.deepEqual(problems, []);
});

test('privacy: PROBE — the same scan flags the writer, so it is detecting writes and not passing blindly', () => {
  const src = fs.readFileSync(path.join(LORO_DIR, 'writer', 'writer.js'), 'utf8');
  assert.match(src, /import .*writePrivateFile.* from '\.\/storage\.js'/);
  const storage = fs.readFileSync(path.join(LORO_DIR, 'writer', 'storage.js'), 'utf8');
  const problems = assertNoSideEffects(storage, 'loro/writer/storage.js');
  assert.ok(problems.some((p) => /filesystem write/.test(p)), `expected the scan to flag a write: ${problems.join('; ')}`);
});

test('write: the AUTHORITY the reader ranks on can actually be written, and a typo is refused', () => {
  withWritableCorpus((ctx) => {
    const corpusRoot = resolveCorpusRoot({ corpus: ctx.dir, env: {} });

    appendRecord({ corpusRoot, now: NOW, id: 'said-by-him', kind: 'decision', scope: 'org-shared', authority: 'self', body: 'The launch deadline is the end of the quarter, and it does not move.' });
    const corpus = loadCorpus({ ...corpusRoot, now: NOW });
    assert.equal(corpus.byId.get('rec:ceo/records/said-by-him').authority, 'self');

    appendRecord({ corpusRoot, now: NOW, id: 'said-by-nobody', kind: 'fact', scope: 'org-shared', body: 'The office lease runs to the end of next year on the current terms.' });
    assert.ok(!/^authority:/m.test(ctx.read('ceo/records/said-by-nobody.md')), 'an omitted authority was written anyway');
    assert.equal(loadCorpus({ ...corpusRoot, now: NOW }).byId.get('rec:ceo/records/said-by-nobody').authority, 'unknown');

    assert.throws(
      () => appendRecord({ corpusRoot, now: NOW, id: 'mistyped', kind: 'fact', scope: 'org-shared', authority: 'ceo', body: 'A body long enough to be a real record body for the test.' }),
      /unknown authority "ceo"/,
    );
    assert.ok(!fs.existsSync(path.join(ctx.dir, 'ceo', 'records', 'mistyped.md')), 'a refused write left a file');
  });
});

test('privacy: the coverage WRITE half is outside loro/lib, and the scan proves it writes', () => {

  assert.ok(fs.existsSync(path.join(LORO_DIR, 'lib', 'coverage.js')), 'the pure analysis belongs in lib/');
  assert.ok(!fs.existsSync(path.join(LORO_DIR, 'lib', 'coverage-write.js')), 'a writer must never be a compiler module');
  assert.deepEqual(assertNoSideEffects(fs.readFileSync(path.join(LORO_DIR, 'lib', 'coverage.js'), 'utf8'), 'lib/coverage.js'), []);
  const writeSrc = fs.readFileSync(path.join(LORO_DIR, 'writer', 'coverage-write.js'), 'utf8');
  assert.match(writeSrc, /import .*writePrivateFile.* from '\.\/storage\.js'/);
  const storage = fs.readFileSync(path.join(LORO_DIR, 'writer', 'storage.js'), 'utf8');
  assert.ok(
    assertNoSideEffects(storage, 'writer/storage.js').some((p) => /filesystem write/.test(p)),
    'the coverage writer does not actually write — then the split is decorative',
  );
});

const COV_CLI = path.join(LORO_DIR, 'bin', 'loro-coverage.mjs');
function covCli(args, opts = {}) {
  try {
    const stdout = execFileSync(process.execPath, [COV_CLI, ...args], {
      encoding: 'utf8',
      env: { ...cleanEnv(), ...(opts.env || {}) },
    });
    return { code: 0, stdout, stderr: '' };
  } catch (err) {
    return { code: err.status, stdout: err.stdout || '', stderr: err.stderr || '' };
  }
}

function withCoverageRepo(fn) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-cov-'));
  const w = (rel, body) => {
    const p = path.join(dir, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, body);
  };
  const ctx = {
    dir,
    write: w,
    read: (rel) => fs.readFileSync(path.join(dir, rel), 'utf8'),
    rows: (arr) => w('loro/memory/company.jsonl', `${arr.map((o) => JSON.stringify(o)).join('\n')}\n`),
    analyze: () => analyzeCoverage(loadCorpus({ root: dir, now: NOW }).records),
    root: () => ({ root: dir, layout: 'repo' }),
  };

  fs.mkdirSync(path.join(dir, 'loro', 'memory'), { recursive: true });
  w('wiki/pricing.md', page('Pricing', [
    ['Per seat', 'Pricing moved to per-seat in March because usage-based billing made every invoice a negotiation with the customer.'],
    ['Discounts', 'We discount on term length only, never on seat count, so the list price stays the thing everybody quotes.'],
  ]));
  try {
    return fn(ctx);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test('slice: a record citing a section its page no longer has SAYS SO, unprompted', () => {
  withCoverageRepo((ctx) => {
    ctx.rows([{ id: 'a', kind: 'decision', scope: 'org-shared', title: 'Per-seat pricing', text: 'Pricing moved to per-seat and usage-based was rejected.', path: 'wiki/pricing.md', anchor: 'per-seat' }]);
    const topic = 'pricing moved to per-seat and usage-based was rejected';
    const clean = compileContext({ corpus: loadCorpus({ root: ctx.dir, now: NOW }), now: NOW, topic });
    assert.ok(clean.items.some((i) => i.ref === 'mem:company:a'), JSON.stringify(clean.items));
    assert.deepEqual(clean.notes.filter((n) => /no longer exists/.test(n)), [], 'a healthy citation produced a staleness note');

    ctx.write('wiki/pricing.md', page('Pricing', [
      ['Per user', 'Pricing moved to per-user in March because usage-based billing made every invoice a negotiation with the customer.'],
    ]));
    const stale = compileContext({ corpus: loadCorpus({ root: ctx.dir, now: NOW }), now: NOW, topic });
    assert.ok(stale.items.some((i) => i.ref === 'mem:company:a'), 'the record was suppressed rather than flagged');
    assert.ok(stale.notes.some((n) => /no longer exists/.test(n)), stale.notes.join('; '));
  });
});

test('coverage gate: a MISSING baseline is refused, never a silent pass over nothing', () => {
  withCoverageRepo((ctx) => {
    ctx.rows([{ id: 'a', kind: 'decision', scope: 'org-shared', title: 'Per-seat', text: 'Per-seat.', path: 'wiki/pricing.md', anchor: 'per-seat' }]);
    const r = covCli(['check', '--root', ctx.dir]);
    assert.equal(r.code, 2, r.stdout);
    assert.match(r.stderr, /Refusing to check against nothing/);

    covCli(['baseline', '--root', ctx.dir, '--write']);
    assert.equal(covCli(['check', '--root', ctx.dir]).code, 0);
  });
});

test('coverage gate: a NEWLY uncovered section fails the ratchet, and promoting a record clears it', () => {
  withCoverageRepo((ctx) => {
    ctx.rows([
      { id: 'a', kind: 'decision', scope: 'org-shared', title: 'Per-seat', text: 'Per-seat.', path: 'wiki/pricing.md', anchor: 'per-seat' },
      { id: 'b', kind: 'decision', scope: 'org-shared', title: 'Discounts', text: 'Term only.', path: 'wiki/pricing.md', anchor: 'discounts' },
    ]);
    covCli(['baseline', '--root', ctx.dir, '--write']);
    assert.equal(covCli(['check', '--root', ctx.dir]).code, 0);

    ctx.write('wiki/refunds.md', page('Refunds', [['The policy', 'We refund in full inside thirty days and never after, because a discretionary refund window becomes a negotiation.']]));
    const failed = covCli(['check', '--root', ctx.dir]);
    assert.equal(failed.code, 6, failed.stdout);
    assert.match(failed.stdout, /newly uncovered/);
    assert.match(failed.stdout, /wiki\/refunds\.md#the-policy/);

    ctx.rows([
      { id: 'a', kind: 'decision', scope: 'org-shared', title: 'Per-seat', text: 'Per-seat.', path: 'wiki/pricing.md', anchor: 'per-seat' },
      { id: 'b', kind: 'decision', scope: 'org-shared', title: 'Discounts', text: 'Term only.', path: 'wiki/pricing.md', anchor: 'discounts' },
      { id: 'c', kind: 'decision', scope: 'org-shared', title: 'Refunds', text: 'Thirty days, no discretion.', path: 'wiki/refunds.md', anchor: 'the-policy' },
    ]);
    assert.equal(covCli(['check', '--root', ctx.dir]).code, 0);
  });
});

test('coverage gate: a drifted citation FAILS and is never ratchetable', () => {
  withCoverageRepo((ctx) => {
    ctx.rows([{ id: 'a', kind: 'decision', scope: 'org-shared', title: 'Per-seat', text: 'x', path: 'wiki/pricing.md', anchor: 'per-seat' }]);
    covCli(['baseline', '--root', ctx.dir, '--write']);

    ctx.write('wiki/pricing.md', page('Pricing', [
      ['Per user', 'Pricing moved to per-user in March because usage-based billing made every invoice a negotiation with the customer.'],
      ['Discounts', 'We discount on term length only, never on seat count, so the list price stays the thing everybody quotes.'],
    ]));
    const r = covCli(['check', '--root', ctx.dir]);
    assert.equal(r.code, 6, r.stdout);
    assert.match(r.stdout, /no longer has/);

    covCli(['baseline', '--root', ctx.dir, '--write']);
    assert.equal(covCli(['check', '--root', ctx.dir]).code, 6);
  });
});

test('coverage gate: an unreachable file needs a WRITTEN reason — an empty one is not a declaration', () => {
  withCoverageRepo((ctx) => {
    ctx.rows([{ id: 'a', kind: 'decision', scope: 'org-shared', title: 'Concept', text: 'x', path: 'README.md', anchor: 'the-concept' }]);
    covCli(['baseline', '--root', ctx.dir, '--write']);
    const base = JSON.parse(ctx.read('loro/coverage-baseline.json'));

    assert.deepEqual(Object.keys(base.knownUnreachable), ['README.md']);
    assert.equal(base.knownUnreachable['README.md'], '');
    const r = covCli(['check', '--root', ctx.dir]);
    assert.equal(r.code, 6, r.stdout);
    assert.match(r.stdout, /no declared reason/);

    base.knownUnreachable['README.md'] = 'the root README is not prose-layer memory; pending a decision';
    ctx.write('loro/coverage-baseline.json', `${JSON.stringify(base, null, 2)}\n`);
    assert.equal(covCli(['check', '--root', ctx.dir]).code, 0);
  });
});

test('coverage baseline: a regeneration carries forward every declared reason, and is byte-stable', () => {
  withCoverageRepo((ctx) => {
    ctx.rows([{ id: 'a', kind: 'decision', scope: 'org-shared', title: 'Concept', text: 'x', path: 'README.md', anchor: 'the-concept' }]);
    covCli(['baseline', '--root', ctx.dir, '--write']);
    const base = JSON.parse(ctx.read('loro/coverage-baseline.json'));
    base.knownUnreachable['README.md'] = 'a reason somebody thought about';
    ctx.write('loro/coverage-baseline.json', `${JSON.stringify(base, null, 2)}\n`);
    covCli(['baseline', '--root', ctx.dir, '--write']);
    assert.equal(JSON.parse(ctx.read('loro/coverage-baseline.json')).knownUnreachable['README.md'], 'a reason somebody thought about');

    const before = ctx.read('loro/coverage-baseline.json');
    covCli(['baseline', '--root', ctx.dir, '--write']);
    assert.equal(ctx.read('loro/coverage-baseline.json'), before);
  });
});

test('coverage relink: --apply changes one JSONL line and no other byte; a dry run changes none', () => {
  withCoverageRepo((ctx) => {
    ctx.write('wiki/pricing.md', page('Pricing', [
      ["The CEO's ruling", 'Pricing moved to per-seat in March because usage-based billing made every invoice a negotiation with the customer.'],
    ]));

    ctx.rows([
      { id: 'a', kind: 'decision', scope: 'org-shared', title: 'Per-seat', text: 'x', path: 'wiki/pricing.md', anchor: 'the-ceos-ruling' },
      { id: 'untouched', kind: 'fact', scope: 'org-shared', title: 'Other', text: 'y' },
    ]);
    const before = ctx.read('loro/memory/company.jsonl');
    assert.equal(ctx.analyze().totals.driftedAnchors, 1);

    const dry = covCli(['relink', '--root', ctx.dir]);
    assert.match(dry.stdout, /WOULD RELINK 1 record/);
    assert.equal(ctx.read('loro/memory/company.jsonl'), before, 'a dry run wrote to the store');

    const applied = covCli(['relink', '--root', ctx.dir, '--apply']);
    assert.match(applied.stdout, /RELINKED 1 record/);
    const after = ctx.read('loro/memory/company.jsonl').split('\n');
    assert.equal(ctx.analyze().totals.driftedAnchors, 0);
    assert.equal(after[1], before.split('\n')[1], 'an unrelated row was rewritten');

    const row = JSON.parse(after[0]);
    assert.equal(row.anchor, 'the-ceo-s-ruling');
    for (const f of ['kind', 'scope', 'title', 'text', 'path']) {
      assert.equal(row[f], JSON.parse(before.split('\n')[0])[f], `relink changed ${f}`);
    }
  });
});

test('coverage relink: a citation whose heading was REWRITTEN is left alone for a person', () => {
  withCoverageRepo((ctx) => {
    ctx.rows([{ id: 'a', kind: 'decision', scope: 'org-shared', title: 'Perf', text: 'x', path: 'wiki/pricing.md', anchor: 'buyer-psychology' }]);
    const before = ctx.read('loro/memory/company.jsonl');
    const r = covCli(['relink', '--root', ctx.dir, '--apply']);
    assert.match(r.stdout, /no citation is mechanically repairable/);
    assert.match(r.stdout, /need a person/);
    assert.equal(ctx.read('loro/memory/company.jsonl'), before);
  });
});

test('coverage propose: --apply is REFUSED, so a candidate can never become a record by flag', () => {
  withCoverageRepo((ctx) => {
    const r = covCli(['propose', '--root', ctx.dir, '--apply']);
    assert.equal(r.code, 5, r.stdout);
    assert.match(r.stderr, /A candidate is not a record/);

    const ok = covCli(['propose', '--root', ctx.dir, '--limit', '1']);
    assert.equal(ok.code, 0, ok.stderr);
    const c = JSON.parse(ok.stdout.trim());
    assert.equal(c.candidate, true);
    assert.equal(c.kind, null);
  });
});

test('coverage: the corpus root is never inferred for coverage either', () => {
  const r = covCli(['report']);
  assert.equal(r.code, 2);
  assert.match(r.stderr, /no corpus configured/);
});

test('write cli: append then correct, end to end, with an explicit corpus', () => {
  withWritableCorpus((ctx) => {
    const a = writeCli(
      ['append', '--corpus', ctx.dir, '--id', 'seat-pricing', '--kind', 'decision', '--scope', 'org-shared',
        '--body-stdin', '--now', '2026-08-24T12:00:00Z'],
      { input: 'Pricing is per seat. We rejected usage-based.' },
    );
    assert.equal(a.code, 0, a.stderr);
    assert.ok(a.stdout.includes('rec:ceo/records/seat-pricing'));

    const c = writeCli(['correct', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/seat-pricing',
      '--confidence', '0.99', '--why', 'the CEO confirmed it', '--now', '2026-08-24T12:00:00Z']);
    assert.equal(c.code, 0, c.stderr);
    assert.ok(ctx.read('ceo/records/seat-pricing.md').includes('confidence: 0.99'));
  });
});

test('write cli: --dry-run prints the file and writes NOTHING', () => {
  withWritableCorpus((ctx) => {
    const r = writeCli(['append', '--corpus', ctx.dir, '--id', 'ghost', '--kind', 'fact', '--scope', 'org-shared',
      '--body', 'Nothing should land on disk.', '--dry-run', '--now', '2026-08-24T12:00:00Z']);
    assert.equal(r.code, 0, r.stderr);
    assert.ok(r.stdout.includes('WOULD WRITE'));
    assert.ok(!fs.existsSync(path.join(ctx.dir, 'ceo', 'records', 'ghost.md')));
  });
});

test('write cli: writing with NO corpus root configured exits 2, exactly like reading', () => {
  const r = writeCli(['append', '--id', 'x', '--kind', 'fact', '--body', 'x']);
  assert.equal(r.code, 2);
  assert.ok(/no corpus configured/.test(r.stderr), r.stderr);
});

test('write cli --json: append emits ONE parseable object carrying ref, file and text', () => {
  withWritableCorpus((ctx) => {
    const r = writeCli(
      ['append', '--corpus', ctx.dir, '--id', 'json-append', '--kind', 'decision', '--scope', 'org-shared',
        '--body-stdin', '--json', '--now', '2026-08-24T12:00:00Z'],
      { input: 'The machine door returns JSON.' },
    );
    assert.equal(r.code, 0, r.stderr);
    const out = JSON.parse(r.stdout);
    assert.equal(out.op, 'append');
    assert.equal(out.dryRun, false);
    assert.equal(out.ref, 'rec:ceo/records/json-append');
    assert.equal(out.file, path.join(ctx.dir, 'ceo', 'records', 'json-append.md'));

    assert.equal(out.text, fs.readFileSync(out.file, 'utf8'));
  });
});

test('write cli --json: a --dry-run says dryRun:true, carries the full text, and touches nothing', () => {
  withWritableCorpus((ctx) => {
    const r = writeCli(['append', '--corpus', ctx.dir, '--id', 'json-ghost', '--kind', 'fact', '--scope', 'org-shared',
      '--body', 'Nothing should land on disk.', '--json', '--dry-run', '--now', '2026-08-24T12:00:00Z']);
    assert.equal(r.code, 0, r.stderr);
    const out = JSON.parse(r.stdout);
    assert.equal(out.dryRun, true);
    assert.ok(out.text.includes('Nothing should land on disk.'), 'the PREVIEW is what the CEO is shown before confirming');
    assert.ok(!fs.existsSync(path.join(ctx.dir, 'ceo', 'records', 'json-ghost.md')));
  });
});

test('write cli --json: supersede names both refs, and correct names the fields it changed', () => {
  withWritableCorpus((ctx) => {
    writeCli(
      ['append', '--corpus', ctx.dir, '--id', 'json-old', '--kind', 'decision', '--scope', 'org-shared',
        '--body-stdin', '--now', '2026-08-24T12:00:00Z'],
      { input: 'The belief that turns out to be wrong.' },
    );
    const s = writeCli(
      ['supersede', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/json-old', '--id', 'json-new',
        '--kind', 'decision', '--scope', 'org-shared', '--body-stdin', '--why', 'the CEO said it never happened',
        '--json', '--now', '2026-08-24T12:00:00Z'],
      { input: 'What is actually true.' },
    );
    assert.equal(s.code, 0, s.stderr);
    const sup = JSON.parse(s.stdout);
    assert.equal(sup.op, 'supersede');
    assert.equal(sup.supersededRef, 'rec:ceo/records/json-old');
    assert.equal(sup.ref, 'rec:ceo/records/json-new');
    assert.ok(sup.oldFile.endsWith('json-old.md'), 'the OLD file is named too — nothing is deleted');

    const c = writeCli(['correct', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/json-new',
      '--confidence', '0.99', '--why', 'the CEO confirmed it', '--json', '--now', '2026-08-24T12:00:00Z']);
    assert.equal(c.code, 0, c.stderr);
    const cor = JSON.parse(c.stdout);
    assert.equal(cor.op, 'correct');
    assert.ok(cor.changed.includes('confidence'), `changed was ${JSON.stringify(cor.changed)}`);
  });
});

test('write cli --json: show returns the file behind a ref as data', () => {
  withWritableCorpus((ctx) => {
    writeCli(['append', '--corpus', ctx.dir, '--id', 'json-show', '--kind', 'fact', '--scope', 'org-shared',
      '--body', 'What does loro actually believe?', '--now', '2026-08-24T12:00:00Z']);
    const r = writeCli(['show', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/json-show', '--json']);
    assert.equal(r.code, 0, r.stderr);
    const out = JSON.parse(r.stdout);
    assert.equal(out.op, 'show');
    assert.equal(out.ref, 'rec:ceo/records/json-show');
    assert.ok(out.text.includes('What does loro actually believe?'));
  });
});

test('write cli --json: a REFUSAL still exits on its code and still writes stderr, not a JSON error', () => {

  withWritableCorpus((ctx) => {
    const r = writeCli(['correct', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/nope',
      '--kind', 'fact', '--why', 'w', '--json']);
    assert.equal(r.code, 3, r.stderr);
    assert.equal(r.stdout.trim(), '', 'stdout stays empty on a refusal — there is no half-object to parse');
    assert.ok(/no record with ref/.test(r.stderr));
  });
});

test('write cli --json: prose output is UNCHANGED when the flag is absent', () => {
  withWritableCorpus((ctx) => {
    const r = writeCli(['append', '--corpus', ctx.dir, '--id', 'json-absent', '--kind', 'fact', '--scope', 'org-shared',
      '--body', 'Prose by default.', '--now', '2026-08-24T12:00:00Z']);
    assert.equal(r.code, 0, r.stderr);
    assert.ok(r.stdout.startsWith('wrote rec:ceo/records/json-absent'), r.stdout);
    assert.throws(() => JSON.parse(r.stdout), 'the default output is prose, and must stay prose');
  });
});

test('write cli: correcting a ref loro does not hold exits 3 and changes nothing', () => {
  withWritableCorpus((ctx) => {
    const r = writeCli(['correct', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/nope', '--kind', 'fact', '--why', 'w']);
    assert.equal(r.code, 3, r.stderr);
    assert.ok(/no record with ref/.test(r.stderr));
  });
});

function tinyLaneRecords(n) {
  const text = (i) =>
    i === 0
      ? 'The deadline for the launch is the end of the quarter and the commitment is one pilot site.'
      :

        `Note ${i} records who attended, what was tabled, and which supplier quoted for item ${i} of the fit-out.`;
  return Array.from({ length: n }, (_, i) =>
    normalizeRecord(
      { id: `tiny-${i}`, kind: 'passage', scope: 'org-shared', title: `Note ${i}`, text: text(i) },
      { now: SCALE_NOW, source: 'records' },
    ),
  );
}

test("scale: a company's relevance floor is set by its OWN best hit, never by another company's", () => {
  const corpus = fourCompanies(5100);
  const youngIds = new Set(corpus.records.filter((r) => r.company === 'quarry').map((r) => r.id));
  const rankOf = (recs) => {
    const index = buildIndex(recs);
    const query = expandQuery(SCALE_TOPIC, [], index);
    return applyFloor(scoreRecords({ records: recs, index, query, now: SCALE_NOW }));
  };

  const flatSurvivors = rankOf(flattenPartitions(corpus).records).filter((c) => youngIds.has(c.ref)).length;

  const laneSurvivors = rankOf(corpus.records.filter((r) => r.company === 'quarry')).length;
  assert.equal(flatSurvivors, 0, 'the young company was expected to be crowded out by the flat floor');
  assert.ok(laneSurvivors > 0, `its own lane kept ${laneSurvivors} — expected more than zero`);
});

test('scale: a raw score is not comparable across index sizes, so an absolute floor cannot be', () => {

  const scoreAt = (n) => {
    const recs = tinyLaneRecords(n);
    const index = buildIndex(recs);
    const query = expandQuery(SCALE_TOPIC, [], index);
    const scored = scoreRecords({ records: recs, index, query, now: SCALE_NOW });
    return scored.find((c) => c.ref === 'tiny-0').score;
  };
  const small = scoreAt(1);
  const large = scoreAt(2000);
  assert.ok(large / small > 5, `the same record scored ${small.toFixed(3)} at n=1 and ${large.toFixed(3)} at n=2000`);
});

test("scale: the relevance floor can never delete a lane's own best candidate, at any lane size", () => {

  const emptiedByTheOldFloor = [];
  for (const n of [1, 2, 3, 8, 64, 500]) {
    const recs = tinyLaneRecords(n);
    const index = buildIndex(recs);
    const query = expandQuery(SCALE_TOPIC, [], index);
    const scored = scoreRecords({ records: recs, index, query, now: SCALE_NOW });
    assert.ok(scored.length > 0, `n=${n}: nothing scored at all`);
    const kept = applyFloor(scored);
    assert.equal(kept[0].ref, scored[0].ref, `n=${n}: the floor deleted the lane's own best candidate`);

    const preFix = scored.filter((c) => c.score >= Math.max(0.35, TUNING.FLOOR_FRACTION * scored[0].score));
    if (preFix.length === 0) emptiedByTheOldFloor.push(n);
  }
  assert.deepEqual(
    emptiedByTheOldFloor,
    [1],
    'the old absolute floor was expected to empty exactly the one-record lane, and nothing larger — ' +
      `it emptied [${emptiedByTheOldFloor.join(', ')}]. If that set changed, the measurement behind ` +
      'removing FLOOR_ABSOLUTE changed with it and needs re-stating, not re-baselining.',
  );
});

test('scale: a one-record company partition compiles its record instead of an honest-looking nothing', () => {
  const corpus = makeScaleCorpus({ companies: [{ id: 'newthing', records: 0, distractors: 1 }] });
  const slice = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 2000 });
  assert.equal(slice.thin, false, `a one-record lane compiled to nothing: ${slice.text}`);
  assert.equal(slice.items.length, 1);
  assert.equal(slice.items[0].company, 'newthing');
});

test("scale: a term is weighted against comparable material, not against another company's usage", () => {
  const corpus = fourCompanies(12000);
  const flatIndex = buildIndex(flattenPartitions(corpus).records);
  const laneIndex = buildIndex(corpus.records.filter((r) => r.company === 'quarry'));
  for (const term of ['deadline', 'commitment', 'pricing']) {
    const lane = idf(term, laneIndex);
    const flat = idf(term, flatIndex);
    const lost = (lane - flat) / lane;
    assert.ok(lost > 0.3, `${term}: flat IDF ${flat.toFixed(2)} vs lane ${lane.toFixed(2)} — expected >30% dilution`);
  }
});

test('scale: the youngest company reaches the slice on its own merit, not on its share of the mass', () => {
  for (const size of [1400, 5100, 12000]) {
    const corpus = fourCompanies(size);
    const youngIds = new Set(corpus.records.filter((r) => r.company === 'quarry').map((r) => r.id));
    const inFlat = compileContext({ corpus: flattenPartitions(corpus), now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000 })
      .items.filter((i) => youngIds.has(i.ref)).length;
    const inPart = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000 })
      .items.filter((i) => youngIds.has(i.ref)).length;
    assert.equal(inFlat, 0, `${size}: flat ranking was expected to lose the young company entirely`);
    assert.ok(inPart > 0, `${size}: partitioned ranking returned ${inPart} of its records`);
  }
});

test('scale: the coverage label does not degrade because a different company arrived', () => {

  const big = fourCompanies(12000);
  assert.equal(compileContext({ corpus: flattenPartitions(big), now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000 }).coverage, 'adjacent');
  assert.equal(compileContext({ corpus: big, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000 }).coverage, 'direct');

  const small = fourCompanies(700);
  assert.equal(compileContext({ corpus: flattenPartitions(small), now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000 }).coverage, 'direct');
});

test('scale: the CEO layer reaches a company slice even when nothing in it can win a slot', () => {

  for (const size of [1400, 5100, 12000]) {
    const corpus = fourCompanies(size, { typedCeo: false });
    const ceoIds = new Set(corpus.records.filter((r) => r.company === null).map((r) => r.id));
    const inFlat = compileContext({ corpus: flattenPartitions(corpus), now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000 })
      .items.filter((i) => ceoIds.has(i.ref)).length;
    const slice = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000 });
    const inPart = slice.items.filter((i) => ceoIds.has(i.ref)).length;
    assert.equal(inFlat, 0, `${size}: an un-promoted CEO layer was expected to lose every slot flat`);
    assert.ok(inPart > 0, `${size}: the reserved lane returned ${inPart} CEO-layer record(s)`);
    assert.ok(slice.budget.lanes.find((l) => l.isCeo).chars >= 200, 'the CEO lane lost its reserved floor');
  }
});

test('scale: narrowing with --company indexes one lane, not the whole corpus', () => {

  const corpus = fourCompanies(5100);
  const all = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000 });
  const one = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 8000, company: 'quarry' });
  assert.equal(all.request.companies.length, 4);
  assert.deepEqual(one.request.companies, ['quarry']);
  assert.ok(one.budget.itemsConsidered < all.budget.itemsConsidered / 2, 'narrowing did not reduce the candidate pool');
  assert.ok(one.items.every((i) => i.company === null || i.company === 'quarry'), 'a narrowed slice leaked another company');
});

test('partition: a corpus with NO company partitions compiles as one lane holding the whole budget', () => {

  const slice = compileAcme({ topic: 'pricing and customer commitments', budgetChars: 1500 });
  assert.equal(slice.budget.lanes.length, 1);
  assert.equal(slice.budget.lanes[0].isCeo, true);
  assert.equal(slice.budget.lanes[0].itemSlots, 8);
  assert.equal(slice.budget.lanes[0].chars, 1500 - slice.text.split('\n')[0].length);
  assert.deepEqual(slice.request.companies, []);
});

test('partition: an unknown --company id is REFUSED, never compiled as an empty lane', () => {
  const corpus = fourCompanies(700);
  assert.throws(
    () => compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, company: 'quary' }),
    /no such company partition "quary"/,
  );

  const ok = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, company: 'quarry' });
  assert.deepEqual(ok.request.companies, ['quarry']);
});

test("partition: a lane's character share is monotone in the budget, for every lane", () => {

  const lanes = [{ isCeo: true }, { isCeo: false }, { isCeo: false }, { isCeo: false }];
  let prev = allocateChars(0, lanes);
  for (let budget = 1; budget <= 4000; budget += 1) {
    const now = allocateChars(budget, lanes);
    for (let i = 0; i < lanes.length; i += 1) {
      assert.ok(now[i] >= prev[i], `lane ${i} shrank from ${prev[i]} to ${now[i]} when the budget grew to ${budget}`);
    }
    assert.ok(now.reduce((a, b) => a + b, 0) <= budget, `lanes over-allocated at budget ${budget}`);
    prev = now;
  }
});

test('partition: item slots do not depend on the character budget', () => {

  const corpus = fourCompanies(1400);
  const slots = (budgetChars) =>
    compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars }).budget.lanes.map((l) => l.itemSlots);
  assert.deepEqual(slots(600), slots(6000));
});

test('partition: a larger budget yields a SUPERSET of a smaller one ACROSS a split budget', () => {

  const corpus = fourCompanies(1400);
  for (const topic of [SCALE_TOPIC, 'the hiring decision and its risk to revenue']) {
    let prev = [];
    for (let budget = 100; budget <= 6000; budget += 50) {
      const refs = compileContext({ corpus, now: SCALE_NOW, topic, budgetChars: budget }).items.map((i) => i.ref);
      for (const ref of prev) assert.ok(refs.includes(ref), `${ref} vanished when the budget grew to ${budget}`);
      prev = refs;
    }
  }
});

test('partition: FLOW-FORWARD of an unspent lane share would break that superset guarantee', () => {

  const lanes = [{ isCeo: true }, { isCeo: false }];

  const laneItemLens = [[190, 190, 190], Array.from({ length: 10 }, () => 200)];
  const packFlowForward = (budget) => {
    const shares = allocateChars(budget, lanes);
    const taken = [];
    let carry = 0;
    for (let i = 0; i < lanes.length; i += 1) {
      let room = shares[i] + carry;
      let n = 0;
      for (const len of laneItemLens[i]) {
        if (len > room) break;
        room -= len;
        taken.push(`${i}:${n}`);
        n += 1;
      }
      carry = room;
    }
    return taken;
  };
  let violations = 0;
  let prev = [];
  for (let budget = 200; budget <= 2000; budget += 10) {
    const now = packFlowForward(budget);
    for (const ref of prev) if (!now.includes(ref)) violations += 1;
    prev = now;
  }
  assert.ok(violations > 0, 'flow-forward was expected to evict an item that fit at a smaller budget');
});

test('partition: a slice reports its unspent budget rather than quietly reusing it', () => {
  const corpus = fourCompanies(1400);
  const slice = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 2400 });
  assert.ok(slice.budget.unspentChars > 0);
  const laneUsed = slice.budget.lanes.reduce((s, l) => s + l.usedChars, 0);
  const heading = slice.text.split('\n')[0].length;
  assert.equal(slice.budget.unspentChars, 2400 - heading - laneUsed);
  assert.ok(slice.notes.some((n) => /unspent/.test(n)), 'the unspent budget was not surfaced in notes');
});

test("partition: a lane's rendered width comes from ITS share, not from the whole budget", () => {

  const corpus = fourCompanies(1400);
  const slice = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 1200 });
  assert.ok(slice.budget.lanes.length >= 4, 'expected a multi-lane corpus');
  const lanesWithItems = slice.budget.lanes.filter((l) => l.itemsIncluded > 0).length;
  assert.ok(lanesWithItems >= 4, `only ${lanesWithItems} of ${slice.budget.lanes.length} lanes fit anything`);
  assert.ok(slice.budget.unspentChars < 1200 * 0.35, `${slice.budget.unspentChars} chars unspent out of 1200`);
});

test('partition: the deep-fetch index draws from every lane, never sorted across incomparable scores', () => {
  const corpus = fourCompanies(5100);
  const slice = compileContext({ corpus, now: SCALE_NOW, topic: SCALE_TOPIC, budgetChars: 1200, maxAvailable: 8 });
  const companies = new Set(slice.deepFetch.available.map((a) => a.company));
  assert.ok(companies.size >= 3, `deepFetch drew from ${companies.size} lane(s): ${[...companies].join(', ')}`);
});

test("partition: a record's lane is its company field, and an unfiled record is CEO-layer", () => {
  assert.equal(laneOf({ company: 'northwind' }), 'northwind');
  assert.equal(laneOf({ company: null }), CEO_LANE);
  assert.equal(laneOf({}), CEO_LANE);
});

test('manifest: a RETIRED company leaves live slices and stays fetchable by name', () => {
  withWritableCorpus((ctx) => {
    ctx.write('companies/northwind/company.yaml', 'id: northwind\nname: Northwind\nrole: owner\nstatus: retired\n');
    ctx.write(
      'companies/northwind/records/direct-only.md',
      '---\nkind: decision\nscope: org-shared\n---\nWe ship direct to studios and never through a distributor.\n',
    );
    const corpusRoot = resolveCorpusRoot({ corpus: ctx.dir, env: {} });
    const corpus = loadCorpus({ ...corpusRoot, now: NOW });
    assert.deepEqual(corpus.retiredCompanies, ['northwind']);

    const live = compileContext({ corpus, now: NOW, topic: 'do we sell through a distributor' });
    assert.ok(!live.items.some((i) => i.company === 'northwind'), 'a retired company entered a live slice');
    assert.deepEqual(live.request.companies, []);

    const named = compileContext({ corpus, now: NOW, topic: 'do we sell through a distributor', company: 'northwind' });
    assert.ok(named.items.some((i) => i.company === 'northwind'), 'a retired company could not be read by name');

    ctx.write('companies/northwind/company.yaml', 'id: northwind\nname: Northwind\nrole: owner\nstatus: active\n');
    const reactivated = loadCorpus({ ...resolveCorpusRoot({ corpus: ctx.dir, env: {} }), now: NOW });
    assert.deepEqual(reactivated.retiredCompanies, []);
    assert.ok(
      compileContext({ corpus: reactivated, now: NOW, topic: 'do we sell through a distributor' })
        .items.some((i) => i.company === 'northwind'),
    );
  });
});

test('manifest: an unreadable manifest degrades to ACTIVE and says so, never loses the memory', () => {
  withWritableCorpus((ctx) => {
    ctx.write('companies/northwind/company.yaml', 'id: northwind\nstatus: mothballed\nthis line is not yaml at all\n');
    const m = readCompanyManifest(ctx.dir, 'northwind');
    assert.equal(m.status, 'active');
    assert.equal(m.dormant, false);
    assert.ok(m.problems.some((p) => /unknown status "mothballed"/.test(p)), m.problems.join('; '));
    assert.ok(m.problems.some((p) => /unparsable line/.test(p)), m.problems.join('; '));

    const corpus = loadCorpus({ ...resolveCorpusRoot({ corpus: ctx.dir, env: {} }), now: NOW });
    assert.ok(corpus.problems.some((p) => /mothballed/.test(p)), corpus.problems.join('; '));
  });
});

test('manifest: a manifest cannot rename the partition its own records live in', () => {
  withWritableCorpus((ctx) => {
    ctx.write('companies/northwind/company.yaml', 'id: southwind\nname: Northwind\nstatus: active\n');
    const m = readCompanyManifest(ctx.dir, 'northwind');
    assert.equal(m.id, 'northwind');
    assert.ok(m.problems.some((p) => /declares id "southwind"/.test(p)), m.problems.join('; '));
  });
});

test('create-company: a partition is created with every directory the layout specifies', () => {
  withWritableCorpus((ctx) => {
    const corpusRoot = resolveCorpusRoot({ corpus: ctx.dir, env: {} });
    const out = createCompany({ corpusRoot, id: 'partner-book', name: 'The partner book', role: 'co-founder', startedAt: '2025-01' });
    assert.equal(out.created, true);
    for (const d of ['pages', 'records', 'evidence', 'inbox', 'mirrors']) {
      assert.ok(fs.existsSync(path.join(ctx.dir, 'companies', 'partner-book', d)), `missing ${d}/`);
    }
    assert.equal(ctx.read('companies/partner-book/company.yaml'),
      'id: partner-book\nname: The partner book\nrole: co-founder\nstatus: active\nstartedAt: 2025-01\n');

    const corpus = loadCorpus({ ...resolveCorpusRoot({ corpus: ctx.dir, env: {} }), now: NOW });
    assert.ok(corpus.companies.includes('partner-book'));
    assert.equal(readCompanyManifest(ctx.dir, 'partner-book').role, 'co-founder');
  });
});

test('create-company: a second partition for a company that already exists is REFUSED', () => {
  withWritableCorpus((ctx) => {
    const corpusRoot = resolveCorpusRoot({ corpus: ctx.dir, env: {} });
    assert.throws(() => createCompany({ corpusRoot, id: 'northwind' }), /already exists/);

    assert.equal(createCompany({ corpusRoot, id: 'northwind-two' }).created, true);
  });
});

test('create-company: a mistyped role or status is REFUSED rather than stored for a later reader', () => {
  withWritableCorpus((ctx) => {
    const corpusRoot = resolveCorpusRoot({ corpus: ctx.dir, env: {} });
    assert.throws(() => createCompany({ corpusRoot, id: 'a-thing', role: 'cofounder' }), /is not a company role/);
    assert.throws(() => createCompany({ corpusRoot, id: 'a-thing', status: 'paused' }), /is not a company status/);
    assert.throws(() => createCompany({ corpusRoot, id: '../escape' }), /not a usable record id/);
    assert.ok(!fs.existsSync(path.join(ctx.dir, 'companies', 'a-thing')), 'a refused create left a directory behind');

    assert.equal(createCompany({ corpusRoot, id: 'a-thing', role: 'co-founder' }).created, true);
  });
});

test('create-company: --dry-run shows the manifest and touches NOTHING', () => {
  withWritableCorpus((ctx) => {
    const corpusRoot = resolveCorpusRoot({ corpus: ctx.dir, env: {} });
    const out = createCompany({ corpusRoot, id: 'quiet', dryRun: true });
    assert.equal(out.created, false);
    assert.ok(/id: quiet/.test(out.manifest));
    assert.ok(!fs.existsSync(path.join(ctx.dir, 'companies', 'quiet')));
  });
});

function withDogfoodRoot(fn, opts = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'loro-dogfood-'));
  const write = (rel, body) => {
    const f = path.join(dir, rel);
    fs.mkdirSync(path.dirname(f), { recursive: true });
    fs.writeFileSync(f, body);
  };
  write('wiki/pricing.md', '# Pricing\n\n## Seat pricing\n\nThe seat price is 40 dollars a month and has been since the launch of the product.\n');
  write('loro/records/a-belief.md', '---\nkind: decision\nscope: org-shared\n---\n\nThe launch is in March, decided by the CEO after the two-week slip.\n');
  if (opts.provision !== false) {
    fs.mkdirSync(path.join(dir, 'ceo', 'records'), { recursive: true });
    fs.mkdirSync(path.join(dir, 'ceo', 'unfiled'), { recursive: true });
  }
  try {
    const corpusRoot = resolveCorpusRoot({ root: dir, env: {} });
    return fn({
      dir,
      corpusRoot,
      write,
      reroot: () => resolveCorpusRoot({ root: dir, env: {} }),
      corpus: () => loadCorpus({ ...resolveCorpusRoot({ root: dir, env: {} }), now: NOW }),
    });
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

test('dogfood layout: an UNPROVISIONED checkout is unchanged — one records dir, no partitions', () => {
  withDogfoodRoot((ctx) => {
    const corpus = ctx.corpus();
    assert.deepEqual(corpus.companies, [], 'a checkout with no companies/ tree has no partitions');
    assert.ok(corpus.records.every((r) => r.company === null), 'every record is CEO-layer');
    assert.throws(
      () => resolvePartition(ctx.corpusRoot, 'northwind'),
      /not provisioned/,
      'and the writer says so instead of inventing a directory',
    );
    assert.equal(resolvePartition(ctx.corpusRoot, 'ceo').dir, path.join(ctx.dir, 'loro', 'records'));
  }, { provision: false });
});

test('dogfood layout: create-company works on a repo root, and a fresh compile sees the lane', () => {
  withDogfoodRoot((ctx) => {
    const out = createCompany({ corpusRoot: ctx.corpusRoot, id: 'northwind', name: 'Northwind' });
    assert.equal(out.created, true);
    assert.ok(fs.existsSync(path.join(ctx.dir, 'companies', 'northwind', 'company.yaml')));
    const corpus = ctx.corpus();
    assert.deepEqual(corpus.companies, ['northwind']);

    const target = resolvePartition(ctx.reroot(), 'northwind');
    assert.equal(target.company, 'northwind');
    assert.equal(target.dir, path.join(ctx.dir, 'companies', 'northwind', 'records'));
  });
});

test('dogfood layout: `pages:` in a manifest gives the repo\'s own prose a home, and refs do NOT move', () => {
  withDogfoodRoot((ctx) => {
    const before = ctx.corpus().records.filter((r) => r.provenance.source === 'wiki').map((r) => r.id);
    createCompany({ corpusRoot: ctx.corpusRoot, id: 'northwind', name: 'Northwind' });
    ctx.write('companies/northwind/company.yaml', 'id: northwind\nname: Northwind\nrole: owner\nstatus: active\npages: [wiki]\n');
    const after = ctx.corpus().records.filter((r) => r.provenance.source === 'wiki');
    assert.deepEqual(after.map((r) => r.id), before, 'a ref is a stable handle — binding must not move one');
    assert.ok(after.length > 0);
    assert.ok(after.every((r) => r.company === 'northwind'), 'the bound prose is now that company\'s memory');
    assert.ok(
      after.every((r) => r.provenance.path.startsWith('wiki/')),
      'and the file did not move — declaring beats moving, because the path is cited from elsewhere',
    );
  });
});

test('dogfood layout: a bound company lane is EXCLUDED from another company\'s compile', () => {
  withDogfoodRoot((ctx) => {
    createCompany({ corpusRoot: ctx.corpusRoot, id: 'northwind', name: 'Northwind' });
    createCompany({ corpusRoot: ctx.reroot(), id: 'contoso', name: 'Contoso' });
    ctx.write('companies/northwind/company.yaml', 'id: northwind\nname: Northwind\nrole: owner\nstatus: active\npages: [wiki]\n');
    const opts = { corpus: ctx.corpus(), topic: 'seat pricing', audience: 'rich', now: NOW, budgetChars: 2000 };
    const mine = compileContext({ ...opts, company: 'northwind' });
    assert.ok(mine.items.some((i) => i.company === 'northwind'), 'its own company reads it');
    const theirs = compileContext({ ...opts, company: 'contoso' });
    assert.ok(
      theirs.items.every((i) => i.company !== 'northwind'),
      'and a DIFFERENT company never does — this is the measured defect, refused',
    );
  });
});

test('dogfood layout: two partitions claiming one prose directory is REPORTED, and the first wins', () => {
  withDogfoodRoot((ctx) => {
    createCompany({ corpusRoot: ctx.corpusRoot, id: 'aaa', name: 'Aaa' });
    createCompany({ corpusRoot: ctx.reroot(), id: 'bbb', name: 'Bbb' });
    ctx.write('companies/aaa/company.yaml', 'id: aaa\nname: Aaa\nstatus: active\npages: [wiki]\n');
    ctx.write('companies/bbb/company.yaml', 'id: bbb\nname: Bbb\nstatus: active\npages: [wiki]\n');
    const corpus = ctx.corpus();
    assert.ok(
      corpus.problems.some((p) => /already claimed by "aaa"/.test(p)),
      `the collision must be reported, got: ${JSON.stringify(corpus.problems)}`,
    );
    const wiki = corpus.records.filter((r) => r.provenance.source === 'wiki');
    assert.ok(wiki.every((r) => r.company === 'aaa'), 'one home company, deterministically the first');
  });
});

test('dogfood layout: a pages claim that escapes the corpus is REFUSED, not obeyed', () => {
  withDogfoodRoot((ctx) => {
    createCompany({ corpusRoot: ctx.corpusRoot, id: 'aaa', name: 'Aaa' });
    ctx.write('companies/aaa/company.yaml', 'id: aaa\nname: Aaa\nstatus: active\npages: [../elsewhere]\n');
    const corpus = ctx.corpus();
    assert.ok(corpus.problems.some((p) => /not a relative path inside the/.test(p)), JSON.stringify(corpus.problems));
    assert.ok(corpus.records.filter((r) => r.provenance.source === 'wiki').every((r) => r.company === null));
  });
});

test('dogfood layout: ceo/entities.json is the vocabulary once it exists; loro/entities.json is the fallback', () => {
  withDogfoodRoot((ctx) => {
    ctx.write('loro/entities.json', JSON.stringify({ version: 'old', entities: [{ canonical: 'Legacy Co', type: 'company' }] }));
    assert.ok(ctx.corpus().byId.has('entity:legacy-co'), 'an unprovisioned checkout still reads loro/entities.json');
    ctx.write('ceo/entities.json', JSON.stringify({ version: 'new', entities: [{ canonical: 'Northwind', type: 'company' }] }));
    const corpus = ctx.corpus();
    assert.equal(corpus.entitiesVersion, 'new', 'the CEO layer owns the ONE vocabulary once it exists');
    assert.ok(!corpus.byId.has('entity:legacy-co'));
  });
});

test('write cli: create-company writes a partition a fresh compile can immediately use', () => {
  withWritableCorpus((ctx) => {
    const r = writeCli(['create-company', '--corpus', ctx.dir, '--id', 'partner-book', '--name', 'The partner book']);
    assert.equal(r.code, 0, r.stderr);
    assert.ok(/created company partition "partner-book"/.test(r.stdout), r.stdout);
    const w = writeCli([
      'append', '--corpus', ctx.dir, '--id', 'first-belief', '--kind', 'decision', '--scope', 'org-shared',
      '--partition', 'partner-book', '--body', 'The launch deadline for the partner book is the end of the quarter.',
    ]);
    assert.equal(w.code, 0, w.stderr);
    const corpus = loadCorpus({ ...resolveCorpusRoot({ corpus: ctx.dir, env: {} }), now: NOW });
    const slice = compileContext({ corpus, now: NOW, topic: 'the launch deadline for the partner book' });
    assert.ok(slice.items.some((i) => i.company === 'partner-book'), JSON.stringify(slice.items));
  });
});

test('cli: --company refuses to WIDEN — a bare flag and an unknown id both exit 2', () => {
  withWritableCorpus((ctx) => {
    ctx.write(
      'companies/northwind/records/launch.md',
      '---\nkind: decision\nscope: org-shared\n---\nThe launch deadline is the end of the quarter.\n',
    );
    const env = cleanEnv();

    const bare = cli(['compile', '--topic', 'the launch deadline', '--corpus', ctx.dir, '--company'], { env });
    assert.equal(bare.code, 2, bare.stdout);
    assert.ok(/--company needs a value/.test(bare.stderr), bare.stderr);

    const typo = cli(['compile', '--topic', 'the launch deadline', '--corpus', ctx.dir, '--company', 'northwnd'], { env });
    assert.equal(typo.code, 2, typo.stdout);
    assert.ok(/no such company partition/.test(typo.stderr), typo.stderr);

    const ok = cli(['compile', '--topic', 'the launch deadline', '--corpus', ctx.dir, '--company', 'northwind', '--now', '2026-08-24T12:00:00Z'], { env });
    assert.equal(ok.code, 0, ok.stderr);
    const slice = JSON.parse(ok.stdout);
    assert.deepEqual(slice.request.companies, ['northwind']);
    assert.ok(slice.items.some((i) => i.company === 'northwind'), ok.stdout);
  });
});

const ROUND_TRIP_TOPICS = [
  'manufacturing pricing policy written quote',
  'sensor installation supply risk',
  'Halstead integration commitment',
  'customer data region constraint',
  'written brief preference',
];

function emittedRefs(corpus) {
  const out = new Map();
  for (const topic of ROUND_TRIP_TOPICS) {
    const slice = compileContext({ corpus, now: NOW, topic, budgetChars: 4000, maxItems: 12, maxAvailable: 24 });
    for (const ref of [...slice.items.map((i) => i.ref), ...slice.deepFetch.available.map((a) => a.ref)]) {
      if (!out.has(ref)) out.set(ref, corpus.byId.get(ref));
    }
  }
  return out;
}

function fileHoldsRecord(file, rec) {
  if (!file || !fs.existsSync(file)) return false;
  const text = fs.readFileSync(file, 'utf8');
  switch (rec.provenance.source) {
    case 'memory': {
      const bare = rec.id.split(':').slice(2).join(':');
      return text.split(/\r?\n/).some((line) => {
        try {
          return JSON.parse(line).id === bare;
        } catch {
          return false;
        }
      });
    }
    case 'records':
      return path.basename(file) === `${rec.id.split('/').pop()}.md`;
    case 'wiki': {
      const page = rec.id.slice('wiki:'.length).split('#')[0];
      return file.split(path.sep).join('/').endsWith(page);
    }
    case 'entities': {
      if (path.basename(file) !== 'entities.json') return false;
      const want = rec.id.slice('entity:'.length);
      return (JSON.parse(text).entities || []).some((e) => slugify(e.canonical) === want);
    }
    default:
      return false;
  }
}

const REFUSAL_MUST_SAY = {
  wiki: /PROSE section/,
  entities: /vocabulary entr/i,
};

test('round trip: every ref a compiled slice emits resolves to the file that HOLDS it, or is refused for its own named reason', () => {
  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  const corpusRoot = { root: REPO_ROOT, layout: 'repo' };
  const broken = [];
  for (const [ref, rec] of emittedRefs(corpus)) {
    if (!rec) {
      broken.push(`${ref}: the compiler emitted a ref the corpus does not hold`);
      continue;
    }
    const shape = REFUSAL_MUST_SAY[rec.provenance.source];
    try {
      const found = locate({ corpus, corpusRoot, ref });
      if (shape) {
        broken.push(`${ref} (${rec.provenance.source}): the writer accepted a ref it must refuse`);
      } else if (!fileHoldsRecord(found.file, rec)) {
        broken.push(
          `${ref} (${rec.provenance.source}): resolved to ${path.relative(REPO_ROOT, found.file)}, ` +
            'which does not contain that record',
        );
      }
    } catch (err) {
      if (!shape) broken.push(`${ref} (${rec.provenance.source}): ${err.message}`);
      else if (!shape.test(err.message)) {
        broken.push(`${ref} (${rec.provenance.source}): refused, but the reason is wrong — ${err.message}`);
      }
    }
  }
  assert.deepEqual(broken.slice(0, 12), [], `${broken.length} emitted ref(s) the writer cannot address`);
});

test('round trip NEGATIVE CONTROL: the round trip saw refs, from more than one source (it cannot pass on an empty slice)', () => {
  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  const refs = emittedRefs(corpus);
  assert.ok(refs.size >= 20, `the round trip only examined ${refs.size} ref(s) — an empty slice proves nothing`);
  const sources = new Set([...refs.values()].filter(Boolean).map((r) => r.provenance.source));
  assert.ok(sources.size >= 2, `the round trip only saw one source (${[...sources].join(', ')})`);
});

test('round trip: the WHOLE emission set is addressable — the compiler can emit any record id, not just the ranked ones', () => {
  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  const corpusRoot = { root: REPO_ROOT, layout: 'repo' };
  const bySource = new Map();
  const note = (rec, msg) => {
    const s = rec.provenance.source;
    if (!bySource.has(s)) bySource.set(s, []);
    bySource.get(s).push(msg);
  };
  for (const rec of corpus.records) {
    const shape = REFUSAL_MUST_SAY[rec.provenance.source];
    try {
      const found = locate({ corpus, corpusRoot, ref: rec.id });
      if (shape) note(rec, `${rec.id}: accepted a ref that must be refused`);
      else if (!fileHoldsRecord(found.file, rec)) note(rec, `${rec.id}: resolved to a file that does not hold it`);
    } catch (err) {
      if (!shape) note(rec, `${rec.id}: ${err.message}`);
      else if (!shape.test(err.message)) note(rec, `${rec.id}: refused for the wrong reason — ${err.message}`);
    }
  }
  const summary = [...bySource].map(([s, msgs]) => `${s}: ${msgs.length} broken (e.g. ${msgs[0]})`);
  assert.deepEqual(summary, [], `unaddressable refs by source:\n  ${summary.join('\n  ')}`);
});

test('round trip: the writer has a resolver for EXACTLY the sources the compiler can emit from', () => {

  assert.deepEqual([...SOURCES].sort(), Object.keys(REF_RESOLVERS).sort());
});

test('round trip: a source the compiler emits from but the writer cannot resolve is REFUSED by name, never guessed at', () => {

  const corpus = loadCorpus({ root: FIX_ACME, now: NOW });
  const rec = corpus.records.find((r) => r.provenance.source === 'memory');
  const forged = { ...rec, provenance: { ...rec.provenance, source: 'inbox' } };
  const fakeCorpus = { byId: new Map([[forged.id, forged]]) };
  assert.throws(
    () => resolveRef({ corpus: fakeCorpus, corpusRoot: { root: FIX_ACME, layout: 'repo' }, ref: forged.id }),
    /has no resolver for/,
  );
});

test('round trip: show reads back EVERY ref kind — a ref the compiler hands out is a ref a human can open', () => {

  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  const env = { ...cleanEnv(), LORO_ROOT: REPO_ROOT };
  const seen = [];
  for (const source of SOURCES) {
    const rec = corpus.records.find((r) => r.provenance.source === source);
    if (!rec) continue;
    seen.push(source);
    const r = writeCli(['show', '--ref', rec.id, '--json'], { env });
    assert.equal(r.code, 0, `show ${rec.id}: ${r.stderr}`);
    const out = JSON.parse(r.stdout);
    assert.equal(out.source, source, rec.id);
    assert.ok(fileHoldsRecord(out.file, rec), `show ${rec.id} named ${out.file}, which does not hold it`);
  }
  assert.deepEqual(seen.sort(), [...SOURCES].sort(), 'every source must have been exercised');
});

test('round trip: show on a CITED promoted record names its JSONL store, and reports the citation separately', () => {

  const env = { ...cleanEnv(), LORO_ROOT: REPO_ROOT };
  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  const cited = corpus.records.find(
    (r) => r.provenance.source === 'memory' && String(r.provenance.path || '').endsWith('.md'),
  );
  assert.ok(cited, 'this corpus has no promoted record citing a page — the case under test is gone');
  const r = writeCli(['show', '--ref', cited.id, '--json'], { env });
  assert.equal(r.code, 0, r.stderr);
  const out = JSON.parse(r.stdout);
  assert.ok(/\.jsonl$/.test(out.file), `held-in file was ${out.file}`);
  assert.equal(out.cites, cited.provenance.path, 'the citation is REPORTED, never mistaken for the container');
  assert.ok(out.text.includes(cited.id.split(':').slice(2).join(':')), 'the named file must contain the row');
});

test('round trip: supersede REACHES a cited promoted row instead of hunting for JSON in a markdown page', () => {

  const env = { ...cleanEnv(), LORO_ROOT: REPO_ROOT };
  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  const cited = corpus.records.find(
    (r) => r.provenance.source === 'memory'
      && String(r.provenance.path || '').endsWith('.md')
      && !r.supersededBy,
  );
  assert.ok(cited, 'no live cited promoted record to supersede');
  const r = writeCli(
    ['supersede', '--ref', cited.id, '--id', 'round-trip-probe', '--kind', 'fact', '--scope', 'org-shared',
      '--body', 'A probe body, never written.', '--why', 'probe', '--dry-run', '--json'],
    { env },
  );
  assert.equal(r.code, 0, r.stderr);
  assert.equal(JSON.parse(r.stdout).oldFile.endsWith('.jsonl'), true, r.stdout);
});

test('supersede: the replacement cites its own evidence — --ref-source is not collapsed into the ref it replaces', () => {

  withWritableCorpus((ctx) => {
    writeCli(['append', '--corpus', ctx.dir, '--id', 'cited-old', '--kind', 'fact', '--scope', 'org-shared',
      '--body', 'The old belief.', '--now', '2026-08-24T12:00:00Z']);
    const r = writeCli(
      ['supersede', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/cited-old', '--id', 'cited-new',
        '--kind', 'fact', '--scope', 'org-shared', '--body', 'The new belief.', '--why', 'moved on',
        '--ref-source', 'ceo/pages/worldview.md#how-i-decide', '--now', '2026-08-24T12:00:00Z'],
    );
    assert.equal(r.code, 0, r.stderr);
    const fm = parseFrontMatter(ctx.read('ceo/records/cited-new.md')).data;
    assert.equal(fm.provenance.ref, 'ceo/pages/worldview.md#how-i-decide');
    assert.equal(fm.supersedes, 'rec:ceo/records/cited-old', 'the replaced ref keeps its own field');
    const cite = citationOf({ provenance: { source: 'records', ref: fm.provenance.ref } });
    assert.deepEqual(cite, { path: 'ceo/pages/worldview.md', anchor: 'how-i-decide' });
  });
});

test('supersede: PROBE — with no --ref-source the replacement still points back at what it replaced', () => {

  withWritableCorpus((ctx) => {
    writeCli(['append', '--corpus', ctx.dir, '--id', 'plain-old', '--kind', 'fact', '--scope', 'org-shared',
      '--body', 'The old belief.', '--now', '2026-08-24T12:00:00Z']);
    const r = writeCli(['supersede', '--corpus', ctx.dir, '--ref', 'rec:ceo/records/plain-old',
      '--id', 'plain-new', '--kind', 'fact', '--scope', 'org-shared', '--body', 'The new belief.',
      '--why', 'moved on', '--now', '2026-08-24T12:00:00Z']);
    assert.equal(r.code, 0, r.stderr);
    const fm = parseFrontMatter(ctx.read('ceo/records/plain-new.md')).data;
    assert.equal(fm.provenance.ref, 'rec:ceo/records/plain-old');
  });
});

test('round trip: an entity ref is refused as generated VOCABULARY, not misdescribed as prose', () => {

  const corpus = loadCorpus({ root: REPO_ROOT, now: NOW });
  const rec = corpus.records.find((r) => r.provenance.source === 'entities');
  const corpusRoot = { root: REPO_ROOT, layout: 'repo' };
  for (const op of ['supersede', 'correct']) {
    assert.throws(
      () => locate({ corpus, corpusRoot, ref: rec.id, op }),
      (err) => /vocabulary entr/i.test(err.message) && /entities\.json/.test(err.message) && err.code === 5,
      `${op} must refuse an entity ref by naming what it is`,
    );
  }
});

console.log(`\n${passed} passed, ${failures.length} failed`);
if (failures.length) {
  for (const f of failures) console.log(`\nFAIL ${f.name}\n${f.err.stack}`);
  process.exit(1);
}
