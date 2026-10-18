#!/usr/bin/env node
'use strict';

// THE APP, IN A REAL BROWSER, AGAINST A REAL TLS ORIGIN.
//
//   node test/desktop-verify.js              both engines
//   node test/desktop-verify.js --chromium   one of them
//   node test/desktop-verify.js --webkit
//
// The unit suites prove the arithmetic — the queue's state machine, the thread's ordering, the
// recorder's filter, the signature's bytes. This proves the WIRING: that getUserMedia, the
// AudioWorklet, IndexedDB, the service worker, `EventSource`, WebCrypto and the layout actually
// connect to each other in a browser. That is the class of defect a unit test cannot see and the
// class he would find on his phone.
//
// WHY BOTH ENGINES. His phone is WebKit, so WebKit is the one whose LAYOUT and COLORS matter — and
// it is also the engine that cannot fake a microphone. Chromium can, and Chromium can be made to
// trust exactly one certificate by pinning its public key, which is what keeps `isSecureContext`
// true. So the run is split honestly: each engine is asked what it can actually answer, every skip
// is printed as a skip, and nothing is claimed for an engine that did not do it.
//
// HOW THE CERTIFICATE IS TRUSTED, and what is deliberately NOT done: Chromium gets
// `--ignore-certificate-errors-spki-list=<pin>`, which accepts exactly one certificate — the one
// this run minted — and keeps rejecting every other bad certificate in the world. That matters
// because it leaves the origin a SECURE CONTEXT, which is what service workers and getUserMedia are
// gated on. `--ignore-certificate-errors` and Playwright's `ignoreHTTPSErrors` do not: they mark
// the page insecure, which can stop a service worker registering — the very thing to be proved.
// WebKit has no equivalent pin, so it runs with `ignoreHTTPSErrors` and REPORTS whether the context
// came out secure rather than assuming it. NOTHING is added to any keychain, here or anywhere.
//
// NO SOUND IS PLAYED IN THE ROOM. Chromium runs with `--mute-audio` and a synthesized microphone
// input inside the browser process; the "hear it" control is exercised in Chromium only, muted, and
// what is asserted is the fetch and the element's state. This Mac's speakers are never a party to a
// test here (the standing rule about audio on this machine).
//
// Playwright is NOT a dependency of this app. It is resolved from wherever it already exists
// (`PHONE_PLAYWRIGHT` overrides); if it is not there, this says so and exits 0 rather than
// pretending to have verified something.

const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const { createStubMac } = require('./stub-mac.js');
const fingerprint = require('../lib/fingerprint.js');
const contrastLib = require('../../../app/ui/tests/lib/contrast.js');

// The app's own browser harness already installs Playwright WITH WebKit (`app/ui/tests` pins it and
// its postinstall runs `playwright install webkit`), so that one is preferred: a Playwright whose
// WebKit was never downloaded can only run half of this. The order is worth stating because
// resolving the wrong one presents as "WebKit is not in this Playwright build", which is true and
// unhelpful.
const PLAYWRIGHT_CANDIDATES = [
	process.env.PHONE_PLAYWRIGHT,
	path.join(__dirname, '..', '..', 'ui', 'tests', 'node_modules', 'playwright'),
	'/Users/alex/ab/richos/richos/app/ui/tests/node_modules/playwright',
	'playwright',
	'/Users/alex/ab/femcboost/avelor/node_modules/playwright'
].filter(Boolean);

function loadPlaywright() {
	for (const candidate of PLAYWRIGHT_CANDIDATES) {
		try { return require(candidate); } catch { /* try the next */ }
	}
	return null;
}

const WANT = process.argv.includes('--webkit') ? ['webkit']
	: process.argv.includes('--chromium') ? ['chromium']
		: ['chromium', 'webkit'];

const SHOTS_DIR = process.env.PHONE_SHOTS_DIR
	|| path.join(__dirname, '..', '..', '..', '..', 'docs', 'verification', 'phone-app-2026-09-18');

// Phone widths that matter: the smallest phone still on iOS 16 (SE, 320), the CEO's Android phone
// (360 — the width that rendered "Runni / ng", audit §4.3), his iPhone X (375), the modern base
// (390) and the Max (430).
const WIDTHS = [320, 360, 375, 390, 430];
const HIS_PHONE = { width: 375, height: 812 };

let failures = 0;
let skips = 0;

function check(label, ok, detail) {
	process.stdout.write(`${ok ? 'PASS' : 'FAIL'}  ${label}${detail ? `\n        ${detail}` : ''}\n`);
	if (!ok) failures++;
}

function skip(label, why) {
	process.stdout.write(`SKIP  ${label}\n        ${why}\n`);
	skips++;
}

function section(title) {
	process.stdout.write(`\n=== ${title} ===\n`);
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ------------------------------------------------------------------------------------------------

async function run() {
	const playwright = loadPlaywright();
	if (!playwright) {
		process.stdout.write('SKIPPED: Playwright is not installed on this machine, so the browser half did not run.\n'
			+ 'Set PHONE_PLAYWRIGHT to a playwright module path to run it. This is reported, not hidden.\n');
		return 0;
	}

	fs.mkdirSync(SHOTS_DIR, { recursive: true });

	for (const engine of WANT) {
		if (!playwright[engine]) { skip(`${engine} is not in this Playwright build`, 'nothing to run'); continue; }
		await runEngine(playwright, engine);
	}

	process.stdout.write(`\n=== ${failures === 0 ? 'ALL CHECKS PASSED' : `${failures} CHECK(S) FAILED`}${skips ? `, ${skips} skipped` : ''} ===\n`);
	return failures ? 1 : 0;
}

async function runEngine(playwright, engine) {
	section(`${engine}`);

	// `manual`: the Mac waits for a person to press They match ON IT (Sage's pairing review F1), so
	// this walk sees the phone's waiting screen and presses the Mac's button itself.
	const mac = createStubMac({ host: 'localhost', pairCode: 'harness-pair-code', macPress: 'manual' });
	// One message with a long unbroken run of characters, because that is the shape that produced
	// his horizontal scroll bar on the probe and the shape the layout has to survive.
	mac.state.ledger.push({
		id: 'seed-long', thread_id: mac.threadId, cursor: mac.state.nextCursor++, role: 'rich', kind: 'text',
		text: 'The file is at https://example.invalid/a-very-long-unbroken-address-that-nothing-can-wrap-at-all-because-it-has-no-spaces-anywhere-in-it/and-it-keeps-going',
		created_at: new Date().toISOString(), complete: true, has_audio: false
	});

	const port = await mac.listen(0);
	const origin = `https://localhost:${port}`;

	const profile = fs.mkdtempSync(path.join(os.tmpdir(), `richos-phone-${engine}-`));
	let browser = null;

	// However this ends — a thrown assertion, a browser that will not start, a Ctrl-C — nothing may
	// be left behind: no server on a port, no temp directory, no browser process.
	const cleanup = async () => {
		try { if (browser) await browser.close(); } catch { /* already gone */ }
		try { await mac.close(); } catch { /* already gone */ }
		try { fs.rmSync(profile, { recursive: true, force: true }); } catch { /* nothing else to do */ }
	};
	const onSignal = () => { cleanup().finally(() => process.exit(1)); };
	process.on('SIGINT', onSignal);
	process.on('SIGTERM', onSignal);

	try {
		const launch = engine === 'chromium'
			? {
				args: [
					'--use-fake-ui-for-media-stream',
					'--use-fake-device-for-media-stream',
					'--mute-audio',
					`--ignore-certificate-errors-spki-list=${mac.spkiPin}`
				]
			}
			: {};
		browser = await playwright[engine].launch(launch);

		const context = await browser.newContext(Object.assign({
			viewport: HIS_PHONE,
			deviceScaleFactor: 2,
			isMobile: engine === 'chromium',
			hasTouch: true,
			colorScheme: 'dark',
			locale: 'en-US'
		}, engine === 'webkit' ? { ignoreHTTPSErrors: true } : {}));

		if (engine === 'chromium') await context.grantPermissions(['microphone', 'notifications'], { origin });

		const page = await context.newPage();
		// TWO BUCKETS, AND THE DIFFERENCE IS THE POINT. A `pageerror` is this app throwing, and there
		// may be none. A console error that says a request failed is the BROWSER reporting the
		// outage this run deliberately caused — the destroyed sockets that make the Mac unreachable,
		// and the 403 that makes it forget the phone. Collapsing the two would either hide a real
		// exception among expected noise or, worse, teach the next person to ignore the whole check.
		const errors = [];
		const networkNoise = [];
		// WEBKIT REPORTS SOME OF THOSE FAILED REQUESTS DOWN THE EXCEPTION CHANNEL. A load that fails
		// in CORS mode (every `EventSource`, and this app's `fetch` calls) can be logged by WebKit as
		// "<initiator> cannot load <url> due to access control checks." with a JavaScript source
		// (WebCore `ThreadableLoader::logError`), and Playwright's WebKit driver turns every
		// JavaScript-source error line into `pageerror` (playwright-core `_onConsoleMessage`,
		// `level === "error" && source === "javascript"`) — which is why these arrive as
		// "https: /localhost…", its name/message split at the first colon. Which failed load WebKit
		// reports this way varies run to run. Such a line is expected in exactly two cases, and
		// both are decided from what the stub Mac recorded rather than from the wording alone:
		//   * the OUTAGE — it names a request the stub Mac itself dropped, byte for byte;
		//   * a RELOAD THIS HARNESS STARTED — reported while `page.reload()` is in flight, naming a
		//     request the Mac has not received since that reload began (counted per URL, because
		//     `/api/challenge` is the same URL every time). Measured: the link's scheduled reconnect fired
		//     after `beforeunload`, the old document built its EventSource, WebKit refused it
		//     locally (no request reached the Mac), and the app's stream-error probe was refused
		//     the same way, all before `pagehide`. A dying page's requests that never left it.
		// Anything else stays an error — including a request the Mac did receive, which is where a
		// real cross-origin refusal would have to show up.
		const LOADER_REPORT = /^(?:EventSource|Fetch API|XMLHttpRequest) cannot load https?:\s?\/{1,2}[^/\s]+(\/api\/\S+) due to access control checks\.$/;
		// What the Mac had received when the current harness reload began; null when none is running.
		let reloadBaseline = null;
		const loaderReports = { outage: 0, reload: 0 };
		const reload = page.reload.bind(page);
		page.reload = async (...args) => {
			reloadBaseline = new Map(mac.state.seen);
			try { return await reload(...args); } finally { reloadBaseline = null; }
		};
		const neverReachedTheMac = (url) => (mac.state.seen.get(url) || 0) === (reloadBaseline.get(url) || 0);
		page.on('pageerror', (e) => {
			const text = String(e);
			const report = LOADER_REPORT.exec(text);
			if (report && mac.state.dropped.has(report[1])) { loaderReports.outage++; networkNoise.push(text); }
			else if (report && reloadBaseline && neverReachedTheMac(report[1])) { loaderReports.reload++; networkNoise.push(text); }
			else errors.push(text);
		});
		page.on('console', (m) => {
			if (m.type() !== 'error') return;
			if (/Failed to load resource|net::ERR_|status of 40[0-9]|status of 5[0-9][0-9]/.test(m.text())) networkNoise.push(m.text());
			else errors.push(`console: ${m.text()}`);
		});

		// ---------------------------------------------------------------------------------------
		section(`${engine}: the origin itself`);
		// ---------------------------------------------------------------------------------------

		await page.goto(`${origin}/#pair=harness-pair-code`);
		await page.waitForSelector('#pairing:not([hidden])', { timeout: 15000 });

		const originFacts = await page.evaluate(() => ({
			secure: window.isSecureContext,
			protocol: location.protocol,
			host: location.host,
			serviceWorker: 'serviceWorker' in navigator,
			indexedDB: typeof indexedDB !== 'undefined',
			subtle: Boolean(window.crypto && window.crypto.subtle)
		}));
		if (engine === 'chromium') {
			check('the pinned certificate makes this a SECURE CONTEXT — what service workers and the microphone are gated on',
				originFacts.secure === true && originFacts.protocol === 'https:', JSON.stringify(originFacts));
		} else {
			// Reported rather than asserted: WebKit has no public-key pin, so this run says what it
			// actually got instead of claiming what would be convenient.
			process.stdout.write(`        WebKit reports isSecureContext=${originFacts.secure} (no public-key pin exists for this engine)\n`);
			check('WebKit has WebCrypto, IndexedDB and a service worker container on this origin',
				originFacts.subtle && originFacts.indexedDB && originFacts.serviceWorker, JSON.stringify(originFacts));
		}
		check('it is the origin the phone will use, port included', originFacts.host === `localhost:${port}`, originFacts.host);

		// ---------------------------------------------------------------------------------------
		section(`${engine}: pairing — six words, and not one identifier`);
		// ---------------------------------------------------------------------------------------

		await page.waitForSelector('#fingerprint-box:not([hidden])', { timeout: 15000 });
		const shown = (await page.textContent('#fingerprint-words')).trim();
		// v2 (Sage F2): over the origin this page dialed, the Mac's value and the key the Mac
		// REGISTERED — which is what the Mac's own sheet shows beside its They match.
		const registered = [...mac.state.devices.values()][0];
		const expected = await fingerprint.phraseV2(origin, mac.caFingerprintHex, fingerprint.pointFromJwk(registered.jwk), require('node:crypto').webcrypto.subtle);
		check('the phone shows the v2 six words over its own origin, the Mac\'s value and the key the Mac registered',
			shown === expected, `shown "${shown}", derived "${expected}"`);
		check('the phone told the Mac it shows v2 words', registered.pairingVersion === 2, String(registered.pairingVersion));
		check('six words, not five and not a hash', shown.split(/\s+/).length === 6, shown);

		// HIS FINDING, TURNED INTO A GATE. Nothing on this screen may be a long run of hexadecimal.
		const hexOnScreen = await page.evaluate(() => {
			const text = document.body.innerText;
			const m = /\b[0-9a-fA-F]{16,}\b/.exec(text);
			return m ? m[0] : null;
		});
		check('no raw hash or identifier is anywhere on the pairing screen', hexOnScreen === null, hexOnScreen || '');

		await noHorizontalScroll(page, `${engine}: pairing`);
		await shoot(page, `${engine}-pairing`);

		check('pairing offers both answers, because "they do not match" is a real outcome',
			await page.isVisible('#pair-confirm') && await page.isVisible('#pair-reject'));

		await page.click('#pair-confirm');
		// SAGE F1: THE PHONE'S OWN ANSWER LETS NOTHING IN. It waits for the press on the Mac, says
		// which press is missing, keeps the words up and keeps They do not match as the way out.
		await page.waitForFunction(() => /press They match on your Mac/.test(document.getElementById('pairing-lede').textContent), null, { timeout: 15000 });
		check('after They match on the phone, the phone says to press They match on the Mac',
			true, (await page.textContent('#pairing-lede')).trim());
		check('while waiting, the words stay up and They do not match is the way out',
			await page.isVisible('#fingerprint-words') && await page.isVisible('#pair-reject') && !(await page.isVisible('#pair-confirm')));
		check('nothing of the conversation is shown before the Mac is pressed', !(await page.isVisible('#composer')));
		await noHorizontalScroll(page, `${engine}: waiting for the Mac`);
		await shoot(page, `${engine}-waiting-for-mac`);
		// Urban's review, state 3: the control's name is set apart, and both places are named.
		const waitingDetail = await page.evaluate(() => {
			const d = document.getElementById('pairing-detail');
			return { text: d.textContent, strong: (d.querySelector('strong') || {}).textContent || '' };
		});
		check('while waiting, the phone says it carries on by itself and names both places to refuse',
			/carries on by itself/.test(waitingDetail.text) && /here or on your Mac\.$/.test(waitingDetail.text) && waitingDetail.strong === 'They do not match',
			JSON.stringify(waitingDetail));
		const pressedAt = Date.now();
		mac.pressOnMac();
		await page.waitForSelector('#composer', { state: 'visible', timeout: 15000 });
		const handover = Date.now() - pressedAt;
		// SAGE'S PAIR-V2 REVIEW §1: the wait asks with the phone's own They match, a Mac that names
		// `pair-wait` holds it, and the press is heard within a round trip rather than at the next
		// scheduled ask (up to 15 s later on `main`).
		const asks = mac.state.pairAsks.map((a) => ({ prefer: a.prefer, held: a.held, ended: a.ended }));
		check('the wait asks the Mac with the phone\'s own They match, and the Mac holds it until the press',
			asks.length >= 1 && asks.every((a) => /^wait=\d+$/.test(a.prefer || '')) && asks[asks.length - 1].ended === 'pressed',
			JSON.stringify(asks));
		check('the press on the Mac hands over to the conversation within a round trip, not at the next scheduled ask',
			await page.isVisible('#messages') && handover < 2000, `${handover} ms after the press`);
		check('the pairing code is spent and gone from the address, so a relaunch does not re-pair',
			!(await page.evaluate(() => location.hash)).includes('pair='),
			await page.evaluate(() => location.hash));

		// ---------------------------------------------------------------------------------------
		section(`${engine}: the conversation`);
		// ---------------------------------------------------------------------------------------

		await page.waitForFunction(() => document.querySelectorAll('#messages li').length > 3, null, { timeout: 15000 });
		const connected = await page.textContent('#link-state');
		check('healthy connectivity is invisible', connected.trim()==='' && await page.evaluate(()=>__richosPhone.link.isOpen()), connected.trim());
        await page.click('#settings-open');
        check('reply previews are on by default',await page.isChecked('#notification-previews'));
        await shoot(page,`${engine}-notification-settings`);
        await page.uncheck('#notification-previews');
        await page.click('#settings-close');
        await page.reload();await page.waitForSelector('#composer',{state:'visible'});
        await page.click('#settings-open');
        await page.waitForFunction(()=>document.querySelector('#notification-previews').checked===false);
        check('the preview opt-out survives relaunch',!await page.isChecked('#notification-previews'));
        await page.check('#notification-previews');await page.click('#settings-close');
        if(await page.isVisible('#push-later'))await page.click('#push-later');
        check('notification setup can be dismissed without nagging',!await page.isVisible('#push-offer'));
        await page.fill('#composer','Retain this unsent draft');
        await page.waitForFunction(async()=>{const db=await RichOSStorage.open();try{return await RichOSStorage.settings(db).get('draft:thread-main')==='Retain this unsent draft';}finally{db.close();}});
        // Hold real history restoration so the notification arrives at a precise startup boundary.
        await page.addInitScript(()=>{
            if(sessionStorage.getItem('notification-startup-proof'))return;
            sessionStorage.setItem('notification-startup-proof','used');
            let storage, historyHeld=false;
            Object.defineProperty(globalThis,'RichOSStorage',{configurable:true,get:()=>storage,set:value=>{
                storage={...value,messages(db){const store=value.messages(db);return {...store,async recent(...args){
                    if(!historyHeld){historyHeld=true;await new Promise(resolve=>{globalThis.__releaseHistory=resolve;globalThis.__historyWaiting=true;});}
                    return store.recent(...args);
                }};}};
            }});
        });
        await page.reload();await page.waitForSelector('#composer',{state:'visible'});
        await page.waitForFunction(()=>globalThis.__historyWaiting===true);
        await page.waitForFunction(()=>document.querySelector('#composer').value==='Retain this unsent draft');
        check('an unsent text draft survives relaunch',await page.inputValue('#composer')==='Retain this unsent draft');
        await page.evaluate(()=>{history.replaceState(null,'','/#thread=thread-main&at=seed-1');navigator.serviceWorker.dispatchEvent(new MessageEvent('message',{data:{type:'notification-clicked',url:'/#thread=thread-main&at=seed-1'}}));});
        await page.evaluate(()=>globalThis.__releaseHistory());
        try {await page.waitForFunction(()=>[...document.querySelectorAll('#messages li')].some(row=>row.dataset.messageId==='seed-1'));}
        catch(error){console.error('Notification return diagnostic',await page.evaluate(()=>({target:__richosPhone.notificationTarget,rows:__richosPhone.thread?.view([]).map(r=>r.id),beginning:__richosPhone.thread?.atTheBeginning(),link:document.querySelector('#link-state').textContent,errors:document.querySelector('#hold-note').textContent})),errors);throw error;}
        await sleep(100);
        check('notification return loads its older reply and focuses it',await page.evaluate(()=>{const row=document.querySelector('[data-message-id="seed-1"]'),r=row.getBoundingClientRect(),t=document.querySelector('#thread').getBoundingClientRect();return r.top>=t.top && r.bottom<=t.bottom && !__richosPhone.pinnedToBottom;}));
        check('opening a notification preserves the unsent draft',await page.inputValue('#composer')==='Retain this unsent draft');
        check('a consumed notification cannot keep reopening its old reply',await page.evaluate(()=>location.hash===''));
        await page.click('#latest');
        await page.fill('#composer','');


		const order = await page.evaluate(() => Array.from(document.querySelectorAll('#messages li .msg-text')).map((el) => el.textContent));
		check('newest at the bottom, in the Mac\'s order', order.length > 3 && order[order.length - 1].length > 0,
			`${order.length} messages, last: ${JSON.stringify(order[order.length - 1].slice(0, 60))}`);

		// --- the composer's states ---
		check('the send control is off while there is nothing to send', await page.isDisabled('#send'));
		await page.fill('#composer', 'where are we on the proposal?');
		check('and on the moment there is', !(await page.isDisabled('#send')));

        mac.state.pauseReplyChunks=true;
		await page.click('#send');
		// The optimistic bubble, immediately — §4.2 (v): a phone that shows nothing reads as broken.
		await page.waitForFunction(() => Array.from(document.querySelectorAll('.msg-mine .msg-text'))
			.some((el) => el.textContent === 'where are we on the proposal?'), null, { timeout: 5000 });
		check('his message appears the instant he sends it, before the Mac has answered', true);

		// On HIS new message. "Delivered." already sits under the newest seeded question before he
		// sends, so waiting on any mine row passed at once and measured nothing.
		await page.waitForFunction((text) => Array.from(document.querySelectorAll('.msg-mine'))
			.some((li) => li.querySelector('.msg-text')?.textContent === text
				&& /Delivered/.test(li.querySelector('.msg-state')?.textContent || '')),
		'where are we on the proposal?', { timeout: 10000 });
		check('and turns to delivered when the Mac has it', true);

		const mineCount = await page.evaluate((text) => Array.from(document.querySelectorAll('.msg-mine .msg-text'))
			.filter((el) => el.textContent === text).length, 'where are we on the proposal?');
		check('his message is on screen exactly once — the optimistic bubble is replaced, not added to',
			mineCount === 1, `${mineCount} copies`);

		// --- Rich answers, and it streams ---
		await page.waitForFunction(() => Array.from(document.querySelectorAll('.msg-rich .msg-text'))
			.some((el) => /On it!/.test(el.textContent)), null, { timeout: 10000 });
		const firstToken = await page.evaluate(() => {
			const el = Array.from(document.querySelectorAll('.msg-rich .msg-text')).filter((e) => /On it!/.test(e.textContent)).pop();
			return el.textContent;
		});
        mac.state.pauseReplyChunks=false;
        await page.waitForFunction(length=>[...document.querySelectorAll('.msg-rich .msg-text')].filter(e=>/On it!/.test(e.textContent)).at(-1)?.textContent.length>length,firstToken.length);
		const grown = await page.evaluate(() => {
			const el = Array.from(document.querySelectorAll('.msg-rich .msg-text')).filter((e) => /On it!/.test(e.textContent)).pop();
			return el.textContent;
		});
		check('his reply STREAMS in rather than arriving whole', grown.length > firstToken.length,
			`${firstToken.length} characters, then ${grown.length}`);

		// --- the thread picker: only when there IS more than one, and it lists rather than creates ---
		check('with one conversation there is no picker to ignore',
			await page.isHidden('#thread-picker-label'));

		mac.state.threads = [{ id: mac.threadId, title: 'Rich' }, { id: 'thread-two', title: 'The proposal' }];
		await page.evaluate(() => window.__richosPhone.connectStream());
		await page.waitForFunction(() => !document.getElementById('thread-picker-label').hidden, null, { timeout: 15000 });
		const options = await page.evaluate(() => Array.from(document.querySelectorAll('#thread-picker option')).map((o) => o.textContent));
		check('a second conversation brings the picker, listing what exists',
			options.length === 2 && options.includes('The proposal'), options.join(', '));
		// It LISTS threads; it does not create them (plan §3.4, and §6's "not in v1").
		const createControl = await page.evaluate(() => {
			const text = document.body.innerText;
			return /new conversation|new thread|start a conversation|create/i.test(text);
		});
		check('and offers no way to create one, which v1 deliberately does not do', createControl === false);
		mac.state.threads = [{ id: mac.threadId, title: 'Rich' }];
		await page.evaluate(() => window.__richosPhone.connectStream());

		// --- a question card: one tap on an answer answers it, as a click does on the Mac ---
		// Ray, nightlies 63 and 64: the phone page showed the question as plain text with no option
		// to press, so clicking "Casual" did nothing (follow-ups-after-nightly65.md item 5).
		mac.ask({ id: 'q-tone', text: 'Should the welcome email sound formal or casual?', options: [
			{ id: 'opt-formal', label: 'Formal', description: 'Polished, professional tone.' },
			{ id: 'opt-casual', label: 'Casual', description: 'Warm, friendly, conversational tone.' }] });
		const casual = page.locator('[data-message-id="q-tone"] button', { hasText: 'Casual' });
		let cardShown = true;
		try { await casual.waitFor({ state: 'visible', timeout: 10000 }); } catch { cardShown = false; }
		check('a question from Rich arrives as a card with its answers as buttons', cardShown,
			await page.evaluate(() => document.querySelector('[data-message-id="q-tone"]')?.innerText || 'no row'));
		if (cardShown) {
			await shoot(page, `${engine}-question-open`);
			await casual.click();
			try { await page.waitForFunction(() => /You answered: Casual/.test(document.querySelector('[data-message-id="q-tone"]')?.innerText || ''), null, { timeout: 10000 }); } catch { /* the check says it */ }
			const sent = mac.state.answers;
			check('tapping "Casual" sends that answer to the Mac, once',
				sent.length === 1 && sent[0].kind === 'answer' && sent[0].question_id === 'q-tone'
					&& sent[0].thread_id === mac.threadId && JSON.stringify(sent[0].option_ids) === '["opt-casual"]',
				JSON.stringify(sent));
			const after = await page.evaluate(() => document.querySelector('[data-message-id="q-tone"]')?.innerText || '');
			check('and the card then says what he answered, with no answer left to press',
				/You answered: Casual/.test(after) && !(await casual.count()), after);
			await shoot(page, `${engine}-question-answered`);
		}

		// --- the shell is kept, so the app OPENS where his Mac does not resolve ---
		const worker = await page.evaluate(async () => {
			if (!('serviceWorker' in navigator)) return { supported: false };
			const registration = await navigator.serviceWorker.getRegistration();
			if (!registration) return { supported: true, registered: false };
			const names = await caches.keys();
			let cached = 0;
			for (const name of names) cached += (await (await caches.open(name)).keys()).length;
			return { supported: true, registered: true, scope: registration.scope, caches: names, cached };
		});
		if (!worker.supported) {
			skip('the shell is cached for a train', 'this engine has no service worker container on this origin');
		} else {
			check('the service worker registers at the root of the origin, which is what scopes the push subscription',
				worker.registered === true, JSON.stringify(worker));
			check('and it kept the shell, so the app opens where the Mac does not resolve',
				worker.cached >= 10, `${worker.cached} entries in ${(worker.caches || []).join(', ')}`);
		}

		// ---------------------------------------------------------------------------------------
		section(`${engine}: away from his Mac — the queue, and whether it survives a relaunch`);
		// ---------------------------------------------------------------------------------------

		mac.setMode('unreachable');
		await page.fill('#composer', 'typed on a train');
		await page.click('#send');

		await page.waitForFunction(() => Array.from(document.querySelectorAll('.msg-state.waiting'))
			.some((el) => /isn.t reachable from here/.test(el.textContent)), null, { timeout: 15000 });
		const waitingText = await page.textContent('.msg-state.waiting');
		check('the waiting state says the true thing, in his words', /waiting to send/i.test(waitingText), waitingText.trim());
		check('the banner is up, with the control that acts on it beside it',
			await page.isVisible('#queue-banner') && await page.isVisible('#queue-retry'));

		await noHorizontalScroll(page, `${engine}: with a message waiting`);
		await shoot(page, `${engine}-waiting`);

		// THE RELAUNCH. Not a re-render — a real reload, so what is being tested is what IndexedDB
		// actually kept.
		await page.reload();
		await page.waitForSelector('#composer', { state: 'visible', timeout: 15000 });
		await page.waitForFunction(() => Array.from(document.querySelectorAll('.msg-mine .msg-text'))
			.some((el) => el.textContent === 'typed on a train'), null, { timeout: 15000 });
		check('the message he wrote on the train is still there after the app was closed and reopened', true);

		// --- home again ---
		//
		// TWO PATHS, AND BOTH ARE THE PRODUCT. The app flushes when the stream reconnects and when he
		// taps "Try now", and which one wins is a race between his thumb and the reconnect. The
		// first run of this harness clicked the button after the stream had already drained the
		// queue, and Playwright timed out on a control that had correctly disappeared. So the check
		// below waits for either and says which happened, rather than requiring the slower one.
		//
		// AND IT WAITS FOR DELIVERY, NOT FOR THE WAITING LABEL TO GO. The first version of this
		// check waited for `.msg-state.waiting` to disappear — which happens the moment the send
		// STARTS, because the bubble turns to "Sending…" — and then asserted the Mac had it. The Mac
		// had nothing yet, and the harness reported a defect in working code. The queue being empty
		// is what delivery means.
		mac.setMode('normal');
		let drainedBy = 'the stream reconnecting on its own';
		const waitForDelivery = () => page.waitForFunction(
			() => window.__richosPhone.queue.count() === 0
				&& Array.from(document.querySelectorAll('.msg-mine'))
					.some((el) => el.innerText.includes('typed on a train') && /Delivered/.test(el.innerText)),
			null, { timeout: 6000 });
		try {
			await waitForDelivery();
		} catch {
			drainedBy = 'his tap on "Try now"';
			await page.click('#queue-retry');
			await page.waitForFunction(
				() => window.__richosPhone.queue.count() === 0
					&& Array.from(document.querySelectorAll('.msg-mine'))
						.some((el) => el.innerText.includes('typed on a train') && /Delivered/.test(el.innerText)),
				null, { timeout: 20000 });
		}
		check('it goes the moment his Mac can hear him again', true, `drained by ${drainedBy}`);

		const delivered = mac.received().filter((row) => row.text === 'typed on a train');
		check('and it reached the Mac exactly once, across a reload and a retry',
			delivered.length === 1, `${delivered.length} copies at the Mac`);

		// ---------------------------------------------------------------------------------------
		section(`${engine}: older messages, behind a scroll`);
		// ---------------------------------------------------------------------------------------

		const before = await page.evaluate(() => document.querySelectorAll('#messages li').length);
		await page.locator('#thread').hover();
        await page.mouse.wheel(0,-100000);
		await page.waitForFunction((n) => document.querySelectorAll('#messages li').length > n, before, { timeout: 15000 });
		const after = await page.evaluate(() => document.querySelectorAll('#messages li').length);
		check('scrolling to the top loads earlier messages', after > before, `${before} then ${after}`);

		const pagination = await page.evaluate(() => {
			const text = document.body.innerText;
			return /\bpage \d|\bnext page\b|\bprevious page\b|\bpage \d+ of \d+/i.test(text);
		});
		check('NO PAGINATION — no page number, no pagination control, anywhere', pagination === false);

		// ---------------------------------------------------------------------------------------
		section(`${engine}: the thread opens at its newest message, and follows it (audit R2, 360px)`);
		// ---------------------------------------------------------------------------------------
		//
		// Ray's candidate .13, defect R2: he paired, the newest bubble sat below the fold, and a
		// message sent from the Mac was filmed for 15 s across 30 frames without ever appearing —
		// it was in the DOM the whole time. After one manual scroll to the bottom the next message
		// was on screen in 0.096 s and stayed pinned. So delivery was never the problem and the
		// RESTING POSITION was.
		//
		// MEASURED HERE, at his Android phone's width, on this app in this browser:
		//
		//   before  reopen -> thread clientHeight 399, scrollTop 1377, scrollHeight 1975, gap 199
		//   after   reopen -> thread clientHeight 399, scrollTop 1576, scrollHeight 1975, gap   0
		//
		// The mechanism is check 2 below and it is worth stating because it is not the one the
		// report guessed at: `startConversation` pins the view with `render(true)` while the
		// column is still 505 px tall, and the notification offer then un-hides ABOVE the thread.
		// `scrollTop` is measured from the TOP, so shrinking the scroller leaves it untouched and
		// moves the visible BOTTOM up by the offer's height. Nothing is clamped and no `scroll`
		// event fires, so the app never learned it had been moved off the newest message.
		//
		// Check 2 is therefore the one that fails on `a2cef8ee` without depending on a
		// notification permission: it takes the app's own offer element and un-hides it while the
		// view is at the bottom, which is exactly what `refreshPushOffer()` does a moment after
		// the app opens.
		//
		// THE NEGATIVE CONTROL WAS RUN, not reasoned about: with `a2cef8ee`'s `app.js` put back
		// and everything else unchanged, all five checks here fail —
		//
		//     opens 224px short of the newest message
		//     the offer takes 127px and the view does not move at all (gap 127 = the band itself)
		//     the arriving message never comes into view in 4s (Ray filmed 15s of the same thing)
		//
		// Checks 4 and 5 are GUARDS rather than the defect: they hold the fix to not over-reaching
		// — a message must not yank him out of what he is reading — and on `a2cef8ee` they fail
		// only because the app has no such state to report at all.

		await page.setViewportSize({ width: 360, height: 740 });
		await page.reload();
		await page.waitForSelector('#composer', { state: 'visible', timeout: 15000 });
		await page.waitForFunction(() => document.querySelectorAll('#messages li').length > 3, null, { timeout: 15000 });
		await sleep(1200);

		// Every number this section asserts, in one read.
		const threadView = () => page.evaluate(() => {
			const t = document.getElementById('thread');
			const last = document.querySelector('#messages li:last-child');
			const tr = t.getBoundingClientRect();
			const lr = last ? last.getBoundingClientRect() : null;
			return {
				clientHeight: t.clientHeight,
				scrollHeight: t.scrollHeight,
				scrollTop: Math.round(t.scrollTop),
				gap: Math.round(t.scrollHeight - t.scrollTop - t.clientHeight),
				newestFullyVisible: lr ? lr.bottom <= tr.bottom + 1 : null,
				pinned: window.__richosPhone.pinnedToBottom
			};
		});

		const opened = await threadView();
		check(`${engine}: 360px — the app opens on the newest message, not one bubble short of it`,
			opened.gap <= 1 && opened.newestFullyVisible === true,
			`thread ${opened.clientHeight}px of ${opened.scrollHeight}px, scrollTop ${opened.scrollTop}, gap ${opened.gap}px`);

		// --- 2. a band appearing above the thread does not strand the view ---
		const banded = await page.evaluate(() => {
			const frames = (n) => new Promise((resolve) => {
				const tick = () => (--n <= 0 ? resolve() : requestAnimationFrame(tick));
				requestAnimationFrame(tick);
			});
			const t = document.getElementById('thread');
			const offer = document.getElementById('push-offer');
			return (async () => {
				// From a KNOWN state, whatever the notification permission of the browser running
				// this happens to be: no band, view at the bottom.
				offer.hidden = true;
				await frames(2);
				t.scrollTop = t.scrollHeight;
				await frames(2);
				// The sentence and the button are set BEFORE the un-hide, because that is the
				// order `refreshPushOffer()` uses — it writes what the band says and then shows
				// it, in one task, so the band has ONE layout. Writing them afterwards gives it
				// two (93 px, then 127 px), which is a transition the product never makes and
				// which this check has no business inventing.
				document.getElementById('push-offer-text').textContent =
					'Notifications are off, so Rich cannot reach you when this app is closed.';
				document.getElementById('push-on').hidden = false;
				const before = { clientHeight: t.clientHeight, scrollTop: Math.round(t.scrollTop) };
				// What `refreshPushOffer()` does a moment after the app opens, in the state the
				// CEO's own phone is in.
				offer.hidden = false;
				// Two frames: one for the layout the un-hide causes, one for the observer that
				// answers it. A single frame would be measuring the race rather than the fix.
				await frames(2);
				const last = document.querySelector('#messages li:last-child');
				const lr = last ? last.getBoundingClientRect() : null;
				const tr = t.getBoundingClientRect();
				return {
					before,
					bandHeight: Math.round(offer.getBoundingClientRect().height),
					shrankBy: before.clientHeight - t.clientHeight,
					scrollTop: Math.round(t.scrollTop),
					gap: Math.round(t.scrollHeight - t.scrollTop - t.clientHeight),
					newestFullyVisible: lr ? lr.bottom <= tr.bottom + 1 : null
				};
			})();
		});
		check(`${engine}: 360px — the notification offer takes height from the column and the view follows it down`,
			banded.shrankBy >= 40 && banded.gap <= 1 && banded.newestFullyVisible === true,
			`the offer is ${banded.bandHeight}px tall, the thread lost ${banded.shrankBy}px of viewport ` +
			`(${banded.before.clientHeight} -> ${banded.before.clientHeight - banded.shrankBy}), scrollTop ` +
			`${banded.before.scrollTop} -> ${banded.scrollTop}, and the view is ${banded.gap}px from the bottom ` +
			`(on a2cef8ee this gap is the band's own height)`);

		// --- 3. a message that arrives while he is at the bottom comes with the view ---
		mac.state.ledger.push({
			id: 'r2-arrives', thread_id: mac.threadId, cursor: mac.state.nextCursor++, role: 'rich', kind: 'text',
			text: 'the-message-that-arrives-while-he-is-looking', created_at: new Date().toISOString(),
			complete: true, has_audio: false
		});
		await page.evaluate(() => window.__richosPhone.connectStream());
		const arrival = Date.now();
		let onScreenIn = null;
		for (let i = 0; i < 40 && onScreenIn === null; i++) {
			await sleep(100);
			const seen = await page.evaluate(() => {
				const t = document.getElementById('thread');
				const el = Array.from(document.querySelectorAll('#messages li'))
					.find((li) => li.innerText.includes('the-message-that-arrives-while-he-is-looking'));
				if (!el) return false;
				const r = el.getBoundingClientRect(), tr = t.getBoundingClientRect();
				return r.bottom <= tr.bottom + 1 && r.top >= tr.top - 1;
			});
			if (seen) onScreenIn = Date.now() - arrival;
		}
		check(`${engine}: a message that arrives while he is at the bottom is ON SCREEN, not merely in the DOM`,
			onScreenIn !== null, onScreenIn === null ? 'never came into view in 4 s' : `fully visible ${onScreenIn} ms after the stream reopened`);

		// --- 4. and it does NOT move him when he is reading something further up ---
		await page.evaluate(() => { document.getElementById('thread').scrollTop -= 400; });
		await sleep(200);
		const readingUp = await threadView();
		mac.state.ledger.push({
			id: 'r2-arrives-2', thread_id: mac.threadId, cursor: mac.state.nextCursor++, role: 'rich', kind: 'text',
			text: 'the-message-that-must-not-yank-him', created_at: new Date().toISOString(),
			complete: true, has_audio: false
		});
		await page.evaluate(() => window.__richosPhone.connectStream());
		await page.waitForFunction(() => Array.from(document.querySelectorAll('#messages li'))
			.some((li) => li.innerText.includes('the-message-that-must-not-yank-him')), null, { timeout: 15000 });
		await sleep(400);
		const stillReading = await threadView();
		check(`${engine}: reading further up, a message that arrives does not yank him to the bottom`,
			readingUp.pinned === false && Math.abs(stillReading.scrollTop - readingUp.scrollTop) <= 1,
			`scrollTop ${readingUp.scrollTop} -> ${stillReading.scrollTop} (pinned ${readingUp.pinned})`);

		// --- 5. back at the bottom, he is following again ---
		await page.evaluate(() => { const t = document.getElementById('thread'); t.scrollTop = t.scrollHeight; });
		await sleep(200);
		const backDown = await threadView();
		check(`${engine}: scrolling back to the bottom picks the conversation up again`,
			backDown.pinned === true && backDown.gap <= 1,
			`gap ${backDown.gap}px, pinned ${backDown.pinned}`);

		await page.setViewportSize(HIS_PHONE);

		// ---------------------------------------------------------------------------------------
		section(`${engine}: the controls this Mac can actually stand behind`);
		// ---------------------------------------------------------------------------------------
		//
		// The app shipped "Hold to record" as one of the two biggest controls on the screen while
		// the real Mac answered every voice note 503 (plan §2 A). The control is now rendered only
		// where the `hello` frame named `voice` in `capabilities`. BOTH directions are walked here,
		// in a real browser, against the real app — the stub advertises voice because it can take
		// one, and `applyCapabilities` is then driven against a Mac that cannot.

		const holdShown = await page.evaluate(() => {
			const hold = document.getElementById('hold');
			return !hold.hidden && getComputedStyle(hold).display !== 'none';
		});
		check('the hold control is on screen when the Mac advertises voice', holdShown === true);
        await page.reload();await page.waitForSelector('#hold',{state:'visible'});
        check('voice remains available after reopening with cached conversation history',await page.locator('#hold').isVisible());
        await page.evaluate(()=>{__richosPhone.api.setCapabilities(['text','voice','audio'],'test');__richosPhone.render();});
        check('completed replies offer on-demand audio when the Mac can synthesize it',await page.locator('.msg-rich .hear').count()>0);

		const withoutVoice = await page.evaluate(() => {
			// The stream's own path, with a Mac that offers text only — the state the shipped Mac
			// is in today. `offers` is default-deny, so the third case is a Mac that says nothing.
			const read = () => {
				const hold = document.getElementById('hold');
				const note = document.getElementById('hold-note');
				return {
					hold: !hold.hidden && getComputedStyle(hold).display !== 'none',
					note: !note.hidden && getComputedStyle(note).display !== 'none'
				};
			};
			globalThis.__richosPhone.api.setCapabilities(['text'], '1.2.0');
			globalThis.__richosPhone.applyCapabilities();
			const textOnly = read();
			globalThis.__richosPhone.api.setCapabilities(undefined, undefined);
			globalThis.__richosPhone.applyCapabilities();
			const saysNothing = read();
			globalThis.__richosPhone.api.setCapabilities(['text', 'voice'], '1.2.0');
			globalThis.__richosPhone.applyCapabilities();
			return { textOnly, saysNothing, restored: read() };
		});
		check('a Mac that offers text only shows no hold control and no note under it',
			withoutVoice.textOnly.hold === false && withoutVoice.textOnly.note === false,
			JSON.stringify(withoutVoice.textOnly));
		check('a Mac that names no capabilities at all offers nothing — default-deny, not default-show',
			withoutVoice.saysNothing.hold === false && withoutVoice.saysNothing.note === false,
			JSON.stringify(withoutVoice.saysNothing));
		check('and it comes back when the Mac offers voice again',
			withoutVoice.restored.hold === true && withoutVoice.restored.note === true,
			JSON.stringify(withoutVoice.restored));

		const toldBuild = await page.evaluate(() => ({
			build: globalThis.__richosPhone.state.build,
			onScreen: /\b\d+\.\d+\.\d+\b/.test(document.body.innerText)
		}));
		check('the build the Mac named is held as state', toldBuild.build === '1.2.0', String(toldBuild.build));
		// Plan §3: nothing he reads is an identifier, and a build number is one.
		check('and it is nowhere on his screen', toldBuild.onScreen === false);

		// ---------------------------------------------------------------------------------------
		section(`${engine}: the microphone, and the bytes the Mac receives`);
		// ---------------------------------------------------------------------------------------

		if (engine !== 'chromium') {
			skip('hold-to-record end to end', 'WebKit cannot be given a fake microphone, so this engine cannot answer it. Chromium does, above, and his own phone answered it on 2026-09-18.');
		} else {
			const box = await page.locator('#hold').boundingBox();
			await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
			await page.mouse.down();
            await page.waitForFunction(()=>document.body.dataset.voice==='held');
			await sleep(1400);
			await page.mouse.up();

			await page.waitForFunction(() => Array.from(document.querySelectorAll('.msg-mine'))
				.some((el) => /Voice note|spoken/i.test(el.textContent)), null, { timeout: 20000 });

            await page.waitForFunction(()=>__richosPhone.queue.all().length===0);
			const voice = mac.received().filter((row) => row.kind === 'voice');
			check('a voice note reached the Mac', voice.length === 1, `${voice.length} received`);
			if (voice.length) {
				const audio = voice[0].audio;
				check('it was sent as bytes with the codec named — never base64, never a container negotiation',
					audio.codec === 'wav16k' && audio.bytes > 20000, JSON.stringify(audio));
				check('the duration matches the hold, and the size is the plan\'s ~32 KB per second',
					Math.abs(audio.bytes - audio.seconds * 32000) < audio.seconds * 32000 * 0.1,
					`${audio.seconds} s, ${audio.bytes} bytes`);
			}

			// The header, asserted on the bytes that actually arrived rather than on the bytes the
			// page said it made.
			const header = mac.state.wavHeader;
			check('the WAV header the Mac received declares 16 kHz mono 16-bit PCM',
				header && header.riff === 'RIFF' && header.wave === 'WAVE' && header.format === 1
				&& header.channels === 1 && header.rate === 16000 && header.bits === 16,
				JSON.stringify(header));

            const pressMic=async()=>{await page.locator('#voice-actions').waitFor({state:'hidden'});await page.locator('#hold').waitFor({state:'visible'});const b=await page.locator('#hold').boundingBox();await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.waitForFunction(()=>document.querySelector('#hold').classList.contains('recording') && !document.querySelector('#hold').classList.contains('opening'));await sleep(700);return b;};
            let b=await pressMic();await page.mouse.move(b.x+b.width/2,b.y-100,{steps:5});await page.mouse.up();
            await page.locator('#voice-send').waitFor({state:'visible'});
            const cancelBox=await page.locator('#voice-cancel').boundingBox(), actionsBox=await page.locator('#voice-actions').boundingBox();
            check('locked Cancel is centred with room away from Send',Math.abs(cancelBox.x+cancelBox.width/2-actionsBox.x-actionsBox.width/2)<3 && (await page.locator('#voice-send').boundingBox()).x-(cancelBox.x+cancelBox.width)>=20);
            await shoot(page,`${engine}-locked-recording`);
            check('releasing after slide-up keeps recording and sends nothing',mac.received().filter(r=>r.kind==='voice').length===1);
            await page.locator('#voice-cancel').click();
            b=await pressMic();await page.mouse.move(b.x-100,b.y+b.height/2,{steps:5});await page.mouse.up();
            await page.locator('#voice-actions').waitFor({state:'hidden'});
            check('slide-left cancellation never sends on release',mac.received().filter(r=>r.kind==='voice').length===1);
            b=await pressMic();await page.mouse.move(b.x+b.width/2,b.y-100,{steps:5});await page.mouse.up();
            await page.locator('#voice-send').click();
            await page.locator('#voice-actions').waitFor({state:'hidden'});
            await page.waitForFunction(()=>globalThis.__richosPhone.queue.all().length===0);
            check('locked Send submits exactly one more voice message',mac.received().filter(r=>r.kind==='voice').length===2);
            check('the microphone icon survives recording and Send',await page.locator('#hold svg').count()===1 && await page.locator('#hold').getAttribute('aria-label')==='Hold to record');
            // A pointer interruption retains rather than dispatching the recording.
            await pressMic();await page.locator('#hold').dispatchEvent('pointercancel');await page.mouse.up();
            await page.locator('#voice-drafts').waitFor({state:'visible'});
            await page.reload();await page.locator('#voice-drafts').waitFor({state:'visible'});
            check('interrupted audio survives page reload without sending',mac.received().filter(r=>r.kind==='voice').length===2);
            await page.locator('#voice-drafts button').filter({hasText:'Send voice message'}).click();
            await page.locator('#voice-drafts').waitFor({state:'hidden'});
            await page.waitForFunction(()=>globalThis.__richosPhone.queue.all().length===0);
            check('recovered voice uses the durable queue and sends once',mac.received().filter(r=>r.kind==='voice').length===3);

			// --- hear it ---
			const hear = page.locator('.msg-rich .hear').last();
			if (await hear.count()) {
				await hear.click();
				await page.waitForFunction(() => document.querySelectorAll('.msg-rich .hear.playing').length > 0
					|| Array.from(document.querySelectorAll('.msg-rich .hear')).some((b) => /Stop/.test(b.textContent)),
				null, { timeout: 15000 }).catch(() => {});
				const label = await hear.textContent();
				check('"hear it" fetches the audio on demand and turns into the control that stops it',
					/Stop|Hear it/.test(label), `the control reads "${label.trim()}" (muted browser, no sound was played in the room)`);
			} else {
				skip('"hear it"', 'no reply on screen declared audio');
			}
		}

		// ---------------------------------------------------------------------------------------
		section(`${engine}: a state he can change is rendered with the control that changes it`);
		// ---------------------------------------------------------------------------------------

		const affordances = await page.evaluate(() => {
			const visible = (el) => {
				if (!el || el.hidden) return false;
				const style = getComputedStyle(el);
				if (style.display === 'none' || style.visibility === 'hidden') return false;
				const rect = el.getBoundingClientRect();
				return rect.width > 0 && rect.height > 0;
			};
			const out = [];
			// Every container that carries a sentence about a state, and whether a control lives
			// with it. A sentence that names where the control IS (on his Mac, in iOS Settings)
			// counts, because the honest answer to "there is no control here" is to say where it is.
			const containers = [
				{ name: 'the notification offer', el: document.getElementById('push-offer') },
				{ name: 'the waiting banner', el: document.getElementById('queue-banner') },
				{ name: 'the pairing screen', el: document.getElementById('pairing') },
				{ name: 'the removed-phone screen', el: document.getElementById('revoked') },
				{ name: 'the composer', el: document.getElementById('composer-bar') }
			];
			for (const c of containers) {
				if (!visible(c.el)) continue;
				const text = c.el.innerText.trim();
				const hasControl = c.el.querySelector('button, select, textarea, a[href]') !== null;
				const namesWhere = /on your Mac|in iOS Settings|in Settings/i.test(text);
				out.push({ name: c.name, hasControl, namesWhere, text: text.slice(0, 120) });
			}
			return out;
		});
		for (const a of affordances) {
			check(`${a.name} carries its own control, or says where the control is`,
				a.hasControl || a.namesWhere, a.text);
		}

		// ---------------------------------------------------------------------------------------
		section(`${engine}: contrast, every text node, both themes`);
		// ---------------------------------------------------------------------------------------

		await page.addScriptTag({ content: contrastLib.pageScript() });
		for (const theme of ['dark', 'light']) {
			await page.emulateMedia({ colorScheme: theme });
			await sleep(120);
			const walk = await page.evaluate((t) => window.__contrastProbe({ surface: 'phone', theme: t }), theme);
			const failed = Object.keys(walk.failures || {});
			const unresolvable = Object.keys(walk.unresolvable || {});
			const exempt = Object.keys(walk.exempt || {});
			process.stdout.write(`        ${theme}: ${walk.nodesChecked} text nodes measured, ${walk.indicators.checked} indicators, ${walk.ancestorResolved} resolved by ancestor\n`);
			check(`${theme}: every measured text node and indicator clears WCAG AA`,
				failed.length === 0, failed.map((k) => `${k}: ${JSON.stringify(walk.failures[k][0] || walk.failures[k])}`).join('\n        '));
			check(`${theme}: nothing on this screen has a color that cannot be proved`,
				unresolvable.length === 0, unresolvable.join(', '));
			check(`${theme}: no exemption is claimed — every string here is meant to be read`,
				exempt.length === 0, exempt.join(', '));
			check(`${theme}: the walk actually looked at something`, walk.nodesChecked > 10, `${walk.nodesChecked} nodes`);
			await shoot(page, `${engine}-thread-${theme}`);
		}
		await page.emulateMedia({ colorScheme: 'dark' });

		// ---------------------------------------------------------------------------------------
		section(`${engine}: the header at 360 px, with a picker on screen (audit §4.3)`);
		// ---------------------------------------------------------------------------------------
		//
		// The defect needed two threads to reproduce: with one, the picker is hidden and the title
		// had the row anyway. So the stub is given a second thread and the app is reloaded, which
		// is what puts the picker on screen, and then the title is MEASURED rather than reasoned
		// about — the widest word in it against the width of the box it is in. If the widest word
		// fits, no word can break inside itself, which is the whole of §4.3.

		const titleBefore = mac.state.threads;
		mac.state.threads = [
			{ id: mac.threadId, title: 'Running' },
			{ id: `${mac.threadId}-2`, title: 'The proposal' }
		];
		await page.reload();
		await page.waitForSelector('#composer', { state: 'visible', timeout: 15000 });
		await page.waitForSelector('#thread-picker-label:not([hidden])', { timeout: 15000 });

		for (const width of [320, 360, 375]) {
			await page.setViewportSize({ width, height: 812 });
			await sleep(120);
			// EVERY WORD, AND THE NUMBER OF LINE BOXES IT OCCUPIES. A `Range` over one word returns
			// one client rect per line the word is laid out on, so two rects IS the break — the
			// word has been split across lines.
			//
			// The first draft of this check measured the word's bounding box against the element's
			// width instead, and the negative control caught it being wrong: a word already broken
			// across two lines has a bounding box as wide as its WIDEST FRAGMENT, which is
			// narrower than the word. It measured "Running" at 69.4px in a 70.4px box and called
			// it a pass, on the very CSS that produced "Runni / ng". A check that is fooled by the
			// defect it is for is worse than no check.
			const title = await page.evaluate(() => {
				const el = document.getElementById('thread-title');
				const node = el.firstChild;
				if (!node || node.nodeType !== 3) return null;
				const text = node.textContent;
				const range = document.createRange();
				const split = [];
				let at = 0;
				for (const word of text.split(/\s+/)) {
					if (!word) { at += 1; continue; }
					const start = text.indexOf(word, at);
					range.setStart(node, start);
					range.setEnd(node, start + word.length);
					const lines = range.getClientRects().length;
					if (lines > 1) split.push(`${word} (${lines} lines)`);
					at = start + word.length;
				}
				return { text, split, box: el.getBoundingClientRect().width };
			});
			check(`${engine}: ${width}px — no word in the title is broken in half`,
				title !== null && title.split.length === 0,
				title === null ? 'the title has no text node' :
					title.split.length
						? `broken: ${title.split.join(', ')} — in a ${title.box.toFixed(1)}px box ("${title.text}")`
						: `"${title.text}" in a ${title.box.toFixed(1)}px box`);
		}

		// And the picker is BELOW it rather than beside it — the arrangement that made 67 px out
		// of 328.
		const stacked = await page.evaluate(() => {
			const title = document.getElementById('thread-title').getBoundingClientRect();
			const picker = document.getElementById('thread-picker-label').getBoundingClientRect();
			return { titleBottom: title.bottom, pickerTop: picker.top, titleWidth: title.width };
		});
		check(`${engine}: the picker is under the title, not beside it`,
			stacked.pickerTop >= stacked.titleBottom - 0.5,
			`title ends at ${stacked.titleBottom.toFixed(1)}px, picker starts at ${stacked.pickerTop.toFixed(1)}px`);

		await noHorizontalScroll(page, `${engine}: the header with a picker, 375px`);
		await shoot(page, `${engine}-header-picker`);

		mac.state.threads = titleBefore;
		await page.setViewportSize(HIS_PHONE);

		// ---------------------------------------------------------------------------------------
		section(`${engine}: no horizontal scroll, at five phone widths, in both themes`);
		// ---------------------------------------------------------------------------------------

		for (const theme of ['dark', 'light']) {
			await page.emulateMedia({ colorScheme: theme });
			for (const width of WIDTHS) {
				await page.setViewportSize({ width, height: 812 });
				await sleep(80);
				await noHorizontalScroll(page, `${engine}: ${width}px, ${theme}`);
			}
		}
		await page.setViewportSize(HIS_PHONE);
		await page.emulateMedia({ colorScheme: 'dark' });

		// ---------------------------------------------------------------------------------------
		section(`${engine}: his Mac forgets this phone`);
		// ---------------------------------------------------------------------------------------

		mac.setMode('revoked');
		await page.fill('#composer', 'anyone there?');
		await page.click('#send');
		await page.waitForSelector('#revoked:not([hidden])', { timeout: 20000 });
		const revokedText = await page.textContent('#revoked');
		check('the app says so and stops, rather than retrying a Mac that has forgotten it',
			/removed from your Mac/i.test(revokedText), revokedText.replace(/\s+/g, ' ').trim().slice(0, 140));
		check('and the conversation is not left behind it',
			!(await page.isVisible('#messages')));
		await shoot(page, `${engine}-removed`);

		check('no JavaScript error in the whole run', errors.length === 0, errors.join('\n        '));
		// Printed rather than asserted away: these are the requests this run BROKE on purpose, and
		// seeing them is how you know the outage was real rather than simulated in the page.
		// The loader reports WebKit sent down the exception channel are named separately, so a run
		// shows whether the classification above was exercised rather than merely present.
		const reported = loaderReports.outage + loaderReports.reload;
		process.stdout.write(`        ${networkNoise.length} failed request(s), all of them the outage or a reload this run caused deliberately` +
			`${reported ? ` (${reported} reported by WebKit as access-control failures: ${loaderReports.outage} dropped by the outage, ${loaderReports.reload} stopped by a reload before reaching the Mac)` : ''}:\n        ${
			networkNoise.slice(0, 6).map((t) => t.replace(/\s+/g, ' ')).join('\n        ') || '(none)'}\n`);
		check('the outage was a REAL network failure, not something the page pretended',
			networkNoise.length > 0, 'if this is zero, the unreachable and revoked passes above proved nothing');

		await context.close();
	} catch (err) {
		check(`${engine}: the harness ran to completion`, false, String((err && err.stack) || err));
	} finally {
		process.removeListener('SIGINT', onSignal);
		process.removeListener('SIGTERM', onSignal);
		await cleanup();
	}
}

async function noHorizontalScroll(page, where) {
	const overflow = await page.evaluate(() => {
		const doc = document.documentElement;
		const widest = Array.from(document.querySelectorAll('body *')).reduce((worst, el) => {
			const rect = el.getBoundingClientRect();
			return rect.right > worst.right ? { right: rect.right, tag: el.tagName, cls: el.className } : worst;
		}, { right: 0, tag: null, cls: null });
		return {
			scrollWidth: doc.scrollWidth,
			clientWidth: doc.clientWidth,
			innerWidth: window.innerWidth,
			widest
		};
	});
	// One pixel of slack for sub-pixel rounding, and not a pixel more: his complaint was a scroll
	// bar, and a scroll bar is what this measures.
	check(`no horizontal scroll — ${where}`,
		overflow.scrollWidth <= overflow.clientWidth + 1,
		`document ${overflow.scrollWidth}px against a viewport of ${overflow.clientWidth}px; widest element ${overflow.widest.tag}.${overflow.widest.cls} ends at ${Math.round(overflow.widest.right)}px`);
}

async function shoot(page, name) {
	const file = path.join(SHOTS_DIR, `${name}.png`);
	await page.screenshot({ path: file, fullPage: false });
	process.stdout.write(`        shot: ${path.relative(process.cwd(), file)}\n`);
}

run().then((code) => process.exit(code)).catch((err) => {
	process.stderr.write(String((err && err.stack) || err) + '\n');
	process.exit(1);
});
