// AMERICAN ENGLISH ON THE SIGN-IN PATH — a scanner over the two files that shipped a British
// spelling on text a person reads: `provider_auth.rs`'s `AuthState::Cancelled` message and
// `main.js`'s cancel-failure catch clause (nightly QA audit
// `docs/verification/2026-09-17-nightly-1.2.0-20260917.1-silent-audit.md` §D1, richos
// `5f448983`). CEO ruling, 2026-08-29: American English is the language of every string a
// person reads.
//
// SCOPE IS DELIBERATELY NARROW, not a blanket grep for the British spelling over either whole
// file. `AuthState::Cancelled` is a Rust enum variant identifier, and its
// `#[serde(rename_all = "kebab-case")]` wire value is the ACP-style protocol value `mock.js`
// matches against (`providerState = "cancelled"`) — neither is prose a person reads, and the
// dialect rule never binds an identifier or a wire value. So this scan reads ONLY the two
// literals that hold what the human sees, each pulled out by the shape of the call that
// carries it rather than by the word itself, and checks that literal alone.
//
// EVERY CHECK CARRIES A POSITIVE CONTROL, for the reason `voice-model.js` states: a "nothing
// found" assertion passes identically when the scanner works and when it is not looking at
// anything at all. Each check re-runs its own extractor against a string that plants the
// forbidden British spelling in the exact shape it targets, and requires the extractor to
// both find the literal and see the forbidden word in it — proving the clean result above it
// is not vacuous. Those fixtures are never written to disk, and each carries its own
// `dialect-exempt:` marker: the word inside them is the thing under test, not a claim about
// what this file itself says.
//
// No browser: this is a plain node script, in the shape `docs-claims.js` already uses. `run.js`
// runs every .js file in this directory, so it is registered by existing, and its row in this
// README keeps `docs-claims.js`'s own table check green.

// NO M-DASH OR N-DASH IN ANY UI TEXT (CEO feedback 2026-10-06, item 3: "No UI text should
// contain any m-dashes or n-dashes"). The last check below reads, every run:
//
//   1. every string literal, text node and readable attribute (placeholder, title,
//      aria-label) in every shipped UI file, derived by `lib/ui-sources.js` from the tree.
//      Not only the prose `state-strings.js` inventories: a lone dash used as a label is UI
//      text too. HTML comments inside a template literal are dropped, because code comments
//      are not UI text; so are `${...}` holes, which are code.
//   2. the Rust sentences the UI relays: the CEO-facing scrape (`rustStrings()`, the same set
//      the affordance rule classifies) plus every sentence-shaped literal (a capital first
//      letter, sentence punctuation last, four words or more) outside tests and examples.
//
// WHAT IT DOES NOT READ, NAMED: `mock.js` and `home/field-data.js` (browser-preview sample
// data, blind spot B4); a Rust literal on a log, print, assert or panic line (a terminal, not
// the screen); a model prompt whose text opens with an all-caps header word or carries an
// all-caps run of two words ("ONBOARDING", "COMPANY MEMORY"), which Rich reads and the CEO
// does not; lowercase Rust error text relayed through `String(e)` (blind spot B5, an
// unbounded set). And it cannot see what Rich himself writes in a reply: that is generated at
// run time, not shipped text.
//
// NO OXFORD COMMA IN ANY UI TEXT (CEO, 2026-10-19: "in the UI as well as any of our docs etc, the
// Oxford comma must be always off"). The check after the dash check reads the same UI text and
// fails on any ", and", ", or" or ", nor" left in it. Git deletes them at every commit
// (`app/scripts/oxford-comma.js`), so this is the backstop for a commit made without the hook.

"use strict";

const fs = require("fs");
const path = require("path");
const { createRun, assert, assertEqual, rustSentenceAfter, UI_DIR } = require("./lib/harness");
const STRINGS = require("./lib/state-strings");
const OXFORD_FIX = require("../../scripts/oxford-comma.js");

const APP_DIR = path.resolve(UI_DIR, "..");
const PROVIDER_AUTH_RS = path.join(APP_DIR, "crates", "richos-core", "src", "provider_auth.rs");
const MAIN_JS = path.join(UI_DIR, "main.js");

// The word this whole suite exists to forbid, built at runtime rather than written literally
// here, so THIS declaration is never itself a hit for the very pattern it defines.
const FORBIDDEN = "cance" + "lled";
const BRITISH = new RegExp("\\b" + FORBIDDEN + "\\b", "i");

/// The message `AuthView::new` sets for `AuthState::Cancelled`. Read the same way
/// `lib/harness.js` reads any other CEO-facing Rust sentence: backward from a marker inside
/// the literal to its opening quote, so code after the literal's closing quote is never
/// mistaken for part of the sentence.
function providerAuthCancelledMessage(src) {
  return rustSentenceAfter(src, "Sign-in was");
}

/// `main.js`'s catch clause for a failed `provider_auth_cancel` round trip. Anchored on the
/// surrounding call shape — `renderProviderAuth({state: "connecting", message: "…"})` — which
/// is stable regardless of which word the sentence uses, so this finds the literal whether it
/// currently reads the American spelling or (as a regression, or in the positive control
/// below) the British one.
function mainJsCancelFailureMessage(src) {
  const m = src.match(/renderProviderAuth\(\{state: "connecting", message: "([^"]*)"\}\)/);
  return m ? m[1] : null;
}

async function main() {
  const run = createRun("American English on the sign-in path");

  await run.check("provider_auth.rs's AuthState::Cancelled message is American, and the scan is not vacuous", async () => {
    const src = fs.readFileSync(PROVIDER_AUTH_RS, "utf8");
    const msg = providerAuthCancelledMessage(src);
    assert(!BRITISH.test(msg), `provider_auth.rs still says "${msg}"`);

    // Positive control: the same extractor, over a fixture that plants the British spelling
    // in the identical literal shape — never written to disk.
    const planted =
      'AuthState::Cancelled => "Sign-in was ' + FORBIDDEN + // dialect-exempt: planted fixture is the word under test, not this file's own prose
      '. You can try again when you are ready.",';
    const plantedMsg = providerAuthCancelledMessage(planted);
    assert(BRITISH.test(plantedMsg), "positive control failed: the extractor did not see a planted British spelling");
    return `"${msg}"`;
  });

  await run.check("main.js's cancel-failure message is American, and the scan is not vacuous", async () => {
    const src = fs.readFileSync(MAIN_JS, "utf8");
    const msg = mainJsCancelFailureMessage(src);
    assert(msg, "the cancel-failure catch clause's message literal was not found at all");
    assert(!BRITISH.test(msg), `main.js still says "${msg}"`);

    // Positive control: the same extractor, over a fixture that plants the British spelling
    // in the identical call shape — never written to disk.
    const planted =
      'catch (_) { renderProviderAuth({state: "connecting", message: "Sign-in could not be ' + FORBIDDEN + // dialect-exempt: planted fixture is the word under test, not this file's own prose
      '. Try again."}); }';
    const plantedMsg = mainJsCancelFailureMessage(planted);
    assert(plantedMsg, "positive control failed: the extractor did not find the planted literal at all");
    assert(BRITISH.test(plantedMsg), "positive control failed: the extractor found the planted literal but missed its British spelling");
    return `"${msg}"`;
  });

  await run.check("no UI text holds an m-dash or an n-dash (CEO 2026-10-06 item 3), and the scan is not vacuous", async () => {
    const hits = dashedUiText();
    assertEqual(hits, [], "UI text with an m-dash or n-dash; rewrite each with a comma, colon, period or parentheses");

    // Positive controls, never written to disk: each extractor, over a planted dash in the
    // exact shape it reads, must see it. And the two negative controls: a dash in a code
    // comment and in an HTML comment inside a template literal is NOT UI text.
    const D = String.fromCharCode(0x2014), N = String.fromCharCode(0x2013);
    const js = 'const a = "Saved ' + D + ' all good"; // a comment ' + D + " is fine\n" +
      "const b = `<p>Range 1" + N + "4</p><!-- a comment " + D + " is fine -->`;";
    const jsHits = jsDashes(js, "planted.js");
    assertEqual(jsHits.length, 2, "positive control: the JS extractor saw " + JSON.stringify(jsHits));
    const htmlHits = htmlDashes('<p title="a ' + D + ' b">Text ' + N + " here</p><!-- " + D + " -->", "planted.html");
    assertEqual(htmlHits.length, 2, "positive control: the HTML extractor saw " + JSON.stringify(htmlHits));
    const rs = 'const S: &str = "I could not do that ' + D + ' try again later.";\n\n\n' +
      'fn f() {\n    eprintln!("Log line ' + D + ' this one is for a terminal.");\n}\n\n\n' +
      'const P: &str = "ONBOARDING ' + D + ' you have nothing on file about this.";';
    const rsHits = rustSentenceDashes(rs, "planted.rs");
    assertEqual(rsHits.length, 1, "positive control: the Rust sentence net saw " + JSON.stringify(rsHits));
    return "UI files, the CEO-facing Rust scrape and sentence-shaped Rust literals hold no dash";
  });

  await run.check("no UI text holds a comma before and/or/nor (CEO 2026-10-19), and the scan is not vacuous", async () => {
    const repoRoot = path.resolve(APP_DIR, "..", "..");
    const hits = [];
    for (const abs of OXFORD_FIX.uiFiles()) {
      const src = fs.readFileSync(abs, "utf8");
      for (const o of OXFORD_FIX.uiCommas(OXFORD_FIX.uiKind(abs), abs, src)) {
        hits.push(path.relative(repoRoot, abs) + ":" + src.slice(0, o).split("\n").length + "  " + src.slice(Math.max(0, o - 40), o + 20).replace(/\s+/g, " "));
      }
    }
    assertEqual(hits, [], "UI text with ', and', ', or' or ', nor'; git deletes it at commit (app/scripts/oxford-comma.js --write <file> does it by hand)");

    // Positive control, never written to disk: a planted comma in a string is deleted; the one
    // in a code comment is not UI text and stays.
    const js = 'const a = "Red, green, and blue."; // a comment, and fine\n';
    assertEqual(OXFORD_FIX.fixed("richos/app/ui/main.js", js), 'const a = "Red, green and blue."; // a comment, and fine\n', "the fixer");
    return "UI files, the CEO-facing Rust scrape and sentence-shaped Rust literals hold no comma before and/or/nor";
  });

  const failed = run.report();
  return failed;
}

// ---- the dash scan (item 3) -----------------------------------------------------------

const DASH = /[–—]/;
const snippet = (t) => {
  const i = t.search(DASH);
  return t.slice(Math.max(0, i - 40), i + 40).replace(/\s+/g, " ");
};

function jsDashes(src, name) {
  const out = [];
  for (const lit of STRINGS.jsStringLiterals(src)) {
    const text = lit.text.replace(/<!--[\s\S]*?-->/g, " ");
    if (DASH.test(text)) out.push(name + ":" + lit.line + "  " + snippet(text));
  }
  return out;
}

function htmlDashes(src, name) {
  return STRINGS.htmlVisibleStrings(src)
    .filter((s) => DASH.test(s.text))
    .map((s) => name + ":" + s.line + "  " + snippet(s.text));
}

/// Sentence-shaped Rust literals outside test modules, minus log/assert lines and prompts
/// (`lib/state-strings.js`, shared with the Oxford comma fixer).
function rustSentenceDashes(src, name) {
  return STRINGS.rustSentenceLiterals(src)
    .filter((lit) => DASH.test(lit.text.trim()))
    .map((lit) => name + ":" + lit.line + "  " + snippet(lit.text.trim()));
}

function dashedUiText() {
  const hits = [];
  for (const name of STRINGS.uiSources()) {
    const src = fs.readFileSync(path.join(UI_DIR, name), "utf8");
    hits.push(...(name.endsWith(".html") ? htmlDashes(src, name) : jsDashes(src, name)));
  }
  for (const r of STRINGS.rustStrings()) if (DASH.test(r.text)) hits.push(r.file + ":" + r.line + "  " + snippet(r.text));
  const repoRoot = path.resolve(UI_DIR, "..", "..", "..");
  for (const f of STRINGS.rustSourceFiles()) {
    for (const h of rustSentenceDashes(fs.readFileSync(f, "utf8"), path.relative(repoRoot, f))) {
      if (!hits.includes(h)) hits.push(h);
    }
  }
  return hits;
}

main().then((f) => { process.exitCode = f ? 1 : 0; });
