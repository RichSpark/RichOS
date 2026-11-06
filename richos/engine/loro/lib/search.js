// Loro public component. SPDX-License-Identifier: AGPL-3.0-only
import path from 'node:path';
import { pathsFor, resolveCorpusRoot } from './layout.js';
import { scopeAllowed } from './privacy.js';
import { loadCorpus } from './store.js';

// This is a read projection for RichFinder, separate from the context compiler and
// historical fetch. The app supplies the grant from its own verified binding, never
// from search arguments. Nothing in the record itself can widen the grant.
export function loadSearchProjection({ corpusRoot, root, grant, now = Date.now() }) {
  if (grant?.audience !== 'rich' || typeof grant.includeCeo !== 'boolean' ||
      !Array.isArray(grant.companyIds) || !grant.companyIds.every(id => typeof id === 'string' && id)) {
    throw new Error('Loro search requires an explicit Rich audience and company scope.');
  }
  if (!Number.isFinite(now)) throw new Error('Loro search requires a valid snapshot time.');

  const location = resolveCorpusRoot({ corpus: corpusRoot, root });
  const layout = pathsFor(location.root, location.layout);
  const active = new Set(layout.companies);
  const retired = new Set(layout.retiredCompanies);
  const allowedCompanies = new Set(grant.companyIds.filter(id => active.has(id) && !retired.has(id)));
  const grantedDirectory = spec => spec.company ? allowedCompanies.has(spec.company) : grant.includeCeo;
  const pageDirs = layout.pageDirs.filter(grantedDirectory);
  const recordDirs = layout.recordDirs.filter(grantedDirectory);
  const ownerEntries = dirs => dirs.map(spec => ({
    prefix: path.relative(location.root, spec.dir).split(path.sep).join('/') + '/',
    company: spec.company || null,
  })).sort((a, b) => b.prefix.length - a.prefix.length);
  const owners = { records: ownerEntries(recordDirs), wiki: ownerEntries(pageDirs) };
  // The legacy flat JSONL directory mixes owners and stores user-supplied
  // provenance. Its origin cannot attest a company, so leave it out of this view.
  const sources = grant.includeCeo ? ['records', 'wiki', 'entities'] : ['records', 'wiki'];
  const corpus = loadCorpus({ ...location, now, sources, pageDirs, recordDirs });
  const nowRetired = new Set(corpus.retiredCompanies);
  const hasStorageGrant = record => {
    if (record.provenance.source === 'entities') return grant.includeCeo && record.company === null;
    const owner = owners[record.provenance.source]?.find(spec => record.provenance.path?.startsWith(spec.prefix));
    return !!owner && owner.company === record.company && !nowRetired.has(record.company);
  };
  const records = corpus.records.filter(record =>
    record.status === 'current' && scopeAllowed(record.scope, grant.audience) && hasStorageGrant(record)
  ).map(record => ({
    ref: record.id,
    company: record.company,
    scope: record.scope,
    title: record.title,
    text: record.text,
    provenance: record.provenance,
    supersedes: record.supersedes,
    validUntil: record.validUntil,
  }));
  return { schemaVersion: 1, asOf: now, records, excludedSources: layout.memoryDir ? ['legacy_memory'] : [] };
}
