#!/usr/bin/env node
import { readFileSync, writeFileSync, existsSync, renameSync, mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { createRequire } from 'node:module';
import * as simulator from './simulator.mjs';
import * as pwa from './pwa.mjs';
import { device } from './device.mjs';
const require = createRequire(import.meta.url);
const { createRuntime } = require('../dev/runtime.js');
const usage = `Mobile development loop (JSON output; nonzero exit on failure)
  node richos/mobile/cli/mobile.mjs connect artifact
  node richos/mobile/cli/mobile.mjs connect managed-artifact <private-profile.json>
  node richos/mobile/cli/mobile.mjs client scenario connection-restart|recording-interruption|voice-restart|voice-gestures|update-controls
  node richos/mobile/cli/mobile.mjs lab serve|mac [--manual]|updates
  node richos/mobile/cli/mobile.mjs device          RETIRED: a physical iPhone is reached only through native-ios/bin/rios device
  node richos/mobile/cli/mobile.mjs update preview|publish <request.json>
  node richos/mobile/cli/mobile.mjs update serve|metrics
  node richos/mobile/cli/mobile.mjs update worker-artifact <private-policy-profile.json>
  RICHOS_POLICY_PROFILE=<profile.json> RICHOS_POLICY_TOKEN_FILE=<0600 file> node richos/mobile/cli/mobile.mjs update schema-hosted
  RICHOS_POLICY_PROFILE=<profile.json> RICHOS_POLICY_TOKEN_FILE=<0600 file> node richos/mobile/cli/mobile.mjs update publish-hosted <request.json>
  node richos/mobile/cli/mobile.mjs report artifact <private-report-profile.json>
  RICHOS_REPORT_PROFILE=<profile.json> RICHOS_REPORT_TOKEN_FILE=<0600 file> node richos/mobile/cli/mobile.mjs report schema|pending
  RICHOS_REPORT_PROFILE=<profile.json> RICHOS_REPORT_TOKEN_FILE=<0600 file> node richos/mobile/cli/mobile.mjs report control off | control on '<operator>' <daily cap>
  RICHOS_REPORT_PROFILE=<profile.json> RICHOS_REPORT_TOKEN_FILE=<0600 file> node richos/mobile/cli/mobile.mjs report resolve <report ID> <issue number>
  node richos/mobile/cli/mobile.mjs release-config-check
  node richos/mobile/cli/mobile.mjs sim policy '<policy JSON>'
  node richos/mobile/cli/mobile.mjs doctor
  node richos/mobile/cli/mobile.mjs headless state|reset|restart
  node richos/mobile/cli/mobile.mjs headless fixture offline|online|queued|interrupted|revoked
  node richos/mobile/cli/mobile.mjs headless action '{"type":"compose","text":"Hello"}'
  node richos/mobile/cli/mobile.mjs headless action '{"type":"send"}'
  node richos/mobile/cli/mobile.mjs headless transport accept|unreachable|lose-ack|revoked
  node richos/mobile/cli/mobile.mjs headless advance 1000
  node richos/mobile/cli/mobile.mjs headless scenario offline-reconnect|revoked|interrupted
  node richos/mobile/cli/mobile.mjs sim prepare|client-prepare|refresh|restart|verify|ui-test|screenshot
  node richos/mobile/cli/mobile.mjs sim <same state/action/fixture/scenario commands>
  node richos/mobile/cli/mobile.mjs pwa prepare|state|reset|refresh|restart|verify|screenshot|stop
  node richos/mobile/cli/mobile.mjs pwa fixture offline|online|queued|revoked
  node richos/mobile/cli/mobile.mjs pwa action '<JSON>'
  node richos/mobile/cli/mobile.mjs pwa scenario offline-reconnect
  node richos/mobile/cli/mobile.mjs pwa transport accept|unreachable|revoked
  node richos/mobile/cli/mobile.mjs build Debug|Release
  node richos/mobile/cli/mobile.mjs check-release
Stateful headless commands share a session under the external cache.
Sim prepare builds/installs/boots a dedicated simulator; refresh copies JS/CSS without a native build.
RICHOS_MOBILE_CACHE overrides the per-checkout cache. A selected external volume must be mounted.
RICHOS_MOBILE_TEST_APP=integration selects a separate installed app and cache namespace.
Simulator device data defaults to an external set. Where permitted, set
RICHOS_MOBILE_SIMULATOR_STORAGE=system before its first creation to use Apple's default system device storage. The choice persists per cache.`;
function payload(command, arg) {
  switch (command) {
    case 'state': case 'reset': case 'restart': return { command };
    case 'fixture': case 'scenario': if (!arg) throw new Error(`${command} needs a name`); return { command, name: arg };
    case 'transport': return { command, mode: arg };
    case 'advance': return { command, ms: Number(arg) };
    case 'policy': return { command, policy: JSON.parse(arg) };
    case 'action': return { command, action: JSON.parse(arg) };
    default: throw new Error(`Unknown command: ${command}. Use --help.`);
  }
}
const [mode, command, arg, platform] = process.argv.slice(2);
const start = performance.now();
let lock;
try {
  if (!mode || mode === '--help') { console.log(usage); process.exit(0); }
  // Build's resource script re-enters only this stateless branch, never the session lock.
  if (mode === 'bundle') {
    simulator.stageAssets(command, arg === 'Debug' && platform === 'iphonesimulator', undefined, arg === 'Debug' ? 'Debug' : 'Release');
    process.exit(0);
  }
  const cache = simulator.cacheRoot();
  const proposedLock = join(cache, mode === 'lab' ? 'lab.cli.lock' : mode === 'pwa' ? 'pwa.cli.lock' : mode === 'update' ? (command === 'serve' ? 'update-service.cli.lock' : 'update-admin.cli.lock') : 'cli.lock');
  try { mkdirSync(proposedLock); lock = proposedLock; writeFileSync(join(lock, 'owner.json'), JSON.stringify({ pid: process.pid, mode, command })); }
  catch { throw new Error(`Another mobile CLI command owns ${proposedLock}. If it crashed, verify its owner.json PID is gone before removing that directory.`); }
  let result;
  if (mode === 'connect') result = await (await import('./connect.mjs')).command(command, arg);
  else if (mode === 'report') result = await (await import('./report.mjs')).command(command, arg, process.argv.slice(5));
  else if (mode === 'update') result = await (await import('../service/policy.mjs')).command(command, arg, cache);
  else if (mode === 'client' && command === 'scenario') result = await require('../dev/client-runtime.js').scenario(arg);
  else if (mode === 'lab' && command === 'updates') result = await (await import('../dev/policy-lab.mjs')).policyLab(cache);
  else if (mode === 'lab' && command === 'mac') {
    if (arg && arg !== '--manual') throw Error('Expected lab mac [--manual]');
    result = await (await import('../dev/mac-server.mjs')).serveMac(cache, { manual: arg === '--manual' });
  }
  else if (mode === 'lab' && command === 'serve') result = await (await import('../dev/lab.mjs')).serve(cache);
  else if (mode === 'release-config-check') result = (await import('./release.mjs')).configuration(undefined, true);
  else if (mode === 'doctor') result = simulator.doctor();
  else if (mode === 'device') result = await device(command, arg);
  else if (mode === 'build') result = simulator.build(command || 'Debug');
  else if (mode === 'check-release') result = simulator.checkRelease();
  else if (mode === 'headless') {
    const file = join(cache, 'headless.json');
    const runtime = await createRuntime({ initial: existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : undefined,
      save: async (data) => { const temp = file + '.new'; writeFileSync(temp, JSON.stringify(data)); renameSync(temp, file); } });
    result = await runtime.execute(payload(command, arg));
  } else if (mode === 'pwa') {
    result = await pwa.command(['prepare', 'refresh', 'verify', 'screenshot', 'stop'].includes(command)
      ? { command } : payload(command, arg));
  } else if (mode === 'sim') {
    const native = { prepare: simulator.prepare, 'client-prepare': simulator.prepareClient, refresh: simulator.refresh, restart: simulator.restart,
      verify: simulator.verify, 'ui-test': simulator.uiTest, screenshot: simulator.screenshot };
    result = native[command] ? await native[command]() : await simulator.request(payload(command, arg));
  } else throw new Error(`Unknown mode: ${mode}. Use --help.`);
  console.log(JSON.stringify({ ok: true, mode, command: command || mode, elapsedMs: +(performance.now() - start).toFixed(2), result }, null, 2));
} catch (error) {
  console.error(JSON.stringify({ ok: false, error: error.message, elapsedMs: +(performance.now() - start).toFixed(2) }));
  process.exitCode = 1;
} finally { if (lock) rmSync(lock, { recursive: true }); }
