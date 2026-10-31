import { cacheDir, validateStorage, machineEnvironment } from './machine-settings.mjs';
import { mkdirSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
export const mobile = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const key = createHash('sha256').update(mobile).digest('hex').slice(0, 10);
// The per-checkout cache the CLI's sessions, locks and labs live in. RICHOS_MOBILE_TEST_APP=integration
// selects a separate namespace so a test run never touches the ordinary session.
export function cacheRoot() {
  const base = machineEnvironment().RICHOS_MOBILE_CACHE || join(cacheDir('richos-mobile'), key);
  const path = process.env.RICHOS_MOBILE_TEST_APP === 'integration' ? join(base, 'integration') : base;
  const resolved = validateStorage(resolve(path));
  mkdirSync(resolved, { recursive: true });
  return resolved;
}
