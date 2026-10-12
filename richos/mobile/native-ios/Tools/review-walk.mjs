#!/usr/bin/env node
import { cacheDir, stateDir } from '../../cli/machine-settings.mjs';
// rios review-walk --commit SHA — the automated walk of exactly what Apple's reviewer does, on the
// RELEASE app built from exactly that commit, against the live review service.
//
// THE CEO, 2026-10-04 (richos-hq/wiki/ceo-decisions.md §107): the CEO is never the first tester. No
// build goes to a store review until an automated walk of exactly what the reviewer does has passed on
// that exact build, and the command checks it itself. `testflight.ts upload` refuses an archive whose
// stamped commit has no passing walk record (richos/mobile/perf/reviewwalk.py).
//
// The walk follows the review notes (review-mock/README.md, "DRAFT: review access instructions"):
//   1. build the Release app      `git archive` of the commit's richos/mobile, built Release for a
//                                 leased prepared simulator (testdevices.py: app data, keychain and
//                                 permissions reset at boot, so the app starts unpaired)
//   2. sign in                    on the access page, in real headless Chromium (Playwright)
//   3. get a pairing link         "Get a pairing link" on the page
//   4. open it in the app         pasted into the app's pairing link field (UITests/ReviewWalkTests.swift)
//   5. six words match ...        the app's six words equal the page's; They match in the app, then on
//                                 the page; the app goes on to its conversation
//   6. send a message ...         one message, and the "Demo reply" quoting it arrives in the app
//   7. reset the review host      "Reset this review host" on the page, so the reviewer finds it unpaired
// Every step passes or the walk fails, naming the step. The record (pass or fail) is written for the
// commit by reviewwalk.py; a later walk replaces it. The review host is reset whether the walk passed
// or not, and the simulator is shut down however the walk ends.
//
// The app under test is the commit's own code. The walk's test (UITests/ReviewWalkTests.swift) and
// its scheme come from THIS checkout, laid over the exported tree's UI-test folder, so a commit from
// before the walk existed can still be walked; the UI-test bundle is never part of the app.
//
// Credentials are read from the review service's private file and never printed.
//
// Battery-check: NO. This runs on the Mac and a simulator only; it changes nothing in the app.

import { spawn, spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import * as fs from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const HERE = dirname(fileURLToPath(import.meta.url));
const NATIVE = resolve(HERE, '..');
const MOBILE = resolve(NATIVE, '..');
const RICHOS = resolve(MOBILE, '..');
const ENGINE_LIB = join(RICHOS, 'engine/scripts/lib');
const TESTDEVICES = join(ENGINE_LIB, 'testdevices.py');
const NATIVE_WORK = join(ENGINE_LIB, 'native-work.py');
const REVIEWWALK = join(MOBILE, 'perf/reviewwalk.py');
const CREDENTIALS = stateDir('review-mock/secrets/review-credentials.json');
const WORK = cacheDir('richos-review-walk/ios');
const DEVICE_TYPE = 'com.apple.CoreSimulator.SimDeviceType.iPhone-16-Pro';
const STEPS = {
	build: 'build the Release app',
	signIn: 'sign in',
	link: 'get a pairing link',
	open: 'open it in the app',
	words: 'six words match and They match on both sides',
	reply: 'send a message and see the Demo reply',
	reset: 'reset the review host'
};
const APP_STEPS = [STEPS.open, STEPS.words, STEPS.reply];

class StepFailed extends Error {
	constructor(step, detail) { super(`${step}: ${detail}`); this.step = step; this.detail = detail; }
}

const log = (line) => process.stderr.write(`review-walk: ${line}\n`);
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

function run(cmd, args, options = {}) {
	const result = spawnSync(cmd, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...options });
	return { code: result.status ?? 1, out: result.stdout ?? '', err: (result.stderr ?? '') + (result.error ? String(result.error) : '') };
}

/** Re-run this command under the machine's worker budget and the simulator cache budget, as
 * Release/simulator-tests.sh does, so the walk takes its turn with every other simulator user. */
function underBudgets(argv) {
	if (!process.env.RICHOS_WORKER_TOKENS) {
		return ['python3', [join(ENGINE_LIB, 'worker_tokens.py'), 'machine', '--', process.execPath, fileURLToPath(import.meta.url), ...argv]];
	}
	fs.mkdirSync(WORK, { recursive: true });
	if (process.env.RICHOS_SIMULATOR_CACHE_HELD !== fs.realpathSync(WORK)) {
		return ['python3', [join(RICHOS, 'app/scripts/lib/simulator_budget.py'), 'cache', WORK, '--', process.execPath, fileURLToPath(import.meta.url), ...argv]];
	}
	return null;
}

function fullCommit(ref) {
	const r = run('git', ['-C', RICHOS, 'rev-parse', '--verify', '--quiet', `${ref}^{commit}`]);
	const sha = r.out.trim();
	if (r.code !== 0 || !/^[0-9a-f]{40}$/u.test(sha)) throw new Error(`${ref} is not a commit in ${RICHOS}`);
	return sha;
}

function credentials() {
	let data;
	try {
		data = JSON.parse(fs.readFileSync(CREDENTIALS, 'utf8'));
	} catch {
		throw new StepFailed(STEPS.signIn, `the review credentials file ${CREDENTIALS} is missing or unreadable`);
	}
	const apple = (data.credentials || []).find((c) => c.username === 'apple-review');
	if (!data.access_page || !apple?.password || !apple?.hostname) {
		throw new StepFailed(STEPS.signIn, `${CREDENTIALS} has no access page or apple-review credentials`);
	}
	const access = String(data.access_page).startsWith('https://') ? String(data.access_page) : `https://${data.access_page}/`;
	return { access: new URL(access).origin + '/', username: apple.username, password: apple.password, host: apple.hostname };
}

/** Step 1: the commit's richos/mobile, the walk's test laid over it, a Release build for testing. */
function build(sha, udid, logs) {
	const src = join(WORK, 'src');
	fs.rmSync(src, { recursive: true, force: true });
	fs.mkdirSync(src, { recursive: true });
	// From the repository's top: `git archive` reads its paths relative to the working directory.
	const top = run('git', ['-C', RICHOS, 'rev-parse', '--show-toplevel']).out.trim();
	const archive = spawnSync('bash', ['-c', 'set -o pipefail; git -C "$1" archive --format=tar "$2" richos/mobile | tar -x -C "$3"', '_', top, sha, src]);
	if (archive.status !== 0 || !fs.existsSync(join(src, 'richos/mobile/native-ios/project.yml'))) throw new StepFailed(STEPS.build, `could not export ${sha.slice(0, 12)}: ${String(archive.stderr).trim().slice(0, 300)}`);
	const native = join(src, 'richos/mobile/native-ios');
	fs.copyFileSync(join(NATIVE, 'UITests/ReviewWalkTests.swift'), join(native, 'UITests/ReviewWalkTests.swift'));
	// The commit's project, its release additions, and one scheme that runs only the walk, in Release.
	const project = fs.readFileSync(join(native, 'project.yml'), 'utf8');
	const includesPlatform = /^\s*-\s*path:\s*Release\/platform\.yml/mu.test(project);
	const scheme = join(WORK, 'walk-scheme.yml');
	fs.writeFileSync(scheme, `schemes:
  RichOSReviewWalk:
    build:
      targets:
        RichOSNative: all
    test:
      config: Release
      targets:
        - name: RichOSNativeUITests
          selectedTests:
            - ReviewWalkTests
      gatherCoverageData: false
`);
	const spec = join(WORK, 'walk-spec.yml');
	fs.writeFileSync(spec, `include:\n  - path: ${join(native, 'project.yml')}\n${includesPlatform ? '' : `  - path: ${join(native, 'Release/platform.yml')}\n`}  - path: ${scheme}\n`);
	const projectDir = join(WORK, 'project');
	fs.mkdirSync(projectDir, { recursive: true });
	const gen = run('xcodegen', ['generate', '--spec', spec, '--project', projectDir, '--project-root', native, '--quiet'],
		{ env: { ...process.env, RICHOS_NATIVE_IOS_ROOT: native } });
	if (gen.code !== 0) throw new StepFailed(STEPS.build, `xcodegen failed: ${gen.err.trim().slice(0, 300)}`);
	const buildLog = join(logs, 'build.log');
	const b = run('python3', [TESTDEVICES, 'run-active', '--kind', 'ios-simulator', '--id', udid, '--owner-pid', String(process.pid), '--',
		'python3', NATIVE_WORK, '--', 'xcodebuild', 'build-for-testing', '-project', join(projectDir, 'RichOSNative.xcodeproj'),
		'-scheme', 'RichOSReviewWalk', '-configuration', 'Release', '-destination', `platform=iOS Simulator,id=${udid}`,
		'-derivedDataPath', join(WORK, 'DerivedData'), '-clonedSourcePackagesDirPath', join(WORK, 'SourcePackages'), 'CODE_SIGN_IDENTITY=-']);
	fs.writeFileSync(buildLog, b.out + b.err);
	if (b.code !== 0) {
		const errors = (b.out + b.err).split('\n').filter((l) => /error:|BUILD FAILED|LEASE LOST/u.test(l)).slice(0, 3).join(' | ');
		throw new StepFailed(STEPS.build, `the Release build failed (exit ${b.code}): ${errors || 'see ' + buildLog}`);
	}
	const app = join(WORK, 'DerivedData/Build/Products/Release-iphonesimulator/RichOSNative.app');
	if (!fs.existsSync(join(app, 'Info.plist'))) throw new StepFailed(STEPS.build, `no Release app at ${app}`);
	const xctestrun = fs.readdirSync(join(WORK, 'DerivedData/Build/Products')).filter((f) => f.startsWith('RichOSReviewWalk_') && f.endsWith('.xctestrun'));
	if (xctestrun.length !== 1) throw new StepFailed(STEPS.build, `expected one RichOSReviewWalk test plan, found ${xctestrun.length}`);
	return { detail: `Release app of ${sha.slice(0, 12)} for simulator ${udid}`, project: join(projectDir, 'RichOSNative.xcodeproj') };
}

/** Starts the app's half; resolves with its exit code and log when it ends. */
function startAppHalf(project, udid, walkDir, link, message, logs) {
	const testLog = join(logs, 'test.log');
	const out = fs.openSync(testLog, 'w');
	const child = spawn('python3', [TESTDEVICES, 'run-active', '--kind', 'ios-simulator', '--id', udid, '--owner-pid', String(process.pid), '--',
		'python3', NATIVE_WORK, '--', 'xcodebuild', 'test-without-building', '-project', project, '-scheme', 'RichOSReviewWalk',
		'-configuration', 'Release', '-destination', `platform=iOS Simulator,id=${udid}`, '-derivedDataPath', join(WORK, 'DerivedData'),
		'-clonedSourcePackagesDirPath', join(WORK, 'SourcePackages'), '-resultBundlePath', join(logs, 'result.xcresult'),
		'-only-testing:RichOSNativeUITests/ReviewWalkTests/testReviewerWalk', 'CODE_SIGN_IDENTITY=-'],
	{ stdio: ['ignore', out, out], env: { ...process.env, TEST_RUNNER_RICHOS_REVIEW_WALK: JSON.stringify({ link, dir: walkDir, message }) } });
	const done = new Promise((finish) => child.once('exit', (code) => { fs.closeSync(out); finish({ code: code ?? 1, log: testLog }); }));
	return { child, done, exited: () => child.exitCode !== null || child.signalCode !== null };
}

/** The app's own step lines, from its log. */
function appSteps(testLog) {
	const text = fs.existsSync(testLog) ? fs.readFileSync(testLog, 'utf8') : '';
	const steps = [];
	for (const m of text.matchAll(/REVIEW_WALK_STEP (\{.*?\})\s*$/gmu)) {
		try { const s = JSON.parse(m[1]); if (!steps.some((x) => x.name === s.name)) steps.push(s); } catch { /* a torn line */ }
	}
	return { steps, text };
}

async function waitFile(path, seconds, app) {
	const end = Date.now() + seconds * 1000;
	while (Date.now() < end) {
		if (fs.existsSync(path)) return fs.readFileSync(path, 'utf8');
		if (app.exited()) return null;
		await sleep(250);
	}
	return null;
}

async function pageText(page) {
	return page.locator('body').innerText();
}

async function walk(sha) {
	const stamp = new Date().toISOString().replace(/[:.]/gu, '-');
	const logs = join(WORK, 'logs', `${sha.slice(0, 12)}-${stamp}`);
	const walkDir = join(WORK, 'run');
	fs.mkdirSync(logs, { recursive: true });
	fs.rmSync(walkDir, { recursive: true, force: true });
	fs.mkdirSync(walkDir, { recursive: true });
	const steps = [];
	const pass = (name, detail = '') => { steps.push({ name, ok: true, detail }); log(`PASS ${name}${detail ? ` (${detail})` : ''}`); };
	let host = '';
	let creds = null;
	// Only once this walk has taken the host (a fresh link on an empty host) does it reset it: a phone
	// found there at the start is someone else's.
	let ours = false;
	let udid = '';
	let browser = null;
	let page = null;
	let app = null;
	let failure = null;
	try {
		creds = credentials();
		host = creds.host;
		// The simulator: the machine's one prepared iPhone 16 Pro, leased to this process.
		const guard = run('python3', [join(ENGINE_LIB, 'cpu_guard.py'), 'check-ios']);
		if (guard.code !== 0) throw new StepFailed(STEPS.build, `the Mac refused a simulator now: ${(guard.out + guard.err).trim().slice(0, 300)}`);
		const runtimes = JSON.parse(run('xcrun', ['simctl', 'list', 'runtimes', '--json']).out || '{"runtimes":[]}').runtimes
			.filter((r) => r.isAvailable && r.name.startsWith('iOS'));
		if (!runtimes.length) throw new StepFailed(STEPS.build, 'no iOS simulator runtime is installed');
		log('waiting for the prepared simulator (one lease at a time on this Mac)');
		const lease = run('python3', [TESTDEVICES, 'acquire-ios', '--type', DEVICE_TYPE, '--runtime', runtimes.at(-1).identifier, '--owner-pid', String(process.pid)]);
		udid = lease.out.trim();
		if (lease.code !== 0 || !udid) throw new StepFailed(STEPS.build, `no simulator: ${lease.err.trim().slice(-300)}`);
		const reg = run('python3', [TESTDEVICES, 'register', '--kind', 'ios-simulator', '--id', udid, '--owner-pid', String(process.pid), '--script', 'review-walk.mjs']);
		if (reg.code !== 0) throw new StepFailed(STEPS.build, `could not register simulator ${udid}: ${reg.err.trim()}`);
		const boot = run('python3', [TESTDEVICES, 'boot-ios', '--id', udid]);
		if (boot.code !== 0) throw new StepFailed(STEPS.build, `simulator ${udid} did not boot: ${boot.err.trim().slice(-300)}`);
		log(`building the Release app of ${sha.slice(0, 12)} (log ${join(logs, 'build.log')})`);
		const built = build(sha, udid, logs);
		pass(STEPS.build, built.detail);

		// 2. Sign in, in real headless Chromium.
		const { locatePlaywright } = await import(pathToFileURL(join(MOBILE, 'review-mock/dev/playwright.mjs')).href);
		const playwrightDir = locatePlaywright();
		if (!playwrightDir) throw new StepFailed(STEPS.signIn, 'Playwright is not installed (set RICHOS_PLAYWRIGHT_DIR)');
		const { chromium } = await import(pathToFileURL(join(playwrightDir, 'index.mjs')).href);
		browser = await chromium.launch();
		page = await browser.newPage();
		await page.goto(creds.access);
		await page.fill('input[name=username]', creds.username);
		await page.fill('input[name=password]', creds.password);
		await page.click('button[type=submit]');
		await page.locator('h2, p.warn, h1:has-text("Not allowed")').first().waitFor({ timeout: 20_000 });
		let body = await pageText(page);
		if (!body.includes(`Signed in as ${creds.username}`)) throw new StepFailed(STEPS.signIn, `the access page did not sign in: ${body.split('\n').find((l) => l.trim()) ?? ''}`.slice(0, 300));
		pass(STEPS.signIn, `${creds.access} as ${creds.username}`);

		// 3. A fresh pairing link. A phone already on the host is someone else's: never removed here.
		if (/A phone reached this review host|\nPaired\n/u.test(`\n${body}\n`)) {
			throw new StepFailed(STEPS.link, `${host} already has a phone (paired or waiting); someone may be using it, or an earlier walk did not reset it`);
		}
		const getLink = page.getByRole('button', { name: /^(Get a pairing link|Get a new link instead)$/u });
		ours = true;
		await getLink.first().click();
		const linkField = page.locator('#pairing-link');
		await linkField.waitFor({ timeout: 20_000 });
		const link = await linkField.inputValue();
		if (!link.startsWith(`https://${host}/`) || !link.includes('#pair=')) throw new StepFailed(STEPS.link, `the page gave no pairing link for ${host}`);
		pass(STEPS.link, `a link for ${host}`);

		// 4-6. The app's half runs now; the page's half answers it.
		const message = `Review walk ${sha.slice(0, 12)} ${randomBytes(3).toString('hex')}`;
		app = startAppHalf(built.project, udid, walkDir, link, message, logs);
		const appWords = await waitFile(join(walkDir, 'app-words'), 240, app);
		if (appWords !== null) {
			let pageWords = '';
			for (const end = Date.now() + 60_000; Date.now() < end && !pageWords;) {
				await page.goto(creds.access);
				const words = page.locator('p.words');
				if (await words.count()) pageWords = (await words.innerText()).trim().split(/\s+/u).join(' ');
				else await sleep(1000);
			}
			const verdict = !pageWords ? 'the access page never showed the phone and its words'
				: pageWords === appWords.trim() ? 'match' : `the app shows "${appWords.trim()}", the page "${pageWords}"`;
			fs.writeFileSync(join(walkDir, 'page-verdict'), verdict);
			if (verdict === 'match' && (await waitFile(join(walkDir, 'app-pressed'), 60, app)) !== null) {
				await page.getByRole('button', { name: 'They match', exact: true }).click();
				await page.locator('h2').first().waitFor({ timeout: 20_000 });
				body = await pageText(page);
				if (!/\nPaired\n/u.test(`\n${body}\n`)) fs.writeFileSync(join(walkDir, 'page-press-failed'), body.slice(0, 300));
			}
		}
		const ended = await app.done;
		const { steps: fromApp, text } = appSteps(ended.log);
		for (const name of APP_STEPS) {
			const s = fromApp.find((x) => x.name === name);
			if (s?.ok) { pass(name, name === STEPS.reply ? `"${message}" answered` : name === STEPS.words ? `"${appWords?.trim()}"` : ''); continue; }
			let detail = s?.detail || '';
			if (name === STEPS.words && fs.existsSync(join(walkDir, 'page-press-failed'))) detail = `They match on the page did not pair: ${fs.readFileSync(join(walkDir, 'page-press-failed'), 'utf8')}`;
			if (!detail) {
				const why = text.split('\n').filter((l) => /error:|failed|LEASE LOST|Testing failed|\*\* TEST/u.test(l)).slice(0, 3).join(' | ');
				detail = `the app's half ended (exit ${ended.code}) before this step finished: ${why || 'see ' + ended.log}`;
			}
			throw new StepFailed(name, detail.slice(0, 500));
		}
		if (ended.code !== 0) throw new StepFailed(STEPS.reply, `every step passed but the test run exited ${ended.code}; see ${ended.log}`);
	} catch (error) {
		failure = error instanceof StepFailed ? error : new StepFailed(nextStep(steps), `${error?.message ?? error}`.slice(0, 500));
		steps.push({ name: failure.step, ok: false, detail: failure.detail });
		log(`FAIL ${failure.step}: ${failure.detail}`);
	} finally {
		if (app && !app.exited()) { app.child.kill('SIGTERM'); await app.done; }
		// 7. Reset, so the real reviewer finds the host unpaired: always, whatever happened above.
		if (page && ours) {
			try {
				await resetHost(page, creds);
				if (!failure) pass(STEPS.reset, `${host} unpaired`);
			} catch (error) {
				const detail = `${error?.message ?? error}`.slice(0, 300);
				if (!failure) {
					failure = new StepFailed(STEPS.reset, detail);
					steps.push({ name: STEPS.reset, ok: false, detail });
				}
				log(`FAIL ${STEPS.reset}: ${detail}`);
			}
		}
		if (browser) await browser.close().catch(() => {});
		if (udid) {
			const released = run('python3', [TESTDEVICES, 'release-ios', '--id', udid, '--owner-pid', String(process.pid), '--if-ours']);
			if (released.code !== 0) log(`simulator ${udid} was not released: ${released.err.trim()}`);
			else log(`simulator ${udid} shut down`);
		}
	}
	const record = { platform: 'ios', commit: sha, passed: !failure, at: new Date().toISOString().replace(/\.\d+Z$/u, 'Z'), host: host || 'unknown', steps };
	const written = run('python3', [REVIEWWALK, 'write'], { input: JSON.stringify(record), env: { ...process.env, PYTHONDONTWRITEBYTECODE: '1' } });
	if (written.code !== 0) throw new Error(`the walk record was not written: ${written.err.trim()}`);
	return { record, path: written.out.trim(), logs };
}

function nextStep(steps) {
	const order = Object.values(STEPS);
	return order[steps.length] ?? STEPS.reset;
}

/** Opens the access page, signing in again with the review credentials when its sign-in has expired;
 * returns the page's text. */
async function openSignedIn(page, creds) {
	await page.goto(creds.access);
	let body = await pageText(page);
	if (!body.includes('Signed in as')) {
		await page.fill('input[name=username]', creds.username);
		await page.fill('input[name=password]', creds.password);
		await page.click('button[type=submit]');
		await page.locator('h2, p.warn, h1:has-text("Not allowed")').first().waitFor({ timeout: 20_000 });
		body = await pageText(page);
		if (!body.includes(`Signed in as ${creds.username}`)) throw new Error(`the access page's sign-in expired and signing in again failed: ${body.split('\n').find((l) => l.trim()) ?? ''}`.slice(0, 300));
	}
	return body;
}

/** Unpairs the review host from its page: Reset for a paired phone, They do not match for a waiting
 * one; then reads the page back, fails unless the host shows no phone and offers a pairing link
 * again, and signs out. The page's sign-in may have expired by now: it signs in again first. */
async function resetHost(page, creds) {
	let body = await openSignedIn(page, creds);
	if (await page.locator('#reset').count()) {
		await page.locator('#reset').check();
		await page.getByRole('button', { name: 'Reset this review host' }).click();
	} else if (body.includes('A phone reached this review host')) {
		await page.getByRole('button', { name: 'They do not match' }).click();
	}
	body = await openSignedIn(page, creds);
	if (/\nPaired\n/u.test(`\n${body}\n`) || body.includes('A phone reached this review host')) throw new Error('the page still shows a phone after the reset');
	if (!/Get a (pairing|new) link/u.test(body)) throw new Error('the page offers no pairing link after the reset');
	await page.getByRole('button', { name: 'Sign out' }).click();
}

async function main() {
	const { values } = parseArgs({ options: { commit: { type: 'string' }, help: { type: 'boolean' } } });
	if (values.help || !values.commit) {
		process.stdout.write('Usage: rios review-walk --commit <sha>\n  Walks what Apple\'s reviewer does, on the Release app of that commit, against the live review service;\n  writes resolved review-walk state/ios/<sha>.json (testflight.ts upload requires its pass).\n');
		process.exitCode = values.help ? 0 : 2;
		return;
	}
	const sha = fullCommit(values.commit);
	const wrapped = underBudgets(process.argv.slice(2));
	if (wrapped) {
		const r = spawnSync(wrapped[0], wrapped[1], { stdio: 'inherit' });
		process.exitCode = r.status ?? 1;
		return;
	}
	const { record, path, logs } = await walk(sha);
	process.stdout.write(`${JSON.stringify({ ok: record.passed, record: path, logs, ...record }, null, 1)}\n`);
	if (!record.passed) {
		const failed = record.steps.find((s) => !s.ok);
		process.stderr.write(`REVIEW WALK FAILED for iPhone ${sha.slice(0, 12)} at step "${failed.name}": ${failed.detail}\n`);
		process.exitCode = 1;
	}
}

export { appSteps, credentials, nextStep, openSignedIn, pageText, resetHost, STEPS, APP_STEPS };

if (import.meta.main) {
	main().catch((error) => {
		process.stderr.write(`review-walk: ${error?.message ?? error}\n`);
		process.exitCode = 1;
	});
}
