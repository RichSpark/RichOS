// Suite-owned storage under the selected process allocator.
const fs = require('node:fs');
const { join, dirname, resolve } = require('node:path');
const { execFileSync } = require('node:child_process');
const { tmpdir } = require('node:os');
const reader = resolve(__dirname, '../../engine/scripts/lib/machine_settings.py');
class StorageUnavailable extends Error {}
function createScratch(label) {
  const configured = process.env.TMPDIR || tmpdir();
  let base;
  try { base = execFileSync('python3', [reader, 'validate', configured], { encoding: 'utf8' }).trimEnd(); }
  catch (error) { throw new StorageUnavailable(String(error.stderr || error.message)); }
  try {
    fs.mkdirSync(base, { recursive: true });
    return fs.mkdtempSync(join(base, `richos-mobile-${label}-`));
  } catch (error) { throw new StorageUnavailable(`Cannot allocate mobile scratch in ${base}: ${error.message}`); }
}
function proofCache(base, storage, override) {
  if (!['system', 'external'].includes(storage)) throw new Error('Simulator storage must be system or external');
  if (override) return override; // Explicit paths remain subject to the CLI's policy checks.
  const policy = join(base, 'simulator-storage.json');
  // Reuse matching earlier caches. Never rewrite a saved choice or reuse a failed
  // external device set when the suite now requests the approved system storage.
  if (fs.existsSync(policy) && JSON.parse(fs.readFileSync(policy, 'utf8')).storage === storage) return base;
  return `${base}-${storage}`;
}
module.exports = { createScratch, proofCache, StorageUnavailable };
