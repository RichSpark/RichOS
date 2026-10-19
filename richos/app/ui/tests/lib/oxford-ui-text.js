// THE APP'S OWN TEXT, WITHOUT THE OXFORD COMMA (CEO, 2026-10-19): "in the UI as well as any of
// our docs etc, the Oxford comma must be always off".
//
// The UI text here is exactly the text the m-dash check in `dialect.js` reads, so the two rules
// can never disagree about what UI text is:
//
//   1. every string literal, text node and readable attribute (placeholder, title, aria-label)
//      in every shipped UI file `lib/ui-sources.js` derives; HTML comments inside a template
//      literal are code comments and are skipped;
//   2. the CEO-facing Rust scrape (`rustCeoLiterals`, the per-file form of `rustStrings()`) plus
//      every sentence-shaped Rust literal outside tests, minus log, print, assert and panic lines
//      and model prompts (`rustSentenceLiterals`, below).
//
// What counts as the comma, and what is never touched inside that text (inline code, quoted
// material), is `ui/oxford-comma.js`: the one rule the display filter and the docs fixer use too.
//
// Two users: `dialect.js` (the check: nothing may be left) and `app/scripts/oxford-comma.js`
// (the fixer git runs at every commit, which deletes them from the files a commit changes).

"use strict";

const path = require("path");
const STRINGS = require("./state-strings");
const OXFORD = require("../../oxford-comma.js");

const UI_DIR = STRINGS.UI_DIR;
const REPO_ROOT = path.resolve(UI_DIR, "..", "..", "..");
const OPTS = { quoted: true };

const RUST_MACHINE_LINE = /\b(eprintln|println|print|panic|assert|assert_eq|assert_ne|debug_assert|unreachable|todo|unimplemented|debug|info|warn|trace)!|\.expect\(/;
const PROMPT_HEADER = /^[A-Z]{4,}\b|\b[A-Z][A-Z']{2,}\b[ :]+[A-Z][A-Z']{1,}\b/;

/// Sentence-shaped Rust literals outside test modules, minus log/assert lines and prompts: a
/// capital first letter, sentence punctuation last, four words or more. The m-dash check's net,
/// shared so both rules read the same set.
function rustSentenceLiterals(src, withOffsets) {
  const lines = src.split("\n");
  const tests = STRINGS.testModuleRanges(lines);
  const out = [];
  for (const lit of STRINGS.rustStringLiterals(src, withOffsets)) {
    const idx = lit.line - 1;
    if (tests.some(([a, b]) => idx >= a && idx <= b)) continue;
    const t = lit.text.trim();
    if (RUST_MACHINE_LINE.test([lines[idx - 2] || "", lines[idx - 1] || "", lines[idx]].join("\n"))) continue;
    if (!(/^[A-Z]/.test(t) && /[.?!]["”)]?$/.test(t) && t.split(/\s+/).length >= 4)) continue;
    if (PROMPT_HEADER.test(t)) continue;
    out.push(lit);
  }
  return out;
}

/// Which kind of UI text file `abs` is: "html" or "js" (a shipped UI source), "rs" (a Rust
/// source the scrape walks), or null when it holds no UI text.
function kindOf(abs) {
  const rel = path.relative(UI_DIR, abs);
  if (!rel.startsWith("..") && !path.isAbsolute(rel)) {
    const name = rel.split(path.sep).join("/");
    if (STRINGS.uiSources().indexOf(name) < 0) return null;
    return name.endsWith(".html") ? "html" : "js";
  }
  if (abs.endsWith(".rs") && STRINGS.rustSourceFiles().indexOf(abs) >= 0) return "rs";
  return null;
}

/// Every UI text file, absolute paths with their kinds.
function files() {
  const out = [];
  for (const name of STRINGS.uiSources()) {
    out.push({ abs: path.join(UI_DIR, name), kind: name.endsWith(".html") ? "html" : "js" });
  }
  for (const abs of STRINGS.rustSourceFiles()) out.push({ abs, kind: "rs" });
  return out;
}

const lineOf = (src, offset) => src.slice(0, offset).split("\n").length;
const snippet = (src, offset) => src.slice(Math.max(0, offset - 50), offset + 30).replace(/\s+/g, " ");

/// The offsets in `src` of every comma this rule deletes from its UI text, ascending.
function commaSites(kind, abs, src) {
  const found = new Set();
  const fromLiteral = (lit, skip) => {
    for (const i of OXFORD.commaOffsets(lit.text, OPTS)) {
      if (skip && skip(i)) continue;
      const at = lit.offsets[i];
      if (src[at] === ",") found.add(at);
    }
  };
  if (kind === "html") {
    for (const s of STRINGS.htmlVisibleStrings(src)) {
      for (const i of OXFORD.commaOffsets(s.raw, OPTS)) {
        if (src[s.start + i] === ",") found.add(s.start + i);
      }
    }
  } else if (kind === "js") {
    for (const lit of STRINGS.jsStringLiterals(src, true)) {
      // An HTML comment inside a template literal is a code comment, not UI text.
      const comments = [];
      const re = /<!--[\s\S]*?-->/g;
      let m;
      while ((m = re.exec(lit.text)) !== null) comments.push([m.index, m.index + m[0].length]);
      fromLiteral(lit, (i) => comments.some(([a, b]) => i >= a && i < b));
    }
  } else if (kind === "rs") {
    for (const lit of STRINGS.rustCeoLiterals(abs, src, true)) fromLiteral(lit);
    for (const lit of rustSentenceLiterals(src, true)) fromLiteral(lit);
  }
  return Array.from(found).sort((a, b) => a - b);
}

/// `src` with those commas deleted.
function fix(kind, abs, src) {
  const at = commaSites(kind, abs, src);
  let out = src;
  for (let k = at.length - 1; k >= 0; k--) out = out.slice(0, at[k]) + out.slice(at[k] + 1);
  return out;
}

/// What the check reports: `file:line  snippet` for every comma left in UI text.
function report(kind, abs, src) {
  const rel = path.relative(REPO_ROOT, abs);
  return commaSites(kind, abs, src).map((o) => rel + ":" + lineOf(src, o) + "  " + snippet(src, o));
}

module.exports = { rustSentenceLiterals, kindOf, files, commaSites, fix, report, REPO_ROOT };
