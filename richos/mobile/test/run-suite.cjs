// One scoped TMPDIR for the suite and all descendants, including npm/Playwright.
const { spawn } = require('node:child_process');
const { rmSync } = require('node:fs');
const { join } = require('node:path');
const { createScratch, StorageUnavailable } = require('./storage.cjs');
const target = process.argv[2];
if (target !== 'headless') throw new Error('Expected headless');
let scratch;
try { scratch = createScratch(`suite-${target}`); }
catch (error) {
  if (!(error instanceof StorageUnavailable)) throw error;
  console.log(`  NOT RUN  mobile-${target}: ${error.message}`);
  process.exit(2);
}
const bin = 'npm';
const args = ['--prefix', join(__dirname, '..'), 'test'];
const child = spawn(bin, args, { stdio: 'inherit', env: { ...process.env, TMPDIR: scratch + '/' } });
const signals = ['SIGINT', 'SIGTERM'];
for (const signal of signals) process.on(signal, () => child.kill(signal));
// 'close' follows exit/error and closed child streams. Do not delete active scratch.
child.on('error', (error) => { console.error(`  FAIL  mobile-${target}: ${error.message}`); });
child.on('close', (code) => {
  rmSync(scratch, { recursive: true, force: true });
  process.exitCode = code ?? 1;
});
