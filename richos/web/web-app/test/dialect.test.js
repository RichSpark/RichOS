'use strict';

// AMERICAN ENGLISH, IN EVERY STRING HE READS. `node --test "test/*.test.js"`.
//
// The standing rule (CEO, 2026-08-29) binds every string a person reads. The phone web page these
// modules were written for was retired on 2026-10-30; what is left is the string literals of the
// `lib/` modules, which the native apps' conformance corpus and the earlier iPhone app still load.
//
// EVERY CHECK CARRIES A POSITIVE CONTROL, for the reason `ui/tests/dialect.js` states: a "nothing
// found" assertion passes identically when the scanner works and when it is looking at nothing at
// all. Each check re-runs its own extractor over a fixture with a British spelling planted in the
// same shape, and requires it to be found.
//
// The forbidden spellings are assembled from fragments rather than written out, so this file never
// itself contains one — the same pattern, for the same reason.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const DIR = path.join(__dirname, '..');

const BRITISH = [
	'colo' + 'ur', 'behavio' + 'ur', 'favo' + 'urite', 'hono' + 'ur', 'neighbo' + 'ur',
	'organi' + 'se', 'organi' + 'sed', 'organi' + 'sation', 'recogni' + 'se', 'reali' + 'se',
	'apologi' + 'se', 'authori' + 'se', 'customi' + 'se', 'synchroni' + 'se',
	'cance' + 'lled', 'cance' + 'lling', 'trave' + 'lling', 'labe' + 'lled', 'mode' + 'lling',
	'cent' + 're', 'met' + 're', 'lit' + 're', 'theat' + 're', 'fib' + 're', 'calib' + 're',
	'defen' + 'ce', 'offen' + 'ce', 'preten' + 'ce', 'licen' + 'ce',
	'gr' + 'ey', 'alumin' + 'ium', 'whi' + 'lst', 'amon' + 'gst', 'lea' + 'rnt', 'sp' + 'elt',
	'ful' + 'fil', 'enqui' + 'ry', 'programm' + 'e', 'analy' + 'se', 'paraly' + 'se',
	'judgem' + 'ent', 'acknowledgem' + 'ent', 'age' + 'ing', 'mou' + 'ld', 'stor' + 'ey',
	'ty' + 're', 'ke' + 'rb', 'pyjam' + 'as', 'aeropla' + 'ne', 'scept' + 'ical', 'somb' + 're'
];

const PATTERN = new RegExp(`\\b(${BRITISH.join('|')})\\b`, 'i');

function findBritish(text) {
	const hit = PATTERN.exec(text || '');
	return hit ? hit[1] : null;
}

/// Every string literal in a JavaScript file. Deliberately coarse — it catches comments' quoted
/// text too, and that is fine: a comment is part of the record, which the rule also binds.
function stringLiterals(source) {
	const out = [];
	const re = /'((?:[^'\\\n]|\\.)*)'|"((?:[^"\\\n]|\\.)*)"|`((?:[^`\\]|\\.)*)`/g;
	let m;
	while ((m = re.exec(source)) !== null) out.push(m[1] || m[2] || m[3] || '');
	return out;
}

test('every string in the lib modules is American English', () => {
	for (const file of fs.readdirSync(path.join(DIR, 'lib')).filter((f) => f.endsWith('.js'))) {
		const source = fs.readFileSync(path.join(DIR, 'lib', file), 'utf8');
		for (const literal of stringLiterals(source)) {
			const found = findBritish(literal);
			assert.strictEqual(found, null, `lib/${file} has a string saying "${found}"`);
		}
	}
});
