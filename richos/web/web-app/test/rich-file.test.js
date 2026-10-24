'use strict';

// A FILE RICH SENDS IS A CARD ON THIS PAGE, AND A TAP OPENS IT. `npm test`.
//
// The Mac sends each file Rich attached as a Rich row of kind `file` (`app/src-tauri/src/phone/
// rows.rs`), with the bytes on a signed `GET /api/messages?kind=file` (`routes.rs` `rich_file`).
// Until this card the page drew that row as a bubble holding the file's name and nothing to open
// it with (candidate 73's phone-file walk in the test VM). The shipped `renderRow`, `fileCard`,
// `openFile` and `askAgain` are cut out of `app.js` and run in a `node:vm` context with an
// in-memory DOM, as `question-thread.test.js` does. No browser, app, phone or network is started.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const source = fs.readFileSync(path.join(ROOT, 'app.js'), 'utf8');
const rowSource = source.slice(source.indexOf('function renderRow(row, isLastDelivered)'), source.indexOf('const questionCards = new Map();'));
const GONE = 'No longer on this Mac. It was moved or deleted.';

class Node {
	constructor(tag) { this.tagName = tag; this.children = []; this.dataset = {}; this.events = {}; this.attributes = {}; }
	set textContent(value) { this.text = value; this.children = []; }
	get textContent() { return (this.text || '') + this.children.map((n) => n.textContent).join(''); }
	set innerHTML(value) { this.html = value; }
	appendChild(node) { this.children.push(node); return node; }
	append(...nodes) { nodes.forEach((n) => this.appendChild(n)); }
	setAttribute(name, value) { this.attributes[name] = String(value); }
	addEventListener(name, action) { this.events[name] = action; }
	click() { if (this.events.click) return this.events.click(); }
	all() { return [this, ...this.children.flatMap((n) => n.all())]; }
}

const row = {
	id: 'rf_1', cursor: 7, thread_id: 'thr_A', role: 'rich', kind: 'file', text: 'rich-file-check.pdf',
	created_at: '2026-10-24T12:00:00Z', complete: true, state: 'complete', has_audio: false,
	file: { id: 'rf_1', name: 'rich-file-check.pdf', ext: 'PDF', type: 'PDF', meta: 'PDF · 2 KB · 1 page', bytes: 2048, exists: true },
};

function page(fetchFile, shown = row) {
	const opened = [];
	const sent = [];
	let renders = 0;
	const context = vm.createContext({
		document: { createElement: (tag) => new Node(tag) },
		api: { offers: () => true, fetchFile },
		window: { open: (...args) => { const view = { args, location: {}, closed: false, close() { this.closed = true; } }; opened.push(view); return view; } },
		URL: { createObjectURL: (blob) => 'blob:phone/' + blob.name, revokeObjectURL: () => {} },
		setTimeout: () => 0,
		queue: { enqueue: async (item) => { sent.push(item); } },
		newClientId: () => 'client-1', currentThreadId: 'thr_A', flushQueue: () => {}, render: () => {},
		scheduleRender: () => { renders += 1; }, setHoldNote: () => {}, handleApiError: () => {},
		timeOf: () => '12:00', said: (t) => t,
		RichOSMarkdown: { renderInto: (node, text) => { node.textContent = text; } },
	});
	vm.runInContext(rowSource + '\nthis.renderRow = renderRow;', context);
	return { draw: () => context.renderRow(shown, false), opened, sent, renders: () => renders };
}

const cardOf = (li) => li.all().find((n) => n.tagName === 'button' && (n.attributes['aria-label'] || '').startsWith('Open '));
const buttons = (li) => li.all().filter((n) => n.tagName === 'button').map((n) => n.textContent || n.attributes['aria-label']);
const settle = () => new Promise((resolve) => setImmediate(resolve));

test('a file from Rich is a card that names the file and its line, with nothing to hear', () => {
	const li = page(async () => ({})).draw();
	const card = cardOf(li);
	assert.ok(card, 'no "Open rich-file-check.pdf" button: the row was drawn as ' + JSON.stringify(li.textContent));
	assert.strictEqual(card.attributes['aria-label'], 'Open rich-file-check.pdf, PDF · 2 KB · 1 page');
	assert.match(card.textContent, /rich-file-check\.pdf/);
	assert.match(card.textContent, /PDF · 2 KB · 1 page/);
	assert.ok(!buttons(li).includes('Hear it'), 'a file row offered "Hear it": ' + JSON.stringify(buttons(li)));
});

test('a tap opens the viewer inside the tap and points it at the bytes the Mac sent', async () => {
	let finish;
	const asked = [];
	const p = page((id, thread) => { asked.push([id, thread]); return new Promise((resolve) => { finish = resolve; }); });
	cardOf(p.draw()).click();
	assert.strictEqual(p.opened.length, 1, 'the viewer was not opened inside the tap');
	assert.deepStrictEqual(asked, [['rf_1', 'thr_A']]);
	assert.match(cardOf(p.draw()).attributes['aria-label'], /Coming over from your Mac/);
	cardOf(p.draw()).click();
	assert.strictEqual(asked.length, 1, 'a second tap while it comes over asked the Mac again');
	finish({ name: 'rich-file-check.pdf' });
	await settle();
	assert.strictEqual(p.opened[0].location.href, 'blob:phone/rich-file-check.pdf');
	assert.strictEqual(cardOf(p.draw()).attributes['aria-label'], 'Open rich-file-check.pdf, PDF · 2 KB · 1 page');
});

test('a file moved or deleted on the Mac says so in the Mac\'s sentence and offers Ask Rich again', async () => {
	const p = page(async () => { throw Object.assign(new Error(GONE), { reason: 'refused', aboutThisMessage: true }); });
	cardOf(p.draw()).click();
	await settle();
	assert.ok(p.opened[0].closed, 'the empty viewer was left open');
	const li = p.draw();
	assert.strictEqual(li.dataset.s, 'missing');
	assert.match(cardOf(li).attributes['aria-label'], new RegExp(GONE));
	const ask = li.all().find((n) => n.tagName === 'button' && n.textContent === 'Ask Rich again');
	assert.ok(ask, 'no Ask Rich again: ' + JSON.stringify(buttons(li)));
	await ask.click();
	assert.deepStrictEqual(p.sent.map((s) => [s.threadId, s.kind, s.text]), [['thr_A', 'text', 'Please send rich-file-check.pdf again.']]);
});

test('a file the Mac already reports gone is drawn missing and a tap asks the Mac nothing', () => {
	const asked = [];
	const gone = Object.assign({}, row, { file: Object.assign({}, row.file, { exists: false, meta: 'PDF' }) });
	const p = page(async (...args) => { asked.push(args); }, gone);
	const li = p.draw();
	assert.strictEqual(li.dataset.s, 'missing');
	assert.match(cardOf(li).attributes['aria-label'], new RegExp(GONE));
	cardOf(li).click();
	assert.deepStrictEqual(asked, []);
	assert.strictEqual(p.opened.length, 0);
});

test('a Mac out of reach leaves the card ready, saying so, and the viewer closed', async () => {
	const p = page(async () => { throw Object.assign(new Error('fetch failed'), { reason: 'unreachable' }); });
	cardOf(p.draw()).click();
	await settle();
	assert.ok(p.opened[0].closed);
	const li = p.draw();
	assert.strictEqual(li.dataset.s, 'ready');
	assert.strictEqual(cardOf(li).attributes['aria-label'], 'Open rich-file-check.pdf, Your Mac isn’t reachable from here. Tap to try again.');
});
