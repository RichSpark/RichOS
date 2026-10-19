// THE DICTATION FEATURE IS CALLED RICHSCRIBE. The CEO, 2026-10-19: "The dictation feature i.e.
// the RichOS replacement for Wispr Flow gets renamed to RichScribe. So, the corresponding places
// in the UI need to be updated accordingly."
//
// WHAT IT READS, every run, with the same readers `dialect.js` uses (`lib/state-strings.js`):
//
//   1. every string literal in every shipped UI script, INCLUDING the ones written inside a
//      template's `${...}` holes (`jsStringLiterals(src, { holes: true })`: "<b>F2</b> starts
//      and stops dictation." was one of those), every text node, `<title>` and readable
//      attribute (placeholder, title, aria-label) in every shipped HTML file, and every string a
//      stylesheet draws with `content:`;
//   2. the Rust strings that reach a person: the CEO-facing scrape (`rustStrings()`, the
//      `Err(...)` of a window command and every `ceo_message()`), plus every Rust literal that
//      opens with a capital letter (a label, a title, a tooltip, a sentence: the tray item's
//      tooltip, the "Dictation feature" conversation and Rich's message in it, the reason macOS
//      is given for the key watch).
//
// WHAT COUNTS AS THE NAME: the word "dictation" standing on its own. Joined to code by `-`, `_`,
// `.`, `/`, `:` or `@` it is an identifier, a file, an event or a wire value
// (`dictation_status`, `set-dictation-open`, `dictation.json`, `rich://dictation`), and those stay
// as they are. The verb ("Dictate in any app", "only while you dictate") describes the action,
// not the feature, and is not matched.
//
// WHAT IT DOES NOT READ, NAMED: a JS literal on a `console.` line and a Rust literal on a log,
// print, assert or panic line (`eprintln!`, `log::line`, and the rest of `dialect.js`'s list) or
// opening with a `[tag]`: those go to a terminal or a log file, not the screen. A lowercase Rust
// literal outside the CEO-facing scrape (a log reason built with `format!`, the provenance line
// of a transcript): the same terminal. `mock.js` and `home/field-data.js` are preview data, as
// everywhere else (blind spot B4); the preview's own copy of Rich's message is kept in step by
// `setup.js` case 26, which reads it on screen.
//
// ONE DECLARED EXEMPTION SHAPE: a literal on a line carrying `richscribe-exempt: <reason>`. There
// is one: `main.js` still recognizes the "Dictation feature" conversation an install made before
// the rename, so its first-run notice stays hidden there. That title is stored data the app
// matches, never text it draws. The check lists every exemption it honored, so one cannot hide.
//
// EVERY CHECK CARRIES A POSITIVE CONTROL, for the reason `dialect.js` gives: a "nothing found"
// passes the same way when the scan works and when it reads nothing.
//
// No browser: a plain node script. `run.js` runs every .js file in this directory.

"use strict";

const fs = require("fs");
const path = require("path");
const { createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");
const STRINGS = require("./lib/state-strings");
const SOURCES = require("./lib/ui-sources");

const REPO_ROOT = path.resolve(UI_DIR, "..", "..", "..");
const NEW_NAME = "RichScribe";

/// The feature's old name standing on its own. A neighbor that joins it to code (`-`, `_`, `.`
/// followed by more name, `/`, `:`, `@`) makes it an identifier; a sentence's closing period does not.
const OLD_NAME = /(^|[^A-Za-z0-9_\-./:@])dictations?(?![A-Za-z0-9_\-/:@]|\.[A-Za-z0-9])/i;
const EXEMPT = /richscribe-exempt:\s*\S/;

const JS_LOG_LINE = /\bconsole\.(log|warn|error|info|debug)\s*\(/;
const MATCH_ARM = /\bErr\(\s*\w+\s*\)\s*=>/;
const RUST_MACHINE_LINE =/\b(eprintln|println|print|panic|assert|assert_eq|assert_ne|debug_assert|unreachable|todo|unimplemented|debug|info|warn|trace)!|\.expect\(|\blog::line\(/;

const snippet = (t) => {
  const s = String(t).replace(/\s+/g, " ");
  const i = s.search(OLD_NAME);
  return JSON.stringify(s.slice(Math.max(0, i - 40), i + 50));
};

// ---- the readers -------------------------------------------------------------------------

function jsHits(src, name, exempted) {
  const lines = src.split("\n");
  const out = [];
  for (const lit of STRINGS.jsStringLiterals(src, { holes: true })) {
    if (!OLD_NAME.test(lit.text)) continue;
    const own = lines[lit.line - 1] || "";
    if (JS_LOG_LINE.test(own)) continue;
    if (EXEMPT.test(own)) { exempted.push(name + ":" + lit.line); continue; }
    out.push(name + ":" + lit.line + "  " + snippet(lit.text));
  }
  return out;
}

function htmlHits(src, name) {
  return STRINGS.htmlVisibleStrings(src)
    .filter((s) => OLD_NAME.test(s.text))
    .map((s) => name + ":" + s.line + "  " + snippet(s.text));
}

/// A Rust literal a person can read: in the CEO-facing scrape, or opening with a capital letter,
/// and in neither case on a log line, in a test module or opening with a `[tag]`.
function rustHits(src, rel, ceoSites) {
  const lines = src.split("\n");
  const tests = STRINGS.testModuleRanges(lines);
  const out = [];
  for (const lit of STRINGS.rustStringLiterals(src)) {
    const idx = lit.line - 1;
    if (tests.some(([a, b]) => idx >= a && idx <= b)) continue;
    const t = lit.text.trim();
    if (!OLD_NAME.test(t) || t.startsWith("[")) continue;
    if (RUST_MACHINE_LINE.test([lines[idx - 2] || "", lines[idx - 1] || "", lines[idx]].join("\n"))) continue;
    // The scrape reads any `Err(` near a literal as a returned error, and a match ARM is not
    // one: `Err(e) => format!("…")` on the literal's own line builds a log line from a failure.
    const ceo = ceoSites.has(rel + ":" + lit.line) && !MATCH_ARM.test(lines[idx]);
    if (!(/^[A-Z]/.test(t) || ceo)) continue;
    out.push(rel + ":" + lit.line + "  " + snippet(t));
  }
  return out;
}

function oldNameInUi() {
  const hits = [];
  const exempted = [];
  for (const name of STRINGS.uiSources()) {
    const src = fs.readFileSync(path.join(UI_DIR, name), "utf8");
    hits.push(...(name.endsWith(".html") ? htmlHits(src, name) : jsHits(src, name, exempted)));
  }
  for (const c of SOURCES.cssContentStrings()) if (OLD_NAME.test(c.text)) hits.push(c.site + "  " + snippet(c.text));
  const ceoSites = new Set(STRINGS.rustStrings().map((r) => r.file + ":" + r.line));
  for (const f of STRINGS.rustSourceFiles()) {
    hits.push(...rustHits(fs.readFileSync(f, "utf8"), path.relative(REPO_ROOT, f), ceoSites));
  }
  return { hits, exempted };
}

async function main() {
  const run = createRun("The dictation feature is named RichScribe");

  await run.check("no text a person reads names the feature \"Dictation\", and the scan is not vacuous", async () => {
    // Positive controls FIRST, never written to disk, so a red run still proves the readers work.
    // Each reader, over the old name planted in the exact shape it reads, must see it; over an
    // identifier, a log line or the verb, must not.
    const old = "Dict" + "ation", low = "dict" + "ation";
    const js =
      'const a = "Turn ' + low + ' on";\n' +
      "const b = `<b>${k}</b> ${k === 1 ? \"x\" : `starts and stops " + low + ".`}`;\n" +
      'invoke("' + low + '_status"); listen("rich://' + low + '"); el("set-' + low + '-open"); f("' + low + '.json");\n' +
      'console.warn("[' + low + '] " + e);\n' +
      'const c = "Dictate in any app, only while you dictate.";\n' +
      'const d = "' + old + ' feature"; // richscribe-exempt: planted control\n';
    const ex = [];
    const jsSeen = jsHits(js, "planted.js", ex);
    assertEqual(jsSeen.length, 2, "positive control: the JS reader saw " + JSON.stringify(jsSeen));
    assertEqual(ex.length, 1, "positive control: the declared exemption was not honored");
    const htmlSeen = htmlHits('<title>RichOS ' + low + '</title><div role="menu" aria-label="' + old + '"></div><p>Dictate here</p>', "planted.html");
    assertEqual(htmlSeen.length, 2, "positive control: the HTML reader saw " + JSON.stringify(htmlSeen));
    const rs =
      '        .tooltip("' + old + '")\n\n\n' +
      'fn f() {\n    log::line("Turn ' + low + ' off chosen in the menu bar");\n}\n\n\n' +
      'fn g() {\n    eprintln!("[richos] ' + low + ': the tool started");\n}\n\n\n' +
      'fn h() -> String {\n    format!("whisper-cli is tried at the first ' + low + '")\n}\n\n\n' +
      'fn i() -> Result<(), String> {\n    Err("a ' + low + ' is still listening".into())\n}\n\n\n' +
      'fn j(r: Result<(), String>) -> String {\n    match r {\n        Err(e) => format!("tried at the first ' + low + ' ({e})"),\n        Ok(()) => String::new(),\n    }\n}\n';
    const rsSeen = rustHits(rs, "planted.rs", new Set(["planted.rs:20", "planted.rs:26"]));
    assertEqual(rsSeen.length, 2, "positive control: the Rust reader saw " + JSON.stringify(rsSeen));

    const { hits, exempted } = oldNameInUi();
    assertEqual(hits, [], "text that still names the feature by its old name; it is " + NEW_NAME);
    assert(exempted.length <= 1, "more than the one declared exemption: " + exempted.join(", "));
    return "UI scripts, HTML, stylesheets and the Rust a person reads name it " + NEW_NAME +
      (exempted.length ? " (declared exemption: " + exempted.join(", ") + ")" : "");
  });

  await run.check("the new name is where the old one was: Settings, the sheet, the menu bar and the Mac shell", async () => {
    const where = [
      ["settings-button.js", '"' + NEW_NAME + '"'],
      ["dictation.js", ">" + NEW_NAME + "</h2>"],
      ["dictation-menu.js", NEW_NAME + " settings"],
      ["dictation-menu.html", 'aria-label="' + NEW_NAME + '"'],
      ["dictation-bar.js", NEW_NAME + " is off."],
      ["main.js", "Turn on " + NEW_NAME],
      ["../src-tauri/src/dictation/menubar.rs", '.tooltip("' + NEW_NAME + '")'],
      ["../src-tauri/src/dictation/keytap.rs", NEW_NAME + " listens for its key"],
      ["../src-tauri/src/dictation_offer.rs", '"' + NEW_NAME + ' feature"'],
    ];
    const missing = where.filter(([f, s]) => !fs.readFileSync(path.join(UI_DIR, f), "utf8").includes(s)).map(([f, s]) => f + " lacks " + s);
    assertEqual(missing, [], "the new name is missing");
    return where.length + " places carry " + NEW_NAME;
  });

  return run.report();
}

main().then((f) => process.exit(f ? 1 : 0));
