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
      !Array.isArray(grant.companyIds) || !grant.companyIds.every(id => typeof id === 'string' && id) ||
      (grant.repoOwner !== undefined && (typeof grant.repoOwner !== 'string' || !grant.repoOwner))) {
    throw new Error('Loro search requires an explicit Rich audience and company scope.');
  }
  if (!Number.isFinite(now)) throw new Error('Loro search requires a valid snapshot time.');

  const location = resolveCorpusRoot({ corpus: corpusRoot, root });
  const layout = pathsFor(location.root, location.layout);
  const active = new Set(layout.companies);
  const retired = new Set(layout.retiredCompanies);
  const allowedCompanies = new Set(grant.companyIds.filter(id => active.has(id) && !retired.has(id)));
  // Only the host can attest the owner of an unpartitioned repository corpus.
  // Its project folders and flat memory never become personal memory by default.
  const unpartitionedRepo = location.layout === 'repo' && active.size === 0;
  const legacyOwner = unpartitionedRepo && grant.companyIds.includes(grant.repoOwner) ? grant.repoOwner : undefined;
  const relativeDir = spec => path.relative(location.root, spec.dir).split(path.sep).join('/');
  const legacyDirectory = spec => unpartitionedRepo && !spec.company && !relativeDir(spec).startsWith('ceo/');
  const grantedDirectory = spec => {
    if (spec.company) return allowedCompanies.has(spec.company);
    return legacyDirectory(spec) ? legacyOwner !== undefined : grant.includeCeo;
  };
  const pageDirs = layout.pageDirs.filter(grantedDirectory);
  const recordDirs = layout.recordDirs.filter(grantedDirectory);
  const ownerEntries = dirs => dirs.map(spec => ({
    prefix: relativeDir(spec) + '/',
    company: spec.company || null,
    legacy: legacyDirectory(spec),
  })).sort((a, b) => b.prefix.length - a.prefix.length);
  const owners = { records: ownerEntries(recordDirs), wiki: ownerEntries(pageDirs) };
  const sources = ['records', 'wiki'];
  if (grant.includeCeo) sources.push('entities');
  // Do not rely on a JSONL row's claimed company or provenance path as authority.
  if (legacyOwner !== undefined) sources.push('memory');
  const corpus = loadCorpus({ ...location, now, sources, pageDirs, recordDirs });
  const nowRetired = new Set(corpus.retiredCompanies);
  const storageOwner = record => {
    if (record.provenance.source === 'entities') return grant.includeCeo && record.company === null ? null : undefined;
    if (record.provenance.source === 'memory') {
      return record.company === null || record.company === legacyOwner ? legacyOwner : undefined;
    }
    const owner = owners[record.provenance.source]?.find(spec => record.provenance.path?.startsWith(spec.prefix));
    if (!owner) return undefined;
    if (owner.legacy) return record.company === null || record.company === legacyOwner ? legacyOwner : undefined;
    return owner.company === record.company && !nowRetired.has(record.company) ? owner.company : undefined;
  };
  const records = [];
  for (const record of corpus.records) {
    if (record.status !== 'current' || !scopeAllowed(record.scope, grant.audience)) continue;
    const company = storageOwner(record);
    if (company === undefined) continue;
    records.push({
      ref: record.id,
      company,
      scope: record.scope,
      title: record.title,
      text: record.text,
      provenance: record.provenance,
      supersedes: record.supersedes,
      validUntil: record.validUntil,
    });
  }
  const excludedSources = layout.memoryDir && legacyOwner === undefined ? ['legacy_memory'] : [];
  return { schemaVersion: 1, asOf: now, records, excludedSources };
}
