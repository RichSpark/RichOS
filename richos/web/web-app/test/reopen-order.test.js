'use strict';

// A CONVERSATION REOPENED ON THE PHONE IS IN THE MAC'S ORDER. `npm test`.
//
// Ray's nightly 67 walk (`richos-hq/docs/audits/2026-10-15-nightly-root/evidence/nightly-67-walk`,
// p3/shots/p10-phone-onboarding-find-answered.jpg; p, step 141): after Onboarding was reopened
// from the picker, its question cards were drawn ABOVE the welcome-email request that asked them,
// although they were at the bottom while it was open live, as on the Mac.
//
// THE CAUSE IS TWO NUMBERINGS MEETING ON ONE PAGE. A row's `cursor` is either its position in the
// conversation's projection (a `hello`, a backfill, a question row: `phone/rows.rs`
// `rows_from_payload`, `phone/mod.rs` `question_changed`) or a number from the hub's ONE live
// sequence for every conversation (`phone/stream.rs` `PhoneLiveEmitter`, `next_cursor`). The page
// keeps whatever it last merged. Reopening presented the highest cursor it held, a projection
// position, and the Mac read it as a hub number (`replay_after_marked`): it replayed the live
// frames after it, which put the request and its reply back on their hub numbers, after the
// question that still held its position.
//
// The Mac and stub below behave as `phone/stream.rs` does: no cursor is a `hello` carrying the
// projection; a cursor is a tail of hub frames numbered above it. The shipped `makeLink`,
// `selectThread`, `askForTheQuestion` and `renderThreadPicker` run in `node:vm` with the real
// `lib/link.js` and `lib/thread.js`. No browser, app, phone or network.

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

const row = (id, thread_id, cursor, role, extra) => Object.assign({ id, thread_id, cursor, role, kind: 'text', text: id, complete: true }, extra || {});
const settle = () => new Promise((resolve) => setImmediate(resolve));
const question = (cursor) => row('q', 'A', cursor, 'rich', { kind: 'question', question: { id: 'q', text: 'Formal or casual?', state: 'open', options: [] } });

/// The Mac, as `phone/stream.rs` and `phone/routes.rs` answer the phone. Every frame carries the
/// hub's own number as its SSE `id:` (`Frame::to_wire`); a `hello` carries the hub's number when
/// it is built (`hello_frame`).
function mac() {
	const projection = { A: [row('r1', 'A', 1, 'ceo'), row('r2', 'A', 2, 'rich')], B: [row('b1', 'B', 1, 'ceo')] };
	const hub = [];
	const threads = [{ id: 'A', title: 'Onboarding' }, { id: 'B', title: 'Thread B' }];
	const streams = [];
	// Thread B's traffic has already moved the hub's one sequence to 10.
	let hubCursor = 10;
	return {
		projection, hub, streams,
		openEvents: async (id, since, handlers) => {
			streams.push({ id, since, handlers });
			setImmediate(() => {
				// `replay_after_marked`: no cursor, or one this run never issued, is a `hello`.
				if (since === null || since > hubCursor) {
					hubCursor = Math.max(hubCursor, projection[id].length);
					handlers.hello({ thread_id: id, threads, messages: projection[id] }, String(hubCursor));
				} else {
					hub.filter((f) => f.cursor > since).forEach((f) => handlers.message(f.row, String(f.cursor)));
				}
			});
			return { close() {} };
		},
		backfill: async (id, before, limit) => {
			const earlier = projection[id].filter((r) => r.cursor < before);
			return { messages: earlier.slice(-limit), more: earlier.length > limit };
		},
		/// One live frame: the hub's own number on the frame, and the row's cursor as the Mac sets it.
		live(frameCursor, liveRow, projected) {
			hubCursor = Math.max(hubCursor, frameCursor);
			hub.push({ cursor: frameCursor, row: liveRow });
			projection.A.push(projected);
			streams[streams.length - 1].handlers.message(liveRow, String(frameCursor));
		}
	};
}

/// The page around the shipped functions, against the Mac `m`.
function page(m) {
	const stored = new Map();
	const context = vm.createContext({
		RichOSLink: { createLink },
		thread: createThread(), currentThreadId: 'A', threads: [{ id: 'A', title: 'Onboarding' }, { id: 'B', title: 'Thread B' }],
		link: null, linkStateNow: 'opening', notificationTarget: null, vapidPublicKey: null,
		settings: { set: async () => {} }, composer: { value: '' }, voiceGesture: { interrupt: async () => {} },
		messageCache: {
			put: async (rows) => { for (const r of [].concat(rows)) stored.set(r.id, r); },
			recent: async (id) => [...stored.values()].filter((r) => r.thread_id === id)
		},
		api: { refreshChallenge: async () => {}, openEvents: m.openEvents, backfill: m.backfill },
		$: () => ({ hidden: true, textContent: '', children: [], appendChild() {} }),
		document: { createElement: () => ({}) },
		setLinkState() {}, updateQueueBanner() {}, handleApiError(e) { throw e; }, applyCapabilities() {},
		flushQueue() {}, scheduleRender() {}, render() {}, restoreComposer: async () => {}, renderVoiceDrafts: async () => {},
		resolveNotificationTarget: async () => {},
	});
	vm.runInContext(linkSource + '\n' + ['renderThreadPicker', 'selectThread', 'askForTheQuestion'].map(fn).join('\n')
		+ '\nconst reconciled = new Set();', context);
	context.order = () => context.thread.view([]).map((r) => r.id);
	return context;
}

/// Onboarding open live: the request, Rich's reply and the question card arrive on the stream.
async function openLive(m, context) {
	context.connectStream();
	await settle(); await settle();
	assert.deepStrictEqual(context.order(), ['r1', 'r2']);

	// The welcome-email request and Rich's reply take hub numbers 11 and 12; the question card
	// takes its projection position, 5, on a frame numbered 13 (`question_changed`).
	m.live(11, row('req', 'A', 11, 'ceo'), row('req', 'A', 3, 'ceo'));
	m.live(12, row('reply', 'A', 12, 'rich'), row('reply', 'A', 4, 'rich'));
	m.live(13, question(5), question(5));
	await settle(); await settle();
	assert.deepStrictEqual(context.order(), ['r1', 'r2', 'req', 'reply', 'q'], 'while open live the card is at the bottom, as Ray saw');
}

/// He opens Thread B, then Onboarding again.
async function reopen(context) {
	await context.selectThread('B');
	await settle(); await settle();
	await context.selectThread('A');
	await settle(); await settle();
}

/// An ordinary connection loss, and the page coming back to the front wakes the retry. Returns
/// the cursor the reconnect presented.
async function loseAndWake(m, context) {
	m.streams[m.streams.length - 1].handlers.error({});
	context.link.wake();
	await settle(); await settle();
	return m.streams[m.streams.length - 1].since;
}

test('Ray\'s nightly 67 sequence: a reopened conversation keeps its question cards under the request that asked them', async () => {
	const m = mac();
	const context = page(m);
	await openLive(m, context);
	await reopen(context);
	context.link.close();
	assert.deepStrictEqual(context.order(), ['r1', 'r2', 'req', 'reply', 'q'], 'the question card moved above the request that asked it');
});

// Review `rv-20261019T112205Z-d98f7bde-d3fb`: after the reopen's `hello` the page's highest row
// cursor is the question's projection position, 5. A lost connection presented 4 as a hub number,
// and the Mac replayed frames 11 to 13, which put the request and the reply back on 11 and 12.
test('a lost connection after a reopen resumes from the stream\'s own number and keeps the card under its request', async () => {
	const m = mac();
	const context = page(m);
	await openLive(m, context);
	await reopen(context);
	assert.deepStrictEqual(context.order(), ['r1', 'r2', 'req', 'reply', 'q']);
	const since = await loseAndWake(m, context);
	context.link.close();
	assert.strictEqual(since, 13, 'the reconnect did not present the last frame the stream delivered');
	assert.deepStrictEqual(context.order(), ['r1', 'r2', 'req', 'reply', 'q'], 'the question card moved above the request that asked it');
});

test('a lost connection while open live resumes from the stream\'s own number', async () => {
	const m = mac();
	const context = page(m);
	await openLive(m, context);
	const since = await loseAndWake(m, context);
	context.link.close();
	assert.strictEqual(since, 13, 'the reconnect did not present the last frame the stream delivered');
	assert.deepStrictEqual(context.order(), ['r1', 'r2', 'req', 'reply', 'q']);
});
