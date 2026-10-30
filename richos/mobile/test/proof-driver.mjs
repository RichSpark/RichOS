// Registered platform proofs. No missing tool/runtime can produce a passing suite.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { existsSync, statSync, mkdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { mobile, cacheRoot } from '../cli/simulator.mjs';
import { cacheDir, machineEnvironment } from '../cli/machine-settings.mjs';

const require = createRequire(import.meta.url);
const { createScratch, proofCache } = require('./storage.cjs');
const target = process.argv[2];
assert(['client-ui', 'ios'].includes(target), 'Expected client-ui or ios');
const label = `mobile-${target}`;
function notRun(reason) { console.log(`  NOT RUN  ${label}: ${reason}`); process.exit(2); }
function hostTool(bin, args) {
  const result = spawnSync(bin, args, { encoding: 'utf8' });
  if (result.error?.code === 'ENOENT') notRun(`${bin} is unavailable`);
  if (result.error) throw result.error;
  return result;
}

// Establish host capability before creating a simulator or starting any worker.
if (target === 'ios') {
  if (process.platform !== 'darwin') notRun('iOS Simulator requires macOS');
  for (const [bin, args] of [['xcodebuild', ['-version']], ['xcodegen', ['--version']]]) {
    const result = hostTool(bin, args);
    if (result.status !== 0) notRun(`${bin} is not configured: ${result.stderr.trim()}`);
  }
  const available = hostTool('xcrun', ['simctl', 'list', 'runtimes', '--json']);
  assert.equal(available.status, 0, available.stderr); // A broken simctl query is a failure.
  if (!JSON.parse(available.stdout).runtimes.some((r) => r.isAvailable && r.name.startsWith('iOS'))) notRun('no available iOS simulator runtime');
} else {
  const { loadPlaywright } = require('../../app/ui/tests/lib/harness.js');
  let playwright;
  try { playwright = loadPlaywright(); }
  catch (error) {
    if (error.message.startsWith('playwright not found')) notRun(error.message);
    throw error;
  }
  for(const engine of ['chromium','webkit'])if(!existsSync(playwright[engine].executablePath()))notRun(`Playwright ${engine==='chromium'?'Chromium':'WebKit'} is not installed`);
}
cacheDir(); // Validate selected bulk storage before any build or scratch write.

const key = createHash('sha256').update(mobile).digest('hex').slice(0, 10);
// Browser proofs are disposable. Native builds use an isolated reusable cache and
// device per checkout, separate from the developer's interactive CLI session.
// Apple's default device storage was approved for this workflow. This is an
// explicit suite default, not a fallback after an external creation failure.
const storage = process.env.RICHOS_MOBILE_SIMULATOR_STORAGE || 'system';
if (target === 'ios') process.env.RICHOS_MOBILE_SIMULATOR_STORAGE = storage;
const cache = target === 'ios'
  ? proofCache(`${cacheDir("richos-mobile-ios-proof")}/${key}`, storage, process.env.RICHOS_MOBILE_IOS_TEST_CACHE)
  : createScratch('client-ui-proof');
process.env.RICHOS_MOBILE_CACHE = cache;
// cacheDir already captured the private settings. Apply this suite's explicit
// cache choice to in-process consumers as well as the child CLI environment.
machineEnvironment().RICHOS_MOBILE_CACHE = cache;
cacheRoot(); // Enforce the CLI's external path and symlink checks.
const lock = join(cache, 'suite.lock');
mkdirSync(lock); // Concurrent proofs may not reset one another's fixture.
// This proof, rather than its long-lived editor session, owns the test simulator.
process.env.RICHOS_TEST_DEVICE_OWNER_PID = String(process.pid);
const env = { ...process.env };
let passed = 0;
function cli(...args) {
  const result = spawnSync(process.execPath, [join(mobile, 'cli/mobile.mjs'), ...args], {
    env, encoding: 'utf8', timeout: 300000, maxBuffer: 8 * 1024 * 1024
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, `${args.join(' ')}\n${result.stdout}\n${result.stderr}`);
  const data = JSON.parse(result.stdout);
  assert.equal(data.ok, true);
  return data.result;
}
function pass(message) { passed++; console.log(`  PASS  ${message}`); }
function cleanup() {
  try {
    if (target === 'ios' && existsSync(join(cache, 'device.json'))) {
      const { id, deviceSet } = JSON.parse(readFileSync(join(cache, 'device.json'), 'utf8'));
      const args = ['simctl', ...(deviceSet === 'system' ? [] : ['--set', deviceSet]), 'shutdown', id];
      const result = spawnSync('xcrun', args, { encoding: 'utf8' });
      if (result.status !== 0 && !/current state: Shutdown/.test(result.stderr || '')) throw new Error(`Simulator shutdown failed: ${result.stderr}`);
    }
  } finally {
    rmSync(lock, { recursive: true, force: true });
    if (target === 'client-ui') rmSync(cache, { recursive: true, force: true });
  }
}
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => { cleanup(); process.exit(130); });
try {
  if (target === 'client-ui') {
    for(const engine of ['chromium','webkit']) {
      const client = await (await import('./client-ui.mjs')).clientUI({engine});
      assert(client.rapidTypingPreserved && client.bothThemesFitPhoneWidths);
      assert(client.briefReconnectInvisible);
      assert(client.sendsFollowLatest && client.streamedRepliesFollowLatest && client.deliberateSmallScrollPauses && client.latestButtonResumes && client.olderReadingPreserved && client.lockedCancelCentred);
      pass(`${engine}: native UI follows sends/replies/resizes, respects deliberate scrolls on a settled layout and centres locked Cancel`);
      // Named and printed every run, never folded into a PASS: see client-ui.mjs for each one.
      for (const defect of client.knownDefects || []) console.log(`  KNOWN DEFECT  ${engine}: ${defect}`);
    }
  } else {
    const preview = join(cache,'notification-preview-proof');
    const previewBuild = hostTool('xcrun',['swiftc','-module-cache-path',join(cache,'swift-module-cache'),join(mobile,'ios/Shared/NotificationPreview.swift'),join(mobile,'ios/Tests/NotificationPreviewTests.swift'),'-o',preview]);
    assert.equal(previewBuild.status,0,previewBuild.stderr);
    const previewProof=hostTool(preview,[join(mobile,'test/fixtures/notification-preview.json')]);
    assert.equal(previewProof.status,0,previewProof.stderr);pass('native preview decryption, wrong key and tampered payload refusal');
    const recovery = join(cache, 'recording-recovery-test');
    const compiled = hostTool('xcrun', ['swiftc', '-module-cache-path', join(cache,'swift-module-cache'), join(mobile,'ios/Sources/RecordingRecovery.swift'), join(mobile,'ios/Tests/RecordingRecoveryTests.swift'), '-o', recovery]);
    assert.equal(compiled.status, 0, compiled.stderr);
    const checked = hostTool(recovery, []); assert.equal(checked.status, 0, checked.stderr);
    pass('interrupted WAV recovery preserves audio bytes and rejects other formats');
    cli('sim', 'prepare');
    const verified = cli('sim', 'verify');
    assert.equal(verified.scenarios.length, 3);
    assert(verified.scenarios.every((s) => s.identical));
    assert.equal(verified.processRestartPreservedOutbox, true);
    pass('all native/headless traces agree and native process restart preserves outbox');
    const before = cli('sim', 'state');
    assert.deepEqual(cli('sim', 'refresh').state, before.state);
    pass('current UI assets refresh without losing semantic state');
    const controls = cli('sim', 'ui-test');
    assert.equal(controls.visibleSendVerifiedInCore, true);
    assert.equal(controls.state.outbox[0].text, 'Typed through the visible composer');
    pass(`XCUITest visible composer and Send: ${controls.result}`);
    const client = cli('sim', 'client-prepare');
    assert.equal(client.state.confirmed, false);
    cli('sim', 'action', JSON.stringify({ type: 'compose', text: 'Native client refresh proof' }));
    assert.equal(cli('sim', 'refresh').state.draft, 'Native client refresh proof');
    pass('native client actions and protected-file persistence survive a UI refresh');
    const { policy } = require('../dev/update-fixture.js');
    const revision = Date.now();
    assert.equal(cli('sim', 'policy', JSON.stringify(policy('banner', revision))).state.updates.mode, 'banner');
    assert.equal(cli('sim', 'action', JSON.stringify({ type: 'update-dismiss' })).state.updates.mode, 'none');
    assert.equal(cli('sim', 'policy', JSON.stringify(policy('blocking', revision + 1))).state.updates.mode, 'blocking');
    assert.equal(cli('sim', 'policy', JSON.stringify(policy('none', revision + 2))).state.updates.blocked, false);
    pass('real client update policy decisions and dismissal use the simulator action path');
    assert.equal(cli('check-release').developmentBridgeExcluded, true);
    pass('Release excludes development runtime and native command access');
  }
} catch (error) {
  console.error(`  FAIL  ${label}: ${error.stack}`);
  process.exitCode = 1;
} finally {
  try { cleanup(); }
  catch (error) { console.error(`  FAIL  ${label} cleanup: ${error.stack}`); process.exitCode = 1; }
}
if (!process.exitCode) console.log(`=== ${label} tests: all ${passed} passed ===`);
