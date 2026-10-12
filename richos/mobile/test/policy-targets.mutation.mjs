// policy-targets.mutation.mjs -- the iPhone/Android policy separation, watched going red.
//
// CEO ruling §91, 2026-09-26: the two phone apps are independent apps, so an iPhone release,
// minimum or blocked build never reaches an Android phone and an Android one never reaches an
// iPhone. policy-targets.test.js proves that in one case; this file removes the separation ONE
// direction and ONE layer at a time (the served record, the hint stream, the local store) and
// requires that case to go red with the failure that names the leak. A mutant that is caught by
// an incidental error does not score.
//
// THE SHIPPED FILES ARE NEVER OPENED FOR WRITING. Each mutant runs in a private copy of the
// service, core, dev and test folders on the suite's own scratch volume, removed afterwards.
// Invoked by richos/app/scripts/mobile-headless.test.sh. Exit 0 = every leak is caught.
import { cpSync, readFileSync, writeFileSync, rmSync, mkdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

const require = createRequire(import.meta.url);
const { createScratch } = require('./storage.cjs');
const mobile = fileURLToPath(new URL('..', import.meta.url));
const CASE = 'an iPhone release, minimum or blocked build never reaches the Android app, and an Android one never reaches the iPhone app';
const LATEST = "'SELECT revision, policy FROM revisions WHERE COALESCE(json_extract(policy, ?1), ?2) = ?3 ORDER BY";
const HEAD = "'SELECT revision FROM revisions WHERE COALESCE(json_extract(policy, ?1), ?2) = ?3 ORDER BY";
// The leak: a route for `from` also returns records of `to`. The SQL sits in a single-quoted JS
// string, so its own quotes are escaped in the source.
const leak = (query, from, to) => query.replace('= ?3 ORDER BY', `IN (?3, CASE ?3 WHEN \\'${from}\\' THEN \\'${to}\\' ELSE ?3 END) ORDER BY`);

// [name, file, old text, new text, the failure the case must report]
const MUTANTS = [
  ['the Android route also serves iPhone records', 'service/policy-store.mjs', LATEST, leak(LATEST, 'android-native', 'ios-native'),
    'the Android route still serves the Android record'],
  ['the Android stream announces iPhone revisions', 'service/policy-store.mjs', HEAD, leak(HEAD, 'android-native', 'ios-native'),
    'the Android stream announces nothing for an iPhone revision'],
  ['the iPhone route also serves Android records', 'service/policy-store.mjs', LATEST, leak(LATEST, 'ios-native', 'android-native'),
    'the iPhone route still serves the iPhone record'],
  ['the iPhone stream announces Android revisions', 'service/policy-store.mjs', HEAD, leak(HEAD, 'ios-native', 'android-native'),
    'the iPhone stream announces nothing for an Android revision'],
  ['the local store keeps one record for both native apps', 'service/policy.mjs',
    "const activeFile = target => target === PRESERVED ? 'active.json' : `active-${target}.json`;",
    "const activeFile = target => target === PRESERVED ? 'active.json' : 'active-native.json';",
    'Stored policy targets another app'],
];

function run(name, edit) {
  const scratch = createScratch('policy-mutant');
  try {
    const fixture = join(scratch, 'richos/mobile');
    const library = join(scratch, 'richos/engine/scripts/lib');
    mkdirSync(fixture, {recursive:true});
    mkdirSync(library, {recursive:true});
    cpSync(join(mobile, '../engine/scripts/lib/machine_settings.py'), join(library, 'machine_settings.py'));
    for (const part of ['service', 'core', 'dev', 'test', 'package.json']) cpSync(join(mobile, part), join(fixture, part), { recursive: true });
    if (edit) {
      const [file, before, after] = edit, path = join(fixture, file), text = readFileSync(path, 'utf8');
      if (text.split(before).length !== 2) return { status: null, output: `${file} does not contain the mutated text exactly once` };
      writeFileSync(path, text.replace(before, after));
    }
    const pattern = CASE.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const result = spawnSync(process.execPath, ['--test', '--test-reporter=tap', `--test-name-pattern=^${pattern}$`, join(fixture, 'test/policy-targets.test.js')],
      { encoding: 'utf8', env: { ...process.env, TMPDIR: scratch + '/' }, timeout: 60000 });
    return { status: result.status, output: `${result.stdout}${result.stderr}` };
  } finally { rmSync(scratch, { recursive: true, force: true }); }
}

let failed = 0;
const baseline = run('unmutated');
// The case must run and pass unmutated, or a red mutant proves nothing.
if (baseline.status === 0 && /# pass 1\b/.test(baseline.output) && /# fail 0\b/.test(baseline.output)) console.log('  ok    unmutated: the case runs and passes');
else { failed++; console.log(`  FAIL  unmutated: the case did not run and pass\n${baseline.output.slice(-1500)}`); }
for (const [name, file, before, after, expected] of MUTANTS) {
  const { status, output } = run(name, [file, before, after]);
  if (status !== 0 && status !== null && output.includes(expected)) console.log(`  ok    caught: ${name}`);
  else { failed++; console.log(`  FAIL  survived: ${name} (exit ${status}; expected "${expected}")\n${output.slice(-1500)}`); }
}
console.log(`policy-targets.mutation: ${MUTANTS.length + 1 - failed} of ${MUTANTS.length + 1} passed`);
process.exit(failed ? 1 : 0);
