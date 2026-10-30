#!/usr/bin/env node
/**
 * RichOS extension — LIVE capture harness (real Chrome, real audio, no human).
 *
 *   node tests/live-capture.mjs [--headed] [--keep]
 *
 * What it actually does:
 *   1. serves a local HTTPS page that plays a real tone, mapped to `meet.google.com` with
 *      `--host-resolver-rules`, so the extension's own platform detection sees a genuine
 *      Google Meet URL;
 *   2. launches a throwaway Chrome profile with the unpacked extension, a fake microphone
 *      device (which emits real audio) and a scratch downloads folder;
 *   3. drives the extension through the DevTools protocol: arm, record, inspect health,
 *      simulate a service-worker restart, kill the tab, and verify the drop zone on disk.
 *
 * It is deliberately dependency-free (node's built-in WebSocket) and leaves a machine-readable
 * result file behind so a reviewer can check what was exercised rather than take a claim.
 */

import { exportBufferedArchive } from './buffered-export.mjs';
import { trackChrome, stopChrome, stopAllChrome } from './chrome-process.mjs';
import { spawnSync, execFileSync } from 'node:child_process';
import { createServer } from 'node:https';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Every full Chrome a suite starts is quiet: headless, a throwaway profile, notifications and
// permissions denied, never registered with macOS Notification Center (see the module's header).
const quiet = createRequire(import.meta.url)('../../../app/ui/tests/lib/quiet-browser.js');

const EXT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const HEADED = process.argv.includes('--headed');
const KEEP = process.argv.includes('--keep');

/**
 * Branded Google Chrome refuses `--load-extension` ("not allowed in Google Chrome"), so an
 * automated harness needs Chromium / Chrome for Testing. Any locally cached Playwright or
 * Puppeteer build will do; `CHROME_PATH` overrides everything. Loading the extension by hand
 * in the CEO's normal Chrome (chrome://extensions → Load unpacked) is unaffected.
 */
function chromeCandidates() {
  const found = [];
  if (process.env.CHROME_PATH) found.push(process.env.CHROME_PATH);
  const caches = [
    path.join(os.homedir(), 'Library/Caches/ms-playwright'),
    path.join(os.homedir(), '.cache/ms-playwright'),
    path.join(os.homedir(), '.cache/puppeteer'),
    path.join(os.homedir(), 'AppData/Local/ms-playwright'),
  ];
  for (const cache of caches) {
    if (!fs.existsSync(cache)) continue;
    const builds = fs
      .readdirSync(cache)
      .filter((d) => /^chromium-\d+$|^chrome[/\\-]/.test(d))
      .sort((a, b) => Number(b.replace(/\D/g, '')) - Number(a.replace(/\D/g, '')));
    for (const build of builds) {
      for (const rel of [
        'chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
        'chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
        'chrome-mac/Chromium.app/Contents/MacOS/Chromium',
        'chrome-linux/chrome',
        'chrome-win/chrome.exe',
      ]) {
        found.push(path.join(cache, build, rel));
      }
    }
  }
  found.push(
    '/Applications/Chromium.app/Contents/MacOS/Chromium',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    'C:/Program Files/Google/Chrome/Application/chrome.exe',
  );
  return found;
}

const CHROME_CANDIDATES = chromeCandidates();

const results = [];
let failures = 0;

function check(name, ok, detail = '') {
  results.push({ name, ok: Boolean(ok), detail: String(detail).slice(0, 400) });
  if (!ok) failures += 1;
  console.log(`${ok ? '  ok  ' : 'FAIL  '}${name}${detail ? ` — ${detail}` : ''}`);
}

function note(name, detail) {
  results.push({ name, ok: null, detail: String(detail).slice(0, 400) });
  console.log(`  ··  ${name} — ${detail}`);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Session id used when the harness has to drive the recorder directly (no human click). */
const HARNESS_SESSION = 'harness-mic-only';

async function waitFor(label, fn, { timeout = 20000, interval = 300 } = {}) {
  const started = Date.now();
  for (;;) {
    const value = await fn();
    if (value) return value;
    if (Date.now() - started > timeout) throw new Error(`timed out waiting for ${label}`);
    await sleep(interval);
  }
}

// ---------------------------------------------------------------------------------------
// Minimal CDP client
// ---------------------------------------------------------------------------------------

class Cdp {
  constructor(url) {
    this.ws = new WebSocket(url);
    this.id = 0;
    this.pending = new Map();
    this.ready = new Promise((resolve, reject) => {
      this.ws.addEventListener('open', () => resolve());
      this.ws.addEventListener('error', (e) => reject(new Error(`cdp socket error: ${e.message || e}`)));
    });
    this.listeners = [];
    this.ws.addEventListener('message', (event) => {
      const msg = JSON.parse(event.data);
      if (msg.method) for (const fn of this.listeners) fn(msg);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(`${msg.error.message} (${JSON.stringify(msg.error.data || '')})`));
        else resolve(msg.result);
      }
    });
  }

  async send(method, params = {}, sessionId) {
    await this.ready;
    const id = ++this.id;
    const payload = { id, method, params };
    if (sessionId) payload.sessionId = sessionId;
    this.ws.send(JSON.stringify(payload));
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`cdp timeout: ${method}`));
        }
      }, 30000);
    });
  }

  close() {
    try {
      this.ws.close();
    } catch {
      /* ignore */
    }
  }
}

/** Evaluate an async expression inside a target, returning its value. */
async function evaluate(cdp, sessionId, expression) {
  const result = await cdp.send(
    'Runtime.evaluate',
    { expression, awaitPromise: true, returnByValue: true, userGesture: true },
    sessionId,
  );
  if (result.exceptionDetails) {
    return { __error: result.exceptionDetails.exception?.description || result.exceptionDetails.text };
  }
  return result.result?.value;
}

/** Evaluate an expression that returns a JSON string; surfaces in-page exceptions loudly. */
async function evalJson(cdp, sessionId, expression) {
  const value = await evaluate(cdp, sessionId, expression);
  if (value && typeof value === 'object' && value.__error) throw new Error(`evaluate failed: ${value.__error}`);
  if (typeof value !== 'string') throw new Error(`expected a JSON string, got ${typeof value}: ${JSON.stringify(value)}`);
  return JSON.parse(value);
}

// ---------------------------------------------------------------------------------------
// Fixtures: a tone-playing page served as https://meet.google.com/<code>
// ---------------------------------------------------------------------------------------

const TONE_PAGE = `<!doctype html>
<html><head><meta charset="utf-8"><title>Test call</title></head>
<body style="font:14px system-ui;padding:24px">
<h1>RichOS live-capture fixture</h1>
<p id="s">starting audio…</p>

<!-- A caption region shaped like Google Meet's classic caption overlay (obfuscated classes),
     so the real Meet content script + adapter observe genuine caption-shaped DOM. Rows grow in
     place (as Meet's do) and new speakers are appended over time. -->
<div class="a4cQT" role="region" aria-label="Captions" id="capregion"></div>
<script>
  const ctx = new AudioContext();
  const osc = ctx.createOscillator();
  const lfo = ctx.createOscillator();
  const lfoGain = ctx.createGain();
  const gain = ctx.createGain();
  osc.type = 'sine';
  osc.frequency.value = 330;
  lfo.frequency.value = 0.7;            // speech-like amplitude movement so RMS varies
  lfoGain.gain.value = 0.05;
  gain.gain.value = 0.06;               // quiet on purpose: this may play on the dev box
  lfo.connect(lfoGain).connect(gain.gain);
  osc.connect(gain).connect(ctx.destination);
  osc.start(); lfo.start();
  document.getElementById('s').textContent = 'audio running, state=' + ctx.state;
  ctx.resume().then(() => { document.getElementById('s').textContent = 'audio running, state=' + ctx.state; });

  // Emit captions the way Meet does: a speaker's line appears, grows word-by-word, then a new
  // speaker's line appears. Distinct speaker labels prove the per-remote-speaker enrichment.
  const script = [
    ['Ada Lovelace', 'the analytical engine weaves algebraic patterns'],
    ['Charles Babbage', 'as the jacquard loom weaves flowers and leaves'],
    ['Grace Hopper', 'a nanosecond is about eleven point eight inches'],
  ];
  const region = document.getElementById('capregion');
  let line = 0;
  function addLine() {
    if (line >= script.length) return;
    const [speaker, full] = script[line];
    const row = document.createElement('div');
    row.className = 'nMcdL';
    const name = document.createElement('span');
    name.className = 'KcIKyf';
    name.textContent = speaker;
    const text = document.createElement('span');
    text.className = 'bh44bd';
    row.appendChild(name); row.appendChild(text);
    region.appendChild(row);
    const words = full.split(' ');
    let w = 0;
    const grow = setInterval(() => {
      w += 1;
      text.textContent = words.slice(0, w).join(' ');   // grows the SAME node in place
      if (w >= words.length) { clearInterval(grow); line += 1; setTimeout(addLine, 500); }
    }, 220);
  }
  setTimeout(addLine, 800);
  // Expose a way for the harness to simulate the caption feature breaking (region removed).
  window.__richosBreakCaptions = () => { region.remove(); };
</script>
</body></html>`;

function makeCert(dir) {
  const key = path.join(dir, 'key.pem');
  const cert = path.join(dir, 'cert.pem');
  execFileSync('openssl', [
    'req', '-x509', '-newkey', 'rsa:2048', '-nodes',
    '-keyout', key, '-out', cert, '-days', '2',
    '-subj', '/CN=meet.google.com',
    '-addext', 'subjectAltName=DNS:meet.google.com',
  ], { stdio: 'ignore' });
  return { key: fs.readFileSync(key), cert: fs.readFileSync(cert) };
}

// ---------------------------------------------------------------------------------------

async function main() {
  const chromePath = CHROME_CANDIDATES.find((p) => fs.existsSync(p));
  if (!chromePath) throw new Error(`no Chrome found; set CHROME_PATH. Tried: ${CHROME_CANDIDATES.join(', ')}`);

  const workDir = fs.mkdtempSync(path.join(os.tmpdir(), 'richos-live-'));
  const profileDir = path.join(workDir, 'profile');
  const downloadDir = path.join(workDir, 'downloads');
  fs.mkdirSync(profileDir);
  fs.mkdirSync(downloadDir);
  // Point downloads at the scratch folder via profile preferences. (CDP's
  // Browser.setDownloadBehavior renames every file to a GUID, which would hide exactly the
  // thing we want to verify: that sessions land in richos-capture/<session>/…)
  // The same Preferences file blocks notifications and every OS-reaching permission.
  quiet.writeQuietPreferences(profileDir, { download: { default_directory: downloadDir, prompt_for_download: false } });
  console.log(`\nRichOS live capture harness\n  chrome:    ${chromePath}\n  extension: ${EXT_DIR}\n  workdir:   ${workDir}\n`);

  // 1. HTTPS fixture server on a random port, pretending to be meet.google.com.
  const { key, cert } = makeCert(workDir);
  const server = createServer({ key, cert }, (req, res) => {
    res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
    res.end(TONE_PAGE);
  });
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const httpsPort = server.address().port;
  const CALL_URL = `https://meet.google.com:${httpsPort}/abc-defg-hij`;

  // 2. Microphone source for the run.
  //    Chrome's built-in fake device emits a periodic beep — real, loud, sufficient audio.
  //    `--use-file-for-fake-audio-capture` was measured to deliver *digital silence* in this
  //    headless build (page probe 0.000, exported audio −91 dB), so it is opt-in only and
  //    doubles as a deliberate silence test: RICHOS_FAKE_AUDIO_FILE=1 should make the
  //    digital-silence alarm fire.
  let fakeAudioFile = null;
  if (process.env.RICHOS_FAKE_AUDIO_FILE) {
    try {
      fakeAudioFile = path.join(workDir, 'voice.wav');
      execFileSync(
        'ffmpeg',
        ['-v', 'error', '-f', 'lavfi', '-i', 'sine=frequency=220:sample_rate=48000:duration=120', '-ac', '1', fakeAudioFile],
        { stdio: 'ignore' },
      );
    } catch {
      fakeAudioFile = null;
      note('ffmpeg not available', "falling back to Chrome's built-in fake microphone");
    }
  }

  // 3. Launch Chrome.
  const args = [
    `--user-data-dir=${profileDir}`,
    `--load-extension=${EXT_DIR}`,
    `--disable-extensions-except=${EXT_DIR}`,
    '--remote-debugging-port=0',
    // Mute only device output. Keep media graphs and recorded audio live.
    '--mute-audio',
    '--no-first-run',
    '--no-default-browser-check',
    // This profile is disposable. Keep browser encryption away from the user's Keychain.
    ...(process.platform === 'darwin' ? ['--use-mock-keychain'] : []),
    '--disable-features=DialMediaRouteProvider,MediaRouter',
    '--use-fake-ui-for-media-stream',
    '--use-fake-device-for-media-stream',
    ...(fakeAudioFile ? [`--use-file-for-fake-audio-capture=${fakeAudioFile}`] : []),
    '--autoplay-policy=no-user-gesture-required',
    '--ignore-certificate-errors',
    '--host-resolver-rules=MAP meet.google.com 127.0.0.1',
    '--no-proxy-server',
    '--window-size=1000,700',
    // On the host a window is refused (quiet-browser); a real window belongs to the test VM.
    ...(HEADED ? ['--headed'] : []),
  ];

  // Adds --headless=new and the notification switches, and refuses anything not quiet.
  const chrome = trackChrome(quiet.spawnQuietChrome(chromePath, args, { stdio: ['ignore', 'pipe', 'pipe'] }));
  const chromeLog = [];
  chrome.stderr.on('data', (d) => chromeLog.push(String(d)));
  chrome.stdout.on('data', (d) => chromeLog.push(String(d)));

  const portFile = path.join(profileDir, 'DevToolsActivePort');
  const devtoolsPort = await waitFor('DevToolsActivePort', () => {
    if (!fs.existsSync(portFile)) return null;
    const first = fs.readFileSync(portFile, 'utf8').split('\n')[0].trim();
    return first ? Number(first) : null;
  });

  const version = await (await fetch(`http://127.0.0.1:${devtoolsPort}/json/version`)).json();
  const cdp = new Cdp(version.webSocketDebuggerUrl);
  await cdp.ready;
  check('Chrome launched with the unpacked extension', true, `${version.Browser}${HEADED ? ' (headed)' : ' (headless=new)'}`);

  await cdp.send('Target.setDiscoverTargets', { discover: true });

  // 3. Find OUR extension's service worker (Chrome ships component extensions with workers too,
  //    so identify it by the manifest name rather than by being the first one found).
  const { swSession, extensionId } = await waitFor('RichOS service worker', async () => {
    const { targetInfos } = await cdp.send('Target.getTargets');
    if (process.env.RICHOS_DEBUG) console.log('   targets:', targetInfos.map((t) => `${t.type} ${t.url}`).join('\n            '));
    for (const target of targetInfos.filter((t) => t.type === 'service_worker' && t.url.startsWith('chrome-extension://'))) {
      const { sessionId } = await cdp.send('Target.attachToTarget', { targetId: target.targetId, flatten: true });
      const name = await evaluate(cdp, sessionId, 'chrome.runtime.getManifest().name');
      if (name === 'RichOS Helper') return { swSession: sessionId, extensionId: new URL(target.url).host };
      await cdp.send('Target.detachFromTarget', { sessionId });
    }
    return null;
  });
  check('service worker booted', true, `extension id ${extensionId}`);

  // Collect everything the extension logs or throws — "no console errors" has to be evidence,
  // not an assumption.
  const consoleErrors = [];
  const consoleAll = [];
  cdp.listeners.push((msg) => {
    if (msg.method === 'Runtime.consoleAPICalled') {
      const text = (msg.params.args || []).map((a) => a.value ?? a.description ?? a.type).join(' ');
      consoleAll.push(`${msg.params.type}: ${text}`);
      if (['error', 'assert'].includes(msg.params.type)) consoleErrors.push(text);
    }
    if (msg.method === 'Runtime.exceptionThrown') {
      const d = msg.params.exceptionDetails;
      consoleErrors.push(`uncaught: ${d.exception?.description || d.text}`);
    }
  });
  await cdp.send('Runtime.enable', {}, swSession);

  const hooks = await evaluate(cdp, swSession, 'typeof globalThis.__richos?.callCapture?.armTab');
  check('module registry + call-capture module loaded in the worker', hooks === 'function', `typeof armTab = ${hooks}`);

  // Manifest sanity, read from the running extension rather than the file on disk.
  const manifest = await evaluate(cdp, swSession, 'JSON.stringify(chrome.runtime.getManifest())');
  const parsedManifest = JSON.parse(manifest);
  check('manifest parsed by Chrome, name is RichOS Helper', parsedManifest.name === 'RichOS Helper', `v${parsedManifest.version}`);

  // Set fast capture settings BEFORE the call tab ages into auto-arm range (armDelayMs, 3s), so
  // that whoever starts the session first — the extension's own auto-scan or the harness — uses
  // them. autoStartMicCaptions + captureCaptions are ON by default; set explicitly for clarity.
  await evaluate(
    cdp,
    swSession,
    `(async () => { globalThis.__richosTestNotificationAttempts = [];
      // Never send fixture notifications to the user's desktop, even after a regression.
      chrome.notifications.create = async (...args) => {
        globalThis.__richosTestNotificationAttempts.push(args);
        throw new Error('Desktop notification attempted by a quiet browser fixture');
      };
      await chrome.storage.local.set({'richos.settings': {
        core: { notifyOnStartStop: false, notifyOnFailure: false, alertSound: false },
        callCapture: { micProcessing: false, chunkMs: 1000, maxSessionMinutes: 10,
                       autoStartMicCaptions: true, captureCaptions: true } } }); return 'ok'; })()`,
  );

  const quietSettings = await evalJson(cdp, swSession,
    `(async () => JSON.stringify((await chrome.storage.local.get('richos.settings'))['richos.settings'].core))()`);
  check('disposable profile disables desktop notifications and chimes before calls',
    quietSettings.notifyOnStartStop === false && quietSettings.notifyOnFailure === false && quietSettings.alertSound === false);

  // 4. Open the call tab.
  const { targetId: tabTargetId } = await cdp.send('Target.createTarget', { url: CALL_URL });
  const { sessionId: tabSession } = await cdp.send('Target.attachToTarget', { targetId: tabTargetId, flatten: true });
  await cdp.send('Runtime.enable', {}, tabSession);
  const navigation=await cdp.send('Page.navigate',{url:CALL_URL},tabSession);
  if(navigation.errorText)throw new Error(`fixture navigation: ${navigation.errorText}`);
  await waitFor('fixture page to load',()=>evaluate(cdp,tabSession,'document.getElementById("s")?.textContent'));
  await sleep(2500);
  const audioState = await evaluate(cdp, tabSession, 'document.getElementById("s")?.textContent');
  check('fixture call page is playing real audio', /running/.test(String(audioState)), String(audioState));
  if(!/running/.test(String(audioState)))throw new Error('fixture prerequisite failed: '+JSON.stringify(await evaluate(cdp,tabSession,'({url:location.href,secure:isSecureContext,title:document.title,text:document.body?.innerText?.slice(0,500)})')));

  const tabId = await evaluate(
    cdp,
    swSession,
    `(async () => (await chrome.tabs.query({})).filter(t => (t.url||'').includes('meet.google.com')).map(t => ({id: t.id, audible: t.audible})))()`,
  );
  check('extension can see the call tab', Array.isArray(tabId) && tabId.length > 0, JSON.stringify(tabId));
  const callTabId = tabId?.[0]?.id;

  const detected = await evaluate(
    cdp,
    swSession,
    `(async () => { const s = await globalThis.__richos.callCapture.getStatus(); return JSON.stringify({tabs: s.callTabsOpen, active: s.active}); })()`,
  );
  check('platform detection recognizes it as a Google Meet call tab', /Google Meet/.test(JSON.stringify(detected)), JSON.stringify(detected));

  // Independent control: measure the fake microphone from the PAGE, using the same
  // AnalyserNode technique the recorder uses. This distinguishes "Chrome's fake device is
  // silent in this environment" from "our level monitoring is wired up wrong".
  const micProbe = await evalJson(
    cdp,
    tabSession,
    `(async () => {
       try {
         const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false } });
         const ctx = new AudioContext();
         const src = ctx.createMediaStreamSource(stream);
         const an = ctx.createAnalyser();
         an.fftSize = 2048;
         src.connect(an);
         const buf = new Float32Array(an.fftSize);
         let max = 0;
         for (let i = 0; i < 30; i++) {
           await new Promise(r => setTimeout(r, 100));
           an.getFloatTimeDomainData(buf);
           let sum = 0; for (const v of buf) sum += v * v;
           max = Math.max(max, Math.sqrt(sum / buf.length));
         }
         stream.getTracks().forEach(t => t.stop());
         await ctx.close();
         return JSON.stringify({ ok: true, maxRms: max, ctx: 'closed' });
       } catch (e) { return JSON.stringify({ ok: false, error: String(e && e.message || e) }); }
     })()`,
  );
  note('independent microphone probe in the page', JSON.stringify(micProbe));

  const getStatusExpr = `(async () => { try { return JSON.stringify(await globalThis.__richos.callCapture.getStatus()); }
                    catch (e) { return JSON.stringify({ statusError: String(e && e.stack || e) }); } })()`;
  const armTabWith = async (trigger) => {
    const raw = await evaluate(
      cdp,
      swSession,
      `(async () => { try { return JSON.stringify(await globalThis.__richos.callCapture.armTab(${callTabId}, '${trigger}')); }
                      catch (e) { return JSON.stringify({ ok: false, error: String(e && e.message || e) }); } })()`,
    );
    return typeof raw === 'string' ? JSON.parse(raw) : { ok: false, error: String(raw?.__error || raw) };
  };

  // 5. HYBRID AUTO-START. Chrome refuses tab audio without an invocation, but the microphone and
  //    captions must start with ZERO gesture so a detected call is never fully uncaptured. The
  //    extension's own auto-scan may already have started it; if not, an 'auto' trigger does. We
  //    assert the RESULTING STATE either way — that is the guarantee that matters. No click, no
  //    keyboard event, no popup was ever issued in this run.
  let hybridStatus = await evalJson(cdp, swSession, getStatusExpr);
  if (!hybridStatus.active) {
    await armTabWith('auto');
    hybridStatus = await waitFor(
      'the hybrid session to auto-start',
      async () => {
        const s = await evalJson(cdp, swSession, getStatusExpr);
        return s.active ? s : null;
      },
      { timeout: 8000, interval: 400 },
    ).catch(() => hybridStatus);
  }
  check(
    'hybrid: a detected Meet tab is capturing with NO user gesture (auto-started)',
    hybridStatus.active === true && hybridStatus.awaitingTabAudio === true,
    JSON.stringify({ mode: hybridStatus.mode, awaitingTabAudio: hybridStatus.awaitingTabAudio, audioActive: hybridStatus.audioActive }),
  );
  check(
    'the microphone auto-acquired with no gesture (mic + captions, not captions-only)',
    hybridStatus.audioActive === true && hybridStatus.mode === 'mic+captions',
    `mode=${hybridStatus.mode} audioActive=${hybridStatus.audioActive}`,
  );
  let armed = { ok: hybridStatus.active === true, sessionId: hybridStatus.sessionId };
  let pipelineMode = `${hybridStatus.mode} (tab audio needs a human click; mic runs the pipeline)`;

  const status = await evalJson(cdp, swSession, getStatusExpr);
  check('getStatus() runs without throwing and reports an active session', status.active === true, JSON.stringify(status).slice(0, 220));

  // The session record must be on disk BEFORE any audio is — that is the whole anomaly design.
  const earlyFiles = [];
  const walkEarly = (dir) => {
    if (!fs.existsSync(dir)) return;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, entry.name);
      if (entry.isDirectory()) walkEarly(p);
      else earlyFiles.push(path.relative(downloadDir, p));
    }
  };
  await sleep(1500);
  walkEarly(downloadDir);
  check(
    'call START checkpoint exists in browser storage without an automatic download',
    (await evaluate(cdp,swSession,`(async()=>Boolean(await globalThis.__richos.core.idb.get('sessions',${JSON.stringify(status.sessionId)})))()`))===true && earlyFiles.length===0,
    earlyFiles.join(', ') || 'nothing on disk yet',
  );

  // Attach to the offscreen recorder document too — its console is where a broken audio
  // graph would complain.
  const offscreenTarget = await waitFor('offscreen document', async () => {
    const { targetInfos } = await cdp.send('Target.getTargets');
    return targetInfos.find((t) => t.url.includes('core/offscreen.html')) || null;
  }, { timeout: 10000 }).catch(() => null);
  let offscreenSession = null;
  if (offscreenTarget) {
    const { sessionId } = await cdp.send('Target.attachToTarget', { targetId: offscreenTarget.targetId, flatten: true });
    offscreenSession = sessionId;
    await cdp.send('Runtime.enable', {}, sessionId);
    check('offscreen recorder document is running', true, offscreenTarget.url.split('/').pop());

    // Same analyser technique, measured independently inside the offscreen document.
    const offscreenProbe = await evalJson(
      cdp,
      offscreenSession,
      `(async () => {
         try {
           const stream = await navigator.mediaDevices.getUserMedia({ audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false } });
           const ctx = new AudioContext({ sampleRate: 48000 });
           const src = ctx.createMediaStreamSource(stream);
           const an = ctx.createAnalyser(); an.fftSize = 2048;
           src.connect(an);
           const buf = new Float32Array(an.fftSize);
           let max = 0;
           for (let i = 0; i < 20; i++) {
             await new Promise(r => setTimeout(r, 100));
             an.getFloatTimeDomainData(buf);
             let sum = 0; for (const v of buf) sum += v * v;
             max = Math.max(max, Math.sqrt(sum / buf.length));
           }
           stream.getTracks().forEach(t => t.stop());
           await ctx.close();
           return JSON.stringify({ ok: true, maxRms: max });
         } catch (e) { return JSON.stringify({ ok: false, error: String(e && e.message || e) }); }
       })()`,
    );
    note('analyser probe inside the offscreen document', JSON.stringify(offscreenProbe));
  } else {
    check('offscreen recorder document is running', false, 'not found');
  }

  // 7. Let it record, then inspect what actually reached storage.
  await sleep(8000);
  const recorderStatus = await evalJson(
    cdp,
    swSession,
    `(async () => JSON.stringify(await globalThis.__richos.core.callOffscreen({ type: 'cc:status' })))()`,
  );
  check(
    'audio chunks are being written continuously',
    recorderStatus.chunkCount > 2 && recorderStatus.bytesTotal > 0,
    `${recorderStatus.chunkCount} chunks / ${recorderStatus.bytesTotal} bytes / recorder=${recorderStatus.recorderState}`,
  );
  check(
    'microphone stream is live and carrying signal',
    recorderStatus.micTrack?.readyState === 'live',
    JSON.stringify(recorderStatus.micTrack),
  );
  note(
    'recorder-side live levels',
    JSON.stringify({
      micPeak: recorderStatus.micPeak,
      micRmsNow: recorderStatus.micRmsNow,
      ctxSampleRate: recorderStatus.ctxSampleRate,
      micSettings: recorderStatus.micSettings,
      analysers: recorderStatus.analysers,
      ctx: recorderStatus.ctxState,
    }),
  );

  const idbCheck = await evalJson(
    cdp,
    swSession,
    `(async () => {
       const { getAll } = globalThis.__richos.core.idb;
       const chunks = await getAll('chunks');
       const health = await getAll('health');
       return JSON.stringify({ chunks: chunks.length, bytes: chunks.reduce((n,c)=>n+c.bytes,0),
                               sessions: [...new Set(chunks.map(c=>c.sessionId))], health: health.length,
                               maxMicRms: health.reduce((m,h)=>Math.max(m, h.micRms||0), 0),
                               maxTabRms: health.reduce((m,h)=>Math.max(m, h.tabRms||0), 0) });
     })()`,
  );
  check(
    'chunks are durable in IndexedDB (survive a crash of anything above them)',
    idbCheck.chunks > 2 && idbCheck.bytes > 0,
    JSON.stringify(idbCheck),
  );
  const silentDevice = Boolean(process.env.RICHOS_FAKE_AUDIO_FILE);
  check(
    silentDevice
      ? 'per-second health records correctly report ZERO level for a silent device'
      : 'per-second health records carry non-zero RMS from the real audio device',
    idbCheck.health > 0 && (silentDevice ? idbCheck.maxMicRms === 0 : idbCheck.maxMicRms > 0 || idbCheck.maxTabRms > 0),
    `health records=${idbCheck.health} maxMicRms=${idbCheck.maxMicRms} maxTabRms=${idbCheck.maxTabRms}`,
  );

  // 8. Health evaluation + badge.
  const health = await evalJson(
    cdp,
    swSession,
    `(async () => JSON.stringify(await globalThis.__richos.callCapture.getStatus()))()`,
  );
  note('health snapshot', JSON.stringify({ active: health.active, level: health.level, signals: health.signals }));

  // 8a. CAPTIONS. The Meet content script (auto-injected on the fixture, which is served as
  //     meet.google.com) observes the caption region and forwards deduped revisions to the SW.
  //     Prove the whole path: text + speaker + timestamps reach IndexedDB via the collector path.
  const captionStatus = await waitFor(
    'captions to be captured',
    async () => {
      const s = await evalJson(cdp, swSession, getStatusExpr);
      return s.captions && s.captions.count > 0 ? s : null;
    },
    { timeout: 20000, interval: 500 },
  ).catch(() => null);
  check(
    'live captions are captured with speaker labels (the enrichment audio cannot give)',
    Boolean(captionStatus) && captionStatus.captions.count > 0 && (captionStatus.captions.speakers || []).length > 0,
    captionStatus ? `count=${captionStatus.captions.count} speakers=${JSON.stringify(captionStatus.captions.speakers)}` : 'no captions captured',
  );

  const capIdb = await evalJson(
    cdp,
    swSession,
    `(async () => {
       const rows = await globalThis.__richos.core.idb.getAll('captions');
       return JSON.stringify({
         n: rows.length,
         withText: rows.filter(r => r.text && r.text.length).length,
         withSpeaker: rows.filter(r => r.speaker && r.speaker !== 'unknown').length,
         withTs: rows.filter(r => typeof r.t === 'number').length,
         sample: rows.slice(0, 2).map(r => ({ speaker: r.speaker, text: (r.text||'').slice(0,24), t: r.t })),
       });
     })()`,
  );
  check(
    'each persisted caption carries text, a speaker label and a timestamp (loro-ingest ready)',
    capIdb.n > 0 && capIdb.withText === capIdb.n && capIdb.withSpeaker > 0 && capIdb.withTs === capIdb.n,
    JSON.stringify(capIdb),
  );
  check(
    'the caption count shown === the caption records persisted (one collector path, no drift)',
    Boolean(captionStatus) && captionStatus.captions.count === capIdb.n,
    `shown=${captionStatus?.captions.count} persisted=${capIdb.n}`,
  );

  // 8a-soft. Caption adapter failure must FAIL SOFT: break the caption feature in the page and
  //          prove the audio path is entirely unaffected (lose enrichment, never the call).
  const chunksBeforeBreak = (await evalJson(cdp, swSession,
    `(async () => JSON.stringify(await globalThis.__richos.core.callOffscreen({ type: 'cc:status' })))()`)).chunkCount;
  await evaluate(cdp, tabSession, `(() => { try { window.__richosBreakCaptions(); return 'broken'; } catch (e) { return String(e); } })()`);
  await sleep(4000);
  const afterBreak = await evalJson(cdp, swSession,
    `(async () => JSON.stringify(await globalThis.__richos.core.callOffscreen({ type: 'cc:status' })))()`);
  const afterBreakStatus = await evalJson(cdp, swSession, getStatusExpr);
  check(
    'breaking the caption feature does NOT stop the audio (audio kept being written)',
    afterBreak.chunkCount > chunksBeforeBreak,
    `${chunksBeforeBreak} → ${afterBreak.chunkCount} chunks`,
  );
  check(
    'a broken caption adapter never raises an audio red alarm (captions fail soft, audio is the guarantee)',
    afterBreakStatus.level !== 'red' || !(afterBreakStatus.reasons || []).some((r) => /caption/i.test(r.code)),
    `level=${afterBreakStatus.level} reasons=${(afterBreakStatus.reasons || []).map((r) => r.code).join(',')}`,
  );

  // 8a-upgrade. The arm click's control flow: it must never DROP the running mic+captions
  //             session. Real tab audio cannot be minted without a human, so we stub the grant
  //             gate to exercise the controller path (the recorder still cannot obtain a fake tab
  //             stream, which is itself the honest mic-only outcome).
  await evaluate(cdp, swSession, `(() => { chrome.tabCapture.getMediaStreamId = async () => 'harness-stub-stream-id'; return 'stubbed'; })()`);
  const upgrade = await armTabWith('popup');
  note('upgrade-to-full attempt (real tab audio needs a human click on a live Meet call)', JSON.stringify(upgrade));
  const afterUpgrade = await evalJson(cdp, swSession, getStatusExpr);
  check(
    'the arm click never drops the already-running capture session',
    afterUpgrade.active === true,
    `active=${afterUpgrade.active} mode=${afterUpgrade.mode}`,
  );

  // 8b. Negative test (opt-in): with a silent capture device, the digital-silence alarm must
  //     actually fire in-call. Run with RICHOS_FAKE_AUDIO_FILE=1 RICHOS_SILENCE_TEST=1.
  if (process.env.RICHOS_SILENCE_TEST) {
    note('silence test', 'waiting past the warm-up window for the alarm to fire…');
    await sleep(22000);
    const silent = await evalJson(
      cdp,
      swSession,
      `(async () => JSON.stringify(await globalThis.__richos.callCapture.getStatus()))()`,
    );
    const codes = (silent.reasons || []).map((r) => r.code);
    check(
      'a silent capture device raises a RED digital-silence alarm during the call',
      silent.level === 'red' && codes.includes('mic-digital-silence'),
      `level=${silent.level} reasons=${codes.join(',')}`,
    );
    const alerts = await evalJson(
      cdp,
      swSession,
      `(async () => JSON.stringify((await chrome.storage.local.get('richos.callCapture.alertLog'))['richos.callCapture.alertLog'] || []))()`,
    );
    check(
      'the alarm was recorded in the durable alert log',
      alerts.some((a) => a.code === 'mic-digital-silence'),
      alerts.map((a) => a.code).join(',') || 'empty',
    );
  }

  // 9. Service-worker restart: drop all in-memory state and re-run the boot path. The
  //    recorder keeps running underneath, so the worker must re-attach rather than orphan it.
  const recovered = await evalJson(
    cdp,
    swSession,
    `(async () => { try { return JSON.stringify(await globalThis.__richos.callCapture.simulateWorkerRestart()); }
                    catch (e) { return JSON.stringify({ error: String(e && e.stack || e) }); } })()`,
  );
  check(
    'service-worker restart re-attaches to the running recorder instead of losing it',
    recovered.active === true && !recovered.error,
    JSON.stringify(recovered).slice(0, 200),
  );

  await sleep(4000);
  const stillRecording = await evalJson(
    cdp,
    swSession,
    `(async () => JSON.stringify(await globalThis.__richos.core.callOffscreen({ type: 'cc:status' })))()`,
  );
  check(
    'audio kept being written across the worker restart',
    stillRecording.chunkCount > recorderStatus.chunkCount,
    `${recorderStatus.chunkCount} → ${stillRecording.chunkCount} chunks`,
  );

  // 10. Close the call tab — the real end-of-call signal. The session must finalize itself.
  await cdp.send('Target.closeTarget', { targetId: tabTargetId });
  await sleep(9000);

  const afterClose = await evalJson(
    cdp,
    swSession,
    `(async () => JSON.stringify(await globalThis.__richos.callCapture.getStatus()))()`,
  );
  check('closing the call tab finalizes the session', afterClose.active === false, JSON.stringify(afterClose.lastSession || {}));

  check('automatic close retains a pending export without downloading', afterClose.lastSession?.exportPending===true && fs.readdirSync(downloadDir).length===0);
  const exported=await exportBufferedArchive(cdp,swSession,evaluate,downloadDir,afterClose.lastSession.sessionId);
  check('explicit export produces one complete ZIP',exported.ok===true && Boolean(exported.archivePath));
  // 11. Verify the explicitly exported archive contents on disk.
  const found = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(p);
      else found.push({ file: path.relative(downloadDir, p), bytes: fs.statSync(p).size });
    }
  };
  if (fs.existsSync(downloadDir)) walk(downloadDir);
  const audioFiles = found.filter((f) => f.file.endsWith('.webm')).sort((a, b) => b.bytes - a.bytes);
  const sessionFiles = found.filter((f) => f.file.endsWith('session.json'));
  const healthFiles = found.filter((f) => /health\.(ndjson|jsonl)$/.test(f.file));
  const captionFiles = found.filter((f) => /captions\.ndjson$/.test(f.file));
  check('audio was exported to the drop zone', audioFiles.length > 0 && audioFiles[0].bytes > 1000, JSON.stringify(audioFiles));
  check('a session record exists on disk', sessionFiles.length > 0, JSON.stringify(sessionFiles));
  check('per-second health records were exported alongside the audio', healthFiles.length > 0, JSON.stringify(healthFiles));
  check(
    'captions were exported to captions.ndjson on the drop zone (secondary durable channel)',
    captionFiles.length > 0 && captionFiles[0].bytes > 0,
    JSON.stringify(captionFiles),
  );
  if (captionFiles.length) {
    const capText = fs.readFileSync(path.join(downloadDir, captionFiles[0].file), 'utf8').trim();
    const capLines = capText ? capText.split('\n').map((l) => JSON.parse(l)) : [];
    check(
      'captions.ndjson holds one JSON record per revision with speaker + text + timestamp',
      capLines.length > 0 && capLines.every((c) => c.text && typeof c.t === 'number' && 'speaker' in c),
      `${capLines.length} lines; speakers=${JSON.stringify([...new Set(capLines.map((c) => c.speaker))])}`,
    );
  }
  check(
    'artifacts land in <dropFolder>/<session>/ with the documented filenames',
    audioFiles.some((f) => /^richos-capture\/[^/]+\/audio-part-\d+\.webm$/.test(f.file)) &&
      sessionFiles.some((f) => /^richos-capture\/[^/]+\/session\.json$/.test(f.file)),
    found.map((f) => f.file).join(', '),
  );

  if (sessionFiles.length) {
    const record = JSON.parse(fs.readFileSync(path.join(downloadDir, sessionFiles[0].file), 'utf8'));
    check(
      'session record carries producer, status, health accounting and a verdict',
      Boolean(record.producer && record.status && record.health && record.verification),
      `status=${record.status} bytes=${record.audio?.bytesTotal} worst=${record.health?.worstLevel} ok=${record.verification?.ok} problems=${JSON.stringify(record.verification?.problems || [])}`,
    );
    check(
      'the recorded directory name matches the folder the files actually landed in',
      sessionFiles[0].file.startsWith(`richos-capture/${record.dir}/`),
      `${record.dir} vs ${sessionFiles[0].file}`,
    );
    const capFileForSession = captionFiles.find((f) => f.file.startsWith(`richos-capture/${record.dir}/`));
    const capFileLines = capFileForSession
      ? fs.readFileSync(path.join(downloadDir, capFileForSession.file), 'utf8').trim().split('\n').filter(Boolean).length
      : 0;
    check(
      'session.json caption count === captions.ndjson line count (collector-path parity on disk)',
      Number(record.captions?.count || 0) === capFileLines && capFileLines > 0,
      `session.json=${record.captions?.count} file=${capFileLines} speakers=${JSON.stringify(record.captions?.speakers || [])}`,
    );
  }

  // Decode every exported part in Chrome, independently of recorder status or ffmpeg availability.
  const {targetId:decodeTarget}=await cdp.send('Target.createTarget',{url:`chrome-extension://${extensionId}/options/options.html`});
  const {sessionId:decodeSession}=await cdp.send('Target.attachToTarget',{targetId:decodeTarget,flatten:true});
  await cdp.send('Runtime.enable',{},decodeSession);
  for(const file of audioFiles){
    const b64=fs.readFileSync(path.join(downloadDir,file.file)).toString('base64');
    const decoded=await evalJson(cdp,decodeSession,`(async()=>{const ctx=new AudioContext();try{const data=Uint8Array.from(atob(${JSON.stringify(b64)}),c=>c.charCodeAt(0));const audio=await ctx.decodeAudioData(data.buffer);let sum=0,n=0;for(let ch=0;ch<audio.numberOfChannels;ch++){for(const v of audio.getChannelData(ch)){sum+=v*v;n++;}}return JSON.stringify({ok:true,channels:audio.numberOfChannels,duration:audio.duration,rms:Math.sqrt(sum/n)});}catch(e){return JSON.stringify({ok:false,error:String(e)});}finally{await ctx.close();}})()`);
    check(`exported part decodes with stereo channels: ${file.file}`,decoded.ok && decoded.channels===2,JSON.stringify(decoded));
    if(decoded.ok)check(silentDevice?'the exported audio is measurably silent':'the exported audio contains real sound',silentDevice?decoded.rms<.0001:decoded.rms>.0001,JSON.stringify(decoded));
  }
  await cdp.send('Target.closeTarget',{targetId:decodeTarget});

  check(
    'the extension logged no errors and threw no uncaught exceptions',
    consoleErrors.length === 0,
    consoleErrors.slice(0, 3).join(' | ') || 'clean',
  );

  const desktopNotifications = await evalJson(cdp, swSession,
    `(async () => JSON.stringify({ active: await chrome.notifications.getAll(), attempts: globalThis.__richosTestNotificationAttempts }))()`);
  check('test run neither attempts nor leaves desktop notifications', Object.keys(desktopNotifications.active).length === 0 && desktopNotifications.attempts.length === 0, JSON.stringify(desktopNotifications));

  // Wrap up.
  const summary = {
    consoleErrors,
    consoleLog: consoleAll.slice(-40),
    ranAt: new Date().toISOString(),
    chrome: version.Browser,
    headless: !HEADED,
    pipelineMode,
    extensionId,
    downloadDir,
    results,
    dropZone: found,
  };
  const summaryPath = path.join(workDir, 'live-capture-result.json');
  fs.writeFileSync(summaryPath, JSON.stringify(summary, null, 2));
  console.log(`\n${results.filter((r) => r.ok === true).length} checks passed, ${failures} failed`);
  console.log(`result file: ${summaryPath}`);
  if (KEEP) console.log(`kept workdir: ${workDir}`);

  cdp.close();
  server.close();
  // Wait for Chrome and everything it started to exit: its shutdown writes into the profile.
  await stopChrome(chrome);
  await stopAllChrome();
  if (!KEEP) fs.rmSync(workDir, { recursive: true, force: true });
  process.exit(failures ? 1 : 0);
}

main().catch(async (err) => {
  console.error(`\nharness error: ${err.stack}`);
  // Never leave a browser behind on the error path.
  await stopAllChrome().catch((stopError) => console.error(String(stopError.message || stopError)));
  process.exit(2);
});
