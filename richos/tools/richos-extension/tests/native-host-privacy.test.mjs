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
import { fileURLToPath } from 'node:url';

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

  // ------------------------------------------------ staging off the removable volume
  // A checkout on the SSD (the nightly's state folder can be there) is copied under an internal
  // scratch root, so Chrome and the host read nothing on the volume and no permission is needed.
  let stageOffRemovable = () => { throw new Error('stageOffRemovable is missing from native-host-privacy.mjs'); };
  try { ({ stageOffRemovable } = await import('./native-host-privacy.mjs')); } catch { /* the checks below name it */ }
  const checkout = path.join(scratch, 'tools');
  for (const [dir, file] of [['richos-service/host', 'native-host.js'], ['richos-extension', 'manifest.json'],
    ['richos-service/node_modules/dep', 'index.js'], ['richos-extension/.git', 'HEAD']]) {
    fs.mkdirSync(path.join(checkout, dir), { recursive: true });
    fs.writeFileSync(path.join(checkout, dir, file), `fixture ${file}\n`);
  }
  let internal = '';
  try { internal = spawnSync('/usr/bin/getconf', ['DARWIN_USER_TEMP_DIR'], { encoding: 'utf8' }).stdout.trim(); } catch { /* none */ }
  if (internal && fs.existsSync(internal)) {
    const into = fs.mkdtempSync(path.join(internal, 'native-host-privacy-stage-'));
    try {
      const dirs = { service: path.join(checkout, 'richos-service'), extension: path.join(checkout, 'richos-extension') };
      check('stage: a checkout on the removable volume would be refused (positive control)', () => {
        const text = removableVolumeRefusal({ paths: Object.values(dirs), subjects: [nodeReal], home: path.join(scratch, 'absent') });
        assert.ok(text && /removable volume/.test(text), String(text));
      });
      let staged;
      check('stage: the host and the extension are copied off the removable volume', () => {
        staged = stageOffRemovable({ dirs, into: path.join(into, 'staged') });
        assert.deepEqual(Object.keys(staged).sort(), ['extension', 'service']);
        for (const copy of Object.values(staged)) assert.ok(!fs.realpathSync(copy).startsWith('/Volumes/'), copy);
        assert.equal(fs.readFileSync(path.join(staged.service, 'host', 'native-host.js'), 'utf8'), 'fixture native-host.js\n');
        assert.equal(fs.readFileSync(path.join(staged.extension, 'manifest.json'), 'utf8'), 'fixture manifest.json\n');
      });
      check('stage: node_modules and .git are left behind', () => {
        assert.equal(fs.existsSync(path.join(staged.service, 'node_modules')), false);
        assert.equal(fs.existsSync(path.join(staged.extension, '.git')), false);
      });
      check('stage: after staging nothing asks for a permission (no TCC read, no refusal)', () => {
        assert.equal(removableVolumeRefusal({ paths: [...Object.values(staged), into], subjects: [nodeReal], home: path.join(scratch, 'absent') }), null);
      });
      check('stage: a directory already off the removable volume is used as it is', () => {
        assert.deepEqual(stageOffRemovable({ dirs: { extension: staged.extension }, into: path.join(into, 'again') }), { extension: staged.extension });
      });
      check('stage: staging onto the removable volume is refused, not pretended', () => {
        assert.throws(() => stageOffRemovable({ dirs, into: path.join(scratch, 'still-removable') }), /removable volume/);
      });
    } finally {
      fs.rmSync(into, { recursive: true, force: true });
    }
  } else {
    check('stage: an internal scratch root exists', () => { throw new Error('getconf DARWIN_USER_TEMP_DIR named no folder'); });
  }
} finally {
  fs.rmSync(scratch, { recursive: true, force: true });
}
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
