#!/usr/bin/env node
import { stateDir } from '../../cli/machine-settings.mjs';
// review-walk.mjs — what Google Play's reviewer does with RichConnect, done by a machine first
// (CEO 2026-10-04, richos-hq/wiki/ceo-decisions.md §107: the CEO is never the first tester).
//
// Run by `randroid review-walk`, which builds the RELEASE app of exactly one commit and installs it
// fresh on its own emulator. This walks the review notes (richos/mobile/review-mock/README.md, "App
// access") against the LIVE review service, the access page in real headless Chromium and the app
// through the emulator's screen:
//
//   1. sign in on the access page with the Google review credentials
//   2. get a pairing link
//   3. open it in the app (the app's own way: "Use a pairing link instead", paste, "Pair with this link")
//   4. the six words match; press They match in the app, then on the page; the app opens the conversation
//   5. send one message and see the "Demo reply" to it arrive
//   6. reset the Google review host from its page, so the next reviewer finds it clean
//
// Every step passes or the walk fails, naming the step. Pass or fail, the record is written through
// richos/mobile/perf/reviewwalk.py (`write`), which `randroid bundle` reads. Step 6 runs whenever the
// sign-in worked, so a failed walk still leaves the host clean. A host that already has a phone (another
// walk, or a reviewer) is not touched: the walk fails at step 1 and says so.
//
//   review-walk.mjs --adb ADB --serial emulator-NNNN --commit SHA --package PKG --activity PKG/CLASS
//                   --apk APK --writer reviewwalk.py [--credentials FILE] [--account google-review]
//
// The password is read from the credentials file and never printed; the single-use pairing link never
// reaches a process list (it is typed from a file) or the record. Only an emulator serial is accepted.
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { resetReviewHost } from './review-reset.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const CREDENTIALS = stateDir('review-mock/secrets/review-credentials.json');

function args(list) {
	const out = {};
	for (let i = 0; i < list.length; i += 2) {
		if (!list[i].startsWith('--') || list[i + 1] === undefined) throw new Error(`expected --name value pairs, got ${list[i]}`);
		out[list[i].slice(2)] = list[i + 1];
	}
	return out;
}

const o = args(process.argv.slice(2));
for (const k of ['adb', 'serial', 'commit', 'package', 'activity', 'apk', 'writer']) {
	if (!o[k]) { console.log(JSON.stringify({ ok: false, error: `review-walk.mjs needs --${k}` })); process.exit(2); }
}
if (!/^emulator-\d+$/.test(o.serial)) { console.log(JSON.stringify({ ok: false, error: 'emulators only: --serial emulator-NNNN' })); process.exit(2); }
if (!/^[0-9a-f]{40}$/.test(o.commit)) { console.log(JSON.stringify({ ok: false, error: '--commit must be a full commit' })); process.exit(2); }

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

// --- the emulator's screen ---------------------------------------------------------------------
function adb(...a) {
	return execFileSync(o.adb, ['-s', o.serial, ...a], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 1 << 25 });
}

const unescapeXml = (s) => s.replace(/&(quot|amp|lt|gt|apos|#\d+);/g, (_, e) => ({ quot: '"', amp: '&', lt: '<', gt: '>', apos: "'" })[e] ?? String.fromCharCode(Number(e.slice(1))));

/** The screen's nodes in document order: {text, desc, bounds}. A dump taken mid-change is empty. */
function screen() {
	try { adb('shell', 'uiautomator', 'dump', '/sdcard/review-walk.xml'); } catch { /* retried by the caller */ }
	let raw = '';
	try { raw = adb('exec-out', 'cat', '/sdcard/review-walk.xml'); } catch { /* empty */ }
	try { adb('shell', 'rm', '-f', '/sdcard/review-walk.xml'); } catch { /* nothing to remove */ }
	if (!raw.startsWith('<?xml')) return [];
	const attr = (tag, name) => { const m = tag.match(new RegExp(` ${name}="([^"]*)"`)); return m ? unescapeXml(m[1]) : ''; };
	return [...raw.matchAll(/<node [^>]*>/g)].map(([tag]) => ({ text: attr(tag, 'text'), desc: attr(tag, 'content-desc'), bounds: attr(tag, 'bounds') }));
}

const shows = (n, want, contains) => [n.text, n.desc].some((v) => (contains ? v.includes(want) : v === want));
const texts = (nodes) => nodes.flatMap((n) => [n.text, n.desc]).filter(Boolean);
const glimpse = (nodes) => `on screen: ${JSON.stringify(texts(nodes).slice(0, 25)).slice(0, 600)}`;

async function find(want, { timeout = 20_000, contains = false } = {}) {
	const end = Date.now() + timeout;
	let nodes = [];
	for (;;) {
		nodes = screen();
		const hit = nodes.find((n) => shows(n, want, contains));
		if (hit) return hit;
		if (Date.now() > end) throw new Error(`the app never showed "${want}" in ${Math.round(timeout / 1000)} s; ${glimpse(nodes)}`);
		await sleep(2000);  // each dump starts a JVM in the guest: poll gently (see QUIET below)
	}
}

async function tap(want, options) {
	const node = await find(want, options);
	const [x1, y1, x2, y2] = (node.bounds.match(/\d+/g) || []).map(Number);
	adb('shell', 'input', 'tap', String(Math.round((x1 + x2) / 2)), String(Math.round((y1 + y2) / 2)));
	return node;
}

/** Type into the focused field. `input text` takes one shell word: escape what the device shell reads. */
function type(text) {
	const escaped = text.replace(/([\\"'`$&|;<>()#*?~])/g, '\\$1').replace(/%/g, '\\%').replace(/ /g, '%s');
	adb('shell', 'input', 'text', escaped);
}

/**
 * Type `text` into the field named `label` and read it back: the field must hold exactly `text`
 * (measured 2026-10-04 on the API 34 emulator: one `input text` left a stray character in front of
 * the link). On a mismatch the field is emptied and typed again, at most three times. Returns the
 * number of tries. The text reaches adb only as an argument to `input`, never a host process list.
 */
async function typeInto(label, text) {
	for (let attempt = 1; attempt <= 3; attempt++) {
		await tap(label);
		await sleep(400);
		type(text);
		// `input text` returns before the keyboard has delivered every character (measured: the link
		// arrives over several seconds on a loaded host), so wait for the field to settle.
		// A field whose text the screen reader cannot see reads "": then the next step (the app's own
		// answer to what was typed) is the check. A wrong character breaks off and types again.
		let field = null;
		let seen = '';
		for (const end = Date.now() + 30_000; Date.now() < end;) {
			const nodes = screen();
			field = nodes.find((n) => shows(n, label, false)) || field;
			seen = field ? field.text : '';
			if (seen === text || nodes.some((n) => n.text === text)) return attempt;
			if (seen && !text.startsWith(seen)) break;
			await sleep(2000);
		}
		if (!seen) return attempt;
		if (!field) field = await find(label);
		await tap(label);
		adb('shell', 'input', 'keyevent', 'KEYCODE_MOVE_END');
		adb('shell', 'input', 'keyevent', ...Array(Math.max(field.text.length, text.length) + 8).fill('KEYCODE_DEL'));
	}
	throw new Error(`the field "${label}" never held exactly what was typed (${text.length} characters) after 3 tries`);
}

/** The six words on the app's screen: each word follows its ordinal (1 to 6) in the word grid. */
function appWords(nodes) {
	const words = [];
	for (let i = 0; i < nodes.length - 1 && words.length < 6; i++) {
		if (nodes[i].text === String(words.length + 1)) {
			const next = nodes.slice(i + 1).find((n) => n.text);
			if (next && /^[a-z]+$/.test(next.text)) words.push(next.text);
		}
	}
	return words;
}

// --- the access page, in real headless Chromium --------------------------------------------------
function locatePlaywright() {
	const candidates = [];
	if (process.env.RICHOS_PLAYWRIGHT_DIR) candidates.push(process.env.RICHOS_PLAYWRIGHT_DIR);
	const repo = resolve(here, '..', '..', '..');
	candidates.push(join(repo, 'app', 'ui', 'tests', 'node_modules', 'playwright'));
	try {
		const common = execFileSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'], { cwd: here, encoding: 'utf8' }).trim();
		candidates.push(join(dirname(common), 'richos', 'app', 'ui', 'tests', 'node_modules', 'playwright'));
	} catch { /* not in a checkout */ }
	return candidates.find((c) => existsSync(join(c, 'package.json'))) || null;
}

function account() {
	const data = JSON.parse(readFileSync(o.credentials || CREDENTIALS, 'utf8'));
	const want = o.account || 'google-review';
	const entry = (data.credentials || []).find((c) => c.username === want);
	if (!entry || !data.access_page || !entry.hostname || !entry.password) throw new Error(`no ${want} credentials and access page in ${o.credentials || CREDENTIALS}`);
	return { access: data.access_page.replace(/\/+$/, '') + '/', username: entry.username, password: entry.password, host: entry.hostname };
}

// --- the walk ----------------------------------------------------------------------------------
const steps = [];
async function step(name, run, { always = false } = {}) {
	if (!always && steps.some((s) => !s.ok)) {
		steps.push({ name, ok: false, detail: 'not reached: an earlier step failed' });
		return;
	}
	try {
		steps.push({ name, ok: true, detail: String((await run()) || '') });
	} catch (error) {
		steps.push({ name, ok: false, detail: String(error && error.message || error).slice(0, 900) });
	}
}

const scratch = mkdtempSync(join(process.env.TMPDIR || tmpdir(), 'review-walk-'));
let creds = null;
let browser = null;
let page = null;
let signedIn = false;
let occupied = false;  // another walk or a reviewer has the host: never reset it under them
let link = '';
const marker = `Review walk ${o.commit.slice(0, 8)} ${new Date().toISOString().slice(11, 19)}`;
const apkSha = createHash('sha256').update(readFileSync(o.apk)).digest('hex');

// A reviewer installs the app fresh from Google Play: no earlier install, no earlier data. This is the
// walk's own emulator (the serial is checked above to be emulator-NNNN), never a phone.
try { adb('uninstall', o.package); } catch { /* not installed yet */ } // device-cli-exempt: emulator-NNNN serials only, checked above
try { adb('install', o.apk); } catch (error) { // device-cli-exempt: emulator-NNNN serials only, checked above
	console.log(JSON.stringify({ ok: false, error: `the release build did not install on ${o.serial}: ${String(error.message).slice(0, 300)}` }));
	process.exit(1);
}

async function body() { return page.locator('body').innerText(); }

try {
	creds = account();
} catch (error) {
	steps.push({ name: 'sign in on the access page', ok: false, detail: `no review credentials: ${error.message}` });
}

if (creds) {
	await step('sign in on the access page', async () => {
		const dir = locatePlaywright();
		if (!dir) throw new Error('Playwright is not installed (set RICHOS_PLAYWRIGHT_DIR to its package directory)');
		const { chromium } = await import(pathToFileURL(join(dir, 'index.mjs')).href);
		browser = await chromium.launch();
		page = await browser.newPage();
		await page.goto(creds.access, { waitUntil: 'load', timeout: 30_000 });
		await page.fill('input[name=username]', creds.username);
		await page.fill('input[name=password]', creds.password);
		await page.click('button[type=submit]');
		await page.locator('h2, h1:has-text("Not allowed")').first().waitFor({ timeout: 20_000 });
		const text = await body();
		if (/Not allowed|did not match|Too many sign-in attempts/.test(text)) throw new Error(`the page refused the sign-in: ${text.slice(0, 300)}`);
		if (!text.includes(creds.host)) throw new Error(`signed in, but the page does not name ${creds.host}: ${text.slice(0, 300)}`);
		signedIn = true;
		if (/is paired with this review host|A phone reached this review host/.test(text)) {
			occupied = true;
			throw new Error(`${creds.host} already has a phone (another walk or a reviewer is using it); nothing was changed. ` +
				'Wait for it, or reset the host on the access page, then walk again');
		}
		return `signed in as ${creds.username} at ${creds.access} (headless Chromium); the page is the Mac for ${creds.host}`;
	});

	await step('get a pairing link', async () => {
		const fresh = page.getByRole('button', { name: 'Get a pairing link', exact: true });
		if (await fresh.count()) await fresh.click();
		else await page.getByRole('button', { name: 'Get a new link instead', exact: true }).click();
		const field = page.locator('#pairing-link');
		await field.waitFor({ timeout: 20_000 });
		link = await field.inputValue();
		const prefix = `https://${creds.host}/#pair=`;
		if (!link.startsWith(prefix) || link.length <= prefix.length) throw new Error(`the page's link is not a pairing link for ${creds.host}`);
		return `the page issued a pairing link for ${creds.host} (${link.length} characters; not recorded, it is single-use)`;
	});

	await step('open the pairing link in the app', async () => {
		// QUIET: the emulator draws in software, and the host's CPU guard stops any process above 3
		// cores for 10 s; measured 2026-10-04, three walks in a row had qemu stopped at 3.1-3.7 cores
		// while the link was typed (cpu-guard events.jsonl, "CPU circuit breaker stopped an owned
		// workload"). System animations off and gentle screen polling keep the emulator under it.
		for (const scale of ['window_animation_scale', 'transition_animation_scale', 'animator_duration_scale']) {
			adb('shell', 'settings', 'put', 'global', scale, '0');
		}
		// A quarter of the pixels at half the density: the same 411 x 914 dp screen the app lays out
		// for on the emulator's 1080 x 2400 at 420 dpi, drawn for a quarter of the software cost.
		adb('shell', 'wm', 'size', '540x1200');
		adb('shell', 'wm', 'density', '210');
		adb('shell', 'am', 'start', '-W', '-n', o.activity);
		await tap('Use a pairing link instead', { timeout: 45_000 });
		const tries = await typeInto('Pairing link', link);
		await tap('Pair with this link');
		await find('Do they match your Mac?', { timeout: 45_000 });
		return `the release app opened the link (Use a pairing link instead, pasted, Pair with this link) and showed its six words (Do they match your Mac?)` +
			(tries > 1 ? ` (the link field was typed ${tries} times before it read exactly the link)` : '');
	});

	await step('the six words match; They match in the app and on the page', async () => {
		let nodes = screen();
		let words = appWords(nodes);
		for (let i = 0; words.length < 6 && i < 10; i++) { await sleep(500); nodes = screen(); words = appWords(nodes); }
		if (words.length !== 6) throw new Error(`the app does not show six words; ${glimpse(nodes)}`);
		await page.goto(creds.access, { waitUntil: 'load' });  // the page's Refresh
		const shown = page.locator('p.words');
		await shown.waitFor({ timeout: 20_000 });
		const pageWords = (await shown.innerText()).trim().split(/\s+/);
		if (pageWords.join(' ') !== words.join(' ')) throw new Error(`the words differ: app "${words.join(' ')}", page "${pageWords.join(' ')}"`);
		await tap('They match');
		await find('Now press They match on your Mac', { timeout: 30_000 });
		await page.getByRole('button', { name: 'They match', exact: true }).click();
		await page.getByText('Paired. RichConnect is now connected to this review host.').waitFor({ timeout: 20_000 });
		await find('Message Rich', { timeout: 45_000 });
		return `app and page both show "${words.join(' ')}"; They match pressed in the app, then on the page; the page says Paired and the app opened the conversation`;
	});

	await step('send a message and see the Demo reply', async () => {
		await typeInto('Message Rich', marker);
		await tap('Send message');
		const sent = Date.now();
		const end = sent + 60_000;
		let nodes = [];
		for (;;) {
			nodes = screen();
			const reply = nodes.find((n) => [n.text, n.desc].some((v) => v.startsWith('Demo reply') && v.includes(marker)));
			if (reply) return `sent "${marker}"; "${(reply.text || reply.desc).slice(0, 160)}" arrived ${((Date.now() - sent) / 1000).toFixed(1)} s later`;
			if (Date.now() > end) throw new Error(`no Demo reply to "${marker}" within 60 s of sending; ${glimpse(nodes)}`);
			await sleep(2000);
		}
	});

	if (signedIn && !occupied) {
		const walked = steps.every((s) => s.ok);  // the phone is paired now, so the reset is the step itself
		await step('reset the Google review host', async () => {
			return resetReviewHost(page, creds, { walked });
		}, { always: true });
	} else {
		steps.push({ name: 'reset the Google review host', ok: false, detail: occupied ? 'not done: the host was in use by someone else' : 'not reached: the sign-in failed' });
	}
}

if (browser) await browser.close().catch(() => {});
// Give the emulator its own screen back (it may be one this walk did not boot).
for (const what of ['size', 'density']) { try { adb('shell', 'wm', what, 'reset'); } catch { /* the emulator is gone */ } }
rmSync(scratch, { recursive: true, force: true });

const record = {
	platform: 'android', commit: o.commit, passed: steps.length > 0 && steps.every((s) => s.ok),
	at: new Date().toISOString().replace(/\.\d+Z$/, 'Z'), host: creds ? creds.host : 'unknown', steps,
	apkSha256: apkSha, emulator: o.serial
};
const written = spawnSync('python3', [o.writer, 'write'], { input: JSON.stringify(record), encoding: 'utf8', env: { ...process.env, PYTHONDONTWRITEBYTECODE: '1' } });
if (written.status !== 0) {
	console.log(JSON.stringify({ ok: false, mode: 'review-walk', error: `the walk record was not written: ${written.stderr.trim()}`, result: record }, null, 2));
	process.exit(1);
}
console.log(JSON.stringify({ ok: record.passed, mode: 'review-walk', command: 'review-walk', result: { record: written.stdout.trim(), ...record } }, null, 2));
process.exit(record.passed ? 0 : 1);
