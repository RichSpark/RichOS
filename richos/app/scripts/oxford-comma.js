#!/usr/bin/env node
// oxford-comma.js — deletes the comma before " and", " or" and " nor" from the app's own UI text
// and from Markdown docs (CEO, 2026-10-19): "in the UI as well as any of our docs etc, the Oxford
// comma must be always off" and "when one of the existing docs gets changed/updated etc, THEN the
// Oxford comma linter should clean that doc."
//
// Git runs it at every commit, through `autocheck/autocheck.py` (the pre-commit hook), over the
// files the commit adds or changes: the whole of each changed doc, and the UI text in each changed
// UI file. A doc or UI file no commit touches stays exactly as it is.
//
//   node oxford-comma.js --json            stdin {"files":[{"path","content"}]} (paths relative
//                                          to the repository root), stdout the same shape
//                                          holding only the files it changed
//   node oxford-comma.js --write PATH...   fix those files in place (repository-relative or
//                                          absolute) and name each one it changed
//   node oxford-comma.js --check PATH...   name every comma it would delete; exit 1 if any
//
// WHAT IT TOUCHES:
//   * a Markdown doc (`.md`, `.markdown`) anywhere in the repository, except a test fixture or
//     captured or third-party material: a path with a `fixtures`, `fixture`, `testdata`,
//     `__fixtures__`, `__snapshots__`, `third_party`, `third-party`, `vendor`, `node_modules`,
//     `raw`, `cold-open` or `transcripts` directory in it.
//   * the UI text of a UI file: exactly the text the m-dash check in `ui/tests/dialect.js` reads
//     (`ui/tests/lib/oxford-ui-text.js` says what that is).
// WHAT IT NEVER TOUCHES: code. In a doc, fenced and indented code blocks, inline code, front
// matter, blockquotes, HTML comments and anything in double quotes keep every comma; in a UI file
// only the text inside string literals, text nodes and readable attributes is read at all, and
// inline code and quoted words inside it are kept the same way. The rule itself is
// `ui/oxford-comma.js`, the same file that filters what Claude writes on screen.

"use strict";

const fs = require("fs");
const path = require("path");

const APP = path.resolve(__dirname, "..");
const REPO = path.resolve(APP, "..", "..");
const OXFORD = require(path.join(APP, "ui", "oxford-comma.js"));

const NOT_DOCS = new Set([
  "fixtures", "fixture", "testdata", "__fixtures__", "__snapshots__", "third_party", "third-party",
  "vendor", "node_modules", "raw", "cold-open", "transcripts",
]);

function isDoc(rel) {
  if (!/\.(md|markdown)$/i.test(rel)) return false;
  return !rel.split("/").slice(0, -1).some((seg) => NOT_DOCS.has(seg));
}

let uiText = null;
function ui() {
  // Loaded only when a path could be UI text: it derives the shipped UI from `ui/index.html`.
  if (!uiText) uiText = require(path.join(APP, "ui", "tests", "lib", "oxford-ui-text.js"));
  return uiText;
}

function couldBeUi(rel) {
  return rel.startsWith("richos/app/ui/") || (rel.startsWith("richos/app/") && rel.endsWith(".rs"));
}

/// The fixed content of `rel`, or null when nothing in it is this rule's business.
function fixed(rel, content) {
  rel = rel.split(path.sep).join("/");
  if (isDoc(rel)) {
    const out = OXFORD.strip(content, { quoted: true, doc: true });
    return out === content ? null : out;
  }
  if (!couldBeUi(rel)) return null;
  const abs = path.join(REPO, rel);
  const kind = ui().kindOf(abs);
  if (!kind) return null;
  const out = ui().fix(kind, abs, content);
  return out === content ? null : out;
}

function sites(rel, content) {
  rel = rel.split(path.sep).join("/");
  if (isDoc(rel)) {
    return OXFORD.commaOffsets(content, { quoted: true, doc: true }).map(
      (o) => rel + ":" + content.slice(0, o).split("\n").length + "  " +
        content.slice(Math.max(0, o - 50), o + 30).replace(/\s+/g, " ")
    );
  }
  if (!couldBeUi(rel)) return [];
  const abs = path.join(REPO, rel);
  const kind = ui().kindOf(abs);
  return kind ? ui().report(kind, abs, content) : [];
}

function relOf(p) {
  return path.relative(REPO, path.resolve(process.cwd(), p)).split(path.sep).join("/");
}

function main(argv) {
  const mode = argv[0];
  if (mode === "--json") {
    const input = JSON.parse(fs.readFileSync(0, "utf8") || "{}");
    const files = [];
    for (const f of input.files || []) {
      const out = fixed(f.path, f.content);
      if (out !== null) files.push({ path: f.path, content: out });
    }
    process.stdout.write(JSON.stringify({ files }) + "\n");
    return 0;
  }
  if (mode === "--write" || mode === "--check") {
    let found = 0;
    for (const p of argv.slice(1)) {
      const rel = relOf(p);
      const abs = path.join(REPO, rel);
      const content = fs.readFileSync(abs, "utf8");
      if (mode === "--check") {
        const hits = sites(rel, content);
        found += hits.length;
        for (const h of hits) console.log(h);
        continue;
      }
      const out = fixed(rel, content);
      if (out !== null) {
        fs.writeFileSync(abs, out);
        found += 1;
        console.log("cleaned " + rel);
      }
    }
    return mode === "--check" && found ? 1 : 0;
  }
  console.error("usage: oxford-comma.js --json | --write PATH... | --check PATH...");
  return 2;
}

module.exports = { isDoc, fixed, sites };

if (require.main === module) process.exit(main(process.argv.slice(2)));
