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
module.exports = { createScratch, StorageUnavailable };
