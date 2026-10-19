// DERIVE THE USER-VISIBLE STATE INVENTORY FROM SOURCE. NEVER TYPE IT.
//
// WHY THIS FILE EXISTS
// ====================
// The rule this module serves: *a state the user could change must be rendered together
// with the control that changes it. A state change requiring a human action is not a
// status, it is a request.*
//
// Enforcing that rule needs an INVENTORY of the states the app can show, and the one thing
// an inventory must never be is hand-written. A typed list of 14 against a registration of
// 15 is the defect that opened this whole sequence; `run.js` beside this file lost a whole
// suite the same way; the engine's `run-all-tests.sh` lost five. So the inventory here is
// scraped out of the shipped source every time the suite runs, and the classification
// registry is checked AGAINST it — source is the authority, the registry is the annotation.
//
// HOW THE SCRAPE WORKS
// ====================
// Two extractors, both purely lexical, both deliberately dumb:
//
//   HTML  every non-empty text node outside <script>/<style>/comments, plus the
//         `placeholder`, `title` and `aria-label` attribute values.
//   JS    every string literal (single, double, template) that survives comment stripping,
//         filtered by `looksLikeProse` below.
//
// Comment stripping is a real character scanner (strings, template literals, line and block
// comments, and regex literals via the standard prev-significant-token heuristic), because
// this source is roughly 40% prose commentary and a naive regex would harvest all of it.
//
// THE BLIND SPOTS, NAMED RATHER THAN GLOSSED — each one is text a reader could see that
// this scrape does not produce a literal for:
//
//   B1. COMPOSED strings. `"Working for " + d` yields the literal `Working for ` and never
//       the rendered `Working for 4m 7s`. The state is SEEN; its rendered form is not.
//   B2. BACKEND-AUTHORED strings. `showUnboundView` prints `navTree.unbound_explanation`
//       and `String(e)` — text minted in Rust and relayed verbatim. Covered by the
//       separate `rustStrings()` scrape below, which greps the Rust sources that feed
//       those fields and is therefore weaker than the JS one.
//   B3. Strings under the prose floor. "Rich", "Copied", "Show more" are real UI text and
//       are deliberately excluded: they are labels ON controls, not states, and including
//       them buries the states in nouns.
//   B4. The `preview-data` files — `mock.js` and `home/field-data.js`. Their strings are
//       SEEDED SAMPLE CONTENT for the browser preview, not states the product can enter.
//       Scanned separately and reported, never mixed in. WHICH files those are is not
//       decided here: `lib/ui-sources.js` classifies every shipped file and reconciles the
//       classification against the tree in both directions.
//   B6. GLSL. `home/field-engine.js` carries its vertex and fragment shaders as string
//       literals, and a shader is three hundred characters of `vec4`, `uniform` and
//       `gl_FragColor` that clears every prose test written for English. It is code the GPU
//       reads, never text a person reads, so `looksLikeShader` below drops it — and drops
//       it BY SHAPE (a `void main()` or a `gl_` symbol), not by file name, so a shader that
//       moves to another file is still excluded and a sentence that moves INTO this one is
//       still caught.
//   B5. UNBOUNDED MACHINERY LEAK. `send()` prints `String(e)` from `send_message` into the
//       timeline. If the failure came from inside richos-core rather than from the command
//       layer's own authored refusal, that text is a Display impl — "cognition io: broken
//       pipe", "the loro slice did not parse". Any error in the crate can reach that
//       sentence, so the set is not enumerable and this module does not pretend to
//       enumerate it. Reported as a finding, not folded into the inventory.
//
// A blind spot that is named is a bounded gap. A blind spot that is quietly omitted is the
// defect this module exists to stop.

"use strict";

const fs = require("fs");
const path = require("path");
const SOURCES = require("./ui-sources");

const UI_DIR = SOURCES.UI_DIR;

// ---------------------------------------------------------------------------------------
// JS: strip comments, keep string literals
// ---------------------------------------------------------------------------------------

/// Scan `src` character by character and return the string literals with their 1-based
/// line numbers. Comments are dropped. Regex literals are recognized so that a slash inside
/// one cannot be mistaken for the start of a comment, and vice versa.
///
/// ADJACENT LITERALS JOINED BY `+` ARE FOLDED INTO ONE. The unbound-thread explanation is
/// one sentence written across three source lines by prettier; scraped naively it becomes
/// three entries, none of which is the sentence the CEO reads. Folding is what makes the
/// registry key equal to the rendered text.
///
/// `opts.holes` (default off) ALSO returns the literals written INSIDE a template's `${...}`
/// holes, appended after the rest with their own lines. A label chosen by a ternary inside a
/// template (`${k === "F1" ? "…" : `<b>${k}</b> starts and stops it.`}`) is text a person reads,
/// and without this it is invisible (blind spot B1's other half). Off by default, so the
/// inventory every other gate keys on is unchanged.
///
/// `opts.offsets` (the Oxford comma fixer, `app/scripts/oxford-comma.js`) adds `offsets`: for every
/// character of `text`, the index in `src` it was read from, so a character found in the text
/// can be deleted from the source.
function jsStringLiterals(src, opts) {
  const holes = !!(opts && opts.holes);
  const withOffsets = !!(opts && opts.offsets);
  const nested = [];
  const out = [];
  let i = 0;
  let line = 1;
  // The last significant character before the cursor, used only to decide whether a slash
  // opens a regex (after an operator, keyword, `(` or `,`) or is a division (after a value).
  let prevSig = "";
  // Every significant character seen since the previous string literal closed. Exactly `+`
  // means the two literals are one concatenated string.
  let sigSince = null;

  const n = src.length;
  while (i < n) {
    const c = src[i];

    if (c === "\n") { line++; i++; continue; }
    if (c === " " || c === "\t" || c === "\r") { i++; continue; }

    // line comment
    if (c === "/" && src[i + 1] === "/") {
      while (i < n && src[i] !== "\n") i++;
      continue;
    }
    // block comment
    if (c === "/" && src[i + 1] === "*") {
      i += 2;
      while (i < n && !(src[i] === "*" && src[i + 1] === "/")) { if (src[i] === "\n") line++; i++; }
      i += 2;
      continue;
    }
    // regex literal — only where a value cannot precede it
    if (c === "/" && !/[A-Za-z0-9_$)\]]/.test(prevSig)) {
      i++;
      let inClass = false;
      while (i < n) {
        if (src[i] === "\\") { i += 2; continue; }
        if (src[i] === "[") inClass = true;
        else if (src[i] === "]") inClass = false;
        else if (src[i] === "/" && !inClass) { i++; break; }
        else if (src[i] === "\n") { line++; break; }
        i++;
      }
      while (i < n && /[a-z]/.test(src[i])) i++;
      prevSig = "/";
      if (sigSince !== null) sigSince += "/";
      continue;
    }
    // string / template literal
    if (c === '"' || c === "'" || c === "`") {
      const quote = c;
      const startLine = line;
      let buf = "";
      const offs = withOffsets ? [] : null;
      const at = (k, len) => { if (offs) for (let x = 0; x < len; x++) offs.push(k); };
      i++;
      while (i < n) {
        const d = src[i];
        if (d === "\\") {
          // Keep the escaped character's VALUE where it is one the UI can render.
          const e = src[i + 1];
          if (e === "n") { buf += "\n"; at(i, 1); }
          else if (e === "t") { buf += " "; at(i, 1); }
          else if (e === "u" && /^[0-9a-fA-F]{4}$/.test(src.slice(i + 2, i + 6))) {
            buf += String.fromCharCode(parseInt(src.slice(i + 2, i + 6), 16));
            at(i, 1);
            i += 6;
            continue;
          } else { buf += e; at(i + 1, String(e).length); }
          i += 2;
          continue;
        }
        if (d === quote) { i++; break; }
        if (d === "\n") line++;
        // `${...}` inside a template literal is an expression, not text. It is dropped so
        // the surrounding words survive; the hole is blind spot B1.
        if (quote === "`" && d === "$" && src[i + 1] === "{") {
          let depth = 1;
          i += 2;
          const holeFrom = i, holeLine = line;
          while (i < n && depth > 0) {
            if (src[i] === "{") depth++;
            else if (src[i] === "}") depth--;
            else if (src[i] === "\n") line++;
            i++;
          }
          if (holes) {
            for (const h of jsStringLiterals(src.slice(holeFrom, i - 1), opts)) {
              nested.push({ text: h.text, line: holeLine + h.line - 1 });
            }
          }
          continue;
        }
        buf += d;
        at(i, 1);
        i++;
      }
      if (sigSince !== null && sigSince.trim() === "+" && out.length) {
        out[out.length - 1].text += buf;
        if (offs) out[out.length - 1].offsets.push(...offs);
      } else {
        out.push(offs ? { text: buf, line: startLine, offsets: offs } : { text: buf, line: startLine });
      }
      prevSig = quote;
      sigSince = "";
      continue;
    }

    prevSig = c;
    if (sigSince !== null) sigSince += c;
    i++;
  }
  return holes ? out.concat(nested) : out;
}

// ---------------------------------------------------------------------------------------
// HTML: text nodes and the three human-readable attributes
// ---------------------------------------------------------------------------------------

function htmlVisibleStrings(src) {
  const out = [];
  // Drop comments and script/style bodies, keeping newlines so line numbers stay true.
  const blank = (m) => m.replace(/[^\n]/g, " ");
  const cleaned = src
    .replace(/<!--[\s\S]*?-->/g, blank)
    .replace(/<script\b[\s\S]*?<\/script>/gi, blank)
    .replace(/<style\b[\s\S]*?<\/style>/gi, blank);

  const attrRe = /\b(placeholder|title|aria-label)\s*=\s*"([^"]*)"/g;
  let m;
  while ((m = attrRe.exec(cleaned)) !== null) {
    // `start`: where the value begins in `src` (the blanking above keeps every offset).
    out.push({ text: m[2], line: cleaned.slice(0, m.index).split("\n").length, kind: "attr:" + m[1],
      raw: m[2], start: m.index + m[0].length - 1 - m[2].length });
  }

  const textRe = />([^<]+)</g;
  while ((m = textRe.exec(cleaned)) !== null) {
    const t = m[1].replace(/\s+/g, " ").trim();
    if (!t) continue;
    out.push({ text: t, line: cleaned.slice(0, m.index).split("\n").length, kind: "text", raw: m[1], start: m.index + 1 });
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// The prose filter
// ---------------------------------------------------------------------------------------

/// MACHINERY, never read by anyone: selectors, event channels, class names, keys, formats.
const MACHINERY = [
  /:\/\//,                        // rich://turn-status
  /^[.#[]/,                       // CSS selectors
  /^[a-z0-9]+([-_][a-z0-9]+)*$/i, // a single identifier-ish token
  /^[A-Za-z-]+\/[A-Za-z-]+$/,     // mime types, short paths
  /\.(js|css|png|html|json|rs)$/,
];

/// Reads as something a person could read: at least three whitespace-separated tokens, at
/// least two of them carrying two or more letters, and at least 12 characters overall.
///
/// THE FLOOR IS DELIBERATE AND IT COSTS SOMETHING (blind spot B3): "Copied", "Show more",
/// "Working" and "Ended" are user-visible and are excluded. They are control labels and
/// one-word statuses whose whole meaning lives in the row around them; a rule about
/// SENTENCES that also swept in every noun in the app would flag roughly 200 strings, and a
/// check that flags 200 things gets deleted within a day.
function looksLikeProse(s) {
  const t = String(s).trim();
  if (t.length < 12) return false;
  if (!/[A-Za-z]/.test(t)) return false;
  if (looksLikeShader(t)) return false;
  for (const re of MACHINERY) if (re.test(t)) return false;
  if (looksLikeCssValue(t)) return false;
  if (isTaglessMarkup(t)) return false;
  const words = t.split(/\s+/).filter(Boolean);
  if (words.length < 3) return false;
  const wordy = words.filter((w) => /[A-Za-z]{2}/.test(w));
  return wordy.length >= 2;
}

// ---------------------------------------------------------------------------------------
// Two more shapes that read as English and are not (found by widening the source list)
// ---------------------------------------------------------------------------------------

/// A CSS *value*: `linear-gradient(100deg, rgba(255,240,205,0) 40%, …)`, `1px solid #56698E`,
/// `clamp(28px, 4.4vh, 44px)`, `50% -1px auto auto`. Every one of those clears the prose
/// floor — three-plus tokens, two-plus with letters in them — and none is text a person
/// reads. `splash-library.js` is nothing but such values, and widening the source list put
/// twenty of them in front of the affordance rule as states to be classified.
///
/// RECOGNIZED POSITIVELY, NOT GUESSED AT. Function calls of the CSS grammar are removed with
/// their arguments, then hex colors, then numbers with CSS units, then punctuation, then a
/// closed vocabulary of CSS keywords. If NO letters survive, every token in the string was a
/// token of CSS and nothing in it was ever English.
///
/// THE KEYWORD LIST IS A LANGUAGE, NOT AN INVENTORY, and that distinction is the reason it
/// is allowed to be written down here when three other lists in this directory were not. It
/// enumerates part of the CSS grammar, which is fixed by a specification and does not change
/// when somebody adds a file to `app/ui/`. A list that drifts with the product is the defect;
/// a list that quotes a standard is a constant. The failure direction is also the safe one:
/// a CSS keyword missing from it means a value is NOT recognized and lands in the inventory
/// as a state to classify, which is noise. Nothing here can hide a state.
const CSS_FUNCTIONS =
  "rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|color-mix|var|env|calc|clamp|min|max|url|attr|" +
  "(?:repeating-)?(?:linear|radial|conic)-gradient|cubic-bezier|steps|" +
  "translate[XYZ3d]*|scale[XYZ3d]*|rotate[XYZ3d]*|skew[XY]?|matrix3?d?|perspective|blur|" +
  "drop-shadow|brightness|contrast|saturate|grayscale|opacity|invert|sepia|hue-rotate";

const CSS_KEYWORDS = new Set([
  "auto", "none", "normal", "initial", "inherit", "unset", "revert", "currentcolor", "transparent",
  "solid", "dashed", "dotted", "double", "groove", "ridge", "inset", "outset", "hidden", "visible",
  "to", "at", "from", "in", "top", "bottom", "left", "right", "center", "circle", "ellipse",
  "closest-side", "closest-corner", "farthest-side", "farthest-corner",
  "cover", "contain", "repeat", "no-repeat", "repeat-x", "repeat-y", "round", "space",
  "border-box", "content-box", "padding-box", "fill-box", "stroke-box", "view-box",
  "bold", "bolder", "lighter", "italic", "oblique", "uppercase", "lowercase", "capitalize",
  "block", "inline", "flex", "grid", "absolute", "relative", "fixed", "sticky", "static",
  "ease", "ease-in", "ease-out", "ease-in-out", "linear", "infinite", "alternate", "forwards",
  "both", "backwards", "running", "paused", "start", "end", "text",
]);

/// A HARD CSS MARKER IS REQUIRED before anything can be called a CSS value: a function call
/// of the grammar, a hex color, or a number carrying a CSS unit. Without this clause the
/// keyword vocabulary alone would swallow a short English sentence made only of words CSS
/// also uses — "at top left", "from left to right" — and a filter that can eat a sentence is
/// a filter that can hide a state. With it, a string has to contain a token that is not
/// English at all before its remaining words are even looked at.
const CSS_MARKER = new RegExp(
  "\\b(?:" + CSS_FUNCTIONS + ")\\(|#[0-9a-fA-F]{3,8}\\b|" +
    "-?\\d*\\.?\\d+(?:px|em|rem|ex|ch|vh|vw|vmin|vmax|pt|pc|deg|rad|turn|fr|ms|q)\\b",
  "i"
);

function looksLikeCssValue(s) {
  let t = String(s);
  if (!CSS_MARKER.test(t)) return false;
  // Function calls, innermost first, until the string stops changing. The bound is a guard
  // against a bug in this loop, not a belief about nesting depth.
  for (let i = 0; i < 20; i++) {
    const next = t.replace(new RegExp("\\b(?:" + CSS_FUNCTIONS + ")\\([^()]*\\)", "gi"), " ");
    if (next === t) break;
    t = next;
  }
  t = t
    .replace(/#[0-9a-fA-F]{3,8}\b/g, " ")
    .replace(/-?\d*\.?\d+(?:px|%|em|rem|ex|ch|vh|vw|vmin|vmax|pt|pc|cm|mm|in|deg|rad|turn|fr|ms|s|q)?\b/gi, " ")
    .replace(/[,/()!;:*+−-]/g, " ");
  for (const tok of t.split(/\s+/).filter(Boolean)) {
    if (!/[A-Za-z]/.test(tok)) continue;
    if (!CSS_KEYWORDS.has(tok.toLowerCase())) return false;
  }
  // Every token that carried a letter was a token of CSS. Nothing in this string is English.
  return true;
}

/// Markup whose TEXT CONTENT is empty or under the prose floor: `<div class="sig"><div
/// class="n"></div><div class="l"></div></div>`, or `home.js:174`'s 3 KB inline SVG whose
/// only text node is the accessible title "RichOS".
///
/// A person reads the text content, never the tags, so the tags are what the prose test
/// should not be reading either. This DROPS a literal only when nothing readable survives
/// stripping — `"loro · <span class=v></span> months · <b>… memories</b>"` keeps its text and
/// stays in the inventory, because that sentence is on the home screen and somebody has to
/// classify it.
function isTaglessMarkup(s) {
  const t = String(s);
  if (t.indexOf("<") < 0 || !/<\/?[a-zA-Z][a-zA-Z0-9-]*[\s/>]/.test(t)) return false;
  const text = t
    .replace(/<[^>]*>/g, " ")
    .replace(/&[a-z]+;|&#\d+;/gi, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (text.length >= 12) {
    const words = text.split(/\s+/).filter(Boolean);
    if (words.length >= 3 && words.filter((w) => /[A-Za-z]{2}/.test(w)).length >= 2) return false;
  }
  return true;
}

/// GLSL, which the prose filter alone reads as thirty perfectly good English-shaped words
/// (blind spot B6). `home/field-engine.js` carries fourteen shaders as string literals and
/// every one of them clears the three-token floor.
///
/// BY SHAPE, NOT BY FILE. A `void main()`, a `gl_`-prefixed symbol or a storage-qualifier
/// declaration is a shader wherever it is written; naming the file instead would mean a
/// shader moved to a new module walks straight back into the inventory, which is the same
/// class of defect as a typed source list. The three markers are chosen because none of them
/// can appear in a sentence a CEO reads: `gl_Position`, `gl_FragColor` and `gl_PointCoord`
/// are reserved GLSL identifiers, `void main()` is a C-family function signature, and
/// `uniform vec2` / `varying float` / `precision mediump` are GLSL declarations.
///
/// THE THIRD MARKER IS NOT DECORATION, AND MEASUREMENT IS WHAT PUT IT THERE. The first
/// version of this function had only the first two, and two of the fourteen literals are
/// SHARED PROLOGUES — a common `quietK()` and a common `toScr()` spliced into several
/// shaders — which contain neither `gl_` nor `void main()`. They sailed through and sat in
/// the state inventory as two 400-character "states" waiting to be hand-classified. A filter
/// verified by reading it would have shipped at 12 of 14.
///
/// MEASURED, not asserted. Across the twelve `ui` sources the derivation returns today there
/// are 285 prose-shaped literals with this clause off; it removes 14 of them, all 14 in
/// `home/field-engine.js`, and 0 anywhere else. `affordances.js` asserts both halves of that
/// — the count and the single file — so a future widening that started eating real states
/// fails rather than quietly shrinking the inventory.
function looksLikeShader(s) {
  return (
    /\bgl_[A-Za-z]/.test(s) ||
    /\bvoid\s+main\s*\(\s*\)/.test(s) ||
    /\b(?:uniform|varying|attribute|precision)\s+(?:highp|mediump|lowp|float|int|bool|vec[234]|mat[234]|sampler2D)\b/.test(s)
  );
}

// ---------------------------------------------------------------------------------------
// Rust: the strings that cross the bridge into the UI (blind spot B2)
// ---------------------------------------------------------------------------------------

/// `src-tauri/src` is the command layer — the ONLY Rust the renderer calls, and the only
/// place an authored `Err(String)` is written for the CEO to read. `crates/` is walked too,
/// but only `ceo_message()` bodies inside it survive the filter below: richos-core's
/// `Display` impls are machinery ("cognition io: broken pipe", "the loro slice did not
/// parse"), and although one CAN leak through `String(e)` into a local notice, that surface
/// is unbounded — see blind spot B5. An inventory that listed 37 core error strings as
/// "states" would be inventing a set, not deriving one.
const RUST_ROOTS = [
  path.resolve(UI_DIR, "..", "src-tauri", "src"),
  path.resolve(UI_DIR, "..", "crates"),
];

/// Rust files whose authored `Err(String)` sentences the renderer relays. Outside these,
/// only `ceo_message()` counts.
const RUST_COMMAND_LAYER = path.resolve(UI_DIR, "..", "src-tauri", "src");

function walk(dir, out) {
  let entries;
  try { entries = fs.readdirSync(dir, { withFileTypes: true }); } catch (_e) { return out; }
  for (const e of entries) {
    const p = path.join(dir, e.name);
    // `examples/` and `tests/` are operator binaries and unit tests. Their prose is printed
    // to a terminal by an engineer, never to the CEO's screen, and including it puts
    // "cannot open the microphone" in an inventory of UI states.
    if (e.isDirectory()) {
      if (e.name !== "target" && e.name !== "examples" && e.name !== "tests") walk(p, out);
    }
    else if (e.name.endsWith(".rs")) out.push(p);
  }
  return out;
}

/// Rust string literals with 1-based start lines. Comments dropped, `\`-at-end-of-line
/// continuation resolved the way rustc resolves it (the newline AND the following
/// indentation are swallowed), raw strings skipped whole, and `'a` lifetimes distinguished
/// from `'a'` char literals.
///
/// A LINE-BASED GREP IS NOT ENOUGH HERE, and the first version of this file proved it: every
/// CEO-facing sentence in main.rs is written across two or three lines with a trailing
/// backslash, so a per-line regex saw only fragments — and it found the strings quoted in
/// the TEST module instead of the const the app actually ships.
function rustStringLiterals(src, withOffsets) {
  const out = [];
  let i = 0;
  let line = 1;
  const n = src.length;
  while (i < n) {
    const c = src[i];
    if (c === "\n") { line++; i++; continue; }
    if (c === "/" && src[i + 1] === "/") { while (i < n && src[i] !== "\n") i++; continue; }
    if (c === "/" && src[i + 1] === "*") {
      i += 2;
      let depth = 1;
      while (i < n && depth > 0) {
        if (src[i] === "\n") line++;
        else if (src[i] === "/" && src[i + 1] === "*") { depth++; i++; }
        else if (src[i] === "*" && src[i + 1] === "/") { depth--; i++; }
        i++;
      }
      continue;
    }
    // raw string: r"..." / r#"..."# — skipped whole, never CEO prose in this codebase.
    if (c === "r" && (src[i + 1] === '"' || src[i + 1] === "#")) {
      let j = i + 1;
      let hashes = 0;
      while (src[j] === "#") { hashes++; j++; }
      if (src[j] === '"') {
        const close = '"' + "#".repeat(hashes);
        const end = src.indexOf(close, j + 1);
        const stop = end < 0 ? n : end + close.length;
        for (let k = i; k < stop; k++) if (src[k] === "\n") line++;
        i = stop;
        continue;
      }
    }
    // char literal vs lifetime
    if (c === "'") {
      if (src[i + 1] === "\\") { i += 4; continue; }
      if (src[i + 2] === "'") { i += 3; continue; }
      i++;
      continue;
    }
    if (c === '"') {
      const startLine = line;
      let buf = "";
      const offs = withOffsets ? [] : null;
      i++;
      while (i < n) {
        const d = src[i];
        if (d === "\\") {
          const e = src[i + 1];
          if (e === "\n") {
            // rustc: the newline and all following whitespace vanish.
            line++;
            i += 2;
            while (i < n && (src[i] === " " || src[i] === "\t")) i++;
            continue;
          }
          if (e === "n") buf += " ";
          else if (e === "t") buf += " ";
          else buf += e;
          if (offs) offs.push(e === "n" || e === "t" ? i : i + 1);
          i += 2;
          continue;
        }
        if (d === '"') { i++; break; }
        if (d === "\n") line++;
        buf += d;
        if (offs) offs.push(i);
        i++;
      }
      out.push(offs ? { text: buf, line: startLine, offsets: offs } : { text: buf, line: startLine });
      continue;
    }
    i++;
  }
  return out;
}

/// WHICH RUST STRINGS CAN REACH THE CEO'S SCREEN.
///
/// Two paths, and both are structural rather than a list of blessed identifiers:
///
///   1. `ceo_message()` — richos-voice's own name for "the calm line the CEO sees"
///      (controller.rs:73). Every literal inside a function of that name is CEO-facing by
///      the codebase's own declaration.
///   2. The `Err(String)` of a window command (`#[richos_command::command]`, which expands
///      to `#[tauri::command(async)]`), and the `const … : &str` those errors
///      are built from. `main.js` relays those verbatim: `String(e)` into the local notice
///      (`send()`), into `#composer-blocked` (`create_thread_in`) and into
///      `#unbound-view-detail` (`loadTimeline`). So a sentence written beside `Err(` or
///      `ok_or` in a command is a sentence the CEO can read.
///
/// The first version of this scrape keyed on a typed list of field names, and it missed
/// BOTH of the two worst states in the app — "I'm not connected to my thinking right now"
/// and the RICHOS_ENTITY instruction — while confidently reporting the fragments that
/// appear in main.rs's test assertions. That is the typed-list defect one more time, inside
/// the very module written to stop it.
const RUST_CEO_CONTEXT = /\bErr\(|\bok_or|const\s+[A-Z0-9_]+\s*:\s*&'?\w*\s*str\s*=|\.into\(\)/;

/// The 0-based `[first, last]` line range of every `#[cfg(test)]` item in a Rust file.
///
/// Braces are counted from the item's opening `{`, with string and char literals and
/// comments skipped so a `"}"` inside a fixture cannot close the module early. A
/// `#[cfg(test)]` on something with no block at all (a `use`, a `const`) covers its own
/// line only, which is what it should.
function testModuleRanges(lines) {
  const ranges = [];
  for (let i = 0; i < lines.length; i++) {
    if (!/^\s*#\[cfg\(test\)\]/.test(lines[i])) continue;
    // Walk forward to the block that attribute is attached to. An intervening attribute or
    // a `mod x` written across lines is fine; a `;` before any `{` means there is no block.
    let j = i;
    let opened = false;
    let depth = 0;
    let end = i;
    scan: for (; j < lines.length; j++) {
      const line = lines[j];
      for (let k = 0; k < line.length; k++) {
        const c = line[k];
        if (c === "/" && line[k + 1] === "/") break; // rest of the line is a comment
        if (c === '"') {
          k++;
          while (k < line.length && line[k] !== '"') {
            if (line[k] === "\\") k++;
            k++;
          }
          continue;
        }
        if (c === "{") {
          opened = true;
          depth++;
        } else if (c === "}") {
          depth--;
          if (opened && depth <= 0) {
            end = j;
            break scan;
          }
        } else if (c === ";" && !opened) {
          end = j;
          break scan;
        }
      }
      end = j;
    }
    ranges.push([i, end]);
    i = end;
  }
  return ranges;
}

function rustStrings() {
  const files = [];
  for (const r of RUST_ROOTS) walk(r, files);
  const out = [];
  for (const f of files) out.push(...rustCeoLiterals(f, fs.readFileSync(f, "utf8")));
  return out;
}

/// `rustStrings()` for one file, from its text: the CEO-facing literals of `src`, which is
/// the content of the Rust file at absolute path `f`. `withOffsets` keeps each literal's
/// `offsets` (see `rustStringLiterals`) for the Oxford comma fixer.
function rustCeoLiterals(f, src, withOffsets) {
  const out = [];
  const repoRoot = path.resolve(UI_DIR, "..", "..", "..");
  {
    const lines = src.split("\n");

    // Inline `#[cfg(test)] mod … { … }` — test prose is not product prose, and a
    // directory-based exclusion cannot see a module that lives inside a shipped file.
    //
    // THE MODULE'S EXTENT IS COUNTED, NOT ASSUMED TO RUN TO EOF. Until 2026-08-30 this
    // took the FIRST `#[cfg(test)]` line and skipped everything after it, which is only
    // correct for a file whose test module is last. `src-tauri/src/main.rs` is not that
    // file: `mod navigation_tests` sits at :1378-:1563 with 311 lines of SHIPPING command
    // code after it, so both correction desks' CEO-facing refusals — the loro sentence at
    // :1693 and the spoken one at :1800 — were invisible to this inventory and therefore
    // to the affordance rule. That is the typed-list defect in its scanner costume: a
    // boundary guessed once instead of derived, reporting a complete scrape over 83% of a
    // file. The braces are now balanced from the module's opening `{`.
    const testRanges = testModuleRanges(lines);
    const inTests = (idx) => testRanges.some(([a, b]) => idx >= a && idx <= b);

    // The nearest preceding `fn NAME`, per line.
    const fnAt = [];
    let currentFn = "";
    for (let i = 0; i < lines.length; i++) {
      const m = lines[i].match(/\bfn\s+([A-Za-z0-9_]+)/);
      if (m) currentFn = m[1];
      fnAt.push(currentFn);
    }

    for (const lit of rustStringLiterals(src, withOffsets)) {
      const idx = lit.line - 1;
      if (inTests(idx)) continue;
      if (!looksLikeProse(lit.text)) continue;
      const own = lines[idx] || "";
      if (/assert/.test(own)) continue;
      // The literal's own line plus the two above it — enough for `return Err(` or a
      // `const … : &str =` that sits on the line before the sentence.
      const ctx = [lines[idx - 2] || "", lines[idx - 1] || "", own].join("\n");
      const inCommandLayer = f.startsWith(RUST_COMMAND_LAYER + path.sep);
      const ceoFacing = fnAt[idx] === "ceo_message" || (inCommandLayer && RUST_CEO_CONTEXT.test(ctx));
      if (!ceoFacing) continue;
      const rec = { text: lit.text, file: path.relative(repoRoot, f), line: lit.line };
      if (withOffsets) rec.offsets = lit.offsets;
      out.push(rec);
    }
  }
  return out;
}

// ---------------------------------------------------------------------------------------
// The inventory
// ---------------------------------------------------------------------------------------

/// Collapse whitespace so a sentence wrapped across three source lines matches the same
/// sentence written on one. The registry keys on this normal form.
function normalize(s) {
  return String(s).replace(/\s+/g, " ").trim();
}

/// THE SOURCE LIST IS DERIVED, AND THIS IS WHERE IT USED TO BE TYPED.
///
/// It read `["index.html", "main.js", "timeline.js"]` — three files, under a header that
/// says "NEVER TYPE IT" — while `index.html` shipped nine `<script src>` tags and the tree
/// held twelve product files in total. So `updates.js`, `home.js`, `settings-button.js`,
/// `splash.js`, `splash-library.js`, `theme-boot.js` and the three `home/field-*.js`
/// modules were outside the affordance rule entirely: a state in any of them could ask the
/// CEO to do something with no control anywhere near it and this suite would have reported
/// green. `updates.js` is the one that matters most — it is the update flow, it is the
/// surface CEO ruling §26 governs, and "the update affordance must not be actionable while
/// work is running" was a rule the gate for it could not see the file to check.
///
/// `lib/ui-sources.js` derives it from `index.html` and the closure over what the loaded
/// files themselves load, and reconciles that against the tree in both directions. Adding a
/// script to the shell adds it here; adding a file nobody loads FAILS rather than being
/// skipped.
function uiSources() {
  return SOURCES.stateSources();
}

/// Every user-visible prose string the shipped UI can render, derived from disk.
/// `{ text, normal, sites: ["main.js:1113", ...] }`, sorted by normal form.
function inventory() {
  const byNormal = new Map();
  const add = (text, site) => {
    const normal = normalize(text);
    if (!looksLikeProse(normal)) return;
    let rec = byNormal.get(normal);
    if (!rec) { rec = { text: normal, normal, sites: [] }; byNormal.set(normal, rec); }
    if (rec.sites.indexOf(site) < 0) rec.sites.push(site);
  };

  for (const name of uiSources()) {
    const src = fs.readFileSync(path.join(UI_DIR, name), "utf8");
    if (name.endsWith(".html")) {
      for (const s of htmlVisibleStrings(src)) add(s.text, name + ":" + s.line);
    } else {
      for (const s of jsStringLiterals(src)) add(s.text, name + ":" + s.line);
    }
  }
  for (const s of rustStrings()) add(s.text, s.file + ":" + s.line);

  return Array.from(byNormal.values()).sort((a, b) => (a.normal < b.normal ? -1 : 1));
}

/// The `preview-data` files, kept OUT of the inventory and reported separately (blind spot
/// B4). Which files those are is `lib/ui-sources.js`'s classification, not a name typed
/// here: it was `mock.js` alone until the derivation found `home/field-data.js` beside it.
function previewDataStrings() {
  const seen = new Set();
  for (const name of SOURCES.previewDataSources()) {
    const src = fs.readFileSync(path.join(UI_DIR, name), "utf8");
    for (const s of jsStringLiterals(src)) {
      const nrm = normalize(s.text);
      if (looksLikeProse(nrm)) seen.add(nrm);
    }
  }
  return Array.from(seen).sort();
}

/// The shader literals this scrape drops (blind spot B6), with their sites — so the count
/// is a number a check can assert on rather than a claim in a comment.
function shaderStrings() {
  const out = [];
  for (const name of uiSources()) {
    if (name.endsWith(".html")) continue;
    const src = fs.readFileSync(path.join(UI_DIR, name), "utf8");
    for (const s of jsStringLiterals(src)) {
      const nrm = normalize(s.text);
      if (looksLikeShader(nrm)) out.push({ text: nrm, site: name + ":" + s.line });
    }
  }
  return out;
}

/// Every Rust source file the scrape walks (`RUST_ROOTS`, tests and examples excluded), for a
/// check that reads more of it than `rustStrings()` does (`dialect.js`'s dash scan).
function rustSourceFiles() {
  const files = [];
  for (const r of RUST_ROOTS) walk(r, files);
  return files;
}

const RUST_MACHINE_LINE = /\b(eprintln|println|print|panic|assert|assert_eq|assert_ne|debug_assert|unreachable|todo|unimplemented|debug|info|warn|trace)!|\.expect\(/;
const PROMPT_HEADER = /^[A-Z]{4,}\b|\b[A-Z][A-Z']{2,}\b[ :]+[A-Z][A-Z']{1,}\b/;

/// Sentence-shaped Rust literals outside test modules, minus log/assert lines and prompts: a
/// capital first letter, sentence punctuation last, four words or more. The m-dash check's net,
/// read by the Oxford comma fixer too.
function rustSentenceLiterals(src, withOffsets) {
  const lines = src.split("\n");
  const tests = testModuleRanges(lines);
  const out = [];
  for (const lit of rustStringLiterals(src, withOffsets)) {
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

module.exports = {
  rustSentenceLiterals,
  inventory,
  testModuleRanges,
  rustStringLiterals,
  rustSourceFiles,
  previewDataStrings,
  shaderStrings,
  rustStrings,
  rustCeoLiterals,
  jsStringLiterals,
  htmlVisibleStrings,
  looksLikeProse,
  looksLikeShader,
  normalize,
  UI_DIR,
  uiSources,
};

// `node lib/state-strings.js` prints the derived inventory — the derivation, runnable.
if (require.main === module) {
  const inv = inventory();
  for (const rec of inv) console.log(rec.sites.join(" ") + "\n  " + JSON.stringify(rec.normal));
  console.error(
    "\n" + inv.length + " user-visible prose string(s) derived from " + uiSources().join(", ") +
      " + the Rust bridge scrape"
  );
  console.error(
    previewDataStrings().length + " further prose string(s) in " +
      SOURCES.previewDataSources().join(", ") + " (seeded preview data — NOT states)"
  );
  console.error(shaderStrings().length + " GLSL shader literal(s) dropped (blind spot B6)");
}
