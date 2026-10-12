import { cacheDir, validateStorage } from '../../cli/machine-settings.mjs';
// Physical native checks share the existing USB/process supervisor. The app is
// Release, normally paired and never receives fixture arguments or lab secrets.
//
// THE TEST PHONE KEEPS ITS APPS (CEO, 2026-10-01: the iPhone is dedicated to testing). The app and
// its XCUITest runner (dev.richos.connect.uitests.xctrunner) stay installed permanently: no tool
// uninstalls or wipes them, and their bytes change only when their committed source changed. So
// every committed source tree is built and signed ONCE, into a store shared by every checkout
// (RICHOS_PHYSICAL_STORE), and every later `build` or `verify` of the same tree reuses those exact
// signed bytes, checked by hash, instead of building and signing again in its own per-checkout
// cache. xcodebuild still hands the products to the phone at each session; with unchanged bytes
// that is an empty binary delta ("App installation will use binary delta information"), and the
// runner's identity on the phone does not change. Uncommitted source builds in the checkout's own
// cache as before and is never published.
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, statSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runDeviceProcess } from '../../cli/device-runner.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const checks = {
  pairing: 'testPairAndDeliverAcrossHomeAndRelaunch',
  text: 'testDeliverAcrossHomeAndRelaunch',
  voice: 'testSpokenMessageReachesMacAndReturnsReply',
  notifications: 'testReplyNotificationAfterHomeAndTermination',
  quiet: 'testScrollAndQuietBackgroundReturn',
  recording: 'testActualMicrophonePreservesUnsentAudioAfterHomeAndTermination',
  // A walker's list of real-control steps (richos/app/scripts/qa/phone-ios.py).
  script: 'testScript',
};

export const DEFAULT_STORE = cacheDir('richos-native-ios/physical-store', 'RICHOS_PHYSICAL_STORE');
// A signed Release device build, counted from native-work's admission: the wait before it (a machine
// worker, the build lane, CPU headroom) has its own bound, device-runner's ADMISSION_LIMIT_MS.
export const BUILD_LIMIT_MS = 600000;
const APP = 'Release-iphoneos/RichOSNative.app';

// THE TEST COPY ONLY. Everything this tool builds, installs, stamps or verifies is a separate copy of the Release app
// under its own ID, installed beside the CEO's own RichConnect with its own data (richos/mobile/perf/test_copy.py is
// the one definition; mobile-perf.test.py TC1 checks these values against it). The CEO handles his own app himself:
// nothing here ever builds, installs or opens `dev.richos.connect`, and a bundle that carries that ID is refused.
export const CEO_BUNDLE = 'dev.richos.connect';
export const TEST_BUNDLE = 'dev.richos.connect.perf';
export const TEST_NAME = 'RichConnect Perf';
// The test copy is signed without an App Group (its group is never registered for the team), so it names its own entitlements files.
export const TEST_COPY_ENTITLEMENTS = ['RICHOS_APP_ENTITLEMENTS=RichOSNative.testcopy.entitlements', 'RICHOS_SHARE_ENTITLEMENTS=ShareExtension.testcopy.entitlements'];
export function testCopyOnly(bundleId) {
  if (bundleId === CEO_BUNDLE) throw Error(`Refused: this is the CEO's own RichConnect (${CEO_BUNDLE}); he handles it himself. The device tools install only the test copy (${TEST_BUNDLE})`);
  if (bundleId !== TEST_BUNDLE) throw Error(`Refused: the bundle is ${bundleId}, not the test copy (${TEST_BUNDLE})`);
  return bundleId;
}
// The bundle ID a built app declares (its Info.plist), read the way the signed bytes carry it.
export function bundleIdOf(app, run = execFileSync) {
  const out = run('python3', ['-c', 'import plistlib,sys,pathlib; print(plistlib.loads((pathlib.Path(sys.argv[1])/"Info.plist").read_bytes())["CFBundleIdentifier"])', app], { encoding: 'utf8' });
  return String(out).trim();
}
const RUNNER = 'Release-iphoneos/RichOSNativeUITests-Runner.app';

// A script may hold a timed background interval; its allowance is bounded, never open-ended.
export function allowanceSeconds(config) {
  const seconds = config?.allowanceSeconds === undefined ? 240 : Number(config.allowanceSeconds);
  if (!Number.isInteger(seconds) || seconds < 60 || seconds > 1800) throw Error('allowanceSeconds must be a whole number from 60 to 1800');
  return seconds;
}

export function configuration(env, command, selection) {
  if (!['build', 'install', 'stamp', 'verify'].includes(command)) throw Error('device build | device install | device stamp | device verify pairing|text|recording|quiet|voice|notifications|script');
  if (!/^[A-Fa-f0-9-]{20,64}$/.test(env.RICHOS_IOS_DEVICE || '')) throw Error('Set RICHOS_IOS_DEVICE to the connected physical UDID');
  if (!/^[A-Z0-9]{10}$/.test(env.RICHOS_APPLE_TEAM || '')) throw Error('Set RICHOS_APPLE_TEAM to the signing team');
  if (command === 'verify' && !checks[selection]) throw Error('Choose one named physical check');
  return { device: env.RICHOS_IOS_DEVICE, team: env.RICHOS_APPLE_TEAM, test: checks[selection] };
}

export function verifyPushEnvironment(configured, signed) {
  if (!['development', 'production'].includes(configured) || configured !== signed) {
    throw Error('APNs registration environment must match the signed aps-environment entitlement');
  }
}

// ONLY THE RELEASE BUILD GOES ON THE PHONE (CEO, 2026-10-02). The bundle about to be installed is
// read by richos/mobile/physical.py: a Debug bundle carries every development marker `rios sim
// check-release` proves, a Release bundle none; anything else is refused with the sentence, before
// the phone is touched. The products here are built `-configuration Release`; this is the proof.
export function releaseOnly(app, run = spawnSync) {
  const checked = run('python3', ['-B', physicalTool, 'ios-app', app], { encoding: 'utf8' });
  if (checked.status !== 0) {
    let why = (checked.stderr || checked.stdout || '').trim();
    try { const parsed = JSON.parse(why); why = parsed.refused || parsed.error || why; } catch { /* the raw text */ }
    throw Error(`Refused: ${why}`);
  }
  return JSON.parse(checked.stdout).result;
}

// The grammar richos/mobile/perf/watch.py calls: `--device ID` names the phone (instead of
// RICHOS_IOS_DEVICE) and `--expect-commit SHA` refuses unless the products are that commit's.
export function deviceArgs(args, env) {
  const rest = [];
  let device = env.RICHOS_IOS_DEVICE, expect = null, push = null;
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--device') device = args[++i];
    else if (args[i] === '--expect-commit') expect = args[++i];
    else if (args[i] === '--push') push = args[++i];
    else rest.push(args[i]);
  }
  return { args: rest, env: { ...env, RICHOS_IOS_DEVICE: device }, expect, push };
}

// `install --push production`: the test copy signed the way a store build is, so its push token is a
// PRODUCTION one (CEO 2026-10-04: a reviewer closes the app and waits for the reply's notification, and that
// path had never been seen working). Every other build here is development-signed, whose token is a
// sandbox one. The archive is the store's own recipe (Release/README.md: archive with Apple Development,
// then the export re-signs); the export is ad hoc, with the profiles Tools/adhoc-signing.ts keeps through
// the App Store Connect API, because only an ad hoc profile installs on a test phone and carries production push.
export function pushMode(command, push) {
  if (push === null || push === undefined) return null;
  if (command !== 'install') throw Error('--push is only for `device install`');
  if (push !== 'production') throw Error('--push takes only production (every other install is development-signed already)');
  return push;
}

export const ADHOC_SUFFIXES = ['', '.notification-service', '.share'];

// The export options for the ad hoc export: manual signing with the named profile of each bundle the app carries.
export function adhocExportOptions(team, profiles) {
  const map = {};
  for (const suffix of ADHOC_SUFFIXES) {
    const id = TEST_BUNDLE + suffix;
    if (!profiles[id]?.name) throw Error(`no ad hoc profile for ${id}`);
    map[id] = profiles[id].name;
  }
  return { method: 'release-testing', signingStyle: 'manual', signingCertificate: 'Apple Distribution', teamID: team,
    provisioningProfiles: map, thinning: '<none>', destination: 'export', manageAppVersionAndBuildNumber: false };
}

// xcodebuild's `-destination id=` knows a phone only by its hardware UDID (8-16 hex); devicectl,
// and so `--device` from richos/mobile/perf/watch.py, uses the CoreDevice identifier (a UUID).
// Handing xcodebuild the CoreDevice identifier is "Unable to find a device matching the provided
// destination specifier" (the 2026-10-02 17:22Z phone speed run). `devices` is devicectl's
// `list devices` JSON `result.devices`; devicectl accepts the hardware UDID too, so one spelling serves both.
export function hardwareUdid(device, devices) {
  const want = String(device).toLowerCase();
  for (const d of devices) {
    const udid = d.hardwareProperties?.udid;
    if (udid && (String(d.identifier).toLowerCase() === want || udid.toLowerCase() === want)) return udid;
  }
  if (/^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{16}$|^[0-9A-Fa-f]{40}$/.test(device)) return device;
  throw Error('Refused: that iPhone is not in `xcrun devicectl list devices`; connect it with its cable and unlock it once');
}

// `phone_net.py ensure`: the phone's Wi-Fi carries traffic (one restart if not). Refuses when it still does not.
export function phoneWifi(device, env, run = spawnSync) {
  const checked = run('python3', ['-B', resolve(root, '../phone_net.py'), 'ensure', '--device', device],
    { encoding: 'utf8', env, timeout: 900000, stdio: ['ignore', 'pipe', 'inherit'] });
  let result = null;
  try { result = JSON.parse((checked.stdout || '').trim().split('\n').pop()); } catch { /* said below */ }
  if (checked.status !== 0 || !result?.wifiCarriesTraffic) {
    throw Error(`Installed, but the iPhone's Wi-Fi carries no traffic, so iOS could not verify the app on its first open: ${result?.detail || (checked.stdout || '').trim() || 'phone_net.py ensure failed'}`);
  }
  return result;
}

function listedDevices() {
  const dir = mkdtempSync(join(tmpdir(), 'rios-devices-'));
  try {
    execFileSync('xcrun', ['devicectl', 'list', 'devices', '--json-output', join(dir, 'd.json')], { stdio: 'ignore', timeout: 60000 });
    return JSON.parse(readFileSync(join(dir, 'd.json'), 'utf8')).result.devices;
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

// A store entry is named by the FIRST commit that built its tree, so a later commit with the same
// app sources has the same bytes: `--expect-commit` compares the build TREE of that commit with
// the tree being installed, never the commit names. Uncommitted sources match no commit.
export function sameTree({ expect, wanted, head }) {
  if (head.dirty) throw Error(`Refused: --expect-commit ${expect}, but this checkout's app sources have uncommitted changes; they match no commit`);
  if (wanted !== head.tree) throw Error(`Refused: freshness mismatch: the app sources of ${expect} are not this checkout's (build tree ${String(wanted).slice(0, 12)}…, here ${String(head.tree).slice(0, 12)}…); nothing was built or installed`);
}

// Only `verify` may reuse this checkout's own earlier products on request; `build` never does.
export function prebuilt(env, command) {
  return command === 'verify' && env.RICHOS_PHYSICAL_PREBUILT === '1';
}

// One store entry per (committed source tree, signing team, Xcode). Anything that changes the
// signed bytes is in the key; nothing else is, so the same tree in another checkout finds it.
export function storeKey({ tree, team, xcode, bundle = TEST_BUNDLE }) {
  if (!/^[0-9a-f]{40,64}$/.test(tree || '')) throw Error('storeKey needs the git tree id of the native-ios sources');
  if (!team || !xcode) throw Error('storeKey needs the signing team and the Xcode version');
  return createHash('sha256').update(`${tree}\n${team}\n${xcode}\n${bundle}`).digest('hex').slice(0, 24);
}

// Entries of native-ios that are not build inputs: tooling and prose beside the app. A change to
// them alone must not re-sign the app or put new runner bytes on the phone.
export const NOT_BUILD_INPUTS = ['Tools', 'docs', 'bin', 'README.md'];

// The identity of the build inputs: `git ls-tree` of native-ios without NOT_BUILD_INPUTS.
export function buildTree(lsTree) {
  const kept = lsTree.split('\n').filter(Boolean).filter(line => !NOT_BUILD_INPUTS.includes(line.split('\t')[1]));
  if (!kept.length) throw Error('buildTree: git ls-tree listed no build inputs');
  return createHash('sha256').update(kept.join('\n')).digest('hex');
}

// What one run does about its products, decided before anything is built or installed.
//   store    the committed tree was built before: reuse those exact signed bytes (no build, no signing)
//   checkout RICHOS_PHYSICAL_PREBUILT=1: this checkout's own earlier products (stamp checked by the caller)
//   publish  committed tree never built: build it once, then publish it to the store for everyone
//   local    uncommitted changes: build in this checkout's cache, never published
export function plan({ command, env, source, stored }) {
  if (prebuilt(env, command)) return 'checkout';
  if (source.dirty) return 'local';
  return stored ? 'store' : 'publish';
}

// The identity a store entry was published with, or null when there is none. A present entry whose
// bytes no longer hash to its identity is refused, never rebuilt over: identity or refuse.
export function stored(store, key, hash) {
  const entry = join(store, key);
  const file = join(entry, 'identity.json');
  if (!existsSync(file)) return null;
  const identity = JSON.parse(readFileSync(file, 'utf8'));
  const products = join(entry, 'Products');
  for (const [name, rel] of [['app', APP], ['runner', RUNNER]]) {
    const actual = hash(join(products, rel));
    if (actual !== identity[name]) {
      throw Error(`freshness mismatch: the stored ${name} ${join(products, rel)} hashes ${String(actual).slice(0, 12)}…, `
        + `published as ${String(identity[name]).slice(0, 12)}…. Refusing to install changed bytes under an unchanged source; `
        + `move ${entry} aside by hand to build that tree once more`);
    }
  }
  return { ...identity, products };
}

// Publish a finished build: copy into a private directory, then one rename. The first publisher of a
// key wins; a later one discards its copy and uses the winner's bytes, so every run of one tree
// installs the same signed runner.
export function publish(store, key, products, identity, deps) {
  const entry = join(store, key);
  if (existsSync(join(entry, 'identity.json'))) return stored(store, key, deps.hash);
  mkdirSync(store, { recursive: true });
  const staging = join(store, `.${key}.${process.pid}.${Date.now()}`);
  mkdirSync(staging);
  try {
    deps.copy(products, join(staging, 'Products'));
    const full = { ...identity, key, app: deps.hash(join(staging, 'Products', APP)),
      runner: deps.hash(join(staging, 'Products', RUNNER)), publishedAt: new Date().toISOString() };
    writeFileSync(join(staging, 'identity.json'), JSON.stringify(full, null, 1));
    // The perf.py stamp shape, so `phone-ios.py run --prebuilt --stamp` can name this entry too.
    writeFileSync(join(staging, 'stamp.json'), JSON.stringify({ artifact: join(entry, 'Products', APP), sha256: full.app,
      commit: identity.commit, dirty: false, paths: ['richos/mobile/native-ios'], stampedAt: full.publishedAt }, null, 1));
    try {
      renameSync(staging, entry);
    } catch (error) {
      if (!existsSync(join(entry, 'identity.json'))) throw error;
    }
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
  prune(store, key);
  return stored(store, key, deps.hash);
}

// The store stays bounded: past the newest KEEP entries, an entry untouched for a day is removed.
// A day-old entry is not one a running check is using, and removing it only means that tree would
// be built once more if it is ever checked out again.
export const KEEP = 8;
export function prune(store, keep, now = Date.now()) {
  const entries = readdirSync(store).filter(name => /^[0-9a-f]{24}$/.test(name))
    .map(name => ({ name, at: statSync(join(store, name, 'identity.json'), { throwIfNoEntry: false })?.mtimeMs ?? 0 }))
    .sort((a, b) => b.at - a.at);
  const removed = [];
  for (const entry of entries.slice(KEEP)) {
    if (entry.name === keep || now - entry.at < 86400000) continue;
    rmSync(join(store, entry.name), { recursive: true, force: true });
    removed.push(entry.name);
  }
  return removed;
}

// The products a prebuilt run hands the phone. `phone-ios.py run --prebuilt --stamp` names the
// Build/Products directory of the app it checked against its stamp (RICHOS_PHYSICAL_PRODUCTS), so the
// phone gets exactly those bytes: this checkout's own earlier build, or a shared-store entry such as
// the one a measured Release build came from. Without the name, this checkout's own build.
export function prebuiltProducts(env, own) {
  const named = env.RICHOS_PHYSICAL_PRODUCTS;
  if (!named) return own;
  for (const rel of [APP, RUNNER]) {
    if (!existsSync(join(named, rel))) throw Error(`RICHOS_PHYSICAL_PRODUCTS ${named} has no ${rel}`);
  }
  return named;
}

// Products for this run, and how they were obtained. `deps` are the side effects (git, Xcode, the
// build itself, hashing, copying), injected so the decision is provable without a phone.
export async function resolveProducts({ command, env, team, derived, store }, deps) {
  const source = deps.source();
  const own = join(derived, 'Build/Products');
  if (prebuilt(env, command)) {
    const products = prebuiltProducts(env, own);
    if (!existsSync(join(products, APP))) throw Error('RICHOS_PHYSICAL_PREBUILT=1 but there is no earlier build to reuse; run once without it');
    return { mode: 'checkout', products };
  }
  const key = source.dirty ? null : storeKey({ tree: source.tree, team, xcode: deps.xcode() });
  const entry = key ? stored(store, key, deps.hash) : null;
  const mode = plan({ command, env, source, stored: entry });
  if (mode === 'store') return { mode, key, products: entry.products, identity: entry };
  await deps.build();
  if (mode === 'local') return { mode, products: own };
  const published = publish(store, key, own, { tree: source.tree, commit: source.commit, team, xcode: deps.xcode() }, deps);
  return { mode, key, products: published.products, identity: published };
}

function usb() {
  return execFileSync('ioreg', ['-p', 'IOUSB', '-w0'], { encoding: 'utf8', timeout: 5000 })
    .split('\n').filter(line => line.includes('<class IOUSBHostDevice,'))
    .map(line => line.match(/\+-o (.*?)\s+<class IOUSBHostDevice, id (.*?),/)?.slice(1).join(':')).filter(Boolean);
}

function git(...args) {
  return execFileSync('git', ['-C', root, ...args], { encoding: 'utf8' }).trim();
}

const perf = resolve(root, '../perf');
const physicalTool = resolve(root, '../physical.py');
const hashTree = path => existsSync(path)
  ? execFileSync('python3', ['-B', '-c', 'import sys; sys.path.insert(0, sys.argv[1]); import perfcore; print(perfcore.tree_sha256(sys.argv[2]))', perf, path], { encoding: 'utf8' }).trim()
  : 'absent';

export async function main(args, env = process.env) {
  // The phone is touched only through `rios device ...` (bin/rios sets this; physical.py).
  if (env.RICHOS_DEVICE_VERB !== 'rios') throw Error('A physical iPhone is touched only through `rios device ...`, which puts only the Release build on it (CEO 2026-10-02)');
  const parsed = deviceArgs(args, env);
  env = parsed.env;
  const [command, selection] = parsed.args;
  const settings = configuration(env, command, selection);
  const pushTarget = pushMode(command, parsed.push);
  if (command !== 'stamp') settings.device = hardwareUdid(settings.device, listedDevices());
  const cache = join(env.RICHOS_NATIVE_IOS_CACHE, 'physical');
  mkdirSync(cache, { recursive: true });
  validateStorage(realpathSync(cache));
  const store = env.RICHOS_PHYSICAL_STORE || DEFAULT_STORE;
  validateStorage(resolve(store));
  if (env.RICHOS_PHYSICAL_PRODUCTS) validateStorage(resolve(env.RICHOS_PHYSICAL_PRODUCTS));
  let config;
  if (command === 'verify') {
    const file = env.RICHOS_MOBILE_TEST_CONFIG;
    if (!file) throw Error('Set RICHOS_MOBILE_TEST_CONFIG to the isolated lab JSON');
    validateStorage(realpathSync(file));
    config = JSON.parse(readFileSync(file, 'utf8'));
    if (config.isolatedLab !== 'true') throw Error('The physical test configuration must explicitly mark isolatedLab: true (string)');
    if (selection === 'pairing' && (!config.pairLink?.startsWith('https://') || !config.words?.trim())) throw Error('Pairing needs a current HTTPS pairing link and exact fingerprint words');
    if (selection === 'voice' && !config.spokenPhrase?.trim()) throw Error('Voice requires a unique spokenPhrase expected from actual microphone capture');
    if (selection === 'notifications' && config.delayedReplies !== 'true') throw Error('Notifications require an isolated Mac configured to delay replies until after Home');
    if (selection === 'script' && (typeof config.steps !== 'string' || !config.steps.trim())) throw Error('A script check needs steps: a JSON list, as a string');
    config = Object.fromEntries(Object.entries(config).filter(([, value]) => typeof value === 'string'));
  }
  const native = resolve(root, '../../engine/scripts/lib/native-work.py');
  const stamp = Date.now();
  const log = join(cache, `${command}-${selection || 'app'}-${stamp}.log`);
  const result = join(cache, `${selection}-${stamp}.xcresult`);
  const derived = join(cache, 'derived-test-copy');
  const baseline = usb();
  const health = async () => {
    const current = usb();
    if (baseline.some(device => !current.includes(device))) throw Error('USB device disappeared or reset; stopped without retry or USB reset');
  };
  let xcode;
  // --full-tree: from a subdirectory, ls-tree would otherwise filter the subtree by that
  // subdirectory's own path and list nothing.
  const treeOf = rev => buildTree(git('ls-tree', '--full-tree', git('rev-parse', `${rev}:./`)));
  const source = () => ({ tree: treeOf('HEAD'), commit: git('rev-parse', 'HEAD'),
    dirty: git('status', '--porcelain', '--', '.', ...NOT_BUILD_INPUTS.map(name => `:(exclude)${name}`)) !== '' });
  const xcodeVersion = () => (xcode ??= execFileSync('xcodebuild', ['-version'], { encoding: 'utf8', env }).trim().replace(/\s+/g, ' '));
  if (parsed.expect) sameTree({ expect: parsed.expect, wanted: treeOf(parsed.expect), head: source() });
  if (command === 'stamp') {
    // `rios device perf`'s stamp: the store's Release bytes for this tree, never a build. Its commit
    // is the one asked about (same build tree, sameTree above); builtFrom names the commit that built it.
    const head = source();
    if (head.dirty) throw Error('Refused: uncommitted app sources have no published build to measure; commit, then `rios device install`');
    const entry = stored(store, storeKey({ tree: head.tree, team: settings.team, xcode: xcodeVersion() }), hashTree);
    if (!entry) throw Error('Refused: this tree has no Release build in the store yet; run `rios device install` first');
    const app = join(entry.products, APP);
    testCopyOnly(bundleIdOf(app));
    releaseOnly(app);
    const file = join(cache, `stamp-${head.tree.slice(0, 12)}.json`);
    writeFileSync(file, JSON.stringify({ artifact: app, sha256: entry.app, commit: parsed.expect || head.commit, tree: head.tree,
      builtFrom: entry.commit, dirty: false, paths: ['richos/mobile/native-ios'], stampedAt: new Date().toISOString() }, null, 1));
    return { stamp: file };
  }
  if (pushTarget) return installProduction({ settings, env, cache, native, health, log, head: source() });
  const resolved = await resolveProducts({ command, env, team: settings.team, derived, store }, {
    source,
    xcode: xcodeVersion,
    hash: hashTree,
    copy: (from, to) => execFileSync('ditto', [from, to]),
    build: async () => {
      // The project is needed only to build; a reused build never regenerates it.
      const project = execFileSync(join(root, 'Release/generate.sh'), [join(cache, 'project')], { encoding: 'utf8', env }).trim();
      const base = ['-project', project, '-scheme', 'RichOSPhysical', '-configuration', 'Release',
        '-destination', `id=${settings.device}`, '-derivedDataPath', derived,
        `DEVELOPMENT_TEAM=${settings.team}`, 'CODE_SIGN_STYLE=Automatic', 'CODE_SIGN_IDENTITY=Apple Development',
        'RICHOS_APS_ENVIRONMENT=development', `RICHOS_BUNDLE_ID=${TEST_BUNDLE}`, `RICHOS_APP_DISPLAY_NAME=${TEST_NAME}`, ...TEST_COPY_ENTITLEMENTS, '-allowProvisioningUpdates', '-allowProvisioningDeviceRegistration'];
      // The limit starts when native-work admits the build (`admission: true`), never at the queue:
      // on 2026-10-02 the wait for the Mac's CPU line was 557 s of a 600 s limit counted from the
      // queue, and the Release build (one Swift job) was stopped mid-compile ("BUILD INTERRUPTED").
      const built = await runDeviceProcess('python3', ['-B', native, '--', 'xcodebuild', 'build-for-testing', ...base],
        { log, env, health, timeoutMs: BUILD_LIMIT_MS, admission: true });
      if (built.status !== 0) throw Error(`Device build failed: ${log}`);
    },
  });
  const products = resolved.products;
  const app = join(products, APP);
  const build = { mode: resolved.mode, key: resolved.key ?? null, products, commit: resolved.identity?.commit ?? null };
  const push = JSON.parse(execFileSync('python3', ['-c', `import json,plistlib,pathlib,subprocess,sys
p=pathlib.Path(sys.argv[1]);info=plistlib.loads((p/'Info.plist').read_bytes())
signed=plistlib.loads(subprocess.check_output(['codesign','-d','--entitlements',':-',str(p)],stderr=subprocess.DEVNULL))
print(json.dumps([info.get('RichOSAPNsEnvironment'),signed.get('aps-environment')]))`, app], { encoding: 'utf8', env }));
  verifyPushEnvironment(...push);
  testCopyOnly(bundleIdOf(app));  // before anything is installed or run: a build that carries the CEO's ID is refused
  build.configuration = releaseOnly(app).configuration;
  // A reused build wrote no build log; never name one that does not exist.
  if (!existsSync(log)) build.log = null;
  if (command === 'build') return { log: build.log === null ? null : log, app, build };
  if (command === 'install') {
    // Over the installed app: the bundle is replaced in place and the app's data kept; it is never removed.
    execFileSync('xcrun', ['devicectl', 'device', 'install', 'app', '--device', settings.device, app], { encoding: 'utf8', env, timeout: 600000 });
    // The first open of the installed app is verified by iOS online (Apple's PPQ check for this team's
    // development profile, PPQCheck true). A phone joined to Wi-Fi whose Wi-Fi carries no traffic refuses
    // it: "Unable to Verify App". So every install ends with the phone's Wi-Fi proved to carry traffic,
    // after one restart of the phone (no app opened) when it did not (richos/mobile/phone_net.py).
    return { installed: app, build, wifi: phoneWifi(settings.device, env) };
  }
  const spec = join(cache, `physical-${stamp}.xctestrun`);
  // Rewrite only the runner's environment and paths. Private config is fed via stdin,
  // never process arguments, app launch arguments or a committed test resource.
  // `screenRecording: "true"` keeps XCTest's own recording of the phone's screen for the whole
  // session (recorded ON the phone by XCTest, as Xcode's test reports do; nothing on this Mac asks
  // for camera or screen access). Off by default: the runner's other sessions keep deleting it.
  // RUNNER ONLY (CEO 2026-10-03: "I will handle everything regarding my app myself"): a script whose every step
  // acts in Safari, Settings or SpringBoard needs no app under test. xcodebuild refuses a UI test with no
  // UITargetAppPath ("UITargetAppPath should be provided", measured 2026-10-03), so the runner itself is named the
  // target: RichConnect's bundle is named nowhere, xcodebuild hands the phone the runner alone, and the installed
  // RichConnect is not given its bundle again.
  const runnerOnly = command === 'verify' && selection === 'script' && env.RICHOS_PHYSICAL_RUNNER_ONLY === '1';
  prepareSpec({ products, spec, test: settings.test, record: config?.screenRecording === 'true', runnerOnly, config, env });
  const allowance = allowanceSeconds(config);
  const testLog = log.replace('.log', '-test.log');
  try {
    const tested = await endedSession(() => runDeviceProcess('python3', ['-B', native, '--', 'xcodebuild', 'test-without-building',
      '-xctestrun', spec, '-destination', `id=${settings.device}`, '-resultBundlePath', result,
      '-test-timeouts-enabled', 'YES', '-maximum-test-execution-time-allowance', String(allowance)],
    { log: testLog, env, health, timeoutMs: (allowance + 60) * 1000, admission: true }),
    () => endAutomationUI(settings.device, env, testLog));
    if (tested.status !== 0) throw Error(`Physical check failed: ${result}. No retry attempted.`);
    const summary = JSON.parse(execFileSync('xcrun', ['xcresulttool', 'get', 'test-results', 'summary', '--path', result, '--format', 'json'], { encoding: 'utf8', env }));
    writeFileSync(result + '.summary.json', JSON.stringify(summary, null, 2));
    if (summary.passedTests !== 1 || summary.failedTests !== 0 || summary.skippedTests !== 0 || summary.totalTestCount !== 1) throw Error(`Selected physical check not proved: ${result}`);
    return { passed: 1, result, log: build.log === null ? null : log, app, build, automationUI: tested.automationUI };
  } finally { rmSync(spec, { force: true }); }
}

// The archive, the ad hoc export and the install of `install --push production`. Built each time from this
// checkout (an archive is not a store entry: the store holds development-signed products and their runner).
// The same refusals as every install: the test copy only, the Release build only, and the push environment
// the app registers equal to the one it is signed for, here `production`.
async function installProduction({ settings, env, cache, native, health, log, head }) {
  const work = join(cache, 'adhoc');
  const profiles = join(work, 'profiles');
  const archive = join(work, 'RichOSNative.xcarchive');
  const exported = join(work, 'export');
  for (const path of [archive, exported]) rmSync(path, { recursive: true, force: true });
  mkdirSync(profiles, { recursive: true });
  // 1. The App IDs, Push Notifications, the distribution certificate, the phone and the profiles (App Store Connect API).
  const signing = spawnSync('node', ['--no-warnings', join(root, 'Tools/adhoc-signing.ts'), '--bundle', TEST_BUNDLE,
    '--device', settings.device, '--team', settings.team, '--out', profiles], { encoding: 'utf8', env, timeout: 300000 });
  let signed = null;
  try { signed = JSON.parse((signing.stdout || '').trim().split('\n').pop()); } catch { /* said below */ }
  if (signing.status !== 0 || !signed?.ok) {
    let why = (signing.stderr || signing.stdout || '').trim();
    try { why = JSON.parse(why.split('\n').pop()).error || why; } catch { /* the raw text */ }
    throw Error(`Ad hoc signing through App Store Connect failed: ${why || `exit ${signing.status}`}`);
  }
  // 2. The archive: the store's recipe, with the test copy's identity and production push.
  const project = execFileSync(join(root, 'Release/generate.sh'), [join(cache, 'project')], { encoding: 'utf8', env }).trim();
  const archived = await runDeviceProcess('python3', ['-B', native, '--', 'xcodebuild', 'archive', '-project', project,
    '-scheme', 'RichOSNative', '-configuration', 'Release', '-destination', 'generic/platform=iOS', '-archivePath', archive,
    '-derivedDataPath', join(cache, 'derived-adhoc'), `DEVELOPMENT_TEAM=${settings.team}`, 'CODE_SIGN_STYLE=Automatic',
    'CODE_SIGN_IDENTITY=Apple Development', 'RICHOS_APS_ENVIRONMENT=production', `RICHOS_BUNDLE_ID=${TEST_BUNDLE}`,
    `RICHOS_APP_DISPLAY_NAME=${TEST_NAME}`, ...TEST_COPY_ENTITLEMENTS, '-allowProvisioningUpdates'],
  { log, env, health, timeoutMs: BUILD_LIMIT_MS, admission: true });
  if (archived.status !== 0) throw Error(`Archive failed: ${log}`);
  // 3. The ad hoc export, re-signed with Apple Distribution and the three profiles.
  const options = join(work, 'ExportOptions.plist');
  execFileSync('python3', ['-c', 'import json,plistlib,sys; open(sys.argv[1],"wb").write(plistlib.dumps(json.loads(sys.stdin.read())))', options],
    { input: JSON.stringify(adhocExportOptions(settings.team, signed.result.profiles)) });
  const exportLog = log.replace('.log', '-export.log');
  const exporting = spawnSync('xcodebuild', ['-exportArchive', '-archivePath', archive, '-exportPath', exported,
    '-exportOptionsPlist', options], { encoding: 'utf8', env, timeout: 600000 });
  writeFileSync(exportLog, (exporting.stdout || '') + (exporting.stderr || ''));
  const ipa = existsSync(exported) ? readdirSync(exported).find(name => name.endsWith('.ipa')) : undefined;
  if (exporting.status !== 0 || !ipa) throw Error(`Ad hoc export failed: ${exportLog}`);
  const unpacked = join(exported, 'unpacked');
  execFileSync('ditto', ['-x', '-k', join(exported, ipa), unpacked]);
  const app = join(unpacked, 'Payload/RichOSNative.app');
  // 4. The refusals, before the phone is touched.
  testCopyOnly(bundleIdOf(app));
  const configuration = releaseOnly(app).configuration;
  const push = JSON.parse(execFileSync('python3', ['-c', `import json,plistlib,pathlib,subprocess,sys
p=pathlib.Path(sys.argv[1]);info=plistlib.loads((p/'Info.plist').read_bytes())
signed=plistlib.loads(subprocess.check_output(['codesign','-d','--entitlements',':-',str(p)],stderr=subprocess.DEVNULL))
print(json.dumps([info.get('RichOSAPNsEnvironment'),signed.get('aps-environment'),info.get('RichOSSourceCommit'),info.get('RichOSSourceDirty')]))`, app], { encoding: 'utf8', env }));
  verifyPushEnvironment(push[0], push[1]);
  if (push[1] !== 'production') throw Error(`The export is signed for ${push[1]} push, not production`);
  // 5. Over the installed test copy, data kept; then the phone's Wi-Fi proved, as every install ends.
  execFileSync('xcrun', ['devicectl', 'device', 'install', 'app', '--device', settings.device, app], { encoding: 'utf8', env, timeout: 600000 });
  return { installed: app, push: push[1], configuration, sourceCommit: push[2] ?? head.commit, sourceDirty: push[3] ?? head.dirty,
    ipa: join(exported, ipa), archive, log, exportLog, signing: signed.result, wifi: phoneWifi(settings.device, env) };
}

// THE STUCK INPUT CLIENT (2026-10-04). A UI-test session on the iPhone leaves iOS's AutomationModeUI holding
// one input connection it never reads again, even when xcodebuild ends normally; they pile up until the phone
// stutters (23 on 2026-10-04, failing the speed check). Ending AutomationModeUI removes all of them; iOS starts
// it again for the next session (measured; richos/mobile/physical.py "the input client"). So the session ends,
// however it ends (passed, failed, timed out, interrupted), and then AutomationModeUI is ended. When that
// cannot be done the check fails even if the session passed: what follows on the phone (a timed series)
// would run on a clogged phone, and that must never be reported as slow code.
export async function endedSession(session, end) {
  let outcome, failure, ended, endFailure;
  try { outcome = await session(); } catch (error) { failure = error; }
  try { ended = end(); } catch (error) { endFailure = error; }
  if (failure) {
    if (endFailure) failure.message += `; and after it: ${endFailure.message}`;
    throw failure;
  }
  if (endFailure) throw endFailure;
  return { ...outcome, automationUI: ended };
}

// `physical.py ios-automation-end`: the one place that ends AutomationModeUI. The line goes into the session's log.
export function endAutomationUI(device, env, log = null, run = spawnSync) {
  const done = run('python3', ['-B', physicalTool, 'ios-automation-end', '--device', device],
    { encoding: 'utf8', env, timeout: 300000 });
  let parsed = null;
  try { parsed = JSON.parse((done.status === 0 ? done.stdout : done.stderr) || ''); } catch { /* said below */ }
  if (done.status !== 0 || !parsed?.ok) {
    const why = parsed?.error || parsed?.refused || (done.stderr || done.stdout || '').trim() || `exit ${done.status}`;
    const line = `After the UI-test session, iOS's AutomationModeUI could not be ended: ${why}. It keeps one stuck input client per session until the phone restarts`;
    if (log) appendFileSync(log, `\n${line}\n`);
    throw Error(line);
  }
  if (log) appendFileSync(log, `\nAfter the UI-test session: AutomationModeUI ${parsed.result.ended.length ? `ended (pid ${parsed.result.ended.join(', ')}), and with it its stuck input clients` : 'was not running'}\n`);
  return parsed.result;
}

// The session's test specification: the runner's environment and paths rewritten, nothing else. Private config is
// fed via stdin, never process arguments. With runnerOnly the app under test is named nowhere in it.
export const PREPARE_XCTESTRUN = `import sys,json,pathlib,plistlib
root=pathlib.Path(sys.argv[1]); files=list(root.glob('RichOSPhysical_iphoneos*.xctestrun'))
assert len(files)==1, 'Expected one physical test specification'
x=plistlib.loads(files[0].read_bytes());t=x['RichOSNativeUITests']
t['EnvironmentVariables']['RICHOS_PHYSICAL_CONFIG']=sys.stdin.read()
t['OnlyTestIdentifiers']=['PhysicalDeviceTests/'+sys.argv[3]]
if sys.argv[4]=='record':
    t['PreferredScreenCaptureFormat']='screenRecording'; t['SystemAttachmentLifetime']='keepAlways'
for k in ['TestHostPath','UITargetAppPath']: t[k]=t[k].replace('__TESTROOT__',str(root))
t['DependentProductPaths']=[v.replace('__TESTROOT__',str(root)) for v in t.get('DependentProductPaths',[])]
if sys.argv[5]=='runner-only':
    app=t['UITargetAppPath']; t['UITargetAppPath']=t['TestHostPath']
    t['DependentProductPaths']=[v for v in t['DependentProductPaths'] if v!=app]
    assert not any(v.endswith('/${APP.split('/').pop()}') for v in t['DependentProductPaths']), 'the app is still named'
p=pathlib.Path(sys.argv[2]);p.touch(mode=0o600,exist_ok=False);p.write_bytes(plistlib.dumps(x))`;
export function prepareSpec({ products, spec, test, record, runnerOnly, config, env }) {
  execFileSync('python3', ['-c', PREPARE_XCTESTRUN, products, spec, test, record ? 'record' : '', runnerOnly ? 'runner-only' : ''],
    { input: JSON.stringify(config), env });
}

// Every device tool, for the never-uninstall test: none of them may remove an app from the phone.
export function deviceToolSources() {
  const tools = readdirSync(join(root, 'Tools')).filter(name => /\.(mjs|py|sh)$/.test(name) && !name.includes('.test.'));
  return [...tools.map(name => join(root, 'Tools', name)), join(root, 'bin/rios'),
    resolve(root, '../../app/scripts/qa/phone-ios.py'), resolve(root, '../cli/device-runner.mjs')];
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).then(result => console.log(JSON.stringify({ ok: true, result })))
    .catch(error => { console.error(JSON.stringify({ ok: false, error: error.message })); process.exitCode = 1; });
}
