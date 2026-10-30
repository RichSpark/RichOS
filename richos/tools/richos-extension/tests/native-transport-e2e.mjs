#!/usr/bin/env node
/**
 * RichOS extension — NATIVE-MESSAGING TRANSPORT end-to-end harness (real Chrome, real host, real
 * whisper, no human).
 *
 *   node tests/native-transport-e2e.mjs [--headed] [--keep]
 *
 * Proves the deferred "audio-over-native-messaging" rewire end to end, on this machine:
 *
 *   LEG 1 (the core proof) — the REAL extension, running in real Chrome for Testing, captures audio
 *     through its REAL offscreen recorder + MediaRecorder and STREAMS it over
 *     `chrome.runtime.connectNative` to the REAL native host P1 built — which writes the contract
 *     directory straight into a scratch loro drop zone and runs the REAL transcription pipeline.
 *     Because Chrome for Testing's fake-microphone *file* device delivers digital silence on this
 *     host (measured: page RMS 0.000, −91 dB — the same finding live-capture.mjs documents), we
 *     inject a real spoken WAV into the recorder's OWN encode path (test seam `cc:test-inject-audio`)
 *     so the audio that crosses native messaging is genuine browser MediaRecorder Opus of real
 *     speech. We then assert the transcript contains the spoken words — i.e. the audio really crossed
 *     the native-messaging channel (chunking + ~1 MB framing + ordering + finalize) and produced a
 *     correct transcript, and that NOTHING was written to Downloads.
 *
 *   LEG 2 (the fallback proof) — with NO host registered, the same auto-arm path detects the service
 *     is unreachable and retains audio in browser storage without automatic downloads. One explicit
 *     archive export contains the complete saved session.
 *
 *   TAB-ARMING INVESTIGATION — a best-effort probe of whether tab capture can be armed with no human
 *     gesture in a test build (it is a trusted-gesture security boundary). Non-fatal; reported.
 *
 * Dependency-free (node's built-in WebSocket + https). Leaves a machine-readable result file behind.
 */

import { exportBufferedArchive } from './buffered-export.mjs';
import { trackChrome, stopChrome, stopAllChrome } from './chrome-process.mjs';
import { removableVolumeRefusal, stageOffRemovable } from './native-host-privacy.mjs';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import { createServer } from 'node:https';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
// Where the extension and the native host are read from. A checkout on a removable volume (the nightly's
// state folder can be on the SSD) is copied under the scratch root by main() and these are pointed at
// the copy, so neither Chrome nor the host it launches reads the volume.
let EXT_DIR = path.resolve(HERE, '..');
let SERVICE_DIR = path.resolve(EXT_DIR, '..', 'richos-service');
let HOST_JS = path.join(SERVICE_DIR, 'host', 'native-host.js');
const HEADED = process.argv.includes('--headed');
const KEEP = process.argv.includes('--keep');
const LEG = process.argv.find((arg) => arg.startsWith('--leg='))?.slice(6) || 'all';
if (!['all', 'native', 'fallback'].includes(LEG)) throw new Error(`unknown leg: ${LEG}`);
const NATIVE_HOST_ID = 'com.richos.host';

const results = [];
let failures = 0;
function check(name, ok, detail = '') {
  results.push({ name, ok: Boolean(ok), detail: String(detail).slice(0, 500) });
  if (!ok) failures += 1;
  console.log(`${ok ? '  ok  ' : 'FAIL  '}${name}${detail ? ` — ${detail}` : ''}`);
}
function note(name, detail) {
  results.push({ name, ok: null, detail: String(detail).slice(0, 500) });
  console.log(`  ··  ${name} — ${detail}`);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// --------------------------------------------------------------------------------------- deps
function have(bin, args) {
  try { execFileSync(bin, args, { stdio: 'ignore' }); return true; } catch { return false; }
}
const HOMEBREW = '/opt/homebrew/bin';
const FFMPEG = have('ffmpeg', ['-version']) ? 'ffmpeg' : path.join(HOMEBREW, 'ffmpeg');
const canSay = have('say', ['-v', '?']);
const haveWhisper = have('whisper-cli', ['--help']) || fs.existsSync(path.join(HOMEBREW, 'whisper-cli'));
function resolveModel() {
  const cands = [
    process.env.RICHOS_WHISPER_MODEL,
    // The host pins the default tier's weights (large-v3-turbo-q5_0) and refuses any other file.
    path.join(os.homedir(), 'Models', 'Whisper', 'ggml-large-v3-turbo-q5_0.bin'),
  ].filter(Boolean);
  return cands.find((p) => fs.existsSync(p));
}
const MODEL = resolveModel();

// --------------------------------------------------------------------------------------- chrome
function chromeCandidates() {
  const found = [];
  if (process.env.CHROME_PATH) found.push(process.env.CHROME_PATH);
  const caches = [
    path.join(os.homedir(), 'Library/Caches/ms-playwright'),
    path.join(os.homedir(), '.cache/ms-playwright'),
  ];
  for (const cache of caches) {
    if (!fs.existsSync(cache)) continue;
    const builds = fs.readdirSync(cache).filter((d) => /^chromium-\d+$/.test(d))
      .sort((a, b) => Number(b.replace(/\D/g, '')) - Number(a.replace(/\D/g, '')));
    for (const build of builds) {
      for (const rel of [
        'chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
        'chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
        'chrome-mac/Chromium.app/Contents/MacOS/Chromium',
        'chrome-linux/chrome',
      ]) found.push(path.join(cache, build, rel));
    }
  }
  return found;
}
const CHROME = chromeCandidates().find((p) => fs.existsSync(p));

// --------------------------------------------------------------------------------------- CDP
class Cdp {
  constructor(url) {
    this.ws = new WebSocket(url); this.id = 0; this.pending = new Map(); this.listeners = [];
    this.ready = new Promise((resolve, reject) => {
      this.ws.addEventListener('open', () => resolve());
      this.ws.addEventListener('error', (e) => reject(new Error(`cdp socket: ${e.message || e}`)));
    });
    this.ws.addEventListener('message', (event) => {
      const msg = JSON.parse(event.data);
      if (msg.method) for (const fn of this.listeners) fn(msg);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id); this.pending.delete(msg.id);
        if (msg.error) reject(new Error(`${msg.error.message}`)); else resolve(msg.result);
      }
    });
  }
  async send(method, params = {}, sessionId) {
    await this.ready; const id = ++this.id;
    const payload = { id, method, params }; if (sessionId) payload.sessionId = sessionId;
    this.ws.send(JSON.stringify(payload));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => { if (this.pending.has(id)) { this.pending.delete(id); reject(new Error(`cdp timeout: ${method}`)); } }, 30000);
    });
  }
  close() { try { this.ws.close(); } catch {} }
}
async function evaluate(cdp, sessionId, expression) {
  const r = await cdp.send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true, userGesture: true }, sessionId);
  if (r.exceptionDetails) return { __error: r.exceptionDetails.exception?.description || r.exceptionDetails.text };
  return r.result?.value;
}
async function evalJson(cdp, sessionId, expression) {
  const v = await evaluate(cdp, sessionId, expression);
  if (v && typeof v === 'object' && v.__error) throw new Error(`evaluate failed: ${v.__error}`);
  if (typeof v !== 'string') throw new Error(`expected JSON string, got ${typeof v}: ${JSON.stringify(v)}`);
  return JSON.parse(v);
}
async function waitFor(label, fn, { timeout = 20000, interval = 300 } = {}) {
  const started = Date.now();
  for (;;) { const v = await fn(); if (v) return v; if (Date.now() - started > timeout) throw new Error(`timed out: ${label}`); await sleep(interval); }
}

// --------------------------------------------------------------------------------------- fixture
const SPOKEN = 'Hello Marcus, the quarterly review shows September revenue climbing sharply.';
const ASSERT_WORDS = ['quarterly', 'September', 'Marcus'];
const TONE_PAGE = `<!doctype html><html><head><meta charset="utf-8"><title>Test call</title></head>
<body style="font:14px system-ui;padding:24px"><h1>RichOS native-transport fixture</h1><p id="s">starting…</p>
<div class="a4cQT" role="region" aria-label="Captions" id="capregion"></div>
<script>
  const ctx = new AudioContext(); const osc = ctx.createOscillator(); const gain = ctx.createGain();
  osc.type='sine'; osc.frequency.value=330; gain.gain.value=0.03; osc.connect(gain).connect(ctx.destination); osc.start();
  document.getElementById('s').textContent='audio running, state='+ctx.state; ctx.resume();
  const script=[['Marcus Whitfield','the quarterly review shows september revenue climbing']];
  const region=document.getElementById('capregion'); let line=0;
  (function add(){ if(line>=script.length)return; const [sp,full]=script[line]; const row=document.createElement('div'); row.className='nMcdL';
    const n=document.createElement('span'); n.className='KcIKyf'; n.textContent=sp; const t=document.createElement('span'); t.className='bh44bd';
    row.appendChild(n); row.appendChild(t); region.appendChild(row); const w=full.split(' '); let i=0;
    const g=setInterval(()=>{ i++; t.textContent=w.slice(0,i).join(' '); if(i>=w.length){clearInterval(g);line++;setTimeout(add,400);} },180); })();
</script></body></html>`;
function makeCert(dir) {
  const key = path.join(dir, 'key.pem'); const cert = path.join(dir, 'cert.pem');
  execFileSync('openssl', ['req','-x509','-newkey','rsa:2048','-nodes','-keyout',key,'-out',cert,'-days','2','-subj','/CN=meet.google.com','-addext','subjectAltName=DNS:meet.google.com'], { stdio: 'ignore' });
  return { key: fs.readFileSync(key), cert: fs.readFileSync(cert) };
}

// --------------------------------------------------------------------------------------- host
/** Write a launcher that runs the REAL native host with a scratch drop zone + toolchain on PATH. */
function writeLauncher(dir, zone, host = HOST_JS) {
  const launcher = path.join(dir, 'richos-e2e-launcher.mjs');
  const environment = { PATH: `${process.env.PATH || ''}:${HOMEBREW}:/usr/local/bin:/usr/bin:/bin`,
    RICHOS_DROP_ZONE: zone, RICHOS_LOG_LEVEL: 'error', RICHOS_TOOLCHAIN_LOCK: path.join(dir, 'toolchain-lock.json'), ...(MODEL ? { RICHOS_WHISPER_MODEL: MODEL } : {}) };
  // macOS treats native helpers as separate privacy subjects. A permitted Python
  // interpreter can launch this fixture without granting a system shell full disk access.
  const python = process.env.RICHOS_NATIVE_LAUNCHER_PYTHON;
  if (python) {
    if (!path.isAbsolute(python) || !fs.existsSync(python) || /[\r\n]/.test(python)) {
      throw new Error('RICHOS_NATIVE_LAUNCHER_PYTHON must name an existing absolute interpreter');
    }
    fs.writeFileSync(launcher, `#!${python}\nimport os\n` +
      Object.entries(environment).map(([key, value]) => `os.environ[${JSON.stringify(key)}] = ${JSON.stringify(value)}\n`).join('') +
      `fd = os.open(${JSON.stringify(path.join(dir, 'native-host-stderr.log'))}, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)\nos.dup2(fd, 2)\nos.close(fd)\n` +
      `os.execv(${JSON.stringify(process.execPath)}, [${JSON.stringify(process.execPath)}, ${JSON.stringify(host)}])\n`);
    fs.chmodSync(launcher, 0o755);
    return launcher;
  }
  // Run under the same Node privacy subject checked before Chrome starts. A shell
  // would need its own removable-volume grant just to read this fixture launcher.
  fs.writeFileSync(launcher, `#!${process.execPath}\n` +
    `import fs from 'node:fs';\n` +
    // Native messaging owns descriptors 0 and 1. Reopening the lowest free
    // descriptor captures raw stderr, including startup errors and inherited writes.
    `fs.fstatSync(0); fs.fstatSync(1);\n` +
    `fs.closeSync(2);\n` +
    `if (fs.openSync(${JSON.stringify(path.join(dir, 'native-host-stderr.log'))}, 'a', 0o600) !== 2) throw new Error('stderr descriptor was not reserved');\n` +
    `Object.assign(process.env, ${JSON.stringify(environment)});\n` +
    `await import(${JSON.stringify(pathToFileURL(host).href)});\n`);
  fs.chmodSync(launcher, 0o755);
  return launcher;
}

function checkLauncherStderr(workDir) {
  const dir = path.join(workDir, 'launcher-stderr');
  fs.mkdirSync(dir);
  const host = path.join(dir, 'failing-host.mjs');
  fs.writeFileSync(host, `import fs from 'node:fs';\nimport { spawnSync } from 'node:child_process';\n` +
    `process.stdout.write('protocol-output\\n');\nconsole.error('console-diagnostic');\n` +
    `fs.writeSync(2, 'raw-descriptor-diagnostic\\n');\n` +
    `spawnSync(process.execPath, ['-e', "process.stderr.write('child-diagnostic\\\\n')"], { stdio: ['ignore', 'ignore', 2] });\n` +
    `throw new Error('uncaught-startup-diagnostic');\n`);
  const launcher = writeLauncher(dir, path.join(dir, 'zone'), host);
  const child = spawnSync(launcher, [], { encoding: 'utf8', timeout: 10000 });
  const log = path.join(dir, 'native-host-stderr.log');
  const text = fs.existsSync(log) ? fs.readFileSync(log, 'utf8') : '';
  check('setup: launcher preserves stderr diagnostics and native protocol stdout',
    child.status === 1 && child.stdout === 'protocol-output\n' && child.stderr === '' &&
    ['console-diagnostic', 'raw-descriptor-diagnostic', 'child-diagnostic', 'uncaught-startup-diagnostic'].every((part) => text.includes(part)) &&
    (fs.statSync(log).mode & 0o777) === 0o600, text.slice(-800));
}
function installHostManifest(profileDir, extId, launcher) {
  const dir = path.join(profileDir, 'NativeMessagingHosts');
  fs.mkdirSync(dir, { recursive: true });
  const mp = path.join(dir, `${NATIVE_HOST_ID}.json`);
  fs.writeFileSync(mp, JSON.stringify({
    name: NATIVE_HOST_ID, description: 'RichOS local service (e2e)', path: launcher, type: 'stdio',
    allowed_origins: [`chrome-extension://${extId}/`],
  }, null, 2));
  return mp;
}

// --------------------------------------------------------------------------------------- launch
async function launchChrome(profileDir, downloadDir, httpsPort, extraArgs = []) {
  fs.mkdirSync(path.join(profileDir, 'Default'), { recursive: true });
  fs.writeFileSync(path.join(profileDir, 'Default', 'Preferences'),
    JSON.stringify({ download: { default_directory: downloadDir, prompt_for_download: false } }));
  const args = [
    `--user-data-dir=${profileDir}`, `--load-extension=${EXT_DIR}`, `--disable-extensions-except=${EXT_DIR}`,
    '--remote-debugging-port=0', '--no-first-run', '--no-default-browser-check',
    // Mute only device output. The recorder and transcription still receive real audio.
    '--mute-audio',
    // This profile is disposable. Keep browser encryption away from the user's Keychain.
    ...(process.platform === 'darwin' ? ['--use-mock-keychain'] : []),
    '--enable-logging=stderr', '--vmodule=native_message*=1',
    '--disable-features=DialMediaRouteProvider,MediaRouter',
    '--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream',
    '--autoplay-policy=no-user-gesture-required', '--ignore-certificate-errors',
    '--host-resolver-rules=MAP meet.google.com 127.0.0.1', '--no-proxy-server', '--window-size=1000,700',
    ...extraArgs,
  ];
  if (!HEADED) args.push('--headless=new');
  const chrome = trackChrome(spawn(CHROME, args, { stdio: ['ignore', 'pipe', 'pipe'] }));
  const log = []; chrome.stderr.on('data', (d) => log.push(String(d))); chrome.stdout.on('data', (d) => log.push(String(d)));
  const portFile = path.join(profileDir, 'DevToolsActivePort');
  const devPort = await waitFor('DevToolsActivePort', () => {
    if (!fs.existsSync(portFile)) return null;
    const first = fs.readFileSync(portFile, 'utf8').split('\n')[0].trim(); return first ? Number(first) : null;
  });
  const version = await (await fetch(`http://127.0.0.1:${devPort}/json/version`)).json();
  const cdp = new Cdp(version.webSocketDebuggerUrl); await cdp.ready;
  await cdp.send('Target.setDiscoverTargets', { discover: true });
  return { chrome, cdp, version, log };
}
async function findRichosSw(cdp) {
  return waitFor('RichOS service worker', async () => {
    const { targetInfos } = await cdp.send('Target.getTargets');
    for (const t of targetInfos.filter((t) => t.type === 'service_worker' && t.url.startsWith('chrome-extension://'))) {
      const { sessionId } = await cdp.send('Target.attachToTarget', { targetId: t.targetId, flatten: true });
      const name = await evaluate(cdp, sessionId, 'chrome.runtime.getManifest().name');
      if (name === 'RichOS Helper') return { swSession: sessionId, extensionId: new URL(t.url).host };
      await cdp.send('Target.detachFromTarget', { sessionId });
    }
    return null;
  });
}
const getStatusExpr = `(async () => { try { return JSON.stringify(await globalThis.__richos.callCapture.getStatus()); } catch (e) { return JSON.stringify({ statusError: String(e && e.stack || e) }); } })()`;

async function setFastSettings(cdp, swSession, label) {
  await evaluate(cdp, swSession, `(async () => { globalThis.__richosTestNotificationAttempts = [];
      // Never send fixture notifications to the user's desktop, even after a regression.
      chrome.notifications.create = async (...args) => {
        globalThis.__richosTestNotificationAttempts.push(args);
        throw new Error('Desktop notification attempted by a quiet browser fixture');
      };
      await chrome.storage.local.set({'richos.settings': {
    core: { notifyOnStartStop: false, notifyOnFailure: false, alertSound: false },
    callCapture: { micProcessing: false, chunkMs: 1000, maxSessionMinutes: 10, autoStartMicCaptions: true, captureCaptions: true } } }); return 'ok'; })()`);
  const quietSettings = await evalJson(cdp, swSession,
    `(async () => JSON.stringify((await chrome.storage.local.get('richos.settings'))['richos.settings'].core))()`);
  check(`${label}: disposable profile disables desktop notifications and chimes before calls`,
    quietSettings.notifyOnStartStop === false && quietSettings.notifyOnFailure === false && quietSettings.alertSound === false);
}
async function checkNoDesktopNotifications(cdp, swSession, label) {
  const notifications = await evalJson(cdp, swSession,
    `(async () => JSON.stringify({ active: await chrome.notifications.getAll(), attempts: globalThis.__richosTestNotificationAttempts }))()`);
  check(`${label}: test run neither attempts nor leaves desktop notifications`, Object.keys(notifications.active).length === 0 && notifications.attempts.length === 0, JSON.stringify(notifications));
}
async function openCallTab(cdp, httpsPort) {
  const CALL_URL = `https://meet.google.com:${httpsPort}/abc-defg-hij`;
  const { targetId } = await cdp.send('Target.createTarget', { url: CALL_URL });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  const navigation=await cdp.send('Page.navigate',{url:CALL_URL},sessionId);
  if(navigation.errorText)throw new Error(`fixture navigation: ${navigation.errorText}`);
  await cdp.send('Runtime.enable', {}, sessionId);
  return { tabTargetId: targetId, tabSession: sessionId };
}

// =======================================================================================
// LEG 1 — native-messaging transport, real speech, real host, real pipeline
// =======================================================================================
async function runNativeLeg(workDir, speechB64) {
  console.log('\n=== LEG 1: native-messaging transport (real extension → real host → real pipeline) ===');
  const profileDir = path.join(workDir, 'nat-profile');
  const downloadDir = path.join(workDir, 'nat-downloads');
  const zone = path.join(workDir, 'nat-zone');
  fs.mkdirSync(profileDir); fs.mkdirSync(downloadDir); fs.mkdirSync(zone);

  const { key, cert } = makeCert(workDir);
  const server = createServer({ key, cert }, (_req, res) => { res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }); res.end(TONE_PAGE); });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const httpsPort = server.address().port;

  const { chrome, cdp, version, log } = await launchChrome(profileDir, downloadDir, httpsPort);
  check('LEG1: Chrome for Testing launched with the unpacked extension', true, version.Browser);
  const consoleErrors = [];
  cdp.listeners.push((msg) => {
    if (msg.method === 'Runtime.consoleAPICalled' && ['error', 'assert'].includes(msg.params.type))
      consoleErrors.push((msg.params.args || []).map((a) => a.value ?? a.description ?? a.type).join(' '));
    if (msg.method === 'Runtime.exceptionThrown') consoleErrors.push(`uncaught: ${msg.params.exceptionDetails?.exception?.description || ''}`);
  });

  const { swSession, extensionId } = await findRichosSw(cdp);
  await cdp.send('Runtime.enable', {}, swSession);
  check('LEG1: RichOS service worker booted', true, `ext id ${extensionId}`);
  const manifest = JSON.parse(await evaluate(cdp, swSession, 'JSON.stringify(chrome.runtime.getManifest())'));
  const expectedVersion = JSON.parse(fs.readFileSync(path.join(EXT_DIR, 'manifest.json'), 'utf8')).version;
  check('LEG1: extension manifest declares nativeMessaging + current source version',
    manifest.permissions.includes('nativeMessaging') && manifest.version === expectedVersion,
    `v${manifest.version} perms=${manifest.permissions.includes('nativeMessaging')}`);

  // Register the REAL host for this extension id BEFORE the call tab triggers connectNative.
  const launcher = writeLauncher(workDir, zone);
  const manifestPath = installHostManifest(profileDir, extensionId, launcher);
  check('LEG1: native host manifest installed (profile NativeMessagingHosts, real launcher)', fs.existsSync(manifestPath), manifestPath);

  const hostProbe = await evalJson(cdp, swSession, `new Promise(resolve => {
    const port = chrome.runtime.connectNative(${JSON.stringify(NATIVE_HOST_ID)});
    let done = false;
    const finish = value => {if(done)return;done=true;clearTimeout(timer);resolve(JSON.stringify(value));port.disconnect();};
    const timer = setTimeout(() => finish({available:false,error:'handshake deadline'}),1500);
    port.onMessage.addListener(message => finish({available:message.type==='ready',message}));
    port.onDisconnect.addListener(() => finish({available:false,error:chrome.runtime.lastError?.message || 'disconnected'}));
    port.postMessage({type:'hello'});
  })`);
  check('LEG1: registered host answers the real Chrome handshake', hostProbe.available, JSON.stringify(hostProbe));
  if (!hostProbe.available) {
    fs.writeFileSync(path.join(workDir, 'native-chrome.log'), log.join(''));
    const hostStderr = path.join(workDir, 'native-host-stderr.log');
    console.error('Native launch diagnostics:', fs.existsSync(hostStderr) ? fs.readFileSync(hostStderr, 'utf8').slice(-6000) : 'host did not create stderr log');
    console.error(log.join('').slice(-6000));
    cdp.close(); server.close(); await stopChrome(chrome);
    throw new Error(`native host prerequisite: ${JSON.stringify(hostProbe)}`);
  }

  await setFastSettings(cdp, swSession, 'LEG1');
  const { tabTargetId, tabSession } = await openCallTab(cdp, httpsPort);
  await sleep(2500);
  check('LEG1: fixture call page is playing', /running/.test(String(await evaluate(cdp, tabSession, 'document.getElementById("s")?.textContent'))), '');

  const callTabId = (await evaluate(cdp, swSession,
    `(async () => (await chrome.tabs.query({})).filter(t => (t.url||'').includes('meet.google.com')).map(t => t.id))()`))?.[0];

  // Auto-arm (zero gesture). The controller connects to the host and streams — assert sink=native.
  let status = await evalJson(cdp, swSession, getStatusExpr);
  if (!status.active) {
    await evaluate(cdp, swSession, `globalThis.__richos.callCapture.armTab(${callTabId}, 'auto').catch(()=>{})`);
    status = await waitFor('session to auto-start', async () => {
      const s = await evalJson(cdp, swSession, getStatusExpr); return s.active ? s : null;
    }, { timeout: 10000, interval: 400 }).catch(() => status);
  }
  check('LEG1: a detected Meet tab auto-armed with NO user gesture', status.active === true, `mode=${status.mode} audioActive=${status.audioActive}`);
  check('LEG1: the session chose the NATIVE-MESSAGING transport (not Downloads)', status.transport === 'native', `transport=${status.transport}`);

  // The host must have written session.json at START (open) before any audio — the anomaly design.
  const sessionId = status.sessionId;
  const hostDir = path.join(zone, sessionId);
  await waitFor('host to write session.json at start', () => fs.existsSync(path.join(hostDir, 'session.json')), { timeout: 6000, interval: 200 }).catch(() => {});
  const startRec = fs.existsSync(path.join(hostDir, 'session.json')) ? JSON.parse(fs.readFileSync(path.join(hostDir, 'session.json'), 'utf8')) : null;
  check('LEG1: host wrote session.json into the loro drop zone at call START (status open)',
    startRec && startRec.status === 'open', startRec ? `status=${startRec.status} source=${startRec.capture?.source}` : 'missing');

  // Inject real speech into the extension's OWN recorder so the streamed chunks carry it.
  const injected = await evalJson(cdp, swSession,
    `(async () => JSON.stringify(await globalThis.__richos.core.callOffscreen({ type: 'cc:test-inject-audio', b64: ${JSON.stringify(speechB64)}, loop: true })))()`);
  check('LEG1: real speech injected into the recorder encode path', injected.ok === true, `duration=${injected.duration}`);

  // Let it stream for several seconds of speech.
  await sleep(8000);
  const streaming = await evalJson(cdp, swSession, getStatusExpr);
  check('LEG1: audio chunks are streaming over native messaging (not buffered for Downloads)',
    streaming.transport === 'native' && streaming.streamedChunks > 2 && streaming.streamedBytes > 0,
    `streamedChunks=${streaming.streamedChunks} streamedBytes=${streaming.streamedBytes}`);

  // audio-part file on disk in the loro drop zone is growing from the streamed chunks.
  const audioPart = path.join(hostDir, 'audio-part-00.webm');
  check('LEG1: the host appended streamed chunks to audio-part-00.webm in the drop zone',
    fs.existsSync(audioPart) && fs.statSync(audioPart).size > 0, fs.existsSync(audioPart) ? `${fs.statSync(audioPart).size} bytes` : 'missing');

  // Close the tab → finalize → native session-close → host triggers the pipeline.
  await cdp.send('Target.closeTarget', { targetId: tabTargetId });
  const closed = await waitFor('session to finalize', async () => {
    const s = await evalJson(cdp, swSession, getStatusExpr); return s.active === false ? s : null;
  }, { timeout: 15000, interval: 500 }).catch(() => null);
  check('LEG1: closing the call tab finalized the native session', Boolean(closed), '');

  // Host session.json should now be closed with real audio accounting.
  await waitFor('host to close session.json', () => {
    try { return JSON.parse(fs.readFileSync(path.join(hostDir, 'session.json'), 'utf8')).status === 'closed'; } catch { return false; }
  }, { timeout: 8000, interval: 300 }).catch(() => {});
  const closeRec = JSON.parse(fs.readFileSync(path.join(hostDir, 'session.json'), 'utf8'));
  check('LEG1: host session.json is CLOSED with real audio accounting from the stream',
    closeRec.status === 'closed' && (closeRec.audio?.bytesTotal || 0) > 0 && (closeRec.audio?.parts?.length || 0) > 0,
    `status=${closeRec.status} bytes=${closeRec.audio?.bytesTotal} parts=${closeRec.audio?.parts?.length}`);

  // The decisive assertion: the streamed audio decodes and produced a transcript with the SPOKEN words.
  const finalAudioBytes = fs.existsSync(audioPart) ? fs.statSync(audioPart).size : 0;
  try {
    const probe = execFileSync('ffprobe', ['-v','error','-show_entries','stream=codec_name:format=duration','-of','default=nw=1', audioPart], { encoding: 'utf8' }).replace(/\n/g, ' ');
    check('LEG1: the audio that crossed native messaging decodes as Opus', /opus/.test(probe), probe.trim());
  } catch (e) { note('LEG1: ffprobe', String(e.message).slice(0, 120)); }

  const transcriptOk = await waitFor('pipeline to emit transcript.md', () => fs.existsSync(path.join(hostDir, 'transcript.md')), { timeout: 90000, interval: 1000 }).catch(() => false);
  check('LEG1: the host pipeline produced transcript.md from the streamed audio', Boolean(transcriptOk), transcriptOk ? '' : 'no transcript within 90s');
  let transcript = '';
  if (transcriptOk) {
    transcript = fs.readFileSync(path.join(hostDir, 'transcript.md'), 'utf8');
    console.log('\n----- LEG1 transcript.md (from audio streamed over native messaging) -----\n' + transcript + '\n-------------------------------------------------------------------------\n');
    const hits = ASSERT_WORDS.filter((w) => new RegExp(w, 'i').test(transcript));
    check(`LEG1: the transcript contains the spoken words (proves speech crossed native messaging) — matched [${hits.join(', ')}]`,
      hits.length >= 2, `matched ${hits.length}/${ASSERT_WORDS.length}: ${hits.join(', ')}`);
  }

  // Downloads MUST be empty — this is the proof it went over native messaging, not the old hop.
  const dlEntries = fs.existsSync(downloadDir) ? fs.readdirSync(downloadDir).filter((f) => f !== 'Default') : [];
  const dlCapture = fs.existsSync(path.join(downloadDir, 'richos-capture'));
  check('LEG1: NOTHING was written to the Downloads capture folder (transport bypassed the Downloads hop)',
    !dlCapture, dlCapture ? `richos-capture exists: ${JSON.stringify(fs.readdirSync(path.join(downloadDir, 'richos-capture')))}` : `downloads entries: ${JSON.stringify(dlEntries)}`);

  check('LEG1: the extension logged no errors and threw no uncaught exceptions', consoleErrors.length === 0, consoleErrors.slice(0, 3).join(' | ') || 'clean');

  await checkNoDesktopNotifications(cdp, swSession, 'LEG1');
  cdp.close(); server.close(); await stopChrome(chrome);
  return { sessionId, transcript, finalAudioBytes, streamedBytes: streaming.streamedBytes, hostDir, zone: KEEP ? zone : null };
}

// =======================================================================================
// LEG 2 — Downloads fallback when the host is unreachable
// =======================================================================================
async function runFallbackLeg(workDir, speechB64) {
  console.log('\n=== LEG 2: Downloads runtime fallback (host unreachable → never lose audio) ===');
  const profileDir = path.join(workDir, 'fb-profile');
  const downloadDir = path.join(workDir, 'fb-downloads');
  fs.mkdirSync(profileDir); fs.mkdirSync(downloadDir);

  const { key, cert } = makeCert(path.join(workDir)); // reuse CN=meet cert
  const server = createServer({ key, cert }, (_req, res) => { res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }); res.end(TONE_PAGE); });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const httpsPort = server.address().port;

  // NO host manifest installed → connectNative fails → sink must fall back to Downloads.
  const { chrome, cdp } = await launchChrome(profileDir, downloadDir, httpsPort);
  const { swSession, extensionId } = await findRichosSw(cdp);
  await cdp.send('Runtime.enable', {}, swSession);
  await setFastSettings(cdp, swSession, 'LEG2');
  const { tabTargetId, tabSession } = await openCallTab(cdp, httpsPort);
  await sleep(2500);

  const callTabId = (await evaluate(cdp, swSession,
    `(async () => (await chrome.tabs.query({})).filter(t => (t.url||'').includes('meet.google.com')).map(t => t.id))()`))?.[0];
  let status = await evalJson(cdp, swSession, getStatusExpr);
  if (!status.active) {
    await evaluate(cdp, swSession, `globalThis.__richos.callCapture.armTab(${callTabId}, 'auto').catch(()=>{})`);
    status = await waitFor('fallback session to start', async () => { const s = await evalJson(cdp, swSession, getStatusExpr); return s.active ? s : null; }, { timeout: 10000 }).catch(() => status);
  }
  check('LEG2: with no host reachable, the session retained a durable BROWSER transport', status.active === true && status.transport === 'browser', `transport=${status.transport}`);

  const injected = await evalJson(cdp, swSession,
    `(async () => JSON.stringify(await globalThis.__richos.core.callOffscreen({ type: 'cc:test-inject-audio', b64: ${JSON.stringify(speechB64)}, loop: true })))()`);
  check('LEG2: recorder still capturing on the fallback path', injected.ok === true, '');
  await sleep(5000);

  // session.json at START must be on disk in Downloads (the anomaly guarantee on the fallback path).
  const walk = (dir, acc = []) => { if (!fs.existsSync(dir)) return acc; for (const e of fs.readdirSync(dir, { withFileTypes: true })) { const p = path.join(dir, e.name); if (e.isDirectory()) walk(p, acc); else acc.push(path.relative(downloadDir, p)); } return acc; };
  const early = walk(downloadDir);
  check('LEG2: browser checkpoint exists with no automatic download at START', (await evaluate(cdp,swSession,`(async()=>Boolean(await globalThis.__richos.core.idb.get('sessions',${JSON.stringify(status.sessionId)})))()`))===true && early.length===0, early.join(', ') || 'nothing');

  await cdp.send('Target.closeTarget', { targetId: tabTargetId });
  await sleep(6000);
  const ended=await evalJson(cdp,swSession,getStatusExpr);
  check('LEG2: automatic close preserves a pending browser export without downloads',ended.lastSession?.exportPending===true && walk(downloadDir).length===0);
  await exportBufferedArchive(cdp,swSession,evaluate,downloadDir,ended.lastSession.sessionId);
  const found = walk(downloadDir);
  const audio = found.filter((f) => /audio-part-\d+\.webm$/.test(f));
  check('LEG2: audio was captured to the Downloads folder (no audio lost when the host is absent)',
    audio.length > 0 && fs.statSync(path.join(downloadDir, audio[0])).size > 0, JSON.stringify(audio));
  check('LEG2: fallback artifacts land under richos-capture/<session>/ with the documented names',
    found.some((f) => /^richos-capture\/[^/]+\/audio-part-\d+\.webm$/.test(f)) && found.some((f) => /^richos-capture\/[^/]+\/session\.json$/.test(f)),
    found.join(', '));

  await checkNoDesktopNotifications(cdp, swSession, 'LEG2');
  cdp.close(); server.close(); await stopChrome(chrome);
}

// =======================================================================================
// TAB-ARMING INVESTIGATION (best-effort; non-fatal)
// =======================================================================================
async function investigateTabArming(workDir) {
  console.log('\n=== INVESTIGATION: can tab capture be armed with no human gesture in a test build? ===');
  const profileDir = path.join(workDir, 'tab-profile');
  const downloadDir = path.join(workDir, 'tab-downloads');
  fs.mkdirSync(profileDir); fs.mkdirSync(downloadDir);
  const { key, cert } = makeCert(path.join(workDir));
  const server = createServer({ key, cert }, (_req, res) => { res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }); res.end(TONE_PAGE); });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const httpsPort = server.address().port;

  // Candidate switches that MIGHT auto-grant tab/desktop capture, analogous to fake-media-UI flags.
  const extraArgs = [
    '--auto-select-tab-capture-source-by-title=Test call',
    '--auto-select-desktop-capture-source=Test call',
    '--auto-accept-this-tab-capture',
  ];
  const { chrome, cdp } = await launchChrome(profileDir, downloadDir, httpsPort, extraArgs);
  const { swSession } = await findRichosSw(cdp);
  await cdp.send('Runtime.enable', {}, swSession);
  await setFastSettings(cdp, swSession, 'INVESTIGATION');
  const { tabTargetId } = await openCallTab(cdp, httpsPort);
  await sleep(2500);
  const callTabId = (await evaluate(cdp, swSession,
    `(async () => (await chrome.tabs.query({})).filter(t => (t.url||'').includes('meet.google.com')).map(t => t.id))()`))?.[0];

  // Try to mint a tab-capture stream id with NO invocation, under the candidate flags.
  const mint = await evalJson(cdp, swSession,
    `(async () => { try { const id = await chrome.tabCapture.getMediaStreamId({ targetTabId: ${callTabId} }); return JSON.stringify({ ok: true, id: String(id).slice(0,8) }); } catch (e) { return JSON.stringify({ ok: false, error: String(e && e.message || e) }); } })()`);
  if (mint.ok) {
    note('INVESTIGATION: a Chrome test switch auto-granted tabCapture WITHOUT a gesture', `streamId acquired (${mint.id}…) — a full both-channels automated E2E is possible; flags: ${extraArgs.join(' ')}`);
  } else {
    note('INVESTIGATION: no test switch auto-granted tabCapture', `refused: "${mint.error}". This is Chrome's trusted-gesture boundary — tabCapture.getMediaStreamId requires a real extension invocation (toolbar click / shortcut / context menu) for the target tab; it cannot be minted from the service worker or via CDP userGesture. The fake-media-UI flags only bypass the getUserMedia permission PROMPT, not the tabCapture invocation gate.`);
    note('INVESTIGATION: OS-level synthetic gesture (cliclick) not attempted headless', 'the toolbar action icon is not rendered/hit-testable in --headless=new, and its screen coordinates are unknown/unstable in a headed throwaway window; a trusted click on the action is what the boundary requires. Tab-arming is therefore left to a one-click manual / real-call confirmation (README TEST-PROTOCOL). The mic leg proves the transport; the tab leg is the SAME streaming code path once armed.');
  }
  await checkNoDesktopNotifications(cdp, swSession, 'INVESTIGATION');
  cdp.close(); server.close(); await stopChrome(chrome);
  return mint;
}

// --------------------------------------------------------------------------------------- scratch
/**
 * Everything the Chrome-launched native host reads (launcher, drop zone, fixtures) lives under ONE
 * scratch root on the internal disk: the per-user macOS temp dir, whatever TMPDIR the caller
 * exported. The nightly's gate TMPDIR is on the external SSD (/Volumes/...), and a host reading
 * there is its own privacy subject that asks for Removable Volumes access (native-host-privacy.mjs).
 * TMPDIR is pinned to it so Chrome and the host it starts inherit the same internal path; the
 * removable-volume refusal below still runs on what is actually used, unchanged.
 */
function pinInternalTmpdir() {
  if (process.platform !== 'darwin') return;
  let dir = '';
  try { dir = execFileSync('/usr/bin/getconf', ['DARWIN_USER_TEMP_DIR'], { encoding: 'utf8' }).trim(); } catch { /* keep TMPDIR */ }
  if (dir && path.isAbsolute(dir) && fs.existsSync(dir)) process.env.TMPDIR = dir;
}

// --------------------------------------------------------------------------------------- main
async function main() {
  pinInternalTmpdir();
  if (!CHROME) throw new Error('no Chrome for Testing found; set CHROME_PATH');
  if (!fs.existsSync(HOST_JS)) throw new Error(`native host not found at ${HOST_JS}`);
  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'richos-nat-'));
  const bail = (code) => { fs.rmSync(workDir, { recursive: true, force: true }); process.exit(code); };
  // A checkout on a removable volume is read from a copy under the scratch root, off it.
  ({ extension: EXT_DIR, service: SERVICE_DIR } = stageOffRemovable({
    dirs: { service: SERVICE_DIR, extension: EXT_DIR }, into: path.join(workDir, 'staged') }));
  HOST_JS = path.join(SERVICE_DIR, 'host', 'native-host.js');
  // The host Chrome launches answers to macOS as itself; on a removable volume it would ask. What is
  // still on one now (the model, a scratch root the caller forced there) is refused unchanged.
  const refusal = removableVolumeRefusal({
    paths: [SERVICE_DIR, EXT_DIR, os.tmpdir(), MODEL],
    subjects: [process.execPath, process.env.RICHOS_NATIVE_LAUNCHER_PYTHON],
  });
  if (refusal) { console.error(refusal); bail(1); }
  if (!canSay || !haveWhisper || !MODEL) {
    console.error(`native-transport-e2e needs: say=${canSay} whisper=${haveWhisper} model=${MODEL || 'MISSING'} — all required for the speech-content proof.`);
    bail(1);
  }
  console.log(`\nRichOS native-transport E2E\n  chrome:  ${CHROME}\n  host:    ${HOST_JS}\n  model:   ${MODEL}\n  workdir: ${workDir}\n`);
  checkLauncherStderr(workDir);

  // Build the spoken WAV the recorder will emit (real speech, 48 kHz PCM the offscreen ctx decodes).
  const aiff = path.join(workDir, 'say.aiff'); const wav = path.join(workDir, 'voice.wav');
  execFileSync('say', ['-v', 'Samantha', '-o', aiff, SPOKEN]);
  execFileSync(FFMPEG, ['-y', '-v', 'error', '-i', aiff, '-ac', '1', '-ar', '48000', '-c:a', 'pcm_s16le', wav]);
  const speechB64 = fs.readFileSync(wav).toString('base64');
  check('setup: generated a real spoken WAV for the recorder to emit', fs.statSync(wav).size > 1000, `${fs.statSync(wav).size} bytes; phrase="${SPOKEN}"`);

  let leg1;
  if (LEG !== 'fallback') try { leg1 = await runNativeLeg(workDir, speechB64); } catch (e) { check('LEG1 ran to completion', false, String(e.stack || e).slice(0, 300)); }
  if (LEG !== 'native') try { await runFallbackLeg(workDir, speechB64); } catch (e) { check('LEG2 ran to completion', false, String(e.stack || e).slice(0, 300)); }
  if (LEG === 'all') try { await investigateTabArming(workDir); } catch (e) { note('INVESTIGATION errored (non-fatal)', String(e.message).slice(0, 160)); }
  // A leg that threw never reached its own stop. Nothing may still write into workDir when it is removed.
  try { await stopAllChrome(); } catch (e) { check('every Chrome the harness started has exited', false, String(e.message).slice(0, 300)); }

  const summary = { ranAt: new Date().toISOString(), chrome: CHROME, headless: !HEADED, selection: LEG, results, leg1: leg1 ? { sessionId: leg1.sessionId, streamedBytes: leg1.streamedBytes, finalAudioBytes: leg1.finalAudioBytes } : null };
  const summaryPath = path.join(workDir, 'native-transport-result.json');
  fs.writeFileSync(summaryPath, JSON.stringify(summary, null, 2));
  console.log(`\n${results.filter((r) => r.ok === true).length} checks passed, ${failures} failed`);
  console.log(`result file: ${summaryPath}`);
  if (KEEP || failures) console.log(`kept workdir: ${workDir}`); else fs.rmSync(workDir, { recursive: true, force: true });
  process.exit(failures ? 1 : 0);
}
main().catch((err) => { console.error(`\nharness error: ${err.stack}`); process.exit(2); });
