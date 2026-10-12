#!/usr/bin/env node
import { validateStorage } from '../../cli/machine-settings.mjs';
// The review access page's "Mac side", one action per call, for a walk on a REAL phone (`rios device run`):
// the page's half of what Apple's reviewer does, in real headless Chromium, signed in as `apple-review`.
//
//   node Tools/review-portal.mjs state                 is a phone on the host: none, waiting or paired
//   node Tools/review-portal.mjs link --out FILE       a fresh pairing link into FILE (mode 600, in selected storage);
//                                                      refused when a phone is already on the host (someone's)
//   node Tools/review-portal.mjs words [--wait S]      the six words the page shows once the phone arrived
//   node Tools/review-portal.mjs press                 "They match" on the page; the host reads back Paired
//   node Tools/review-portal.mjs reset                 "Reset this review host" (or They do not match for a waiting
//                                                      phone); reads back no phone and a pairing link offered
//
// The same steps as `rios review-walk` (review-walk.mjs, whose page helpers this reuses), split so the phone's half
// can run between them. Credentials are read from the review service's private file and never printed; the
// link goes only to FILE. Prints one JSON line; exit 1 with the reason when the page did not do it.
//
// Battery-check: NO. This runs on the Mac against the review service; nothing in the app changes.
import * as fs from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { credentials, openSignedIn, pageText, resetHost } from './review-walk.mjs';

const MOBILE = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

/** The host's phone, from the page's text: none, waiting (arrived, not confirmed) or paired. */
export function phoneState(body) {
	if (/\nPaired\n/u.test(`\n${body}\n`)) return 'paired';
	if (body.includes('A phone reached this review host')) return 'waiting';
	return 'none';
}

async function browserPage() {
	const { locatePlaywright } = await import(pathToFileURL(join(MOBILE, 'review-mock/dev/playwright.mjs')).href);
	const dir = locatePlaywright();
	if (!dir) throw new Error('Playwright is not installed (set RICHOS_PLAYWRIGHT_DIR)');
	const { chromium } = await import(pathToFileURL(join(dir, 'index.mjs')).href);
	const browser = await chromium.launch();
	return { browser, page: await browser.newPage() };
}

export async function act(command, values, page, creds) {
	let body = await openSignedIn(page, creds);
	const host = creds.host;
	if (command === 'state') return { host, phone: phoneState(body) };
	if (command === 'link') {
		const out = resolve(String(values.out || ''));
		if (!values.out) throw new Error('link needs --out FILE');
        validateStorage(out);
		if (phoneState(body) !== 'none') throw new Error(`${host} already has a phone (${phoneState(body)}); someone may be using it. Nothing was changed`);
		await page.getByRole('button', { name: /^(Get a pairing link|Get a new link instead)$/u }).first().click();
		const field = page.locator('#pairing-link');
		await field.waitFor({ timeout: 20_000 });
		const link = await field.inputValue();
		if (!link.startsWith(`https://${host}/`) || !link.includes('#pair=')) throw new Error(`the page gave no pairing link for ${host}`);
		fs.rmSync(out, { force: true });
		fs.writeFileSync(out, link, { mode: 0o600, flag: 'wx' });
		return { host, linkFile: out, issuedAt: new Date().toISOString() };
	}
	if (command === 'words') {
		const end = Date.now() + Number(values.wait ?? 60) * 1000;
		for (;;) {
			const words = page.locator('p.words');
			if (await words.count()) return { host, words: (await words.innerText()).trim().split(/\s+/u).join(' ') };
			if (Date.now() > end) throw new Error('the access page never showed the phone and its words');
			await new Promise((done) => setTimeout(done, 1000));
			body = await openSignedIn(page, creds);
		}
	}
	if (command === 'press') {
		if (phoneState(body) !== 'waiting') throw new Error(`no phone is waiting on ${host} (${phoneState(body)})`);
		await page.getByRole('button', { name: 'They match', exact: true }).click();
		await page.locator('h2').first().waitFor({ timeout: 20_000 });
		body = await pageText(page);
		if (phoneState(body) !== 'paired') throw new Error(`They match on the page did not pair: ${body.slice(0, 200)}`);
		return { host, phone: 'paired', pressedAt: new Date().toISOString() };
	}
	if (command === 'reset') {
		const before = phoneState(body);
		await resetHost(page, creds);   // reads back no phone and a link offered, then signs out
		return { host, before, phone: 'none', resetAt: new Date().toISOString() };
	}
	throw new Error('choose state, link --out FILE, words [--wait S], press or reset');
}

async function main() {
	const { values, positionals } = parseArgs({ allowPositionals: true, options: { out: { type: 'string' }, wait: { type: 'string' } } });
	const creds = credentials();
	const { browser, page } = await browserPage();
	try {
		const result = await act(positionals[0], values, page, creds);
		process.stdout.write(`${JSON.stringify({ ok: true, result })}\n`);
	} finally {
		await browser.close().catch(() => {});
	}
}

if (import.meta.main) {
	main().catch((error) => {
		process.stdout.write(`${JSON.stringify({ ok: false, error: String(error?.message ?? error).slice(0, 400) })}\n`);
		process.exitCode = 1;
	});
}
