// RICH'S ANSWERS, FORMATTED THE WAY THE MAC FORMATS THEM — a small Markdown subset, built as DOM.
//
// Ray's nightly 67 walk (p/shots/p01-phone-only-onboarding.jpg): the phone page showed Rich's
// answer as `**323**`, asterisks and all, where the Mac shows a bold 323. The model writes
// Markdown; the Mac's conversation renders it (`app/ui/timeline.js` `renderMarkdownInto`) and this
// page set `textContent`.
//
// THE SUBSET IS THE MAC'S ANSWER SUBSET, with the Mac's rules: `**bold**`, `*italic*`, `` `code` ``,
// headings, bullet and numbered lists, and blank-line paragraph breaks. A closer is never preceded
// by whitespace and an opener is never followed by it, so `3 * 4 * 5` keeps its asterisks; an
// unmatched marker is literal, so nothing can swallow the rest of an answer. Tables, quotations,
// rules and fenced code are not drawn here and read as written.
//
// NO STRING EVER BECOMES HTML. Every leaf is a text node and every container is `createElement`;
// there is no `innerHTML`, so model output cannot become markup.
//
// Loads as a plain script (defines `globalThis.RichOSMarkdown`) and as a CommonJS module.

(function (root, factory) {
	const api = factory();
	if (typeof module === 'object' && module.exports) module.exports = api;
	root.RichOSMarkdown = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
	'use strict';

	const HEADING = /^[ \t]*(#{1,6})[ \t]+(.*?)[ \t]*#*[ \t]*$/;
	// Whitespace after the marker keeps `**bold on its own line**` out of the list branch.
	const BULLET = /^[ \t]*[-*+][ \t]+(.*)$/;
	const ORDERED = /^[ \t]*(\d{1,9})[.)][ \t]+(.*)$/;

	/// Where a `*` or `**` run closes, or -1. A `**` never closes a `*`.
	function close(text, from, marker) {
		for (let j = from; j + marker.length <= text.length; j += 1) {
			if (text[j] !== '*') continue;
			const isDouble = text[j + 1] === '*';
			if (marker === '**') {
				if (!isDouble) continue;
			} else if (isDouble) {
				j += 1;
				continue;
			}
			if (/\s/.test(text[j - 1])) {
				if (marker === '**') j += 1;
				continue;
			}
			return j;
		}
		return -1;
	}

	function inline(doc, text, into) {
		let plain = '';
		const flush = () => { if (plain) { into.appendChild(doc.createTextNode(plain)); plain = ''; } };
		let i = 0;
		while (i < text.length) {
			const c = text[i];
			if (c === '`') {
				const end = text.indexOf('`', i + 1);
				if (end > i + 1) {
					flush();
					const code = doc.createElement('code');
					code.appendChild(doc.createTextNode(text.slice(i + 1, end)));
					into.appendChild(code);
					i = end + 1;
					continue;
				}
			} else if (c === '*') {
				const marker = text.slice(i, i + 2) === '**' ? '**' : '*';
				const next = text[i + marker.length];
				const end = next && !/\s/.test(next) ? close(text, i + marker.length, marker) : -1;
				if (end > i + marker.length) {
					flush();
					const node = doc.createElement(marker === '**' ? 'strong' : 'em');
					inline(doc, text.slice(i + marker.length, end), node);
					into.appendChild(node);
					i = end + marker.length;
					continue;
				}
			}
			plain += c;
			i += 1;
		}
		flush();
	}

	/// `root` is emptied and rebuilt. A paragraph keeps its own single newlines (`.msg-text` is
	/// `white-space: pre-wrap`); only the blank lines between blocks and a list's lines are eaten.
	function renderInto(root, text, doc) {
		doc = doc || root.ownerDocument;
		root.textContent = '';
		const lines = String(text == null ? '' : text).split('\n');
		let i = 0;
		while (i < lines.length) {
			const line = lines[i];
			if (!line.trim()) { i += 1; continue; }
			const heading = HEADING.exec(line);
			if (heading) {
				const node = doc.createElement('div');
				node.className = 'md-h';
				node.setAttribute('role', 'heading');
				node.setAttribute('aria-level', String(Math.min(6, heading[1].length + 2)));
				inline(doc, heading[2], node);
				root.appendChild(node);
				i += 1;
				continue;
			}
			const ordered = ORDERED.exec(line);
			if (ordered || BULLET.test(line)) {
				const list = doc.createElement(ordered ? 'ol' : 'ul');
				if (ordered && ordered[1] !== '1') list.setAttribute('start', ordered[1]);
				let m;
				while (i < lines.length && (m = (ordered ? ORDERED : BULLET).exec(lines[i]))) {
					const item = doc.createElement('li');
					inline(doc, ordered ? m[2] : m[1], item);
					list.appendChild(item);
					i += 1;
				}
				root.appendChild(list);
				continue;
			}
			const para = doc.createElement('div');
			const buf = [];
			while (i < lines.length && lines[i].trim() && !HEADING.test(lines[i]) && !ORDERED.test(lines[i]) && !BULLET.test(lines[i])) {
				buf.push(lines[i]);
				i += 1;
			}
			inline(doc, buf.join('\n'), para);
			root.appendChild(para);
		}
	}

	return { renderInto };
});
