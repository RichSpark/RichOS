// Transport only. Python owns parsing, precedence and volume validation.
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const reader = fileURLToPath(new URL('../../engine/scripts/lib/machine_settings.py', import.meta.url));
let resolved;
function environment() {
  if (!resolved) {
    resolved = { ...process.env, ...JSON.parse(execFileSync('python3', [reader, 'values'], { encoding: 'utf8' })) };
    Object.assign(process.env, resolved);
  }
  return resolved;
}
function read(command, name = '', key) {
  return execFileSync('python3', [reader, command, name, ...(key ? ['--key', key] : [])], { env: environment(), encoding: 'utf8' }).trimEnd();
}
export const cacheDir = (name, key) => read('cache', name, key);
export const stateDir = (name, key) => read('state', name, key);
export const validateStorage = (path) => read('validate', path);
export const machineEnvironment = environment;

export const scratchDir = () => read('agents');
