"use strict";
const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs"), path = require("node:path"), vm = require("node:vm");
const window = {};
vm.runInNewContext(fs.readFileSync(path.resolve(__dirname, "../../authored-input.js"), "utf8"), { window, TextEncoder, TextDecoder });
const A = window.RichAuthored, plain = value => JSON.parse(JSON.stringify(value));
const body = "shit\n\nAttached on this Mac (1 file, saved by RichOS):\n/fixture/wtf.png";
const saved = () => ({ text: body, ranges: [[0, 4]] });
const ownedText = value => value.ranges.map(([a, b]) => new TextDecoder().decode(new TextEncoder().encode(value.text).subarray(a, b)));

test("an appended human answer keeps generated paths outside the submitted ranges", () => {
  const next = A.edit(saved(), body + "\n\nand damn");
  assert.deepEqual(plain(ownedText(next)), ["shit", "\n\nand damn"]);
  assert.equal(A.ranges(next.text, A.forSend(next, next.text)).length, 2);
  const again = A.edit(next, next.text.replace("shit", "damn"));
  assert.deepEqual(plain(ownedText(again)), ["damn", "\n\nand damn"]);
});
test("only the actual edit inside generated context becomes human text", () => {
  const original = { text: "/fixture/class.txt", ranges: [] };
  const next = A.edit(original, "/fixture/classass.txt");
  assert.deepEqual(plain(ownedText(next)), ["ass"]);
  const unicode = A.edit(next, "/fixture/classass-é.txt");
  assert(A.ranges(unicode.text, A.capture(unicode.text, unicode.ranges)));
});
test("an intentional whole-selection paste owns even an identical lookalike header", () => {
  const next = A.edit(saved(), body, { start: 0, end: A.length(body) });
  assert.equal(A.forSend(next, body).authoredBytes, A.length(body));
  assert.equal(A.forSend(saved(), body).authoredBytes, 4);
});
test("trimming and native Unicode whitespace do not shift ownership onto a filename", () => {
  const original = "  résumé" + body.slice(4);
  const capture = A.forSend({ text: original, ranges: [[0, A.length("  résumé")]] }, original.trim());
  assert.equal(capture.authoredBytes, A.length("résumé"));
  const unicode = "\u0085".repeat(80) + "shit";
  assert.equal(A.forSend({ text: unicode, ranges: [[0, A.length(unicode)]] }, "shit").authoredBytes, 4);
});
test("restoring an unsent message alongside a new draft preserves both provenances", () => {
  const next = A.join(body, { authoredBytes: 4 }, "and damn", { authoredBytes: 8 });
  assert.equal(next.text, body + "\n\nand damn");
  assert.deepEqual(plain(ownedText(next)), ["shit", "and damn"]);
  const restored = JSON.parse(JSON.stringify(next));
  assert.deepEqual(plain(A.forSend(restored, restored.text).authoredRanges), plain(next.ranges));
});
test("damaged metadata remains unknown until the whole draft is replaced", () => {
  assert.equal(A.normalized(body, [null]), null);
  assert.equal(A.ranges(body, A.capture(body, null)), null);
  const unknown = { text: body, ranges: null };
  assert.equal(A.edit(unknown, body + " damn").ranges, null);
  const replacement = A.edit(unknown, "shit", { start: 0, end: A.length(body) });
  assert.equal(A.forSend(replacement, "shit").authoredBytes, 4);
});
