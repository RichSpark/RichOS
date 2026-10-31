'use strict';

// WCAG AA, COMPUTED, IN BOTH THEMES. `node --test "test/*.test.js"`.
//
// This is a floor cleared before handover, not a note raised in review afterwards. It is a TEST
// rather than a calculation done once because a number in a report is a claim about a moment and a
// test is a claim about every future edit to `styles.css`. The app this screen borrows its palette
// from shipped four failing light-mode nodes at 2.95:1, 2.95:1, 3.15:1 and 3.60:1, and every one of
// them looked fine to the people who made them.
//
// The floors, per https://webaim.org/resources/contrastchecker/ :
//   4.5:1  normal text
//   3:1    large text — 24px+, or 18.66px+ bold — and non-text indicators
//
// WHAT THIS FILE CAN AND CANNOT DO, said plainly because the difference matters: it checks the
// PAIRS, against the palette as `styles.css` declares it, and it is fast enough to run on every
// save. It cannot see a node whose background is painted by something this list forgot. The
// phone web page this stylesheet was written for was retired on 2026-10-30, and the earlier iPhone
// app that also staged it was removed on 2026-10-31.
//
// NO EXEMPTIONS ARE CLAIMED, here or in the stylesheet. Every string on this screen is meant to be
// read, including the line that says a message is waiting — which is the one he most needs to
// believe. Calling something exempt is a claim that it is skippable, and nothing here is.

const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

// ---------------------------------------------------------------------------
// Color math
// ---------------------------------------------------------------------------

function parse(color) {
	const hex = /^#([0-9a-f]{6})$/i.exec(color.trim());
	if (hex) {
		const n = parseInt(hex[1], 16);
		return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255, a: 1 };
	}
	const rgba = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)\s*(?:,\s*([\d.]+)\s*)?\)$/i.exec(color.trim());
	if (rgba) return { r: +rgba[1], g: +rgba[2], b: +rgba[3], a: rgba[4] === undefined ? 1 : +rgba[4] };
	throw new Error(`cannot parse color: ${color}`);
}

// A translucent token has no contrast of its own; it has the contrast of what it lands on. Testing
// the token instead of the composite is how an alpha passes a checker and fails a person.
function over(fg, bg) {
	const f = parse(fg);
	const b = parse(bg);
	if (f.a === 1) return f;
	return {
		r: f.r * f.a + b.r * (1 - f.a),
		g: f.g * f.a + b.g * (1 - f.a),
		b: f.b * f.a + b.b * (1 - f.a),
		a: 1
	};
}

function luminance(c) {
	const ch = [c.r, c.g, c.b].map((v) => {
		const s = v / 255;
		return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
	});
	return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
}

function ratio(fg, bg) {
	const a = luminance(over(fg, bg));
	const b = luminance(parse(bg));
	const hi = Math.max(a, b);
	const lo = Math.min(a, b);
	return (hi + 0.05) / (lo + 0.05);
}

// ---------------------------------------------------------------------------
// The palette, READ OUT OF styles.css rather than retyped here
// ---------------------------------------------------------------------------
//
// Retyping it is how a stylesheet and its contrast test stop describing the same screen. The
// parse is deliberately dumb — `--name: value;` inside `:root { … }`, then the same inside the
// `prefers-color-scheme: light` block, which overrides it.

const CSS = fs.readFileSync(path.join(__dirname, '..', 'styles.css'), 'utf8');

function tokensFrom(block) {
	const out = {};
	const re = /--([a-z0-9-]+)\s*:\s*([^;]+);/gi;
	let m;
	while ((m = re.exec(block)) !== null) out[m[1]] = m[2].trim();
	return out;
}

function blockAfter(marker) {
	const at = CSS.indexOf(marker);
	assert.notStrictEqual(at, -1, `styles.css no longer contains ${marker}`);
	const open = CSS.indexOf('{', at);
	let depth = 0;
	for (let i = open; i < CSS.length; i++) {
		if (CSS[i] === '{') depth++;
		else if (CSS[i] === '}') {
			depth--;
			if (depth === 0) return CSS.slice(open, i);
		}
	}
	throw new Error(`unbalanced braces after ${marker}`);
}

const DARK = tokensFrom(blockAfter(':root'));
const LIGHT = Object.assign({}, DARK, tokensFrom(blockAfter('@media (prefers-color-scheme: light)')));

// ---------------------------------------------------------------------------
// Every pair the screen actually renders
// ---------------------------------------------------------------------------
//
// `floor` is 4.5 unless the rule's own size in styles.css makes it large text, or it is a non-text
// indicator. The size is quoted in `where` so the claim is checkable against the stylesheet rather
// than taken on trust.

const TEXT_PAIRS = [
	// --- the top bar ---
	{ fg: 'ink', bg: 'surface-raised', floor: 4.5, where: '.title, 22px 650 — under the large-text bar, so 4.5 applies' },
	{ fg: 'ink-soft', bg: 'surface-raised', floor: 4.5, where: '.link-state, 16px' },
	{ fg: 'attention', bg: 'surface-raised', floor: 4.5, where: '.link-state.away, 16px — the sentence that says his Mac cannot be reached' },
	{ fg: 'ink-soft', bg: 'surface-raised', floor: 4.5, where: '.picker-caption, 16px — screen-reader only since the title took its own row (audit §4.3); kept, because the pair is what the rule still sets and un-hiding it must not be a contrast decision' },
	{ fg: 'ink', bg: 'surface-sunk', floor: 4.5, where: 'select, 16px' },

	// --- the offers ---
	{ fg: 'ink', bg: 'card', floor: 4.5, where: '.offer-text, 16px' },
	{ fg: 'attention', bg: 'card', floor: 4.5, where: '.waiting-offer .offer-text, 16px — "waiting to send"' },

	// --- the thread ---
	{ fg: 'ink-soft', bg: 'ground', floor: 4.5, where: '.older, 16px' },
	{ fg: 'ink', bg: 'rich-bg', floor: 4.5, where: '.msg-rich .msg-text, 17px — every word Rich says' },
	{ fg: 'ink', bg: 'mine-bg', floor: 4.5, where: '.msg-mine .msg-text, 17px — every word he writes' },
	{ fg: 'ink-soft', bg: 'rich-bg', floor: 4.5, where: '.msg-rich .msg-meta, 16px' },
	{ fg: 'ink-soft', bg: 'mine-bg', floor: 4.5, where: '.msg-mine .msg-meta, 16px' },
	{ fg: 'attention', bg: 'mine-bg', floor: 4.5, where: '.msg-state.waiting, 16px — the state that actually matters' },
	{ fg: 'danger', bg: 'mine-bg', floor: 4.5, where: '.msg-state.blocked, 16px' },
	{ fg: 'attention', bg: 'rich-bg', floor: 4.5, where: '.truncated, 16px — "the rest is waiting on your Mac"' },
	{ fg: 'ink', bg: 'surface-sunk', floor: 4.5, where: '.hear, 16px 600' },
	{ fg: 'gold-text', bg: 'surface-sunk', floor: 4.5, where: '.hear.playing, 16px 600' },

	// --- the composer ---
	{ fg: 'ink', bg: 'surface-sunk', floor: 4.5, where: 'textarea, 17px' },
	{ fg: 'trim-text', bg: 'surface-sunk', floor: 4.5, where: 'textarea::placeholder, 17px — it names what the field is for' },
	{ fg: 'ink', bg: 'surface-sunk', floor: 4.5, where: 'button, 17px 600' },
	{ fg: 'on-gold', bg: 'gold', floor: 4.5, where: 'button.primary, 17px 600 — dark ink on gold in BOTH themes' },
	{ fg: 'trim-text', bg: 'surface-sunk', floor: 4.5, where: 'button:disabled, 17px 600 — repainted rather than faded' },
	{ fg: 'on-danger', bg: 'danger', floor: 4.5, where: '.hold.recording, 19px 600 — the label while he is speaking' },
	{ fg: 'ink', bg: 'surface-raised', floor: 4.5, where: '.hold.opening, 19px 600' },
	{ fg: 'ink-soft', bg: 'surface-raised', floor: 4.5, where: '.hold-note, 16px' },
	{ fg: 'attention', bg: 'surface-raised', floor: 4.5, where: '.hold-note.trouble, 16px' },
	{ fg: 'ink', bg: 'surface-raised', floor: 4.5, where: '.meter-read, 16px mono' },

	// --- pairing, and the removed-phone screen ---
	{ fg: 'ink', bg: 'ground', floor: 4.5, where: '.takeover h2, 24px 650 (large; also clears 4.5)' },
	{ fg: 'ink', bg: 'ground', floor: 4.5, where: '.takeover-lede, 17px' },
	{ fg: 'ink-soft', bg: 'ground', floor: 4.5, where: '.takeover-detail, 16px' },
	{ fg: 'ink', bg: 'card', floor: 4.5, where: '.fingerprint-caption, 16px' },
	{ fg: 'gold-text', bg: 'card', floor: 4.5, where: '.fingerprint-words, 24px 650 — six words he compares with his Mac' },
	{ fg: 'ink-soft', bg: 'card', floor: 4.5, where: '.fingerprint-note, 16px' }
];

// Non-text indicators. 3:1, and each one is here because it is the only thing on screen carrying
// its meaning — a control's boundary, the level of his own voice, the edge of the pairing card.
const INDICATOR_PAIRS = [
	{ fg: 'line-control', bg: 'surface-raised', floor: 3.0, where: 'the border of a control on the bar' },
	{ fg: 'line-control', bg: 'surface-sunk', floor: 3.0, where: 'the border of the composer and the meter track' },
	{ fg: 'line-control', bg: 'ground', floor: 3.0, where: 'a control on the page ground' },
	{ fg: 'success', bg: 'surface-sunk', floor: 3.0, where: '.meter-fill against its own track — a dead microphone must be obvious' },
	{ fg: 'gold', bg: 'card', floor: 3.0, where: 'the edge of the fingerprint card' },
	{ fg: 'gold', bg: 'ground', floor: 3.0, where: 'the focus ring' },
	{ fg: 'gold', bg: 'surface-raised', floor: 3.0, where: 'the focus ring on the bar and the composer' },
	{ fg: 'attention', bg: 'card', floor: 3.0, where: 'the stripe on the waiting banner' },
	{ fg: 'danger', bg: 'surface-raised', floor: 3.0, where: 'the record control while recording, against the composer' }
];

function check(pairs, palette, themeName) {
	const failures = [];
	const report = [];
	for (const pair of pairs) {
		const fg = palette[pair.fg];
		const bg = palette[pair.bg];
		assert.ok(fg, `styles.css has no --${pair.fg} in ${themeName}`);
		assert.ok(bg, `styles.css has no --${pair.bg} in ${themeName}`);
		const r = ratio(fg, bg);
		report.push(`  ${themeName.padEnd(5)} ${(pair.fg + ' on ' + pair.bg).padEnd(30)} ${r.toFixed(2)}:1  (floor ${pair.floor}) ${pair.where}`);
		if (r < pair.floor) {
			failures.push(`${themeName}: --${pair.fg} on --${pair.bg} is ${r.toFixed(2)}:1, floor ${pair.floor}:1 — ${pair.where}`);
		}
	}
	console.log(report.join('\n'));
	return failures;
}

test('every text pair clears WCAG AA in dark', () => {
	const failures = check(TEXT_PAIRS, DARK, 'dark');
	assert.deepStrictEqual(failures, [], '\n' + failures.join('\n'));
});

test('every text pair clears WCAG AA in light', () => {
	const failures = check(TEXT_PAIRS, LIGHT, 'light');
	assert.deepStrictEqual(failures, [], '\n' + failures.join('\n'));
});

test('every non-text indicator clears 3:1 in both themes', () => {
	const failures = check(INDICATOR_PAIRS, DARK, 'dark').concat(check(INDICATOR_PAIRS, LIGHT, 'light'));
	assert.deepStrictEqual(failures, [], '\n' + failures.join('\n'));
});

// The arithmetic itself, against the published calculator's own worked values. A contrast test that
// computed the wrong ratio would pass everything above and prove nothing.
test('the ratio function agrees with the published calculator', () => {
	assert.ok(Math.abs(ratio('#ffffff', '#000000') - 21) < 0.01, 'white on black is 21:1');
	assert.ok(Math.abs(ratio('#000000', '#ffffff') - 21) < 0.01, 'and it is symmetric');
	assert.ok(Math.abs(ratio('#777777', '#ffffff') - 4.48) < 0.01, '#777 on white is 4.48:1 — just under AA');
	assert.ok(Math.abs(ratio('#767676', '#ffffff') - 4.54) < 0.01, '#767676 on white is 4.54:1 — just over');
	// And an alpha really is composited rather than measured on its own. Half-opacity black over
	// white is rgb(127.5) and computes 3.98:1 — NOT the 21:1 that measuring the token alone would
	// report, which is the failure mode this composite exists to prevent.
	const composited = ratio('rgba(0, 0, 0, 0.5)', '#ffffff');
	assert.ok(Math.abs(composited - 3.98) < 0.01, `half-opacity black on white is ${composited.toFixed(2)}:1`);
	assert.ok(composited < 5, 'an alpha measured on its own would have reported 21:1 here');
});

// The stylesheet must not reintroduce the two failures this project already shipped once.
test('the two traps the app already hit stay shut', () => {
	// White on gold, in either theme.
	assert.ok(ratio('#ffffff', LIGHT.gold) < 4.5, 'the light-mode gold would pass with white on it, which is not the point');
	assert.strictEqual(DARK['on-gold'], LIGHT['on-gold'], '--on-gold must be the DARK ink in both themes');
	// `--gold` paints and `--gold-text` reads, and in light they are NOT the same value.
	assert.notStrictEqual(LIGHT.gold, LIGHT['gold-text'], 'light mode must keep the paint/read split');
	assert.ok(ratio(LIGHT.gold, LIGHT.ground) < 4.5, 'if the painting gold cleared 4.5 the split would be unnecessary');
	assert.ok(ratio(LIGHT['gold-text'], LIGHT.ground) >= 4.5, 'the reading gold must clear 4.5');
});

// Nothing readable below 14px, nothing that is prose below 16px (the type scale, §15). Asserted
// over the stylesheet itself rather than promised in a comment.
test('no text on this screen is smaller than the type scale allows', () => {
	const sizes = [];
	const re = /font-size:\s*(\d+(?:\.\d+)?)px/g;
	let m;
	while ((m = re.exec(CSS)) !== null) sizes.push(Number(m[1]));
	assert.ok(sizes.length > 10, 'the stylesheet stopped declaring sizes in px, so this check stopped checking');
	const tooSmall = sizes.filter((s) => s < 16);
	assert.deepStrictEqual(tooSmall, [], `font sizes under 16px: ${tooSmall.join(', ')}`);
});
