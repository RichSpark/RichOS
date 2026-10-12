// review-walk.test.mjs — the parts of the App Review walk (Tools/review-walk.mjs, CEO §107) that can be
// checked without a simulator, a browser or the live review service. The walk itself is proven by
// running it (`bin/rios review-walk --commit SHA`); these guard what silently breaks it:
//   - the app's step lines are read back exactly (a torn or repeated line never passes a step)
//   - a failure before any step names the first step, and the steps keep the review notes' order
//   - every accessibility identifier the walk's UI test touches still exists in the app's code
//   - a bad commit is refused before anything is leased, built or signed in
//
//   node --test richos/mobile/native-ios/Tools/review-walk.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { appSteps, nextStep, resetHost, STEPS, APP_STEPS } from './review-walk.mjs';
import { act, phoneState } from './review-portal.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const native = dirname(here);

test('the app half\'s step lines are read exactly; torn and repeated lines are not', () => {
	const dir = fs.mkdtempSync(join(os.tmpdir(), 'review-walk-test-'));
	try {
		const log = join(dir, 'test.log');
		fs.writeFileSync(log, [
			'Test Case started',
			'REVIEW_WALK_STEP {"detail":"","name":"open it in the app","ok":true}',
			'REVIEW_WALK_STEP {"detail":"","name":"open it in the app","ok":false}',
			'REVIEW_WALK_STEP {"detail":"the words differ","name":"six words match and They match on both sides","ok":false}',
			'REVIEW_WALK_STEP {"name":"send a message and see the Demo reply","ok":tr',
			''
		].join('\n'));
		const { steps } = appSteps(log);
		assert.deepEqual(steps.map((s) => [s.name, s.ok]), [[STEPS.open, true], [STEPS.words, false]]);
		assert.equal(steps[1].detail, 'the words differ');
		assert.deepEqual(appSteps(join(dir, 'missing.log')).steps, []);
	} finally {
		fs.rmSync(dir, { recursive: true, force: true });
	}
});

test('the steps are the review notes\' order, and a failure before any step names the first', () => {
	assert.deepEqual(Object.values(STEPS), ['build the Release app', 'sign in', 'get a pairing link', 'open it in the app',
		'six words match and They match on both sides', 'send a message and see the Demo reply', 'reset the review host']);
	assert.deepEqual(APP_STEPS, [STEPS.open, STEPS.words, STEPS.reply]);
	assert.equal(nextStep([]), STEPS.build);
	assert.equal(nextStep([{ name: STEPS.build, ok: true }]), STEPS.signIn);
});

test('every identifier the walk\'s UI test touches exists in the app', () => {
	const source = fs.readFileSync(join(native, 'UITests/ReviewWalkTests.swift'), 'utf8');
	const used = new Set([...source.matchAll(/any\("([a-zA-Z.]+)"\)|\["(composer\.field)"\]/gu)].map((m) => m[1] || m[2]));
	assert.ok(used.size >= 7, `found only ${[...used].join(', ')}`);
	// Every literal on an identifier line, so `typing ? "composer.send" : ...` counts too.
	const app = execFileSync('grep', ['-rhoE', 'accessibilityIdentifier\\(.*', join(native, 'App')], { encoding: 'utf8' });
	const defined = new Set([...app.matchAll(/"([^"]+)"/gu)].map((m) => m[1]));
	assert.deepEqual([...used].filter((id) => !defined.has(id)), [], 'identifiers the walk uses that the app no longer has');
	// The words are read by their accessibility label, which the app builds as "Word N: word".
	assert.match(fs.readFileSync(join(native, 'App/Features/Pairing/Takeovers.swift'), 'utf8'), /accessibilityLabel\("Word \\\(index \+ 1\): \\\(word\)"\)/u);
});

test('a bad commit is refused before anything is leased, built or signed in', () => {
	const run = spawnSync(process.execPath, [join(here, 'review-walk.mjs'), '--commit', 'not-a-commit-anywhere'], { encoding: 'utf8' });
	assert.equal(run.status, 1);
	assert.match(run.stderr, /not-a-commit-anywhere is not a commit/u);
	assert.doesNotMatch(run.stderr, /waiting for the prepared simulator|building/u);
	const help = spawnSync(process.execPath, [join(here, 'review-walk.mjs'), '--help'], { encoding: 'utf8' });
	assert.equal(help.status, 0);
	assert.match(help.stdout, /rios review-walk --commit <sha>/u);
});

// A stub of the review host's access page: a sign-in form, then the host with or without a phone.
function stubAccessPage({ signedIn, paired, refuseSignIn = false, resetWorks = true }) {
	const site = { signedIn, paired, typed: {}, signOuts: 0, signIns: 0, resets: 0 };
	const text = () => !site.signedIn ? 'Sign in\nUsername\nPassword'
		: `Signed in as apple-review\n${site.paired ? 'Paired\n' : 'Get a pairing link\n'}`;
	return {
		site,
		goto: async () => {},
		fill: async (selector, value) => { site.typed[selector] = value; },
		click: async () => {
			if (!refuseSignIn && site.typed['input[name=username]'] === 'apple-review' && site.typed['input[name=password]'] === 'pw') { site.signedIn = true; site.signIns += 1; }
		},
		locator: (selector) => ({
			innerText: async () => text(),
			count: async () => (selector === '#reset' && site.signedIn && site.paired ? 1 : 0),
			check: async () => {},
			first() { return this; },
			waitFor: async () => {}
		}),
		getByRole: (_role, { name }) => ({
			click: async () => {
				if (name === 'Reset this review host') { site.resets += 1; if (resetWorks) site.paired = false; }
				if (name === 'Sign out') { site.signedIn = false; site.signOuts += 1; }
			}
		})
	};
}
const creds = { access: 'https://access.example/', username: 'apple-review', password: 'pw', host: 'apple-review.example' };

test('the reset signs in again when the access page\'s sign-in has expired, and leaves the host unpaired', async () => {
	const page = stubAccessPage({ signedIn: false, paired: true });
	await resetHost(page, creds);
	assert.equal(page.site.paired, false, 'the host is still paired');
	assert.equal(page.site.signOuts, 1);
});

test('the reset fails, naming what is wrong, when the host still shows a phone or sign-in is refused', async () => {
	const stuck = stubAccessPage({ signedIn: false, paired: true, resetWorks: false });
	await assert.rejects(resetHost(stuck, creds), /still shows a phone after the reset/u);
	const refused = stubAccessPage({ signedIn: false, paired: true, refuseSignIn: true });
	await assert.rejects(resetHost(refused, creds), /signing in again failed/u);
	assert.equal(refused.site.paired, true);
});

// review-portal.mjs: the page's half of a walk on a real phone (`rios device run --as-installed`).
test('the access page\'s phone is read as none, waiting or paired', () => {
	assert.equal(phoneState('Signed in as apple-review\nGet a pairing link'), 'none');
	assert.equal(phoneState('A phone reached this review host\nchimney magnet'), 'waiting');
	assert.equal(phoneState('Review host\nPaired\nRemove this phone'), 'paired');
});

test('the portal never takes a host that already has a phone, and refuses an unavailable output volume', async () => {
	const creds = { access: 'https://example.invalid/', username: 'apple-review', password: 'x', host: 'apple-review.example.invalid' };
	const page = { goto: async () => {}, locator: () => ({ innerText: async () => 'Signed in as apple-review\nPaired\n' }) };
	await assert.rejects(act('link', { out: join(os.tmpdir(), 'review-link') }, page, creds), /already has a phone \(paired\)/);
	await assert.rejects(act('link', { out: '/Volumes/MissingReviewStorage/link' }, page, creds), /MissingReviewStorage/);
	await assert.rejects(act('press', {}, page, creds), /no phone is waiting/);
});
