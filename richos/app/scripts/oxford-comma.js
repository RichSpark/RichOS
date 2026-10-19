#!/usr/bin/env node
// oxford-comma.js: the commit-time fixer (CEO, 2026-10-19): "in the UI as well as any of our docs
// etc, the Oxford comma must be always off" and a changed doc is cleaned whole. autocheck.py runs
// it on the files a commit adds or changes. The rule is `ui/oxford-comma.js` (code is skipped).
//
//   node oxford-comma.js --json           stdin {"files":[{"path","content"}]}, stdout the changed ones
//   node oxford-comma.js --write PATH...  fix those files in place
//
// A doc is a `.md` file outside fixtures and captured or third-party material; UI text is the
// text the m-dash check in `ui/tests/dialect.js` reads (string literals, text nodes and readable
// attributes of the shipped UI, and the user-facing Rust literals), never code or comments.

"use strict";

const fs = require("fs");
const path = require("path");

const APP = path.resolve(__dirname, "..");
const REPO = path.resolve(APP, "..", "..");
const UI_DIR = path.join(APP, "ui");
const OXFORD = require(path.join(UI_DIR, "oxford-comma.js"));
const NOT_DOCS = /(^|\/)(fixtures?|testdata|__fixtures__|__snapshots__|third[_-]party|vendor|node_modules|raw|cold-open|transcripts)\//;

const strings = () => require(path.join(UI_DIR, "tests", "lib", "state-strings.js"));

function isDoc(rel) {
  return /\.md$/i.test(rel) && !NOT_DOCS.test(rel);
}

/// "html", "js" or "rs" for a file holding UI text, else null.
function uiKind(abs) {
  const rel = path.relative(UI_DIR, abs).split(path.sep).join("/");
  if (!rel.startsWith("..")) {
    if (strings().uiSources().indexOf(rel) < 0) return null;
    return rel.endsWith(".html") ? "html" : "js";
  }
  return abs.endsWith(".rs") && strings().rustSourceFiles().indexOf(abs) >= 0 ? "rs" : null;
}

/// Every UI text file, for the check.
function uiFiles() {
  return strings().uiSources().map((n) => path.join(UI_DIR, n)).concat(strings().rustSourceFiles());
}

/// The offsets in `src` of every comma the rule deletes from its UI text.
function uiCommas(kind, abs, src) {
  const S = strings();
  let texts;
  if (kind === "html") {
    texts = S.htmlVisibleStrings(src).map((s) => ({ text: s.raw, at: (i) => s.start + i }));
  } else {
    const lits = kind === "js" ? S.jsStringLiterals(src, true)
      : S.rustCeoLiterals(abs, src, true).concat(S.rustSentenceLiterals(src, true));
    texts = lits.map((l) => ({ text: l.text, at: (i) => l.offsets[i] }));
  }
  const found = new Set();
  for (const t of texts) {
    const kept = OXFORD.strip(t.text);
    for (let i = 0, j = 0; i < t.text.length; i++) {
      if (t.text[i] === kept[j]) j++;
      else if (src[t.at(i)] === ",") found.add(t.at(i));
    }
  }
  return Array.from(found).sort((a, b) => a - b);
}

/// The fixed content of `rel`, or null when the rule changes nothing in it.
function fixed(rel, content) {
  rel = rel.split(path.sep).join("/");
  let out = content;
  if (isDoc(rel)) {
    out = OXFORD.strip(content);
  } else {
    const abs = path.join(REPO, rel);
    const kind = rel.startsWith("richos/app/") ? uiKind(abs) : null;
    if (kind) {
      const at = uiCommas(kind, abs, content);
      for (let k = at.length - 1; k >= 0; k--) out = out.slice(0, at[k]) + out.slice(at[k] + 1);
    }
  }
  return out === content ? null : out;
}

function main(argv) {
  if (argv[0] === "--json") {
    const input = JSON.parse(fs.readFileSync(0, "utf8") || "{}");
    const files = [];
    for (const f of input.files || []) {
      const out = fixed(f.path, f.content);
      if (out !== null) files.push({ path: f.path, content: out });
    }
    process.stdout.write(JSON.stringify({ files }) + "\n");
    return 0;
  }
  if (argv[0] === "--write") {
    for (const p of argv.slice(1)) {
      const rel = path.relative(REPO, path.resolve(p)).split(path.sep).join("/");
      const out = fixed(rel, fs.readFileSync(path.join(REPO, rel), "utf8"));
      if (out !== null) {
        fs.writeFileSync(path.join(REPO, rel), out);
        console.log("cleaned " + rel);
      }
    }
    return 0;
  }
  console.error("usage: oxford-comma.js --json | --write PATH...");
  return 2;
}

module.exports = { isDoc, fixed, uiKind, uiFiles, uiCommas };

if (require.main === module) process.exit(main(process.argv.slice(2)));
