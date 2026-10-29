// THE SHIPPED-SOURCE MANIFEST, DERIVED FROM THE TREE. NEVER TYPED.
//
// WHY THIS FILE EXISTS
// ====================
// Three gates in this directory each carried their own hand-written list of which files in
// `app/ui/` they look at, and every one of those lists had gone stale:
//
//   lib/state-strings.js  `UI_SOURCES = ["index.html", "main.js", "timeline.js"]`
//                         against nine `<script src>` tags in index.html. So the affordance
//                         rule — "every ACTIONABLE state names a control" — could not see
//                         `updates.js`, the surface the CEO ruled on the day it was found,
//                         nor `home.js`, `settings-button.js`, `splash.js`,
//                         `splash-library.js` or `theme-boot.js`.
//   tests/contrast.js     `SOURCE_FILES` = four of them, so check 12's exemption inventory
//                         could not see a `data-contrast-exempt` declared in the other five;
//                         `CSS_FILES` = two of the three linked stylesheets, missing the
//                         53 KB `home.css` entirely.
//
// The file that held the worst of them opens with the words "DERIVE THE USER-VISIBLE STATE
// INVENTORY FROM SOURCE. NEVER TYPE IT." A typed list under that header is not an oversight;
// it is the proof that a rule written as a sentence does not survive contact with a second
// author. So the lists are gone and this is the derivation that replaced them.
//
// HOW THE DERIVATION WORKS — THREE STEPS, ALL OFF DISK
// ===================================================
//   1. ENTRY.    `index.html` is the shell Tauri loads. Its `<script src>` and
//                `<link rel=stylesheet href>` tags, read out of the markup with comments
//                stripped, are the first generation.
//   2. CLOSURE.  Every JS file already in the set is scanned for string literals naming an
//                existing file under `app/ui/`, and those are added, repeatedly, until the
//                set stops growing. This is what reaches `home/field-*.js`: they are in no
//                script tag, they are loaded at runtime from `home.js:56`'s `FIELD` array,
//                and a manifest built only from `index.html` would have been a shorter list
//                that LOOKED derived. It is four more files than the nine `ls app/ui/*.js`
//                reports.
//   3. RECONCILE. Every `.js`/`.css`/`.html` file that actually exists under `app/ui/`
//                (`tests/` and `node_modules/` excluded — they do not ship) must be in the
//                closure. One that is not throws BY NAME. A file nobody loads is either a
//                gate's blind spot or dead weight in the bundle, and both are worth a
//                failure rather than a silent skip.
//
// THE ONE DECLARED LIST, AND WHY IT IS NOT THE DEFECT AGAIN
// ========================================================
// `ROLES` below classifies each discovered file. It is written down, because "is this file
// seeded preview data or is it product?" is a judgment no scanner can make. What stops it
// rotting is that it is RECONCILED IN BOTH DIRECTIONS against the derivation, every run:
//
//   * a discovered file with no role      -> throws, naming the file
//   * a role for a file nobody discovers  -> throws, naming the role
//
// So the list cannot be short (step 3 plus the first rule) and it cannot be stale (the
// second). That is the difference between a list that IS the inventory — the defect — and a
// list that is checked against one.
//
// RUN IT:  node lib/ui-sources.js     — prints the manifest and the reconciliation.

"use strict";

const fs = require("fs");
const path = require("path");

const UI_DIR = path.resolve(__dirname, "..", "..");
const ENTRY = "index.html";

/// THE OTHER DOCUMENTS TAURI LOADS, each a window of its own that `index.html` never names:
/// the dictation tool's bar (and, with `?role=flight`, the words' flight) and its menu bar menu
/// (dictation plan slice 3, `src-tauri/src/dictation/bar.rs`). Each is an entry exactly as
/// `index.html` is: its own tags are the first generation, and the closure runs over them too.
/// Named here rather than discovered, because "this HTML file is a window the app opens" is a
/// claim about Rust code no scanner here reads; step 3 still refuses an HTML file that is
/// neither, so a stray page cannot hide.
const EXTRA_ENTRIES = ["dictation-bar.html", "dictation-menu.html"];

/// What each shipped file is. Every row says WHY, because every row is a claim about
/// whether a gate needs to look at the file, and an undeclared claim is the defect above.
///
/// `role` is one of:
///   "ui"            product code or markup. Its user-visible strings are STATES, and every
///                   state-facing gate reads it.
///   "preview-data"  seeded data for the browser preview. Its strings are sample content,
///                   not states the product can enter (`state-strings.js` blind spot B4).
///                   Scanned separately and reported, never folded into the inventory.
///   "style"         a stylesheet. No string inventory; read by the contrast gate for
///                   declared exemptions and color literals.
const ROLES = {
  "index.html": { role: "ui", why: "the shell Tauri loads" },
  "main.js": { role: "ui", why: "the chat surface and the bridge" },
  "attachments.js": {
    role: "ui",
    why:
      "screenshots and files on the composer (CEO §86): the tray, its remove controls, the drop " +
      "target and every refusal he reads under the composer",
  },
  "output-panel.js": {
    role: "ui",
    why:
      "the Output panel (output side-panel PRD §6): the two Output buttons and their names, the " +
      "list of the thread's files, its empty, looking and unreadable states, and a file's own view",
  },
  "work-summary.js": { role: "ui", why: "saved work receipts and their availability states" },
  "repositories.js": { role: "ui", why: "company repository connections and their status" },
  "questions.js": { role: "ui", why: "non-blocking question cards, explicit answers and delivery states" },
  "rich-files.js": { role: "ui", why: "the file cards Rich puts in front of the user: Open in …, Show in Finder, gone and Ask Rich again" },
  "your-instructions.js": { role: "ui", why: "the user's own instructions: the Settings row's line, the sheet that edits instructions.md, its two ⓘ and Rich's Saved card" },
  "quit-question.js": { role: "ui", why: "the question he is asked before a quit stops running background work" },
  "timeline.js": { role: "ui", why: "the working timeline's model and render" },
  "instructions-update.js": { role: "ui", why: "the instructions update: its question, its progress and its before and after" },
  "terminal-setup.js": { role: "ui", why: "Settings → Terminal setup: the row's last read, reading it again, what it found and what changed" },
  "git-name.js": { role: "ui", why: "your name in Git: the card asked once in the conversation, the Settings row and its sheet" },
  "oxford-comma.js": { role: "ui", why: "the Oxford comma filter (CEO 2026-10-19): what Claude writes, drawn with the comma before and/or/nor deleted while the setting is off; it has no user-visible strings" },
  "home.js": { role: "ui", why: "the home screen the CEO lands on" },
  "updates.js": { role: "ui", why: "the update surface — CEO ruling §26" },
  "quota.js": { role: "ui", why: "Claude Code quota and weekly reset controls in Technical Settings" },
  "resources.js": { role: "ui", why: "the live Mac resources square opposite the name in the rail footer, its card and its Ink/Readout view menu (round 47)" },
  "accounts.js": { role: "ui", why: "Claude accounts for everyone (round 18): the Settings row, the account cards, adding a second account and the buttons under Rich's lines about accounts" },
  "bug-report.js": { role: "ui", why: "Bust a bug (round 21): Rich's question, the report card with Send report, Change it and Cancel, the stand-in tooltip, Rich's lines about sending, and the Rich panel beside a window" },
  "dictation.js": { role: "ui", why: "Dictation (round 19, dictation plan slice 2): the Settings row, the Dictation sheet, the off notice and the composer's line once dictation is on" },
  "codex-reviews.js": { role: "ui", why: "Let Codex review your team's work (round 20.2): the Settings row below Technical view, its five states, the ⓘ's tooltip and the nudge" },
  "spend.js": { role: "ui", why: "API-equivalent token spend (round 22.1): its own screen, the Meter with its periods, Refresh and the live line, and its Token spend row in quick settings" },
  "settings-button.js": { role: "ui", why: "the universal settings button — CEO ruling §15" },
  "splash.js": { role: "ui", why: "the opening curtain's renderer" },
  "splash-library.js": { role: "ui", why: "the two approved splash compositions — data, with prose in it" },
  "theme-boot.js": { role: "ui", why: "the pre-paint theme mirror" },
  "no-native-menu.js": { role: "ui", why: "the one place that keeps the web view's own right-click menu (Look Up, Translate, Share) from appearing; it has no user-visible strings" },
  "speckle.js": {
    role: "ui",
    why:
      "the speckled ground's engine, copied unedited from the design system (CEO, 2026-09-29). " +
      "Its only user-visible strings are the errors it throws when asked for the splash or home " +
      "surface, which quote the CEO's rule",
  },
  "speckle-app.js": { role: "ui", why: "the desktop app's one call to the speckled ground, and the text colors it holds 4.5:1 for" },
  "home/field-engine.js": { role: "ui", why: "the home screen's WebGL field — it authors the hover card and the loro line" },
  "home/field-prep.js": { role: "ui", why: "the field's geometry preparation" },
  "home/field-ref.js": { role: "ui", why: "the field's reference tables" },
  "phone.js": {
    role: "ui",
    why:
      "the pairing screen — plan §4.1. Nearly every string on it is a state he reads and acts " +
      "on, including the two warnings he must meet BEFORE he starts and the sixteen taps, so " +
      "every state-facing gate has to read it.",
  },
  "qr.js": {
    role: "ui",
    why:
      "the QR encoder the pairing screen draws with. It has NO user-visible strings at all — " +
      "its one throw names a byte length — and it is classified `ui` rather than " +
      "`preview-data` because it ships, it runs in the shell, and `preview-data` is a claim " +
      "that something is sample content. A file with no strings is not a reason to call it " +
      "something it is not.",
  },

  "mock.js": {
    role: "preview-data",
    why:
      "the dev bridge harness, inert whenever `window.__TAURI__` is present. Its strings are " +
      "SEEDED CONVERSATION DATA for the browser preview, not states the product can enter.",
  },
  "home/field-data.js": {
    role: "preview-data",
    why:
      "4.9 MB of wholly synthetic loro, declared as such in its own `meta.synthetic` field. " +
      "Design input for the home screen's composition, not product copy.",
  },

  "dictation-bar.html": {
    role: "ui",
    why: "the dictation tool's bar and the words' flight, two windows over every app (dictation plan slice 3)",
  },
  "dictation-bar.js": {
    role: "ui",
    why:
      "the bar's states and every drawn line it says (round 19 states 14 to 19, Iris's two bar " +
      "lines), the meter, and the flight",
  },
  "dictation-menu.html": { role: "ui", why: "the dictation tool's menu bar menu (round 19 state 17)" },
  "dictation-menu.js": {
    role: "ui",
    why: "the menu bar menu's rows and Iris's line 4 for Secure Event Input",
  },
  "dictation-overlay.css": { role: "style", why: "the bar's, the flight's and the menu bar menu's stylesheet" },
  "style.css": { role: "style", why: "the shell's stylesheet" },
  "home.css": { role: "style", why: "the home screen's stylesheet" },
  "instructions-update.css": { role: "style", why: "the instructions question's and the before and after's stylesheet" },
  "terminal-and-git.css": { role: "style", why: "the Terminal setup sheet's and the Git name card's and sheet's stylesheet" },
  "splash.css": { role: "style", why: "the opening curtain's stylesheet" },
  "speckle.css": { role: "style", why: "the speckled ground's CSS half, copied unedited from the design system" },
  "fonts/fonts.css": { role: "style", why: "the vendored font faces" },
};

// ---------------------------------------------------------------------------------------
// Step 1 — the entry document's own references
// ---------------------------------------------------------------------------------------

/// `<script src>` and `<link rel=stylesheet href>` in `html`, in document order, with HTML
/// comments removed first so a commented-out tag is not counted as shipped.
function entryReferences(html) {
  const src = String(html).replace(/<!--[\s\S]*?-->/g, "");
  const out = [];
  const script = /<script\b[^>]*\bsrc\s*=\s*(["'])([^"']+)\1/gi;
  const link = /<link\b[^>]*>/gi;
  let m;
  while ((m = script.exec(src))) out.push({ ref: m[2].trim(), kind: "script" });
  while ((m = link.exec(src))) {
    const tag = m[0];
    if (!/\brel\s*=\s*["']?stylesheet\b/i.test(tag)) continue;
    const href = tag.match(/\bhref\s*=\s*(["'])([^"']+)\1/i);
    if (href) out.push({ ref: href[2].trim(), kind: "stylesheet" });
  }
  return out;
}

/// An absolute URL or a protocol-relative one. This manifest is about files on disk, and a
/// CDN reference is not one. There are none today; `manifest()` collects any it finds into
/// `remote` so that a gate can assert on the number rather than assume it.
function isRemote(ref) {
  return /^[a-z][a-z0-9+.-]*:/i.test(ref) || ref.indexOf("//") === 0;
}

/// `tests/` is this harness (it does not ship), `node_modules/` is the harness's
/// dependencies. Everything else under `app/ui/` is bundled. Used by BOTH the disk walk and
/// the closure, so the two agree about what "shipped" means by construction.
const NOT_SHIPPED_DIRS = new Set(["tests", "node_modules"]);

// ---------------------------------------------------------------------------------------
// Step 2 — the closure over runtime-loaded files
// ---------------------------------------------------------------------------------------

/// Every `"…/x.js"`-shaped literal in `src` that names a file which EXISTS under `app/ui/`.
///
/// Deliberately existence-gated rather than pattern-gated. `main.js` mentions
/// `app/src-tauri/src/main.rs` and `STREAMING.md` in prose; `home.js` mentions
/// `design/mockups/data/generate-loro.mjs`. None of those resolve to a file under this
/// directory, so none of them enter the manifest, and the rule needs no exception list to
/// achieve that. What DOES resolve is `home/field-data.js` and its three siblings — which is
/// the whole reason this step exists.
///
/// This reads the raw source, comments included, on purpose: a comment naming a file that
/// exists is a file this directory ought to know about, and the cost of a false positive is
/// one extra correct classification while the cost of a false negative is a blind gate.
///
/// `tests/` IS EXCLUDED HERE AS WELL AS IN THE DISK WALK, and it has to be: five shipped
/// files name a suite in this directory in a comment ("`tests/splash.js` check 1 strips the
/// assignment below…"), and reading comments means those resolve. A harness suite is not a
/// shipped file, and classifying one would put this directory's own prose into the product's
/// state inventory.
function referencedFiles(src) {
  const out = [];
  const re = /["'`]([A-Za-z0-9_./-]+\.(?:js|css|html))["'`]/g;
  let m;
  while ((m = re.exec(src))) {
    const ref = m[1];
    if (isRemote(ref) || ref.charAt(0) === "/" || ref.indexOf("..") >= 0) continue;
    if (NOT_SHIPPED_DIRS.has(ref.replace(/^\.\//, "").split("/")[0])) continue;
    const full = path.join(UI_DIR, ref);
    if (full.indexOf(UI_DIR + path.sep) !== 0) continue;
    let st;
    try { st = fs.statSync(full); } catch (_e) { continue; }
    if (st.isFile()) out.push(ref.replace(/^\.\//, ""));
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// Step 3 — what is actually on disk
// ---------------------------------------------------------------------------------------

function onDisk(dir, prefix, out) {
  const entries = fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1));
  for (const e of entries) {
    const rel = prefix ? prefix + "/" + e.name : e.name;
    if (e.isDirectory()) {
      if (NOT_SHIPPED_DIRS.has(e.name) || e.name.charAt(0) === ".") continue;
      onDisk(path.join(dir, e.name), rel, out);
      continue;
    }
    if (/\.(js|css|html)$/.test(e.name)) out.push(rel);
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// The manifest
// ---------------------------------------------------------------------------------------

/// STEP 3, AS A PURE FUNCTION OF ITS THREE INPUTS, so a suite can prove it refuses.
///
/// `manifest()` calls exactly this with the real derivation and the real tree; a test calls
/// it with a short list, or a missing role, or a stale one, and watches it throw. That is
/// the difference between a check that has been OBSERVED passing and one that has been shown
/// able to fail — and it matters more here than almost anywhere, because this function is
/// the thing standing between the gates and a second silent blind spot. A test that
/// re-implemented these three rules would be proving its own copy.
///
/// Throws on the first disagreement. Returns the count reconciled.
function reconcile(order, disk, roles) {
  const seen = new Set(order);

  const unreferenced = disk.filter((f) => !seen.has(f));
  if (unreferenced.length) {
    throw new Error(
      unreferenced.length + " file(s) exist under app/ui/ and nothing loads them:\n  " +
        unreferenced.join("\n  ") +
        "\nEither wire them into index.html (or into a file that is already loaded), or delete " +
        "them. This manifest will not classify a file it cannot prove ships, and every gate " +
        "that reads it would otherwise be silently short by exactly these."
    );
  }

  const unclassified = order.filter((f) => !roles[f]);
  if (unclassified.length) {
    throw new Error(
      unclassified.length + " shipped file(s) have no role in lib/ui-sources.js's ROLES table:\n  " +
        unclassified.join("\n  ") +
        "\nAdd each one with the role it plays and WHY. `ui` puts its strings in the state " +
        "inventory and under the affordance rule; `preview-data` keeps them out and says on " +
        "what grounds. There is no third option and no default, because a default is how six " +
        "files became invisible to three gates."
    );
  }

  const stale = Object.keys(roles).filter((f) => !seen.has(f));
  if (stale.length) {
    throw new Error(
      stale.length + " role(s) in lib/ui-sources.js name a file the derivation does not reach:\n  " +
        stale.join("\n  ") +
        "\nThe file was renamed, deleted, or is no longer loaded. Remove the row — a " +
        "classification of something that does not ship is the list drifting away from the " +
        "tree again, in the other direction."
    );
  }

  return order.length;
}

let CACHE = null;

/// The derived manifest, or a thrown Error naming exactly what disagrees.
///
/// `{ entry, order, roles, byRole, remote, disk }` where `order` is discovery order (the
/// entry document, then its tags in document order, then each generation of the closure).
///
/// Memoized: five consumers call this and the closure reads every shipped file, one of which
/// is 4.9 MB. The cache is per-process and the process is one test run, so it cannot go
/// stale within a run and cannot survive one.
function manifest() {
  if (CACHE) return CACHE;

  const entryPath = path.join(UI_DIR, ENTRY);
  if (!fs.existsSync(entryPath)) {
    throw new Error(
      "lib/ui-sources.js cannot derive anything: " + ENTRY + " is not at " + UI_DIR + ". This " +
        "module refuses to return a manifest it did not derive — an empty inventory reporting " +
        "green is the failure it exists to stop."
    );
  }

  const remote = [];
  const order = [ENTRY];
  const seen = new Set(order);

  const push = (ref) => {
    if (isRemote(ref)) { if (remote.indexOf(ref) < 0) remote.push(ref); return; }
    const rel = ref.replace(/^\.\//, "").split("?")[0].split("#")[0];
    if (seen.has(rel)) return;
    if (!fs.existsSync(path.join(UI_DIR, rel))) {
      throw new Error(
        "a shipped file references " + rel + ", which does not exist under " + UI_DIR + ". A " +
          "shell that loads a file that is not there is a broken build, and a manifest that " +
          "quietly dropped it would hide that."
      );
    }
    seen.add(rel);
    order.push(rel);
  };

  for (const r of entryReferences(fs.readFileSync(entryPath, "utf8"))) push(r.ref);
  for (const extra of EXTRA_ENTRIES) {
    push(extra);
    for (const r of entryReferences(fs.readFileSync(path.join(UI_DIR, extra), "utf8"))) push(r.ref);
  }

  // The closure. Bounded by the number of files on disk, so it terminates; the bound below
  // is a guard against a bug here, not against the tree.
  const SOURCES = {};
  for (let pass = 0; pass < 50; pass++) {
    const before = order.length;
    for (const rel of order.slice()) {
      if (!/\.js$/.test(rel)) continue;
      if (!SOURCES[rel]) SOURCES[rel] = fs.readFileSync(path.join(UI_DIR, rel), "utf8");
      for (const ref of referencedFiles(SOURCES[rel])) push(ref);
    }
    if (order.length === before) break;
  }

  // Step 3, both directions.
  const disk = onDisk(UI_DIR, "", []);
  reconcile(order, disk, ROLES);

  const roles = {};
  const byRole = {};
  for (const f of order) {
    roles[f] = ROLES[f];
    (byRole[ROLES[f].role] = byRole[ROLES[f].role] || []).push(f);
  }
  CACHE = { entry: ENTRY, order, roles, byRole, remote, disk };
  return CACHE;
}

/// Shipped product files whose user-visible strings are STATES — `index.html` plus every
/// `role: "ui"` script, in load order. This is what replaced `UI_SOURCES`.
function stateSources() {
  return manifest().byRole.ui.slice();
}

/// Files whose strings are seeded preview data. Reported, never folded into the inventory.
function previewDataSources() {
  return (manifest().byRole["preview-data"] || []).slice();
}

/// The linked stylesheets, in document order.
function styleSources() {
  return (manifest().byRole.style || []).slice();
}

/// Absolute path, for callers that hand it straight to `fs`.
function abs(rel) {
  return path.join(UI_DIR, rel);
}

// =========================================================================================
// ASKING A QUESTION OF THE WHOLE SHIPPED UI
// =========================================================================================
//
// The manifest above fixed WHICH FILES a gate reads. It did not fix the second half of the
// same defect, and five checks in this directory still carried it: a claim written about
// "the shipped source" and evaluated against `main.js` alone.
//
//   setup.js:351     "there is exactly one run_setup call site in the shipped source"
//   memory.js:310    "there is exactly one call site" (provision_memory)
//   memory.js:329    "main.js must not contain a corpus path"
//   feedback.js:197  no timer may open the feedback desk
//   splash.js:1727   "a measurement timestamp reached the UI"
//
// Every one of those is an ABSENCE or a UNIQUENESS claim, and both of those go green over a
// file they cannot open. A second `invoke("run_setup")` written into `updates.js` leaves
// setup.js check 10 printing "one call site" — and its own comment says why that matters:
// "A SECOND DOOR IS A SECOND PLACE FOR THE GUARD TO BE MISSING." The door it was watching
// was one of twelve.
//
// So the derivation is offered as a QUESTION rather than as a list. `uiMatches(re)` runs a
// pattern over every `role: "ui"` file the manifest reaches and returns `file:line` for
// every hit. A caller cannot accidentally scope it to one file, because there is no file
// argument to pass.
//
// COMMENTS ARE STRIPPED, AND THAT IS LOAD-BEARING RATHER THAN TIDY. `setup.js:497` already
// hand-rolled a line-based `//` filter for exactly one reason, written at the line: "The
// note above the (absent) listener quotes the line it replaced, so a naive grep matches the
// explanation and calls it the defect." That is true of every file in this product — this
// codebase writes long comments that quote the code they removed — so an absence claim that
// reads comments is a false-positive generator, and the fix belongs here once rather than
// once per caller.

/// Comments removed, LINE NUMBERS PRESERVED, from JavaScript.
///
/// `opts.strings` (default true) keeps string and template bodies. `run.js` needs them GONE
/// — it counts `run.check(` and a `"run.check("` inside a literal is not a call — while an
/// absence claim needs them KEPT, because `invoke("run_setup")` is a string. One scanner and
/// one flag, so the two consumers cannot drift, and `run.js` self-tests it on every run
/// against a fixture carrying all four shapes a naive `grep -c` gets wrong.
///
/// THE REGEX BRANCH IS NOT OPTIONAL. `run.js` earned it: a regex literal carrying a backtick
/// opens a template literal without it and swallows the rest of the file. A stripper that
/// swallows the rest of the file is an absence claim that always passes.
///
/// NEITHER IS THE INTERPOLATION BRANCH, AND A REAL RUN EARNED THAT ONE TOO. A template
/// literal is not a quoted string with a different quote: `${ ... }` is CODE, and the code
/// may open another template. Ending the span at the next backtick therefore ends it in the
/// middle of the outer literal, and everything after that is read with the string/code
/// polarity inverted. Measured 2026-09-06 on `tests/home.js:2013`
///
///     `... ${inside ? `inside the curtain's ${hold}ms hold` : `PAST the ${hold}ms hold`}`
///
/// which left `inside the curtain` standing as code in the stripped output and swallowed the
/// `run.check(` eight lines later: `run.js` read home.js as declaring 32 checks where it
/// declares 33, and its floor for that suite was one check lower than the suite's own source.
/// An under-count cannot manufacture a green run — `observed >= declared` still holds — but a
/// floor nothing trips over is the same as no floor. `home/field-engine.js:719` carries the
/// same shape, so this was never one file's spelling.
///
/// The span is the WHOLE literal, interpolations included, which keeps the one-span-per-
/// literal model the quoted branch already has. Code inside an interpolation is therefore
/// blanked with `strings: false` — conservative in the direction that costs nothing here,
/// since a `run.check(` inside a template interpolation is not a thing anyone writes, and
/// with `strings: true` the span is copied verbatim exactly as it is today.
function stripJsComments(src, opts) {
  const keepStrings = !opts || opts.strings !== false;
  let out = "";
  let i = 0;
  const n = src.length;
  // The last significant character, used only to decide whether a slash opens a regex (after
  // an operator, keyword, `(` or `,`) or is a division (after a value).
  let prevSig = "";
  const blank = (from, to) => {
    for (let k = from; k < to && k < n; k++) out += src[k] === "\n" ? "\n" : " ";
  };

  // The index just past the closing backtick of the template literal that starts at `from`.
  // Mutually recursive with `endOfInterpolation` because the two nest without limit.
  const endOfTemplate = (from) => {
    let j = from + 1;
    while (j < n) {
      const ch = src[j];
      if (ch === "\\") {
        j += 2;
        continue;
      }
      if (ch === "`") return j + 1;
      if (ch === "$" && src[j + 1] === "{") {
        j = endOfInterpolation(j + 2);
        continue;
      }
      j++;
    }
    return n;
  };

  // The index just past the `}` that closes the interpolation whose body starts at `from`.
  // Braces are counted, and everything that can carry an unbalanced brace — a string, a
  // template, a comment — is stepped over rather than counted through.
  const endOfInterpolation = (from) => {
    let j = from;
    let depth = 1;
    while (j < n) {
      const ch = src[j];
      if (ch === "}") {
        j++;
        if (--depth === 0) return j;
        continue;
      }
      if (ch === "{") {
        depth++;
        j++;
        continue;
      }
      if (ch === "`") {
        j = endOfTemplate(j);
        continue;
      }
      if (ch === '"' || ch === "'") {
        const quote = ch;
        j++;
        while (j < n) {
          if (src[j] === "\\") {
            j += 2;
            continue;
          }
          if (src[j] === quote) {
            j++;
            break;
          }
          j++;
        }
        continue;
      }
      if (ch === "/" && src[j + 1] === "/") {
        while (j < n && src[j] !== "\n") j++;
        continue;
      }
      if (ch === "/" && src[j + 1] === "*") {
        j += 2;
        while (j < n && !(src[j] === "*" && src[j + 1] === "/")) j++;
        j += 2;
        continue;
      }
      j++;
    }
    return n;
  };

  while (i < n) {
    const c = src[i];

    if (c === "/" && src[i + 1] === "/") {
      const from = i;
      while (i < n && src[i] !== "\n") i++;
      blank(from, i);
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const from = i;
      i += 2;
      while (i < n && !(src[i] === "*" && src[i + 1] === "/")) i++;
      i += 2;
      if (i > n) i = n;
      blank(from, i);
      continue;
    }
    if (c === "/" && !/[A-Za-z0-9_$)\]]/.test(prevSig)) {
      const from = i;
      i++;
      let inClass = false;
      while (i < n) {
        if (src[i] === "\\") { i += 2; continue; }
        if (src[i] === "[") inClass = true;
        else if (src[i] === "]") inClass = false;
        else if (src[i] === "/" && !inClass) { i++; break; }
        else if (src[i] === "\n") break;
        i++;
      }
      while (i < n && /[a-z]/.test(src[i])) i++;
      if (keepStrings) out += src.slice(from, i);
      else blank(from, i);
      prevSig = "/";
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      const from = i;
      if (c === "`") {
        i = endOfTemplate(i);
      } else {
        const quote = c;
        i++;
        while (i < n) {
          if (src[i] === "\\") { i += 2; continue; }
          if (src[i] === quote) { i++; break; }
          i++;
        }
      }
      if (keepStrings) out += src.slice(from, i);
      else blank(from, i);
      prevSig = c;
      continue;
    }

    out += c;
    if (!/\s/.test(c)) prevSig = c;
    i++;
  }
  return out;
}

/// HTML comments removed, line numbers preserved.
function stripHtmlComments(src) {
  return String(src).replace(/<!--[\s\S]*?-->/g, (m) => m.replace(/[^\n]/g, " "));
}

/// CSS comments removed, line numbers preserved.
function stripCssComments(src) {
  return String(src).replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, " "));
}

const CODE_CACHE = new Map();

/// `{ name, src, code }` for every file of `role`, read off disk, comments stripped.
function sourcesOfRole(role, opts) {
  const keepStrings = !opts || opts.strings !== false;
  const names =
    role === "ui" ? stateSources() : role === "style" ? styleSources() : previewDataSources();
  return names.map((name) => {
    const key = role + "|" + name + "|" + keepStrings;
    if (CODE_CACHE.has(key)) return CODE_CACHE.get(key);
    const src = fs.readFileSync(abs(name), "utf8");
    const code = /\.html$/.test(name)
      ? stripHtmlComments(src)
      : /\.css$/.test(name)
      ? stripCssComments(src)
      : stripJsComments(src, { strings: keepStrings });
    const rec = { name, src, code };
    CODE_CACHE.set(key, rec);
    return rec;
  });
}

/// `file:line` for every match of `re` across the WHOLE shipped UI, comments stripped.
///
/// `{ site, file, line, text }`. There is deliberately NO file parameter: the entire reason
/// this function exists is that five callers each named one file and called the result "the
/// shipped source".
function uiMatches(re, opts) {
  const rx = new RegExp(re.source, re.flags.indexOf("g") >= 0 ? re.flags : re.flags + "g");
  const out = [];
  for (const f of sourcesOfRole("ui", opts)) {
    rx.lastIndex = 0;
    let m;
    while ((m = rx.exec(f.code))) {
      const line = f.code.slice(0, m.index).split("\n").length;
      out.push({ site: f.name + ":" + line, file: f.name, line, text: m[0] });
      if (m[0].length === 0) rx.lastIndex++;
    }
  }
  return out;
}

/// Every declaration of one CSS property across every SHIPPED stylesheet.
///
/// `{ site, file, line, value }`. This is what replaced `appearance.js`'s `STYLE_CSS`-only
/// reads: check 12 asserted "every font-size in the shipped CSS is rem" over ONE of the four
/// stylesheets `index.html` links, and the other three hold 15 more `font-size` declarations
/// between them.
function cssDeclarations(prop) {
  const rx = new RegExp("(^|[;{\\s])" + prop + "\\s*:\\s*([^;}]+)", "gi");
  const out = [];
  for (const f of sourcesOfRole("style")) {
    rx.lastIndex = 0;
    let m;
    while ((m = rx.exec(f.code))) {
      const line = f.code.slice(0, m.index).split("\n").length;
      out.push({ site: f.name + ":" + line, file: f.name, line, value: m[2].trim() });
    }
  }
  return out;
}

/// THE SAME DECLARATIONS, WITH THE RULE EACH ONE SITS IN.
///
/// `cssDeclarations` above answers "what values does this property take", which is enough to
/// ask whether a size is in `px` and not enough to ask ANYTHING ELSE about it. Two questions
/// that arrived together on 2026-09-18 both need the rule rather than the value:
///
///   §15's FLOOR ACROSS EVERY UNIT. `0.6875rem` is 11px and a regex over values cannot say
///   so, because the multiplier means nothing without the root size — which is itself a
///   declaration in this same tree (`html { font-size: calc(16px * var(--app-font-scale)) }`).
///   Resolving one needs the other, so the walk has to know which rule it is standing in.
///
///   AND AN EXEMPTION HAS TO BE CHECKABLE. "That 11px label is an all-caps micro-label" is a
///   claim about the RULE — it either declares `text-transform: uppercase` beside its
///   `font-size` or it does not. With the value alone the claim can only be believed.
///
/// Returns `{ site, file, line, selector, context, value, rule }` per match, where `selector`
/// is the innermost prelude, `context` is the whole stack (so an `@media` wrapper is visible
/// rather than silently dropped) and `rule` is every declaration in that block, keyed by
/// property — which is what makes the exemption above a derivation instead of a promise.
///
/// A HAND-ROLLED WALK RATHER THAN A REGEX, deliberately: braces nest (`@media`, `@supports`)
/// and a regex cannot count. Quoted strings are skipped so a `content: "a{b}"` cannot shift
/// the brace depth for everything after it.
function cssRules(prop) {
  const out = [];
  for (const f of sourcesOfRole("style")) {
    const code = f.code;
    const stack = [];
    let decls = [];
    let buf = "";
    let line = 1;
    const flush = (endLine) => {
      const m = /^([-a-z]+)\s*:\s*([\s\S]+)$/i.exec(buf.trim());
      buf = "";
      if (!m) return;
      const property = m[1].toLowerCase();
      const value = m[2].trim();
      decls.push({ property, value, line: endLine });
    };
    for (let i = 0; i < code.length; i++) {
      const c = code[i];
      if (c === "\n") line++;
      if (c === '"' || c === "'") {
        const quote = c;
        buf += c;
        for (i++; i < code.length; i++) {
          if (code[i] === "\n") line++;
          buf += code[i];
          if (code[i] === "\\") { i++; if (code[i] !== undefined) { buf += code[i]; if (code[i] === "\n") line++; } continue; }
          if (code[i] === quote) break;
        }
        continue;
      }
      if (c === "{") {
        stack.push({ prelude: buf.trim().replace(/\s+/g, " "), decls });
        decls = [];
        buf = "";
      } else if (c === "}") {
        // The block is finished: everything it declared is now known, so the matching
        // declarations inside it can be emitted WITH their siblings.
        const closing = decls;
        const frame = stack.pop();
        buf = "";
        if (!frame) continue;
        const rule = {};
        for (const d of closing) rule[d.property] = d.value;
        for (const d of closing) {
          if (d.property !== prop) continue;
          out.push({
            site: f.name + ":" + d.line,
            file: f.name,
            line: d.line,
            selector: frame.prelude,
            context: stack.map((s) => s.prelude).concat(frame.prelude).join(" » "),
            value: d.value,
            rule,
          });
        }
        decls = frame.decls;
      } else if (c === ";") {
        flush(line);
      } else {
        buf += c;
      }
    }
  }
  return out;
}

/// EVERY STRING THE STYLESHEETS THEMSELVES PUT ON SCREEN.
///
/// `content:` on a `::before`/`::after` renders text that is in NO string inventory in this
/// directory: `lib/state-strings.js` scrapes JavaScript literals, HTML text and Rust
/// literals, and `ROLES` above classifies a stylesheet as `style` with the words "No string
/// inventory". There is one in this tree — `style.css`'s `.setbtn::after { content:
/// "Settings" }`, the tooltip on the settings button §15 requires on every screen — and
/// until this function it was authored text that nothing counted and nothing measured.
///
/// Empty strings are dropped: `content: ""` is a decorative box, not text. Three of the four
/// `content:` declarations in this tree are that.
function cssContentStrings() {
  const out = [];
  for (const d of cssDeclarations("content")) {
    const m = d.value.match(/^(["'])((?:\\.|(?!\1)[^\\])*)\1/);
    if (!m) continue; // `none`, `attr()`, `counter()`, a `url()` — not an authored sentence
    const text = m[2].replace(/\\([0-9a-f]{1,6})\s?/gi, (_a, h) => String.fromCodePoint(parseInt(h, 16)));
    if (!text.trim()) continue;
    out.push({ site: d.site, file: d.file, line: d.line, text });
  }
  return out;
}

/// How many `run.check(` calls a suite's source declares, comments and string literals
/// excluded: the floor `run.js` measures a run against, and the `declared` figure the harness
/// writes into a directly run suite's evidence ledger (recheck R26). One scanner for both.
function declaredChecks(src) {
  // `strings: false` — a `"run.check("` inside a literal is not a call.
  const out = stripJsComments(src, { strings: false });
  return (out.match(/\brun\.check\s*\(/g) || []).length;
}

module.exports = {
  declaredChecks,
  UI_DIR,
  ENTRY,
  EXTRA_ENTRIES,
  ROLES,
  manifest,
  reconcile,
  stateSources,
  previewDataSources,
  styleSources,
  abs,
  entryReferences,
  referencedFiles,
  onDisk,
  isRemote,
  stripJsComments,
  stripHtmlComments,
  stripCssComments,
  sourcesOfRole,
  uiMatches,
  cssDeclarations,
  cssRules,
  cssContentStrings,
};

// `node lib/ui-sources.js` prints the derivation — the manifest, runnable.
if (require.main === module) {
  const m = manifest();
  let w = 0;
  for (const f of m.order) w = Math.max(w, f.length);
  for (const f of m.order) {
    console.log("  " + f + " ".repeat(w - f.length) + "  " + (m.roles[f].role + "             ").slice(0, 14) + m.roles[f].why);
  }
  console.error(
    "\n" + m.order.length + " shipped file(s) derived from " + m.entry + " and the closure over it: " +
      (m.byRole.ui || []).length + " ui, " + (m.byRole["preview-data"] || []).length + " preview-data, " +
      (m.byRole.style || []).length + " style."
  );
  console.error(
    m.disk.length + " .js/.css/.html file(s) on disk under app/ui (tests/ and node_modules/ excluded); " +
      "every one of them is above."
  );
  console.error(m.remote.length + " remote reference(s)" + (m.remote.length ? ": " + m.remote.join(", ") : "."));
}
