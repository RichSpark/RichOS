'use strict';

// RICH'S ANSWERS ARE FORMATTED, NOT SHOWN AS RAW MARKDOWN. `npm test`.
//
// Ray's nightly 67 walk (p/shots/p01-phone-only-onboarding.jpg): the phone page showed `**323**`
// with its asterisks where the Mac shows a bold 323. The shipped `renderRow` is cut out of
// `app.js` and run in `node:vm` with the real `lib/markdown.js` and an in-memory DOM. No browser.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const source = fs.readFileSync(path.join(ROOT, 'app.js'), 'utf8');
const renderRowSource = source.match(/function renderRow\(row, isLastDelivered\) \{[\s\S]*?\n\}/)[0];
const markdownPath = path.join(ROOT, 'lib/markdown.js');

class Node {
	constructor(tag, text) { this.tagName = tag; this.children = []; this.dataset = {}; this.attrs = {}; this.text = text || ''; this.className = ''; }
	set textContent(value) { this.text = value; this.children = []; }
	get textContent() { return this.text + this.children.map((n) => n.textContent).join(''); }
	appendChild(node) { this.children.push(node); return node; }
	setAttribute(name, value) { this.attrs[name] = value; }
	addEventListener() {}
	all(tag) { return this.children.flatMap((n) => (n.tagName === tag ? [n] : []).concat(n.all ? n.all(tag) : [])); }
}
const doc = { createElement: (tag) => new Node(tag), createTextNode: (text) => new Node('#text', text) };

function draw(row) {
	const context = vm.createContext({
		document: doc, api: { offers: () => false }, hearIt() {}, timeOf: () => '10:01 AM', questionCard() {}, queue: null,
		RichOSMarkdown: fs.existsSync(markdownPath) ? require(markdownPath) : undefined,
	});
	vm.runInContext(renderRowSource + '\nglobalThis.renderRow = renderRow;', context);
	return context.renderRow(row, false);
}

const text = (li) => li.children.find((n) => /msg-text/.test(n.className));

test('Ray\'s nightly 67 answer: **323** is a bold 323, with no asterisks on screen', () => {
	const li = draw({ id: 'r', cursor: 2, role: 'rich', kind: 'text', text: '17 × 19 = **323**', complete: true });
	const body = text(li);
	assert.strictEqual(body.textContent, '17 × 19 = 323', 'the asterisks are on screen');
	assert.deepStrictEqual(body.all('strong').map((n) => n.textContent), ['323']);
});

test('lists, headings, italics and code read as they do on the Mac; arithmetic and markup stay as written', () => {
	const li = draw({ id: 'r', cursor: 2, role: 'rich', kind: 'text', complete: true,
		text: '## Plan\n\n1. Draft the *welcome* email\n2. Commit `docs/welcome.md`\n\n- 3 * 4 * 5\n- <img src=x onerror=alert(1)>' });
	const body = text(li);
	assert.deepStrictEqual(body.all('div').filter((n) => n.attrs.role === 'heading').map((n) => n.textContent), ['Plan']);
	assert.deepStrictEqual(body.all('ol')[0].children.map((n) => n.textContent), ['Draft the welcome email', 'Commit docs/welcome.md']);
	assert.deepStrictEqual(body.all('em').map((n) => n.textContent), ['welcome']);
	assert.deepStrictEqual(body.all('code').map((n) => n.textContent), ['docs/welcome.md']);
	assert.deepStrictEqual(body.all('ul')[0].children.map((n) => n.textContent), ['3 * 4 * 5', '<img src=x onerror=alert(1)>']);
	assert.deepStrictEqual(body.all('img'), [], 'model output became markup');
});

test('his own words are shown exactly as he wrote them', () => {
	const li = draw({ id: 'm', cursor: 1, role: 'ceo', kind: 'text', text: 'what is **17 x 19**?', complete: true });
	assert.strictEqual(text(li).textContent, 'what is **17 x 19**?');
	assert.deepStrictEqual(text(li).all('strong'), []);
});
