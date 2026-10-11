// EVERY OVERLAY THE APP CAN OPEN, DERIVED FROM THE SHIPPED MARKUP AND CSS. NEVER TYPED.
//
// WHY THIS FILE EXISTS
// ====================
// The CEO's §15 puts the Settings button on every screen and above every overlay. Three review
// rounds in a row each found one more overlay that covered it or took it out of reach, every
// one missing from a hand-written list: Corrections and three more (nightly 48's walk, D24), the
// Technical view question (second review of 9463220ef, finding 2), the Home screen buttons
// panel at z-index 340 (review rv-20261010T081721Z-66ba5386-a4d1, finding 2), and behind them
// six sheets the Settings menu itself opens, which inerted Settings through main.js's question
// walk. So the inventory is derived from what ships, and `tests/appearance.js` check 7c holds
// Settings on top and reachable over each:
//
//   * `coveringRules()`  every rule in the shipped CSS (`lib/ui-sources.js` styleSources) that
//                        pins an element over the whole window: `position: fixed` and all four
//                        edges at 0 (`inset: 0`, or top/right/bottom/left 0, or a full-size box).
//   * `ariaModalSites()` every place the shipped markup or code (comments stripped, strings
//                        kept) declares `aria-modal`. WebKit leaves everything outside an
//                        aria-modal dialog that holds focus out of the accessibility tree, so
//                        Settings is never reachable over one that claims it. Check 7c fails
//                        each site by name, except on the two first-run questions, which hold
//                        Settings back by design (main.js QUESTION_SELECTOR, setup.js check 20).
//
// RUN IT:  node lib/overlays.js   — prints both inventories.

"use strict";

const fs = require("fs");
const SOURCES = require("./ui-sources");

/// Whether a declaration block pins its element over the whole window.
function coversWindow(body) {
  if (!/position:\s*fixed/.test(body)) return false;
  const zero = (edge) => new RegExp("(^|[;\\s{])" + edge + ":\\s*0(px)?\\s*(;|!|$)").test(body);
  if (/(^|[;\s{])inset:\s*0(px)?\s*(;|!|$)/.test(body)) return true;
  if (zero("top") && zero("right") && zero("bottom") && zero("left")) return true;
  const full = (prop, unit) => new RegExp("(^|[;\\s{])" + prop + ":\\s*100(%|" + unit + ")").test(body);
  return zero("top") && zero("left") && full("width", "vw") && full("height", "vh");
}

/// `{ file, selector, z }` for every selector of every covering rule, in source order.
function coveringRules() {
  const out = [];
  for (const file of SOURCES.styleSources()) {
    const css = SOURCES.stripCssComments(fs.readFileSync(SOURCES.abs(file), "utf8"));
    const rule = /([^{}@]+)\{([^{}]*)\}/g;
    let m;
    while ((m = rule.exec(css))) {
      if (!coversWindow(m[2])) continue;
      const z = (m[2].match(/z-index:\s*(-?\d+)/) || [])[1];
      for (const selector of m[1].split(",").map((s) => s.trim()).filter(Boolean)) {
        out.push({ file, selector, z: z === undefined ? null : Number(z) });
      }
    }
  }
  if (!out.length) throw new Error("EMPTY INVENTORY: no rule in the shipped CSS covers the window, which `.overlay` alone does");
  return out;
}

/// `{ file, line, text }` for every `aria-modal` the shipped markup and code declares.
function ariaModalSites() {
  const out = [];
  for (const { name, code } of SOURCES.sourcesOfRole("ui")) {
    code.split("\n").forEach((line, i) => {
      if (/aria-?modal/i.test(line)) out.push({ file: name, line: i + 1, text: line.trim().slice(0, 160) });
    });
  }
  return out;
}

module.exports = { coversWindow, coveringRules, ariaModalSites };

if (require.main === module) {
  console.log("Rules that cover the window:");
  for (const r of coveringRules()) console.log("  " + r.file + "  " + r.selector + "  z-index " + (r.z === null ? "auto" : r.z));
  const sites = ariaModalSites();
  console.log("aria-modal declared: " + (sites.length ? "" : "nowhere"));
  for (const s of sites) console.log("  " + s.file + ":" + s.line + "  " + s.text);
}
