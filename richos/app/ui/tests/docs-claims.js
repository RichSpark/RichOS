// SLICE 9, second half — THE DOCUMENTS, CHECKED AGAINST THE TREE THEY DESCRIBE.
//
// Slice 10 of §24 is "docs: update streaming and UI contracts". Writing them true once is
// easy; the failure is what happens afterwards. Measured on the tree this suite was written
// against, before slice 10 corrected them:
//
//     app/README.md said   `cargo test -p richos-core   # 97/97 green`     ACTUAL 304 + 2
//     app/README.md said   `cargo test -p richos-voice  # 121/121 green`   ACTUAL 163
//     app/README.md said   rotation_tests.rs is 12                          ACTUAL 22
//                          machinery_tests.rs is 14                         ACTUAL 15
//                          steering_tests.rs is 11                          ACTUAL 16
//                          timeline_tests.rs is 7                           ACTUAL 12
//     app/README.md named  9 of richos-core's 12 test files; three whole suites — 40 of
//                          its 153 integration tests — appeared in no document at all
//     app/STREAMING.md     documents 12 of the 16 `rich://` event names the app DECLARES.
//                          `rich://proactive-message` — Rich speaking unprompted, which is
//                          a product headline — is declared in stream.rs beside the four
//                          the document calls "the whole contract the UI needs".
//
// Every one of those was true prose on the day it was written. A number in a document is a
// claim with a shelf life, and nothing was checking any of them, which is exactly the shape
// `engine/scripts/publication-completeness.sh` names as beyond its own reach: "SEMANTIC
// honesty. Every path in a document can resolve while the sentence around it is false."
//
// So this suite joins the kinds of claim in `app/`'s documents to the tree:
//
//   1. test files named            against the test files on disk (no counts: a stated count
//                                  made every branch that adds a test conflict on the README)
//   2. the browser-suite table     against the inventory `run.js` discovers from disk
//   3. `rich://` event names       against the constants the Rust source declares
//
// and, last, holds the land's selector to this suite's own reads: every file and directory
// read above must select this suite through its row in scripts/proof-for.ui-inputs, or a
// change there could make a claim false with nothing running it (2026-09-29).
//
// NOTHING HERE IS TYPED. There is no list of expected files, expected counts or expected
// events anywhere below; each side of every join is read off disk. That is the whole point:
// a typed expectation is a second document to keep true, and this suite exists because the
// first one went stale.
//
// EMPTY IS A FAILURE, NOT A PASS. Every inventory is asserted non-empty before it is
// compared. A `#[test]` regex that stops matching, a crate directory that moves, a table
// that loses its rows — each of those makes some join trivially satisfiable, and a green
// run over nothing is the failure mode this repository has hit twice.
//
// No browser: this one is a plain node script. `run.js` runs every .js in this directory, so
// it is registered by existing.

"use strict";

const fs = require("fs");
const path = require("path");
const { createRun, assert, assertEqual, UI_DIR } = require("./lib/harness");

const APP_DIR = path.resolve(UI_DIR, "..");
const REPO_ROOT = path.resolve(APP_DIR, "..", "..");
const CRATES = path.join(APP_DIR, "crates");
const TESTS_DIR = __dirname;
const SELECTION_MAP = path.join(APP_DIR, "scripts", "proof-for.ui-inputs");

// EVERYTHING THIS SUITE READS IS RECORDED, so its last check can hold the land's selector to
// it. Every claim here is about a whole tree, and a change anywhere in that tree can make one
// false: on 2026-09-29 a Rust test file added in 6f5cfb03 made app/README.md's counts stale,
// and the land that carried it never ran this suite because its row in proof-for.ui-inputs
// named eight exact files. A directory listed is recorded with a trailing slash.
const touched = new Set();
const repoRel = (p) => path.relative(REPO_ROOT, p);
const read = (p) => {
  touched.add(repoRel(p));
  return fs.readFileSync(p, "utf8");
};
const list = (dir) => {
  touched.add(repoRel(dir) + "/");
  return fs.readdirSync(dir);
};

// ---------------------------------------------------------------------------------------
// Derivations
// ---------------------------------------------------------------------------------------

function rustFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return list(dir)
    .filter((f) => f.endsWith(".rs"))
    .sort()
    .map((f) => path.join(dir, f));
}

function crateNames() {
  return list(CRATES)
    .filter((d) => fs.existsSync(path.join(CRATES, d, "Cargo.toml")))
    .sort();
}

/// Every `pub const NAME: &str = "rich://…";` in the app's Rust source. The constants ARE
/// the wire names — `stream.rs`, `live.rs`, `machinery.rs` and `richos-voice/src/event.rs`
/// all declare theirs this way — so this is the app's own answer to "what does it emit",
/// not a guess made from grepping prose.
function declaredEventNames() {
  const roots = [path.join(APP_DIR, "src-tauri", "src")];
  for (const c of crateNames()) roots.push(path.join(CRATES, c, "src"));
  const found = new Map(); // name -> declaring file, repo-relative
  for (const root of roots) {
    for (const f of rustFiles(root)) {
      for (const m of read(f).matchAll(/pub const [A-Z_0-9]+: &str = "(rich:\/\/[a-z0-9-]+)";/g)) {
        if (!found.has(m[1])) found.set(m[1], path.relative(APP_DIR, f));
      }
    }
  }
  return found;
}

/// proof-for.ui-inputs, read the way proof-for.sh reads it: `#` to end of line is a comment,
/// `@name paths…` is an alias, anything else is `<suite> paths…`.
function parseSelectionMap(text) {
  const rows = new Map();
  const aliases = new Map();
  for (const raw of text.split("\n")) {
    const words = raw.replace(/#.*$/, "").trim().split(/\s+/).filter(Boolean);
    if (!words.length) continue;
    const [key, ...rest] = words;
    if (key.startsWith("@")) aliases.set(key, rest);
    else rows.set(key, rest);
  }
  return { rows, aliases };
}

/// A row's tokens with aliases expanded. `@allui` is computed by proof-for.sh from the tree and
/// is not reproduced here; a row that needs it cannot be checked by this function, so it
/// expands to nothing and every read it would have reached is reported, never waved through.
function expandRow(tokens, aliases, seen = new Set()) {
  const out = [];
  for (const t of tokens) {
    if (!t.startsWith("@")) out.push(t);
    else if (aliases.has(t) && !seen.has(t)) out.push(...expandRow(aliases.get(t), aliases, new Set([...seen, t])));
  }
  return out;
}

async function main() {
  const run = createRun("app/ documents vs the tree they describe — derived, never typed");

  const readme = read(path.join(APP_DIR, "README.md"));
  const streaming = read(path.join(APP_DIR, "STREAMING.md"));
  const testsReadme = read(path.join(TESTS_DIR, "README.md"));

  // =====================================================================================
  // 1. Every Rust test FILE is named in app/README.md, and every one it names exists
  // =====================================================================================

  await run.check("app/README.md names every Rust test file, and no file it names is gone", async () => {
    const crates = crateNames();
    assert(crates.length > 0, "EMPTY INVENTORY: no crates found under " + CRATES);

    const onDisk = [];
    for (const c of crates) for (const f of rustFiles(path.join(CRATES, c, "tests"))) onDisk.push(path.basename(f));
    assert(onDisk.length > 0, "EMPTY INVENTORY: no integration test files found in any crate");

    const named = new Set((readme.match(/tests\/[a-z0-9_]+\.rs/g) || []).map((s) => s.replace("tests/", "")));
    assert(named.size > 0, "EMPTY INVENTORY: the README names no test file at all");

    const missing = onDisk.filter((f) => !named.has(f)).sort();
    const phantom = [...named].filter((f) => !onDisk.includes(f)).sort();
    assertEqual(missing, [], "test files that exist and are documented nowhere in app/README.md");
    assertEqual(phantom, [], "app/README.md names test files that do not exist");
    return `${onDisk.length} test files across ${crates.length} crates, all named`;
  });

  // =====================================================================================
  // 4. The browser-suite table vs the inventory run.js discovers
  // =====================================================================================

  await run.check("app/ui/tests/README.md's table is exactly the inventory run.js discovers", async () => {
    // The SAME rule run.js applies: every .js file in this directory except run.js itself.
    // `lib/` holds shared harness code and is not a suite.
    const onDisk = list(TESTS_DIR)
      .filter((f) => f.endsWith(".js") && f !== "run.js")
      .filter((f) => fs.statSync(path.join(TESTS_DIR, f)).isFile())
      .sort();
    assert(onDisk.length > 0, "EMPTY INVENTORY: run.js would discover no suites in " + TESTS_DIR);

    const rows = new Set(
      (testsReadme.match(/^\|\s*`([a-z0-9-]+\.js)`\s*\|/gm) || []).map((s) => s.match(/`([^`]+)`/)[1])
    );
    assert(rows.size > 0, "EMPTY INVENTORY: the tests README's table has no suite rows");

    const undocumented = onDisk.filter((f) => !rows.has(f));
    const phantom = [...rows].filter((f) => !onDisk.includes(f)).sort();
    assertEqual(undocumented, [], "suites that run.js runs and the README does not describe");
    assertEqual(phantom, [], "the README describes suites that no longer exist");
    return `${onDisk.length} suites, all in the table: ${onDisk.join(", ")}`;
  });

  // =====================================================================================
  // 5. Event names: the contract document vs the constants the app declares
  // =====================================================================================

  await run.check("app/STREAMING.md documents every `rich://` event the app declares", async () => {
    const declared = declaredEventNames();
    assert(declared.size > 0, "EMPTY INVENTORY: no `rich://` event constants found in the Rust source");

    const undocumented = [...declared.keys()]
      .filter((n) => !streaming.includes(n))
      .sort()
      .map((n) => `${n} (declared in ${declared.get(n)})`);
    assertEqual(
      undocumented,
      [],
      "STREAMING.md calls itself the whole contract the UI needs, and these reach the webview without appearing in it"
    );
    return `${declared.size} declared event names, all documented`;
  });

  await run.check("app/STREAMING.md names no event the app does not have, unless it says so", async () => {
    const declared = declaredEventNames();
    // §13 lists events this runtime cannot emit; the document carries them in its own table
    // marked DEFERRED, with the reason. Those are the only names allowed to appear without a
    // constant behind them, and the exemption is READ FROM THE DOCUMENT rather than typed
    // here — so a deferred event that goes live, or a live one quietly marked deferred,
    // changes the same table this check reads.
    const deferred = new Set();
    for (const line of streaming.split("\n")) {
      if (!/DEFERRED/.test(line)) continue;
      for (const m of line.matchAll(/`(rich:\/\/[a-z0-9-]+)`/g)) deferred.add(m[1]);
    }
    // A GLOB IS NOT A NAME. The document refers to a whole sub-family as `rich://voice-*`,
    // and a bare `rich:\/\/[a-z0-9-]+` match reads that as an event called
    // `rich://voice-`, which no constant will ever back. So a token that is immediately
    // followed by `*`, or that ends in a hyphen, is a reference to a group rather than a
    // claim about one event.
    const mentioned = new Set();
    for (const m of streaming.matchAll(/rich:\/\/[a-z0-9-]+/g)) {
      const next = streaming[m.index + m[0].length];
      if (next === "*" || m[0].endsWith("-")) continue;
      mentioned.add(m[0]);
    }
    assert(mentioned.size > 0, "EMPTY INVENTORY: STREAMING.md mentions no event names at all");

    const unbacked = [...mentioned].filter((n) => !declared.has(n) && !deferred.has(n)).sort();
    assertEqual(unbacked, [], "STREAMING.md documents events with no constant behind them and no DEFERRED row");
    return `${mentioned.size} names in the document: ${mentioned.size - deferred.size} backed by a constant, ${deferred.size} declared DEFERRED`;
  });

  // =====================================================================================
  // 7. LAST, because it checks the reads of everything above: the land selects this suite
  //    for every file it read. `proof-for.sh` picks the suites a land runs from this suite's
  //    row in proof-for.ui-inputs; a place this suite reads that the row does not reach is a
  //    change that can make a claim here false with nothing running it.
  // =====================================================================================

  await run.check("proof-for.ui-inputs selects this suite for every file and directory it read", async () => {
    const suite = path.basename(__filename);
    // The same override proof-for.sh honors, for the same reason: proof-for.test.sh points both
    // at a deliberately narrowed row and watches this refuse. The real map is itself a read.
    const override = process.env.PROOF_FOR_UI_INPUTS;
    const mapText = override ? fs.readFileSync(override, "utf8") : read(SELECTION_MAP);
    const { rows, aliases } = parseSelectionMap(mapText);
    assert(rows.has(suite), `EMPTY INVENTORY: ${repoRel(SELECTION_MAP)} has no row for ${suite}`);
    const tokens = expandRow(rows.get(suite), aliases);
    const dirs = tokens.filter((t) => t.endsWith("/"));
    const files = new Set(tokens.filter((t) => !t.endsWith("/")));
    assert(touched.size > 0, "EMPTY INVENTORY: this suite recorded no read at all");
    const unselected = [...touched]
      .filter((t) => !(files.has(t) || dirs.some((d) => t.startsWith(d))))
      .sort();
    assertEqual(
      unselected,
      [],
      `read by ${suite} and not reached by its row in ${repoRel(SELECTION_MAP)}; add each (a directory as <dir>/)`
    );
    return `${touched.size} paths read, every one selects this suite (${dirs.length} directory input(s): ${dirs.join(" ")})`;
  });

  const failed = run.report();
  return failed;
}

main().then((f) => process.exit(f ? 1 : 0));
