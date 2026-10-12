import { cacheDir, validateStorage, scratchDir, machineEnvironment } from './machine-settings.mjs';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync, copyFileSync, renameSync, readdirSync, rmSync, realpathSync, statSync } from 'node:fs';
import { join, dirname, resolve, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import { arch } from 'node:os';
import { createHash, randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
import { configuration as releaseConfiguration } from './release.mjs';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
export const mobile = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const key = createHash('sha256').update(mobile).digest('hex').slice(0, 10);
// A separate installed test app keeps previous phone sessions intact.
export const bundleId = process.env.RICHOS_MOBILE_TEST_APP === 'integration' ? 'dev.richos.mobile.integration' : 'dev.richos.mobile.loop';
export function cacheRoot() {
  const base = machineEnvironment().RICHOS_MOBILE_CACHE || join(cacheDir('richos-mobile'), key);
  const path = process.env.RICHOS_MOBILE_TEST_APP === 'integration' ? join(base, 'integration') : base;
  const resolved = validateStorage(resolve(path));
  mkdirSync(resolved, { recursive: true });

  return resolved;
}
export function run(bin, args, { log, allowFailure = false } = {}) {
  const r = spawnSync(bin, args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024,
    env: { ...process.env, CLANG_MODULE_CACHE_PATH: join(cacheRoot(), 'module-cache') } });
  if (log) writeFileSync(log, (r.stdout || '') + (r.stderr || ''));
  if (r.error) throw r.error;
  if (r.status !== 0 && !allowFailure) throw new Error(`${bin} ${args.slice(0, 4).join(' ')} exited ${r.status}: ${(r.stderr || r.stdout || '').slice(-3000)}${log ? `\nFull log: ${log}` : ''}`);
  return r;
}
function devicePolicy() {
  const file = join(cacheRoot(), 'simulator-storage.json');
  const requested = process.env.RICHOS_MOBILE_SIMULATOR_STORAGE;
  if (requested && !['external', 'system'].includes(requested)) throw new Error('RICHOS_MOBILE_SIMULATOR_STORAGE must be external or system');
  const saved = existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')).storage : undefined;
  if (saved && requested && requested !== saved) throw new Error('Use a new RICHOS_MOBILE_CACHE when changing simulator storage policy');
  const storage = saved || requested || 'external';
  const set = storage === 'external' ? join(cacheRoot(), 'devices') : null;
  if (set) mkdirSync(set, { recursive: true });
  if (!saved) writeFileSync(file, JSON.stringify({ storage }));
  return set;
}
const simctl = (...args) => {
  const set = devicePolicy();
  return run('xcrun', ['simctl', ...(set ? ['--set', set] : []), ...args]).stdout.trim();
};
export function stageAssets(destination, development, allowedContainer, build = development ? 'Debug' : 'Release') {
  const allowedRoots = [resolve(cacheRoot()), resolve(scratchDir())];
  const parent = realpathSync(dirname(destination));
  const sandboxDocuments = allowedContainer && realpathSync(join(allowedContainer, 'Documents'));
  if (basename(destination) !== 'mobile-ui' || !(allowedRoots.some(root => parent === root || parent.startsWith(root + '/')) || parent === sandboxDocuments)) {
    throw new Error('Asset staging requires mobile-ui under the selected cache, named scratch or simulator sandbox');
  }
  rmSync(destination, { recursive: true, force: true });
  mkdirSync(destination, { recursive: true });
  const files = { 'queue.js': '../web/web-app/lib/queue.js', 'app.js': 'core/app.js', 'view.js': 'ui/view.js',
    'style.css': 'ui/style.css', 'entry.js': development ? 'dev/entry.js' : 'ui/entry.js' };
  if (development) { files['runtime.js'] = 'dev/runtime.js'; files['client-inspect.js'] = 'dev/client-inspect.js'; files['update-fixture.js'] = 'dev/update-fixture.js'; }
  {
    files['client.html'] = 'ui/client.html';
    Object.assign(files, { 'client.js': 'core/client.js', 'updates.js': 'core/updates.js', 'mobile-links.js': 'core/links.js', 'release-config.json': 'release-config.json', 'client.css': 'ui/client.css', 'styles.css': '../web/web-app/styles.css', 'native.js': 'platform/native.js', 'client-entry.js': 'ui/client-entry.js' });
    for (const name of ['api', 'inbound', 'link', 'fingerprint', 'wordlist', 'thread', 'connection', 'follow', 'notification-target', 'voice']) files[name + '.js'] = '../web/web-app/lib/' + name + '.js';
  }
  for (const [name, source] of Object.entries(files)) copyFileSync(join(mobile, source), join(destination, name));
  writeFileSync(join(destination, 'release-config.json'), JSON.stringify(releaseConfiguration(undefined, false, build)));
  const clientHTML = readFileSync(join(mobile, 'ui/client.html'), 'utf8').replace('<!-- CLIENT_DEV -->', development ? '<script src="update-fixture.js"></script><script src="client-inspect.js"></script>' : '');
  writeFileSync(join(destination, 'client.html'), clientHTML);
  const entry = (development ? '<script src="runtime.js"></script>\n  ' : '') + '<script src="entry.js"></script>';
  writeFileSync(join(destination, 'index.html'), (development ? readFileSync(join(mobile, 'ui/index.html'), 'utf8') : clientHTML).replace('<!-- ENTRY -->', entry));
}
function plist(content) { return `<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict>${content}</dict></plist>`; }
export function project() {
  const cache = cacheRoot();
  const output = join(cache, 'project');
  mkdirSync(output, { recursive: true });
  const release = releaseConfiguration();
  for (const name of ['version', 'build']) if (typeof release[name] !== 'string' || !/^\d+(\.\d+){0,2}$/.test(release[name])) throw Error('Invalid release version or build');
  const catalog = join(output, 'Brand.xcassets'), appIcon = join(catalog, 'AppIcon.appiconset');
  mkdirSync(appIcon, { recursive: true });
  copyFileSync(join(mobile, '../web/web-app/icons/apple-touch-icon.png'), join(appIcon, 'iphone-180.png'));
  writeFileSync(join(appIcon, 'Contents.json'), JSON.stringify({ images: [{ idiom: 'iphone', size: '60x60', scale: '3x', filename: 'iphone-180.png' }], info: { version: 1, author: 'xcode' } }));
  const previewGroup = '$(AppIdentifierPrefix)' + bundleId;
  const previewInfo = '<key>PreviewKeychainGroup</key><string>' + previewGroup + '</string>';
  const base = previewInfo + '<key>CFBundleIdentifier</key><string>$(PRODUCT_BUNDLE_IDENTIFIER)</string><key>CFBundleExecutable</key><string>$(EXECUTABLE_NAME)</string><key>CFBundleName</key><string>RichOS</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>' + release.version + '</string><key>CFBundleVersion</key><string>' + release.build + '</string><key>CFBundleURLTypes</key><array><dict><key>CFBundleURLSchemes</key><array><string>richos</string></array><key>CFBundleURLName</key><string>RichOS conversation</string></dict></array><key>LSRequiresIPhoneOS</key><true/><key>NSCameraUsageDescription</key><string>Scan the pairing code shown on your Mac.</string><key>NSMicrophoneUsageDescription</key><string>Record a voice message for Rich.</string><key>UILaunchScreen</key><dict/><key>UISupportedInterfaceOrientations</key><array><string>UIInterfaceOrientationPortrait</string></array>';
  writeFileSync(join(output, 'Debug.plist'), plist(base));
  writeFileSync(join(output, 'Release.plist'), plist(base));
  const quote = (s) => "'" + s.replaceAll("'", "'\\''") + "'";
  const spec = {
    name: 'RichOSMobile', options: { deploymentTarget: { iOS: '16.7' } },
    settings: { base: { SWIFT_VERSION: '5.0', CODE_SIGNING_ALLOWED: 'YES', CODE_SIGN_IDENTITY: '-', TARGETED_DEVICE_FAMILY: '1', ENABLE_USER_SCRIPT_SANDBOXING: 'NO' } },
    targets: {
      RichOSMobile: { type: 'application', platform: 'iOS', sources: [{path:join(mobile,'ios/Shared')}, ...readdirSync(join(mobile, 'ios/Sources')).filter(name=>name.endsWith('.swift')).sort().map(name=>({path:join(mobile,'ios/Sources',name)})), { path: catalog, buildPhase: 'resources' }],
        settings: { base: { PRODUCT_BUNDLE_IDENTIFIER: bundleId, ASSETCATALOG_COMPILER_APPICON_NAME: 'AppIcon' }, configs: {
          Debug: { INFOPLIST_FILE: join(output, 'Debug.plist'), SWIFT_ACTIVE_COMPILATION_CONDITIONS: 'DEBUG' },
          Release: { INFOPLIST_FILE: join(output, 'Release.plist'), SWIFT_ACTIVE_COMPILATION_CONDITIONS: '' }
        } },
        postBuildScripts: [{ name: 'Bundle shared mobile core and UI', basedOnDependencyAnalysis: false,
          script: `${quote(process.execPath)} ${quote(join(mobile, 'cli/mobile.mjs'))} bundle "$TARGET_BUILD_DIR/$UNLOCALIZED_RESOURCES_FOLDER_PATH/mobile-ui" "$CONFIGURATION" "$PLATFORM_NAME"` }] },
      RichOSNotification: {type:'app-extension',platform:'iOS',sources:[{path:join(mobile,'ios/Shared')},{path:join(mobile,'ios/NotificationExtension')}],settings:{base:{PRODUCT_BUNDLE_IDENTIFIER:bundleId+'.notifications',INFOPLIST_FILE:join(output,'Notification.plist'),APPLICATION_EXTENSION_API_ONLY:'YES',SKIP_INSTALL:'YES'}}},
      RichOSMobileUITests: { type: 'bundle.ui-testing', platform: 'iOS', sources: [{ path: join(mobile, 'ios/UITests') }],
        dependencies: [{ target: 'RichOSMobile' }], settings: { base: { GENERATE_INFOPLIST_FILE: 'YES', PRODUCT_BUNDLE_IDENTIFIER: bundleId + '.uitests', TEST_TARGET_NAME: 'RichOSMobile' } } }
    },
    schemes: { RichOSMobile: { build: { targets: { RichOSMobile: 'all' } }, test: { targets: ['RichOSMobileUITests'], gatherCoverageData: false } } }
  };
  writeFileSync(join(output,'Notification.plist'),plist(previewInfo + '<key>CFBundleIdentifier</key><string>$(PRODUCT_BUNDLE_IDENTIFIER)</string><key>CFBundleExecutable</key><string>$(EXECUTABLE_NAME)</string><key>CFBundleName</key><string>RichOS Notifications</string><key>CFBundlePackageType</key><string>XPC!</string><key>CFBundleShortVersionString</key><string>'+release.version+'</string><key>CFBundleVersion</key><string>'+release.build+'</string><key>NSExtension</key><dict><key>NSExtensionPointIdentifier</key><string>com.apple.usernotifications.service</string><key>NSExtensionPrincipalClass</key><string>$(PRODUCT_MODULE_NAME).NotificationService</string></dict>'));
  const previewEntitlement = '<key>keychain-access-groups</key><array><string>'+previewGroup+'</string></array>';
  const extensionEntitlements = join(output,'Notification.entitlements');
  writeFileSync(extensionEntitlements,plist(previewEntitlement));
  spec.targets.RichOSNotification.settings.base.CODE_SIGN_ENTITLEMENTS=extensionEntitlements;
  spec.targets.RichOSMobile.dependencies=[{target:'RichOSNotification',embed:true}];
  const entitlements = join(output, 'RichOS.entitlements');
  writeFileSync(entitlements, plist(previewEntitlement + '<key>aps-environment</key><string>$(APS_ENVIRONMENT)</string>' +
    (release.universalHosts.length ? '<key>com.apple.developer.associated-domains</key><array>' + release.universalHosts.map(host => '<string>applinks:' + host + '</string>').join('') + '</array>' : '')));
  Object.assign(spec.targets.RichOSMobile.settings.base, {CODE_SIGN_ENTITLEMENTS:entitlements});
  spec.targets.RichOSMobile.settings.configs.Debug.APS_ENVIRONMENT = 'development';
  spec.targets.RichOSMobile.settings.configs.Release.APS_ENVIRONMENT = 'production';
  const testConfig = process.env.RICHOS_MOBILE_TEST_CONFIG;
  if (testConfig) {
    const source = realpathSync(testConfig);
    validateStorage(source);
    JSON.parse(readFileSync(source, 'utf8'));
    const destination = join(output, 'native-test-config.json');
    copyFileSync(source, destination);
    spec.targets.RichOSMobileUITests.sources.push({ path: destination, buildPhase: 'resources' });
  }
  const specPath = join(output, 'project.json');
  const serialized = JSON.stringify(spec, null, 2);
  if (!existsSync(specPath) || readFileSync(specPath, 'utf8') !== serialized || !existsSync(join(output, 'RichOSMobile.xcodeproj'))) {
    writeFileSync(specPath, serialized);
    run('xcodegen', ['generate', '--spec', specPath, '--project', output], { log: join(cache, 'project.log') });
  }
  return join(output, 'RichOSMobile.xcodeproj');
}
export function build(configuration = 'Debug', extra = []) {
  if (!['Debug', 'Release'].includes(configuration)) throw new Error('Configuration must be Debug or Release');
  const cache = cacheRoot();
  const log = join(cache, `build-${configuration}.log`);
  const args = ['-project', project(), '-scheme', 'RichOSMobile', '-configuration', configuration,
    '-derivedDataPath', join(cache, 'derived-data'), '-sdk', 'iphonesimulator',
    '-destination', 'generic/platform=iOS Simulator', 'CODE_SIGNING_ALLOWED=YES', 'CODE_SIGN_IDENTITY=-', ...extra, 'build'];
  run('xcodebuild', args, { log });
  return { app: join(cache, 'derived-data', 'Build/Products', `${configuration}-iphonesimulator/RichOSMobile.app`), log };
}
function registerOwner(id) {
  const deviceSet = devicePolicy();
  run('python3', [join(mobile, '../engine/scripts/lib/testdevices.py'), 'register',
    '--kind', 'ios-simulator', '--id', id, '--checkout', resolve(mobile, '../..'),
    '--script', 'mobile simulator', ...(deviceSet ? ['--device-set', deviceSet] : [])]);
  return id;
}
export function device() {
  const cache = cacheRoot();
  const file = join(cache, 'device.json');
  const all = JSON.parse(simctl('list', 'devices', 'available', '--json')).devices;
  if (existsSync(file)) {
    const { id } = JSON.parse(readFileSync(file, 'utf8'));
    if (Object.values(all).flat().some((d) => d.udid === id)) return registerOwner(id);
  }
  const runtimes = JSON.parse(simctl('list', 'runtimes', '--json')).runtimes.filter((r) => r.isAvailable && r.name.startsWith('iOS'));
  if (!runtimes.length) throw new Error('Install an iOS simulator runtime in Xcode');
  const types = JSON.parse(simctl('list', 'devicetypes', '--json')).devicetypes;
  const type = types.find((t) => t.name === 'iPhone 16 Pro') || types.find((t) => t.name.startsWith('iPhone'));
  const id = simctl('create', `RichOS mobile loop ${key}`, type.identifier, runtimes.at(-1).identifier);
  writeFileSync(file, JSON.stringify({ id, deviceSet: devicePolicy() || 'system' }, null, 2));
  return registerOwner(id);
}
export function boot() {
  const id = device();
  const all = Object.values(JSON.parse(simctl('list', 'devices', '--json')).devices).flat();
  if (all.find((d) => d.udid === id)?.state !== 'Booted') simctl('boot', id);
  simctl('bootstatus', id, '-b');
  return id;
}
function container(id) { return simctl('get_app_container', id, bundleId, 'data'); }
export async function request(payload, refresh = false) {
  const id = device();
  const dir = join(container(id), 'Documents', 'mobile-commands');
  mkdirSync(dir, { recursive: true });
  const token = randomUUID();
  const input = join(dir, `${token}.request.json`);
  const output = join(dir, `${token}.response.json`);
  writeFileSync(input + '.new', JSON.stringify({ ...payload, refreshAssets: refresh }));
  renameSync(input + '.new', input);
  const start = performance.now();
  try {
    while (!existsSync(output)) {
      if (performance.now() - start > 15000) throw new Error('Simulator command timed out. Use sim prepare to install the Debug shell.');
      await new Promise((r) => setTimeout(r, 20));
    }
    const response = JSON.parse(readFileSync(output, 'utf8'));
    if (!response.ok) throw new Error(response.error);
    return response.result;
  } finally { rmSync(input, { force: true }); rmSync(output, { force: true }); }
}
export async function prepare(client = false) {
  const compiled = build();
  const id = boot();
  simctl('install', id, compiled.app);
  const sandbox = container(id);
  stageAssets(join(sandbox, 'Documents', 'mobile-ui'), true, sandbox);
  simctl('launch', '--terminate-running-process', id, bundleId, ...(client ? ['--native-client'] : []));
  return { ...compiled, device: id, ...(await request({ command: 'state' })) };
}
export async function prepareClient() { return prepare(true); }
export async function refresh() {
  const sandbox = container(device());
  stageAssets(join(sandbox, 'Documents', 'mobile-ui'), true, sandbox);
  return request({ command: 'state' }, true);
}
export async function restart() {
  simctl('launch', '--terminate-running-process', device(), bundleId);
  return request({ command: 'state' });
}
export async function verify() {
  const { createRuntime } = require('../dev/runtime.js');
  const results = [];
  for (const name of ['offline-reconnect', 'revoked', 'interrupted']) {
    const headless = await (await createRuntime()).execute({ command: 'scenario', name });
    const native = await request({ command: 'scenario', name });
    assert.deepEqual(native, headless, `${name}: simulator and headless semantic states diverged`);
    results.push({ name, steps: native.trace.length, identical: true });
  }
  // Real process termination, beyond the scenario's in-process core restart.
  await request({ command: 'fixture', name: 'offline' });
  await request({ command: 'action', action: { type: 'compose', text: 'Survives process termination' } });
  await request({ command: 'action', action: { type: 'send' } });
  const before = await request({ command: 'state' });
  const after = await restart();
  assert.deepEqual(after.state, before.state, 'Outbox changed across native process termination');
  return { scenarios: results, processRestartPreservedOutbox: true };
}
export async function uiTest() {
  await restart();
  await request({ command: 'fixture', name: 'offline' });
  const cache = cacheRoot();
  const result = join(cache, `ui-${Date.now()}.xcresult`);
  const log = join(cache, 'ui-test.log');
  run('xcodebuild', ['-project', project(), '-scheme', 'RichOSMobile', '-configuration', 'Debug', '-derivedDataPath', join(cache, 'derived-data'),
    '-destination', `platform=iOS Simulator,id=${device()}`, '-parallel-testing-enabled', 'NO', '-resultBundlePath', result,
    'CODE_SIGNING_ALLOWED=YES', 'CODE_SIGN_IDENTITY=-', 'test'], { log });
  // XCUITest terminates its app on completion. Relaunch to inspect durable state.
  const { state } = await restart();
  assert.equal(state.outbox.length, 1);
  assert.equal(state.outbox[0].text, 'Typed through the visible composer');
  return { result, log, visibleSendVerifiedInCore: true, state };
}
export function checkRelease() {
  const result = build('Release');
  const assets = join(result.app, 'mobile-ui');
  const files = readdirSync(assets);
  assert(!files.includes('runtime.js'));
  assert(!files.includes('update-fixture.js'));
  assert(!files.includes('client-inspect.js'));
  assert(!files.includes('native-test-config.json'));
  const policyURL = JSON.parse(readFileSync(join(assets, 'release-config.json'), 'utf8')).policyURL;
  assert.equal(policyURL, releaseConfiguration(undefined, false, 'Release').policyURL, 'Release must carry the hosted update-policy address');
  assert(policyURL, 'Release has no hosted update-policy address');
  const joined = files.map((file) => readFileSync(join(assets, file), 'utf8')).join('\n');
  assert(!/RichOSDev|RichOSFixtures|mobile-commands|lose-ack/.test(joined), 'Development JS leaked into Release');
  const info = run('plutil', ['-convert', 'json', '-o', '-', join(result.app, 'Info.plist')]).stdout;
  assert.deepEqual(JSON.parse(info).CFBundleURLTypes.flatMap(value => value.CFBundleURLSchemes), ['richos'], 'Unexpected URL registration in Release');
  const binary = readFileSync(join(result.app, 'RichOSMobile'));
  for (const marker of ['richos-mobile-dev', 'mobile-commands', 'Development runtime did not become ready', '--integration-session=', '--integration-copy-voice-to=', '--integration-copy-voice-origin=', 'prepareIntegrationTest', '--update-fixture=']) {
    assert(!binary.includes(Buffer.from(marker)), `Development bridge marker in Release: ${marker}`);
  }
  return { ...result, developmentBridgeExcluded: true, files };
}
export function screenshot() {
  const path = join(cacheRoot(), `screen-${Date.now()}.png`);
  simctl('io', device(), 'screenshot', path);
  return { path };
}
export function doctor() {
  return { cache: cacheRoot(), node: process.version, architecture: arch(),
    xcode: run('xcodebuild', ['-version']).stdout.trim(), xcodegen: run('xcodegen', ['--version']).stdout.trim(),
    runtimes: JSON.parse(run('xcrun', ['simctl', 'list', 'runtimes', '--json']).stdout).runtimes.map(({ name, isAvailable }) => ({ name, isAvailable })) };
}
