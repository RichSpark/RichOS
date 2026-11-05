#!/usr/bin/env node
// THE PACKAGED MEETING-NOTES SERVICE LOADS FROM ITS OWN COPY (Granola PRD V4, "Deliver the
// existing components with the app": "Verify service imports from the installed payload with the
// checkout unavailable").
//
//   node richos/app/scripts/lib/service-imports.mjs <product-root>
//
// <product-root> is a copied product layout (`engine/voice`, `engine/loro`,
// `tools/richos-service`, `tools/richos-extension`, the voice delivery contract's), such as the
// engine archive's `engine/service`. Every module under `tools/richos-service/lib` is imported
// from there, and the CLI answers `tiers` from there with the working directory at `/` and a
// PATH of system directories only. Relative imports resolve against the copy, so a file the
// packaging left out fails here even while a checkout exists elsewhere on the disk.
import { readdirSync } from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = process.argv[2] && path.resolve(process.argv[2]);
if (!root) {
  console.error('usage: service-imports.mjs <product-root>');
  process.exit(2);
}

function modules(dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...modules(p));
    else if (/\.(m?js)$/.test(entry.name)) out.push(p);
  }
  return out.sort();
}

const lib = path.join(root, 'tools/richos-service/lib');
const found = modules(lib);
if (found.length === 0) {
  console.error(`no service modules under ${lib}`);
  process.exit(1);
}
for (const file of found) {
  try {
    await import(pathToFileURL(file).href);
  } catch (error) {
    console.error(`${path.relative(root, file)} does not load from ${root}: ${error.message}`);
    process.exit(1);
  }
}

const cli = path.join(root, 'tools/richos-service/bin/richos-service.js');
const tiers = spawnSync(process.execPath, [cli, 'tiers'], {
  cwd: '/',
  encoding: 'utf8',
  env: { PATH: '/usr/bin:/bin:/usr/sbin:/sbin', HOME: process.env.HOME || '/' },
});
if (tiers.status !== 0) {
  console.error(`richos-service tiers failed from ${root} (exit ${tiers.status}): ${(tiers.stderr || '').trim()}`);
  process.exit(1);
}
console.log(`  service             ${found.length} module(s) load from the copy; \`richos-service tiers\` answers`);
