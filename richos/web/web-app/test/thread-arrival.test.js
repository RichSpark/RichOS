'use strict';

// A CONVERSATION CREATED AFTER THE PAGE LOADED. `npm test`.
//
// Ray's nightly 66 walk (escalation `esc-20261019T075013Z-233dbe9d`, build `d93fbea8` from richos
// main `b41ba3c14`): the phone page was paired while only Onboarding existed; Thread B was then
// created on the Mac and given a job with a question. The page, still titled Onboarding, drew
// Thread B's question card and Rich's Thread B replies among Onboarding's, and showed no
// conversation picker, so Thread B could not be opened on the phone.
//
// Both halves come from one fact about the wire: the Mac's hub is ONE broadcast for every
// conversation, each frame carrying its `thread_id` (`app/src-tauri/src/phone/rows.rs`
// `event_from_live`; a question row through `phone/mod.rs` `question_changed`), and the thread
// list rides only in a `hello`, which the Mac sends only to a stream that presents no cursor
// (`phone/stream.rs` `replay_after_marked`).
//
// The shipped `makeLink`, `connectStream` and `renderThreadPicker` are cut out of `app.js` and run
// in a `node:vm` context with the real `lib/link.js`, the real `lib/thread.js` model and an
// in-memory DOM. No browser, app, phone or network is started.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const { createThread } = require(path.join(ROOT, 'lib/thread.js'));
const { createLink } = require(path.join(ROOT, 'lib/link.js'));
const source = fs.readFileSync(path.join(ROOT, 'app.js'), 'utf8');
const RENDERING = '\n// ---------------------------------------------------------------------------------------------\n// Rendering';
const linkSource = source.slice(source.indexOf('function makeLink() {'), source.indexOf(RENDERING));
const pickerSource = source.match(/function renderThreadPicker\(\) \{[\s\S]*?\n\}/)[0];

function settle() { return new Promise((resolve) => setImmediate(resolve)); }

function element() {
	const node = { hidden: true, children: [], text: '' };
	Object.defineProperty(node, 'textContent', {
		get() { return node.text; },
		set(value) { node.text = value; node.children = []; }
	});
	node.appendChild = (child) => { node.children.push(child); return child; };
	return node;
}

const ONBOARDING = 'thr_onboarding';
const THREAD_B = 'thr_b';

function row(id, thread, cursor, role, extra) {
	return Object.assign({ id, thread_id: thread, cursor, role, kind: 'text', text: id,
		created_at: '2026-10-19T07:40:00.000Z', complete: true, state: 'complete' }, extra || {});
}

async function phone() {
	const dom = { 'thread-picker-label': element(), 'thread-picker': element(), 'thread-title': element() };
	const opened = [];
	const cached = [];
	const context = vm.createContext({
		RichOSLink: { createLink },
		api: {
			refreshChallenge: async () => {},
			openEvents: async (threadId, since, handlers) => { opened.push({ threadId, since, handlers }); return { close() {} }; }
		},
		thread: createThread(),
		threads: [{ id: ONBOARDING, title: 'Onboarding' }],
		currentThreadId: ONBOARDING,
		link: null, linkStateNow: 'opening', notificationTarget: null, vapidPublicKey: null,
		messageCache: { put: async (rows) => { cached.push(...[].concat(rows)); } },
		settings: { set: async () => {} },
		$: (id) => dom[id],
		document: { createElement: () => element() },
		setLinkState() {}, updateQueueBanner() {}, handleApiError() {}, applyCapabilities() {},
		flushQueue() {}, scheduleRender() {}, askForTheQuestion() {}, resolveNotificationTarget: async () => {},
	});
	vm.runInContext(linkSource + '\n' + pickerSource, context);
	context.renderThreadPicker();
	context.connectStream();
	await settle();
	await settle();
	return { context, dom, opened, cached };
}

test('Ray\'s sequence: a conversation created after the page loaded is listed, and its rows stay out of the one on screen', async () => {
	const { context, dom, opened, cached } = await phone();

	// The page loads knowing only Onboarding: no picker, by design (plan §3.4).
	assert.strictEqual(opened.length, 1);
	opened[0].handlers.hello({ threads: [{ id: ONBOARDING, title: 'Onboarding' }],
		messages: [row('onboarding:reply', ONBOARDING, 1, 'rich')] });
	assert.strictEqual(dom['thread-picker-label'].hidden, true);

	// Thread B is created on the Mac and given a job: his words, Rich's reply streaming in, then
	// the question card — each frame on the one shared stream, each naming Thread B.
	const live = opened[0].handlers;
	live.message(row('b1:user', THREAD_B, 2, 'ceo'));
	live.message(row('b1:reply', THREAD_B, 3, 'rich', { text: '', complete: false, state: 'streaming' }));
	live.delta({ message_id: 'b1:reply', thread_id: THREAD_B, cursor: 3, text: 'Looking into it.' });
	live.message(row('b1:reply', THREAD_B, 3, 'rich', { text: 'Looking into it.' }));
	live.message(row('b1:question', THREAD_B, 4, 'rich', { kind: 'question',
		question: { id: 'q1', text: 'Which one?', state: 'open', options: [{ id: 'a', label: 'A' }] } }));
	await settle();
	await settle();

	// Nothing of Thread B's is drawn under Onboarding.
	assert.deepStrictEqual(context.thread.view([]).map((r) => r.id), ['onboarding:reply'],
		'a row that belongs to Thread B was merged into the conversation on screen');

	// It is kept for its own conversation: the phone's message store has every Thread B row,
	// which is what `selectThread` reads when he opens it.
	const keptForB = new Set(cached.filter((r) => r.thread_id === THREAD_B).map((r) => r.id));
	assert.deepStrictEqual([...keptForB].sort(), ['b1:question', 'b1:reply', 'b1:user']);

	// Thread B is in the page's list and the picker is up as soon as there are two. The stream on
	// screen was not reopened to get there, so no frame in flight was dropped.
	assert.ok(context.threads.some((t) => t.id === THREAD_B), 'Thread B never reached the page\'s conversation list');
	assert.strictEqual(dom['thread-picker-label'].hidden, false, 'the picker stayed hidden with two conversations');
	assert.deepStrictEqual(dom['thread-picker'].children.map((o) => o.value), [ONBOARDING, THREAD_B]);
	assert.strictEqual(opened.length, 1);

	// He opens Thread B (what `selectThread` does to the link: a new stream for B). The page knows
	// no title for it, so it presents no cursor and the Mac answers with a `hello` and its list.
	context.currentThreadId = THREAD_B;
	context.connectStream();
	await settle();
	await settle();
	assert.strictEqual(opened.length, 2);
	assert.strictEqual(opened[1].threadId, THREAD_B);
	assert.strictEqual(opened[1].since, null, 'opening the untitled conversation did not ask the Mac for its list');

	// The Mac's list lags until a turn completes (`phone/bridge.rs` `refresh`): a `hello` that does
	// not name Thread B yet leaves it listed.
	opened[1].handlers.hello({ threads: [{ id: ONBOARDING, title: 'Onboarding' }], messages: [] });
	assert.deepStrictEqual(context.threads.map((t) => t.id), [ONBOARDING, THREAD_B]);

	// And once the Mac names it, the picker says its name.
	opened[1].handlers.hello({ threads: [{ id: ONBOARDING, title: 'Onboarding' }, { id: THREAD_B, title: 'Thread B' }],
		messages: [] });
	assert.deepStrictEqual(dom['thread-picker'].children.map((o) => o.textContent), ['Onboarding', 'Thread B']);
	assert.strictEqual(dom['thread-title'].textContent, 'Thread B');

	// Back on Onboarding, Thread B's frames still stay out of it.
	context.currentThreadId = ONBOARDING;
	context.connectStream();
	await settle();
	await settle();
	// A reopened conversation asks for a `hello`, whose rows are the Mac's own positions, rather
	// than presenting a cursor the Mac reads as a hub number (`reopen-order.test.js`).
	assert.strictEqual(opened[2].since, null, 'a reopened conversation presented a cursor instead of asking for its hello');
	opened[2].handlers.message(row('b2:reply', THREAD_B, 5, 'rich'));
	assert.deepStrictEqual(context.thread.view([]).map((r) => r.id), ['onboarding:reply']);
});

test('a row for the conversation on screen, and one from a Mac that names no conversation, still land on screen', async () => {
	const { context, opened } = await phone();
	opened[0].handlers.hello({ threads: [{ id: ONBOARDING, title: 'Onboarding' }], messages: [] });
	opened[0].handlers.message(row('here', ONBOARDING, 1, 'rich'));
	opened[0].handlers.message(row('older-mac', '', 2, 'rich'));
	assert.deepStrictEqual(context.thread.view([]).map((r) => r.id), ['here', 'older-mac']);
	assert.strictEqual(opened.length, 1);
});
