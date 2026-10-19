'use strict';

// THE OXFORD COMMA ON THE PHONE (CEO, 2026-10-19): "for the output the user gets from Claude, the
// user should be able to choose whether or not they want that nuisance." The switch is in the Mac's
// Settings; the phone draws Rich's replies and the questions Claude writes on its cards the way the
// Mac says, through the desktop's own filter (`app/ui/oxford-comma.js`, which the Mac serves here
// as `/lib/oxford-comma.js`). Asserted on the shipped files, in the shape `controls.test.js` uses,
// plus the rule itself run the way the page runs it (a classic script on `self`).

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const ROOT = path.join(__dirname, '..');
const RULE = path.join(ROOT, '..', '..', 'app', 'ui', 'oxford-comma.js');
const html = fs.readFileSync(path.join(ROOT, 'index.html'), 'utf8');
const sw = fs.readFileSync(path.join(ROOT, 'sw.js'), 'utf8');
const appJs = fs.readFileSync(path.join(ROOT, 'app.js'), 'utf8');

test('the phone loads the desktop\'s own rule before app.js, precaches it, and holds no copy of it', () => {
	const rule = html.indexOf('<script src="/lib/oxford-comma.js"></script>');
	assert.ok(rule >= 0, 'index.html does not load /lib/oxford-comma.js');
	assert.ok(rule < html.indexOf('<script src="/app.js"></script>'), 'the rule loads after app.js');
	assert.match(sw, /'\/lib\/oxford-comma\.js'/, 'sw.js does not precache the rule, so the app would open without it away from the Mac');
	assert.ok(!fs.existsSync(path.join(ROOT, 'lib', 'oxford-comma.js')), 'the phone holds its own copy of the rule: two copies are two rules');
	const stub = fs.readFileSync(path.join(__dirname, 'stub-mac.js'), 'utf8');
	assert.match(stub, /lib\/oxford-comma\.js/, 'the stub Mac does not serve the shared rule the real Mac serves');
});

test('what Claude wrote goes through the setting; his own words never do', () => {
	assert.match(appJs, /text\.textContent = mine \? \(row\.text \|\| ''\) : said\(row\.text \|\| ''\);/, 'Rich\'s row is not drawn through the setting');
	assert.match(appJs, /el\('p', 'msg-text', said\(q\.text\)\)/, 'the question on a card is not drawn through it');
	assert.match(appJs, /el\('span', 'ask-label', said\(option\.label\)\)/, 'an option\'s label is not drawn through it');
	assert.match(appJs, /el\('span', 'ask-desc', said\(option\.description\)\)/, 'an option\'s description is not drawn through it');
	assert.match(appJs, /JSON\.stringify\(\[row\.text, row\.created_at, q, globalThis\.RichOxford\.keeping\(\)\]\)/,
		'a held question card is not rebuilt when the setting changes');
});

test('the Mac\'s word reaches the filter: in every hello, and in a state frame the moment he switches it', () => {
	const hello = appJs.match(/hello\(data\) \{[\s\S]*?applyCapabilities\(\);\s*oxfordFrom\(data\);/);
	assert.ok(hello, 'the hello handler does not read oxford_comma');
	assert.match(appJs, /state\(data\) \{ if \(oxfordFrom\(data\)\) return; thread\.applyState\(data\); scheduleRender\(\); \}/,
		'the state handler does not take the setting, or takes it as a row');
	assert.match(appJs, /function oxfordFrom\(data\) \{\n\tif \(!data \|\| typeof data\.oxford_comma !== 'boolean'\) return false;\n\tif \(globalThis\.RichOxford\.setKeeping\(data\.oxford_comma\)\) scheduleRender\(\);/,
		'a change of the setting does not redraw at once');
});

test('the rule, run as the page runs it: off deletes the comma before and/or/nor, on keeps it, code keeps it', () => {
	const self = {};
	vm.runInNewContext(fs.readFileSync(RULE, 'utf8'), { self });
	const O = self.RichOxford;
	assert.ok(O, 'the rule did not install itself on self');
	assert.strictEqual(O.keeping(), false, 'his default: Oxford comma off');
	assert.strictEqual(O.display('Red, green, and blue. Tea, or coffee? Run `a, and b`.'), 'Red, green and blue. Tea or coffee? Run `a, and b`.');
	let told = 0;
	O.onChange(() => { told += 1; });
	assert.strictEqual(O.setKeeping(true), true);
	assert.strictEqual(O.display('Red, green, and blue.'), 'Red, green, and blue.');
	assert.strictEqual(O.setKeeping(true), false, 'the same value again is no change');
	assert.strictEqual(told, 1);
});
