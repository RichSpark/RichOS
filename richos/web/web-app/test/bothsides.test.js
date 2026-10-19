'use strict';

// BOTH SIDES OF THE CONVERSATION REACH THE SCREEN. `npm test`.
//
// Ray's candidate .11 walk on the CEO's own Android, 2026-09-18
// (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260918.6-mac-and-android-audit.md` §4.1):
//
//     "Neither the message typed on the phone nor the one typed on the Mac ever appears in the
//      phone's thread — only Rich's replies do. Positive control: the Mac's copy of the same
//      thread shows 'hello from my phone' correctly, so the message arrived; the phone simply does
//      not render user turns."
//
// WHERE IT WAS, AND WHERE IT WAS NOT. The Mac was not the cause and its own tests say so:
// `rows_from_payload` keeps `user_message` and `rich_message` and drops only machinery
// (`app/src-tauri/src/phone/rows.rs:49-109`, asserted by
// `only_the_conversation_reaches_the_phone_and_no_machinery_does`), and every `hello` carries the
// whole projection — `"messages": rows` at `phone/routes.rs:494`. The page threw the frame's rows
// away: `app.js`'s `hello` handler read `vapid_public_key` and `threads` out of it and never
// touched `data.messages`.
//
// THREE THINGS ARE ASSERTED HERE, and they are three different failures:
//
//   1. THE DATA. A `hello` frame shaped the way the Mac builds it, merged, renders both roles.
//      This is the positive control — it passed before the fix too, which is exactly what makes
//      it worth keeping: it proves the rows were renderable and therefore that the page dropped
//      them.
//   2. THE WIRING. The shipped `app.js` merges `data.messages` in its `hello` handler. This is
//      the one that fails on `05ab7a0c`. It is asserted on the bytes because this file is an IIFE
//      that boots a browser app on load, and `test/desktop-verify.js` — the harness that drives
//      the real thing — needs Playwright and is not part of `npm test`.
//   3. HIS OWN MESSAGE, BETWEEN THE SEND AND THE PROJECTION. The queue drops an item the moment
//      the Mac takes it, and the Mac's own row for it only exists once the intake log is drained
//      — which runs on its own thread, behind the spine. `thread.confirm` holds his words in
//      that window and retires them on the Mac's own row rather than leaving two.
//
// AND A PREMISE THAT WAS TRUE WHEN THIS FILE WAS WRITTEN IS NOT ANY MORE, corrected here rather
// than left where a reader would believe it: *"no live frame in this build carries a CEO turn"*.
// `rich://ceo-message` has existed since `102b7c07`, so his own row arrives live on both roads —
// the desk's (`spine.rs`'s `accept_prompt`) and the phone's (`drain_intake`). Check 2b below is
// about the leg that claim used to excuse.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const { createThread } = require('../lib/thread.js');
const { createQueue } = require('../lib/queue.js');

const ROOT = path.join(__dirname, '..');
const appJs = fs.readFileSync(path.join(ROOT, 'app.js'), 'utf8');

/// A `hello` frame's `messages`, in the shape `rows_from_payload` actually emits: both roles,
/// dense 1-based cursors, `client_id: null` on every row (`phone/rows.rs:82` and `:97`).
const HELLO_MESSAGES = [
	{
		id: 't1:user', thread_id: 'thr_5c1e', cursor: 1, role: 'ceo', kind: 'text',
		text: 'hello from my phone', created_at: '2026-09-18T23:31:00.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'sent', complete: true
	},
	{
		id: 't1:rich', thread_id: 'thr_5c1e', cursor: 2, role: 'rich', kind: 'text',
		text: 'On it!', created_at: '2026-09-18T23:31:04.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'complete', complete: true
	},
	{
		id: 't2:user', thread_id: 'thr_5c1e', cursor: 3, role: 'ceo', kind: 'text',
		text: 'banana', created_at: '2026-09-18T23:33:00.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'sent', complete: true
	},
	{
		id: 't2:rich', thread_id: 'thr_5c1e', cursor: 4, role: 'rich', kind: 'text',
		text: 'A banana it is.', created_at: '2026-09-18T23:33:05.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'complete', complete: true
	}
];

// --- 1. the data -------------------------------------------------------------------------------

test('a hello frame renders BOTH sides — his message from the phone AND the one from the Mac', () => {
	const thread = createThread();
	thread.merge(HELLO_MESSAGES);

	const shown = thread.view([]);
	assert.deepStrictEqual(
		shown.map((row) => [row.role, row.text]),
		[
			['ceo', 'hello from my phone'],
			['rich', 'On it!'],
			['ceo', 'banana'],
			['rich', 'A banana it is.']
		],
		'the thread he is shown is not the conversation the Mac holds'
	);

	// §4.1 in one line: what he was looking at was a list of answers with no questions.
	const his = shown.filter((row) => row.role === 'ceo');
	assert.strictEqual(his.length, 2, 'his own turns are missing from the rendered thread');
	assert.ok(his.some((row) => row.text === 'hello from my phone'), 'the message he typed on the PHONE is missing');
	assert.ok(his.some((row) => row.text === 'banana'), 'the message he typed on the MAC is missing');
});

// --- 2. the wiring -----------------------------------------------------------------------------

test('the shipped app merges the rows the Mac puts in every hello, rather than dropping them', () => {
	const handler = appJs.match(/\n\t+hello\(data(?:, id)?\) \{[\s\S]*?\n\t+\},\n/);
	assert.ok(handler, 'the hello handler is gone from app.js, and with it the one frame that carries his own words');

	assert.match(
		handler[0],
		/thread\.merge\(data\.messages\)/,
		'the hello handler does not merge `data.messages` — the Mac sends both sides and the page keeps Rich\'s half'
	);

	// And the merge happens in the handler, not somewhere a later edit can quietly reorder past
	// the first paint.
	const mergeAt = handler[0].indexOf('thread.merge(data.messages)');
	const renderAt = handler[0].indexOf('scheduleRender()');
	assert.ok(mergeAt !== -1 && renderAt > mergeAt, 'the rows are merged after the repaint that would have shown them');
});

// --- 2b. ONE PATH FOR BOTH ROLES — Ray's `.20260920.1` defect 1 ---------------------------------
//
// He typed on the Mac and the phone showed his sentence 1.3-6.1 s later on four of five turns,
// while the REPLY to that same sentence crossed in under a second
// (`docs/verification/2026-09-20-nightly-1.2.0-nightly.20260920.1-mac-to-phone-in-the-vm-audit.md`,
// step 3). Two legs of one turn, measured against each other, is what makes that a finding rather
// than a slow network.
//
// THE PAGE'S HALF OF THE ANSWER IS THAT THERE IS NO SECOND PATH HERE, and this asserts it on the
// bytes: the `message` handler merges and schedules a repaint for whatever arrives, and the only
// thing it does per-role — `askForTheQuestion` — happens AFTER the merge and cannot hold a row
// back. A future edit that special-cases his own row (a batch, a debounce, a "wait for the reply
// then refresh") fails here.
//
// It does not and cannot measure the Mac's half; that is `phone::listen`'s
// `his_own_message_and_richs_reply_reach_the_phone_within_one_bound`, over real TLS.

test('a live message frame reaches the screen the same way whichever of them said it', () => {
	const handler = appJs.match(/\n\t+message\(row(?:, id)?\) \{[\s\S]*?\n\t+\},\n/);
	assert.ok(handler, 'the live `message` handler is gone from app.js');

	const mergeAt = handler[0].indexOf('thread.merge(row)');
	const renderAt = handler[0].indexOf('scheduleRender()');
	assert.ok(mergeAt !== -1, 'the live handler does not merge the row it was given');
	assert.ok(renderAt > mergeAt, 'the repaint is scheduled before the row it would show');

	// NOTHING DECIDES ON THE ROLE IN FRONT OF THE MERGE. The one role test in this handler is
	// the reconciliation below, and it sits after the repaint.
	const beforeTheMerge = handler[0].slice(0, mergeAt);
	assert.ok(
		!/role/.test(beforeTheMerge),
		`something now looks at the role before merging his row:\n${beforeTheMerge}`
	);
	const roleAt = handler[0].indexOf('row.role');
	assert.ok(roleAt === -1 || roleAt > renderAt, 'a role test was moved in front of the repaint');

	// AND THE PAGE MERGES BOTH ROLES OFF THE LIVE STREAM, not only the projection. Same frame
	// shape the Mac builds in `phone/rows.rs` `event_from_live`, one row each, in the order they
	// are published.
	const thread = createThread();
	thread.merge({
		id: 't9:user', thread_id: 'thr_5c1e', cursor: 1, role: 'ceo', kind: 'text',
		text: 'where are we on the proposal?', created_at: '2026-09-20T07:00:00.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'sent', complete: true
	});
	assert.deepStrictEqual(
		thread.view([]).map((row) => [row.role, row.text]),
		[['ceo', 'where are we on the proposal?']],
		'a LIVE row for his own message does not reach the screen'
	);
});

test('the Mac\'s stand-in for a sentence he just typed retires on the Mac\'s own row, never beside it', () => {
	// THE ROAD THIS IS ON. He types on the Mac. `send_message` writes his words to the durable
	// intake log without the spine lock and announces them to the phone immediately, under the
	// intake id (`app/src-tauri/src/phone/stream.rs` `announce_his_words`). When the spine is
	// free the same words become a turn and the projection sends the row again, under
	// `{turn}:user`. Two frames, one sentence — and the screen must show one bubble the whole
	// way through.
	const thread = createThread();
	const standIn = {
		id: 'intake_41', thread_id: 'thr_5c1e', cursor: 1, role: 'ceo', kind: 'text',
		text: 'where are we on the proposal?', created_at: '2026-09-20T07:00:00.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'sent', complete: true
	};
	thread.merge(standIn);
	assert.deepStrictEqual(
		thread.view([]).map((row) => [row.id, row.text]),
		[['intake_41', 'where are we on the proposal?']],
		'his words did not reach the screen at all on the fast road'
	);

	// The turn exists now. Same words, the projection's id and its own cursor.
	thread.merge({
		id: 't7:user', thread_id: 'thr_5c1e', cursor: 2, role: 'ceo', kind: 'text',
		text: 'where are we on the proposal?', created_at: '2026-09-20T07:00:00.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'sent', complete: true
	});
	assert.deepStrictEqual(
		thread.view([]).map((row) => [row.id, row.text]),
		[['t7:user', 'where are we on the proposal?']],
		'his one sentence is on the screen twice'
	);

	// POSITIVE CONTROL: a DIFFERENT sentence does not retire the stand-in. Without this, a rule
	// that deleted every stand-in on any CEO row would pass the assertion above.
	const other = createThread();
	other.merge(standIn);
	other.merge({
		id: 't8:user', thread_id: 'thr_5c1e', cursor: 2, role: 'ceo', kind: 'text',
		text: 'and the Kestrel deck?', created_at: '2026-09-20T07:01:00.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'sent', complete: true
	});
	assert.deepStrictEqual(
		other.view([]).map((row) => [row.id, row.text]),
		[['intake_41', 'where are we on the proposal?'], ['t8:user', 'and the Kestrel deck?']],
		'a different sentence retired the stand-in, so a message he typed was deleted off his screen'
	);

	// AND A REPLY NEVER RETIRES ONE. Rich's row shares nothing with his, but a rule written on
	// `text` alone would retire a stand-in whose words Rich happened to repeat back.
	const echoed = createThread();
	echoed.merge(standIn);
	echoed.merge({
		id: 't9:rich', thread_id: 'thr_5c1e', cursor: 2, role: 'rich', kind: 'text',
		text: 'where are we on the proposal?', created_at: '2026-09-20T07:02:00.000Z',
		client_id: null, has_audio: false, from_microphone: false, state: 'complete', complete: true
	});
	assert.strictEqual(echoed.view([]).length, 2, 'Rich repeating his words deleted his message');
});

test('a reply that arrives with no question in front of it makes the phone ask the Mac for one', () => {
	// The fallback for the message he types ON THE MAC while watching the phone. It was written
	// when no live event carried a CEO turn at all; `rich://ceo-message` (`102b7c07`) closed that,
	// and this stays because a phone whose stream dropped during his sentence still comes back to
	// a reply with no question in front of it.
	assert.match(appJs, /function askForTheQuestion\(row\)/, 'the reconciliation is gone');
	assert.match(
		appJs,
		/if \(row && row\.role === 'rich' && row\.complete\) askForTheQuestion\(row\);/,
		'nothing triggers the reconciliation when a reply settles'
	);
	const body = appJs.match(/async function askForTheQuestion\(row\) \{[\s\S]*?\n\}/);
	assert.ok(body, 'askForTheQuestion has no body');
	assert.match(body[0], /api\.backfill\(/, 'the reconciliation does not actually ask the Mac');
	assert.match(body[0], /reconciled\.(has|add)\(/, 'the reconciliation is unbounded — one request per reply is the whole budget');
});

// --- 3. his own message, between the send and the projection -------------------------------------

function storagePort() {
	const map = new Map();
	return {
		async all() { return Array.from(map.values()).map((i) => JSON.parse(JSON.stringify(i))); },
		async put(item) { map.set(item.clientId, JSON.parse(JSON.stringify(item))); },
		async remove(clientId) { map.delete(clientId); }
	};
}

test('the Mac\'s answer to a send is handed back, so the phone knows where his message went', async () => {
	const queue = createQueue({ storage: storagePort() });
	await queue.enqueue({ clientId: 'c-1', threadId: 'thr_5c1e', kind: 'text', text: 'hello from my phone' });

	const answer = {
		message_id: 'intake_41', cursor: 1, thread_id: 'thr_5c1e',
		accepted_at: '2026-09-18T23:31:00.000Z', duplicate: false
	};
	const result = await queue.flush({ async sendText() { return answer; } });

	assert.strictEqual(result.sent, 1);
	assert.strictEqual(queue.count(), 0, 'the queue still empties on success — the Mac has it');
	assert.deepStrictEqual(
		(result.accepted || []).map((a) => [a.item.clientId, a.answer.cursor]),
		[['c-1', 1]],
		'the Mac\'s own answer died inside the flush, and with it the only thing that knew where his message had gone'
	);
});

test('his message stays on screen between the send and the Mac\'s own row, and never appears twice', () => {
	const thread = createThread();
	const item = { clientId: 'c-1', kind: 'text', text: 'hello from my phone' };

	// The Mac took it. The queue drops the item; nothing has been projected back yet.
	thread.confirm(item, { message_id: 'intake_41', cursor: 1, accepted_at: '2026-09-18T23:31:00.000Z' });
	assert.deepStrictEqual(
		thread.view([]).map((row) => [row.role, row.text]),
		[['ceo', 'hello from my phone']],
		'his message vanished the instant his Mac accepted it'
	);

	// Rich's reply streams in live. His message is still there, and still in front of the reply.
	thread.merge(HELLO_MESSAGES[1]);
	assert.deepStrictEqual(
		thread.view([]).map((row) => [row.role, row.text]),
		[['ceo', 'hello from my phone'], ['rich', 'On it!']]
	);

	// The projection catches up. ONE of his message, and it is the Mac's row.
	thread.merge(HELLO_MESSAGES[0]);
	const shown = thread.view([]);
	assert.deepStrictEqual(
		shown.map((row) => [row.role, row.text]),
		[['ceo', 'hello from my phone'], ['rich', 'On it!']],
		'his message is on the screen twice — once from the phone and once from the Mac'
	);
	assert.strictEqual(shown[0].id, 't1:user', 'the stand-in outlived the Mac\'s own row for the same words');
});

test('the optimistic bubble and the confirmed row are never both on screen for one message', () => {
	const thread = createThread();
	const item = { clientId: 'c-1', kind: 'text', text: 'on the train' };
	const pending = [{ clientId: 'c-1', kind: 'text', text: 'on the train', state: 'sending', queuedAt: '2026-09-18T23:31:00.000Z' }];

	// Before the Mac answers: one bubble, and it says so.
	assert.deepStrictEqual(thread.view(pending).map((row) => row.pending), [true]);

	// After it answers, the caller drops the queue item — so the view is asked with an empty
	// pending list and the confirmed row is what is left.
	thread.confirm(item, { message_id: 'intake_9', cursor: 4, accepted_at: '2026-09-18T23:31:02.000Z' });
	const settled = thread.view([]);
	assert.strictEqual(settled.length, 1, 'one message, one row');
	assert.strictEqual(settled[0].pending, undefined);
	assert.strictEqual(settled[0].role, 'ceo');
});

test('switching threads leaves no confirmed row of the old one behind', () => {
	const thread = createThread();
	thread.confirm({ clientId: 'c-1', kind: 'text', text: 'hello from my phone' }, { cursor: 1 });
	thread.reset();
	assert.deepStrictEqual(thread.view([]), []);
});
