'use strict';

// A CONVERSATION CREATED OR RENAMED ON THE MAC IS NAMED IN THE PHONE'S PICKER AT ONCE. `npm test`.
//
// Ray's nightly 66 and 67 walks (67: p, step 111): a conversation created on the Mac after the
// phone page loaded was listed as "Conversation" until it was opened, because the list rode only
// in a `hello`. The Mac now sends the list as a `threads` frame when a conversation is created or
// renamed (`phone/mod.rs` `threads_changed`).
//
// The real `lib/api.js` (its `openEvents`, over a stand-in `EventSource`) and `lib/link.js`, and
// the shipped `makeLink`, `connectStream` and `renderThreadPicker` cut out of `app.js` into
// `node:vm`. No browser, app, phone or network.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const { createApi } = require(path.join(ROOT, 'lib/api.js'));
const { createLink } = require(path.join(ROOT, 'lib/link.js'));
const { createThread } = require(path.join(ROOT, 'lib/thread.js'));
const source = fs.readFileSync(path.join(ROOT, 'app.js'), 'utf8');
const RENDERING = '\n// ---------------------------------------------------------------------------------------------\n// Rendering';
const linkSource = source.slice(source.indexOf('function makeLink() {'), source.indexOf(RENDERING));
const pickerSource = source.match(/function renderThreadPicker\(\) \{[\s\S]*?\n\}/)[0];

const settle = () => new Promise((resolve) => setImmediate(resolve));

class FakeEventSource {
	constructor(url) { this.url = url; this.listeners = {}; FakeEventSource.made.push(this); }
	addEventListener(name, fn) { (this.listeners[name] ||= []).push(fn); }
	deliver(name, data) { (this.listeners[name] || []).forEach((fn) => fn({ data: JSON.stringify(data) })); }
	close() {}
}
FakeEventSource.made = [];

function element() {
	const node = { hidden: true, children: [], text: '' };
	Object.defineProperty(node, 'textContent', { get() { return node.text; }, set(v) { node.text = v; node.children = []; } });
	node.appendChild = (child) => { node.children.push(child); return child; };
	return node;
}

test('a conversation created, then renamed, on the Mac is named in the phone\'s picker without being opened', async () => {
	const dom = { 'thread-picker-label': element(), 'thread-picker': element(), 'thread-title': element() };
	const api = createApi({ state: { apiBase: 'https://mac.example', challenge: null, deviceId: null },
		fetchImpl: async () => ({ status: 200, headers: { get: () => null }, json: async () => ({}) }),
		eventSourceImpl: FakeEventSource });
	api.refreshChallenge = async () => {};
	const saved = {};
	const context = vm.createContext({
		RichOSLink: { createLink }, api, thread: createThread(),
		threads: [{ id: 'A', title: 'Onboarding' }], currentThreadId: 'A',
		link: null, linkStateNow: 'opening', notificationTarget: null, vapidPublicKey: null,
		messageCache: { put: async () => {} }, settings: { set: async (k, v) => { saved[k] = v; } },
		$: (id) => dom[id], document: { createElement: () => element() },
		setLinkState() {}, updateQueueBanner() {}, handleApiError(e) { throw e; }, applyCapabilities() {},
		flushQueue() {}, scheduleRender() {}, askForTheQuestion() {}, resolveNotificationTarget: async () => {},
	});
	vm.runInContext(linkSource + '\n' + pickerSource, context);
	context.renderThreadPicker();
	context.connectStream();
	await settle(); await settle();
	const stream = FakeEventSource.made[0];
	stream.deliver('hello', { threads: [{ id: 'A', title: 'Onboarding' }], messages: [] });
	assert.strictEqual(dom['thread-picker-label'].hidden, true);

	// He starts a new conversation on the Mac.
	stream.deliver('threads', { threads: [{ id: 'A', title: 'Onboarding' }, { id: 'B', title: 'Write the welcome email' }] });
	assert.strictEqual(dom['thread-picker-label'].hidden, false, 'the new conversation never reached the picker');
	assert.deepStrictEqual(dom['thread-picker'].children.map((o) => o.textContent), ['Onboarding', 'Write the welcome email']);

	// And renames it there.
	stream.deliver('threads', { threads: [{ id: 'A', title: 'Onboarding' }, { id: 'B', title: 'Welcome email' }] });
	assert.deepStrictEqual(dom['thread-picker'].children.map((o) => o.textContent), ['Onboarding', 'Welcome email']);
	assert.deepStrictEqual(saved.threads.map((t) => t.title), ['Onboarding', 'Welcome email'], 'the list was not kept for the next launch');
	assert.strictEqual(FakeEventSource.made.length, 1, 'the stream on screen was reopened to learn a name');
	assert.strictEqual(dom['thread-title'].textContent, 'Onboarding');
});
