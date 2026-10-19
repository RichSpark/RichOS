'use strict';

// A QUESTION'S ANSWER STAYS IN THE CONVERSATION IT WAS SENT FROM. `npm test`.
//
// Second review of v1.2.1 at `c1490229b`, finding 1: answer a question card in conversation A,
// switch to B while the answer is still on its way to the Mac, and the answer's reply was merged
// into the one shared thread model, which by then held B. A's card appeared among B's messages.
//
// Built from the reviewer's fixture (`phone-question-thread-switch.js`): the shipped
// `questionCard` and `selectThread` are cut out of `app.js` and run in a `node:vm` context with
// an in-memory DOM, the real `lib/thread.js` model and an HTTP reply held back until the test
// releases it. No browser, app, phone or network is started.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const { createThread } = require(path.join(ROOT, 'lib/thread.js'));
const source = fs.readFileSync(path.join(ROOT, 'app.js'), 'utf8');
const cardSource = source.slice(source.indexOf('const questionCards = new Map();'), source.indexOf('\nfunction timeOf(row)'));
const switchSource = source.slice(source.indexOf('async function selectThread(id)'), source.indexOf("\n$('thread-picker').addEventListener"));

class Node {
	constructor(tag) { this.tagName = tag; this.children = []; this.dataset = {}; this.events = {}; }
	set textContent(value) { this.text = value; this.children = []; }
	get textContent() { return (this.text || '') + this.children.map((n) => n.textContent).join(''); }
	appendChild(node) { this.children.push(node); return node; }
	append(...nodes) { nodes.forEach((n) => this.appendChild(n)); }
	setAttribute() {}
	addEventListener(name, action) { this.events[name] = action; }
}

async function answerThen(switchAway) {
	let finish;
	const response = new Promise((resolve) => { finish = resolve; });
	const model = createThread();
	const q = { id: 'q-A', text: 'Question belonging to A', state: 'open', revision: 0,
		options: [{ id: 'yes', label: 'Yes' }], multiple: false, free_answer: false };
	const row = { id: 'question-A', cursor: 10, thread_id: 'A', role: 'rich', kind: 'question', question: q };
	const rowB = { id: 'message-B', cursor: 20, thread_id: 'B', role: 'rich', text: 'B history' };
	// The phone's own message store, keyed the way `lib/storage.js` keys it: by row id, read back
	// per conversation by `thread_id`.
	const stored = new Map([[row.id, row], [rowB.id, rowB]]);
	const messageCache = {
		put: async (rows) => { [].concat(rows).forEach((r) => stored.set(r.id, r)); },
		recent: async (threadId) => [...stored.values()].filter((r) => r.thread_id === threadId),
	};
	const context = vm.createContext({
		document: { createElement: (tag) => new Node(tag) },
		api: { offers: () => false, answerQuestion: () => response }, newClientId: () => 'submission',
		currentThreadId: 'A', thread: model, messageCache,
		threads: [{ id: 'A' }, { id: 'B' }], settings: { set: async () => {} }, composer: { value: '' },
		voiceGesture: { interrupt: async () => {} }, restoreComposer: async () => {}, renderVoiceDrafts: async () => {},
		renderThreadPicker: () => {}, render: () => {}, connectStream: () => {},
		scheduleRender: () => {}, timeOf: () => '', hearIt: () => {}, handleApiError: () => {}, row,
	});
	vm.runInContext(cardSource + '\n' + switchSource + '\nglobalThis.card = questionCard(row);', context);
	model.merge(row);
	const controls = context.card.children.find((n) => n.className === 'ask-controls');
	const sent = controls.children.find((n) => n.className === 'ask-option').events.click();
	if (switchAway) await context.selectThread('B');
	finish({ outcome: 'accepted', question: { ...q, state: 'answered', revision: 1, answer: { option_ids: ['yes'], text: '' } } });
	await sent;
	return { context, model };
}

test('an answer whose reply arrives in the same conversation updates the card on screen', async () => {
	const { context, model } = await answerThen(false);
	const rows = model.view([]);
	assert.strictEqual(context.currentThreadId, 'A');
	assert.deepStrictEqual(rows.map((r) => r.id), ['question-A']);
	assert.strictEqual(rows[0].question.state, 'answered');
});

test('an answer whose reply arrives after switching conversations never shows in the new one', async () => {
	const { context, model } = await answerThen(true);
	const rows = model.view([]);
	assert.strictEqual(context.currentThreadId, 'B');
	assert.deepStrictEqual(rows.map((r) => r.id), ['message-B'],
		`conversation B shows a row from A: ${rows.map((r) => `${r.id}@${r.thread_id}`).join(',')}`);
});

test('that reply is still kept for the conversation it was sent from, so switching back shows it answered', async () => {
	const { context, model } = await answerThen(true);
	await context.selectThread('A');
	const rows = model.view([]);
	assert.deepStrictEqual(rows.map((r) => r.id), ['question-A']);
	assert.strictEqual(rows[0].question.state, 'answered');
});
