'use strict';

// AN OLDER-MESSAGES LOAD OR A QUESTION FETCH STAYS IN THE CONVERSATION THAT ASKED FOR IT. `npm test`.
//
// Review `rv-20261019T081330Z-785564c3-187f`, finding 1: scroll up for older Onboarding messages
// (`loadOlder`), or let a settled reply ask the Mac for the rows before it (`askForTheQuestion`),
// then open another conversation while that request is still on its way. The answer was merged
// into the one shared thread model, which `selectThread` had by then reset for the other
// conversation, so Onboarding's rows were drawn under it.
//
// Built from the reviewer's fixture (`fixtures/backfill-switch.cjs`): the shipped `makeLink`,
// `renderThreadPicker`, `selectThread`, `askForTheQuestion` and `loadOlder` are cut out of
// `app.js` and run in a `node:vm` context with the real `lib/link.js` and `lib/thread.js`, and a
// backfill answer held back until the test releases it. No browser, app, phone or network.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const { createThread } = require(path.join(ROOT, 'lib/thread.js'));
const { createLink } = require(path.join(ROOT, 'lib/link.js'));
const source = fs.readFileSync(path.join(ROOT, 'app.js'), 'utf8');
const fn = (name) => source.match(new RegExp('(?:async )?function ' + name + '\\([^]*?\\n\\}'))[0];
const RENDERING = '\n// ---------------------------------------------------------------------------------------------\n// Rendering';
const linkSource = source.slice(source.indexOf('function makeLink() {'), source.indexOf(RENDERING));

const row = (id, thread_id, cursor, role = 'ceo') => ({ id, thread_id, cursor, role, kind: 'text', text: id, complete: true });
const settle = () => new Promise((resolve) => setImmediate(resolve));

async function switchWhilePending(which) {
	let release;
	let asked = null;
	const page = new Promise((resolve) => { release = resolve; });
	const streams = [];
	const stored = new Map();
	const nodes = {};
	const context = vm.createContext({
		RichOSLink: { createLink },
		thread: createThread(), currentThreadId: 'A', threads: [{ id: 'A', title: 'Onboarding' }],
		link: null, linkStateNow: 'opening', notificationTarget: null, vapidPublicKey: null,
		settings: { set: async () => {} }, composer: { value: '' }, voiceGesture: { interrupt: async () => {} },
		messageCache: {
			put: async (rows) => { for (const r of [].concat(rows)) stored.set(r.id, r); },
			recent: async (id) => [...stored.values()].filter((r) => r.thread_id === id)
		},
		api: {
			refreshChallenge: async () => {},
			openEvents: async (id, since, handlers) => { streams.push({ id, since, handlers }); return { close() {} }; },
			backfill: async (id) => { asked = id; return page; }
		},
		$: (id) => nodes[id] ||= { hidden: true, textContent: '', children: [], appendChild(n) { this.children.push(n); } },
		document: { createElement: () => ({}) },
		setLinkState() {}, updateQueueBanner() {}, handleApiError(e) { throw e; }, applyCapabilities() {},
		flushQueue() {}, scheduleRender() {}, render() {}, restoreComposer: async () => {}, renderVoiceDrafts: async () => {},
		following: { capture() {}, restore() {} }, loadingOlder: false,
	});
	vm.runInContext(linkSource + '\n' + ['renderThreadPicker', 'selectThread', 'askForTheQuestion', 'loadOlder'].map(fn).join('\n')
		+ '\nconst reconciled = new Set();', context);
	context.connectStream();
	await settle();
	streams[0].handlers.hello({ threads: [{ id: 'A', title: 'Onboarding' }], messages: [row('a:reply', 'A', 10, 'rich')] });

	// Ordinary infinite scrolling, or the automatic fetch after a reply settles, leaves a request
	// outstanding while he opens another conversation.
	const pending = which === 'loadOlder'
		? context.loadOlder(10)
		: context.askForTheQuestion(row('a:reply', 'A', 10, 'rich'));
	assert.strictEqual(asked, 'A');
	streams[0].handlers.message(row('b:user', 'B', 11));
	await context.selectThread('B');
	await settle();
	streams[1].handlers.hello({ threads: [{ id: 'A', title: 'Onboarding' }, { id: 'B', title: 'Thread B' }], messages: [row('b:user', 'B', 1)] });

	// Onboarding's answer arrives now, with Thread B on screen.
	release({ messages: [row('a:old-user', 'A', 1), Object.assign(row('a:question', 'A', 2, 'rich'),
		{ kind: 'question', question: { id: 'qa', text: 'Only for A', state: 'open', options: [] } })], more: false });
	await pending;
	await settle();
	return { context, stored };
}

for (const which of ['loadOlder', 'askForTheQuestion']) {
	test(`${which}: an answer for the conversation he left is kept for it and never drawn under the one on screen`, async () => {
		const { context, stored } = await switchWhilePending(which);
		assert.strictEqual(context.currentThreadId, 'B');
		const foreign = context.thread.view([]).filter((r) => r.thread_id !== 'B').map((r) => r.id);
		assert.deepStrictEqual(foreign, [], 'Onboarding rows were merged into Thread B');
		assert.deepStrictEqual(context.thread.view([]).map((r) => r.id), ['b:user']);
		// The rows are still the phone's for Onboarding: `selectThread` reads them back when he returns.
		assert.ok(stored.has('a:old-user') && stored.has('a:question'), 'the answer was thrown away instead of kept for Onboarding');
		// And Thread B's beginning is its own, not Onboarding's `more: false`.
		assert.strictEqual(context.thread.atTheBeginning(), false);
	});
}
