#!/usr/bin/env node
/**
 * native-transport-e2e.mjs refuses BEFORE Chrome starts when the native host Chrome would launch
 * could ask macOS for removable-volume access (native-host-privacy.mjs has the 2026-10-08 incident).
 *
 * No dialog can come from this test: the "Chrome" is a stub shell script that only records that it
 * was started, so no native host is ever launched, and the TCC database read is a scratch copy
 * under a scratch HOME. The scratch folders live on /Volumes/E1TB because that is the removable // machine-path-exempt: negative or parser fixture; this literal is test input, never a storage default
 * volume of this Mac; the harness and this test (children of the caller's terminal) already read it.
 */
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const HARNESS = path.join(HERE, 'native-transport-e2e.mjs');
const configured = fs.realpathSync(os.tmpdir());
const parts = configured.split('/');
const VOLUME = parts[1] === 'Volumes' && parts[2] ? '/' + parts.slice(1, 3).join('/') : null;
if (!VOLUME || fs.statSync(VOLUME).dev === fs.statSync(path.dirname(VOLUME)).dev) {
  console.error('NOT RUN: this privacy fixture requires TMPDIR on a mounted removable volume');
  process.exit(2);
}
const scratch = fs.mkdtempSync(path.join(configured, 'native-host-privacy-'));
const nodeReal = fs.realpathSync(process.execPath);

function makeTccDb(home, grants) {
  const dir = path.join(home, 'Library', 'Application Support', 'com.apple.TCC');
  fs.mkdirSync(dir, { recursive: true });
  const db = path.join(dir, 'TCC.db');
  const rows = grants.map((client) => `insert into access values('kTCCServiceSystemPolicyRemovableVolumes','${client}',1,2,2);`).join('');
  const out = spawnSync('/usr/bin/sqlite3', [db,
    `create table access(service text, client text, client_type integer, auth_value integer, auth_reason integer);${rows}`]);
  assert.equal(out.status, 0, String(out.stderr));
}

let failed = 0;
function check(name, fn) {
  try { fn(); console.log(`  ok  ${name}`); } catch (error) { failed += 1; console.log(`FAIL  ${name} — ${error.message.split('\n')[0]}`); }
}

try {
  // ------------------------------------------------ the harness, TMPDIR on the removable volume
  const home = path.join(scratch, 'home');
  makeTccDb(home, []);
  const marker = path.join(scratch, 'chrome-started');
  const stub = path.join(scratch, 'stub-chrome.sh');
  fs.writeFileSync(stub, `#!/bin/sh\necho started >> '${marker}'\nexit 1\n`, { mode: 0o755 });
  const model = path.join(scratch, 'model.bin');
  fs.writeFileSync(model, 'not a model');
  const tmp = path.join(scratch, 'tmp');
  fs.mkdirSync(tmp);
  const run = spawnSync(process.execPath, [HARNESS, '--leg=native'], {
    encoding: 'utf8', timeout: 120000,
    env: { ...process.env, HOME: home, TMPDIR: tmp, CHROME_PATH: stub, RICHOS_WHISPER_MODEL: model,
      RICHOS_NATIVE_LAUNCHER_PYTHON: '' },
  });
  const output = `${run.stdout}${run.stderr}`;
  const said = output.trim().split('\n').filter((line) => /\S/.test(line)).slice(-1)[0] || '(nothing)';
  check('harness: refuses before starting Chrome when its host would read a removable volume', () => {
    assert.ok(/REFUSED before starting Chrome/.test(output), `no refusal; the harness's last line: ${said}`);
  });
  check('harness: the refusal names the subject and the way to grant it once', () => {
    assert.ok(output.includes(`${nodeReal}: Removable Volumes permission not allowed`), `subject not named; last line: ${said}`);
    assert.match(output, /Privacy & Security > Files & Folders > node > Removable Volumes/);
  });
  check('harness: Chrome was never started', () => {
    assert.equal(fs.existsSync(marker), false, 'the stub Chrome ran');
  });
  check('harness: exits non-zero (a refusal is never a pass)', () => {
    assert.equal(run.status, 1, `exit ${run.status}`);
  });

  // ------------------------------------------------ the check itself
  let removableVolumeRefusal = () => { throw new Error('native-host-privacy.mjs is missing'); };
  try { ({ removableVolumeRefusal } = await import('./native-host-privacy.mjs')); } catch (error) {
    check('check: native-host-privacy.mjs loads', () => { throw error; });
  }
  check('check: nothing on a removable volume -> no refusal, no TCC read', () => {
    assert.equal(removableVolumeRefusal({ paths: [os.homedir(), '/usr/bin'], subjects: [nodeReal], home: path.join(scratch, 'absent') }), null);
  });
  const granted = path.join(scratch, 'home-granted');
  makeTccDb(granted, [nodeReal]);
  check('check: removable path + subject allowed -> no refusal', () => {
    assert.equal(removableVolumeRefusal({ paths: [scratch], subjects: [process.execPath], home: granted }), null);
  });
  check('check: removable path + a second subject not allowed -> refusal', () => {
    const text = removableVolumeRefusal({ paths: [scratch], subjects: [process.execPath, '/bin/sh'], home: granted });
    assert.ok(text && text.includes('/bin/sh: Removable Volumes permission not allowed'), String(text));
    assert.ok(!text.includes(`${nodeReal}:`), 'an allowed subject is not listed');
  });
  check('check: unreadable TCC database -> refusal (unknown is never allowed)', () => {
    const text = removableVolumeRefusal({ paths: [scratch], subjects: [nodeReal], home: path.join(scratch, 'absent') });
    assert.ok(text && /permission unknown/.test(text), String(text));
  });

  // ------------------------------------------------ staging into configured scratch
  const { stageOffRemovable, stageNativeProduct } = await import('./native-host-privacy.mjs');
  const checkout = path.join(scratch, 'tools');
  for (const [dir, file] of [['richos-service/host', 'native-host.js'], ['richos-extension', 'manifest.json'],
    ['richos-service/node_modules/dep', 'index.js'], ['richos-extension/.git', 'HEAD']]) {
    fs.mkdirSync(path.join(checkout, dir), { recursive: true });
    fs.writeFileSync(path.join(checkout, dir, file), `fixture ${file}\n`);
  }
  const into = path.join(scratch, 'stage');
  const dirs = { service: path.join(checkout, 'richos-service'), extension: path.join(checkout, 'richos-extension') };
  const permission = { subjects: [nodeReal], home: granted };
  check('stage: a checkout on the removable volume without a grant is refused (positive control)', () => {
    assert.ok(removableVolumeRefusal({ paths: Object.values(dirs), subjects: [nodeReal], home }));
  });
  let staged;
  check('stage: the host and extension are copied into the configured permitted scratch root', () => {
    staged = stageOffRemovable({ dirs, into, ...permission });
    assert.deepEqual(Object.keys(staged).sort(), ['extension', 'service']);
    for (const copy of Object.values(staged)) assert.ok(copy.startsWith(into + path.sep), copy);
    assert.equal(fs.readFileSync(path.join(staged.service, 'host', 'native-host.js'), 'utf8'), 'fixture native-host.js\n');
    assert.equal(fs.readFileSync(path.join(staged.extension, 'manifest.json'), 'utf8'), 'fixture manifest.json\n');
  });
  check('stage: node_modules and .git are left behind', () => {
    assert.equal(fs.existsSync(path.join(staged.service, 'node_modules')), false);
    assert.equal(fs.existsSync(path.join(staged.extension, '.git')), false);
  });
  check('stage: copied paths remain subject to the removable-volume grant', () => {
    assert.equal(removableVolumeRefusal({ paths: [...Object.values(staged), into], ...permission }), null);
    assert.ok(removableVolumeRefusal({ paths: [...Object.values(staged), into], subjects: [nodeReal], home }));
  });
  check('stage: a source already off the removable volume is used as it is', () => {
    assert.deepEqual(stageOffRemovable({ dirs: { source: '/usr/bin' }, into: path.join(scratch, 'again'), ...permission }), { source: '/usr/bin' });
  });
  check('stage: an external destination without granted host subjects is refused', () => {
    assert.throws(() => stageOffRemovable({ dirs, into: path.join(scratch, 'unapproved') }), /removable volume/);
    assert.throws(() => stageOffRemovable({ dirs, into: path.join(scratch, 'denied'), subjects: [nodeReal], home }), /Removable Volumes permission not allowed/);
  });
  check('stage: the copied service resolves its voice, loro and extension imports without the source tree', () => {
    for (const dir of ['tools/richos-service/lib/workspace', 'tools/richos-extension/sync', 'engine/voice/provisioning', 'engine/loro/lib']) {
      fs.mkdirSync(path.join(scratch, dir), { recursive: true });
    }
    for (const dir of ['tools/richos-service', 'tools/richos-extension', 'engine/voice', 'engine/loro']) {
      fs.writeFileSync(path.join(scratch, dir, 'package.json'), '{"type":"module"}');
    }
    fs.writeFileSync(path.join(scratch, 'engine/voice/provisioning/model-catalog.js'), 'export const pin = 1;');
    fs.writeFileSync(path.join(scratch, 'engine/loro/lib/privacy.js'), 'export const privacy = 1;');
    fs.writeFileSync(path.join(scratch, 'tools/richos-extension/sync/reconcile.js'), 'export const decide = 1;');
    fs.copyFileSync(path.resolve(HERE, '../../richos-service/lib/workspace/privacy.js'), path.join(dirs.service, 'lib/workspace/privacy.js'));
    fs.writeFileSync(path.join(dirs.service, 'lib/fixture.js'), "import { pin } from '../../../engine/voice/provisioning/model-catalog.js';\n" +
      "import { privacy } from '../../../engine/loro/lib/privacy.js';\nimport { decide } from '../../richos-extension/sync/reconcile.js';\nexport const ready = pin + privacy + decide;");
    const copy = stageNativeProduct({ ...dirs, into: path.join(scratch, 'product'), ...permission });
    fs.rmSync(path.join(scratch, 'engine'), { recursive: true });
    fs.rmSync(checkout, { recursive: true });
    const source = `import { ready } from ${JSON.stringify(pathToFileURL(path.join(copy.service, 'lib/fixture.js')).href)};
      import { PRODUCT_REPO, assertEvidenceOutsideProductRepo } from ${JSON.stringify(pathToFileURL(path.join(copy.service, 'lib/workspace/privacy.js')).href)};
      if (ready !== 3) throw new Error('wrong staged imports');
      assertEvidenceOutsideProductRepo(${JSON.stringify(path.join(scratch, 'outside-zone'))});
      let refused = false;
      try { assertEvidenceOutsideProductRepo(PRODUCT_REPO + '/recordings'); } catch (error) { refused = /privacy invariant/.test(error.message); }
      if (!refused) throw new Error('staged product accepted a recording inside its checkout');`;
    const result = spawnSync(process.execPath, ['--input-type=module', '-e', source], { cwd: '/', encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  });

} finally {
  fs.rmSync(scratch, { recursive: true, force: true });
}
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
